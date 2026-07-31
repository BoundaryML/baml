//! §6.2 raw firehose sink (`BAML_PROFILE_RAW=1`, absorbing N5 "tcpdump").
//!
//! Verbatim drained ring ranges, framed per range, under the owning
//! session's `raw/` directory. Raw records are fully self-describing
//! (every record carries `thread_id`; §5.3), so concatenated ranges losslessly
//! reproduce exactly what the consumer's transcode fan-out saw, in drain
//! order — including the cross-ring interleavings the CCT engine's
//! defer/retry path exists to handle. That makes these files the raw-path
//! oracle input (§10.3) and the first casualty of retention (§6.8).
//!
//! File layout (`raw-NNNNNN.bamlprof`, 1-based, rotated at 64 MiB):
//!
//! ```text
//! [64-byte header]
//!   magic            8B  "BAMLRAW1"
//!   version          u16 LE (1)
//!   header_len       u16 LE (64)
//!   flags            u32 LE (0)
//!   process_euid     [u8; 16]
//!   engine_id        u64 LE
//!   clock_kind       u8      \  TickConverter identity: raw record
//!   clock_quality    u8       ) timestamps are ticks; ns = ticks *
//!   pad              [u8; 6] /  numer / denom
//!   clock_numer      u64 LE
//!   clock_denom      u64 LE
//! [ranges]*
//!   range_len        u32 LE
//!   range_bytes      [u8; range_len]   (raw ring records, tags 0x01–0x09)
//! ```
//!
//! A torn tail (partial frame at EOF) marks the crash point; readers stop
//! at the last complete frame. No fsync — this is debug telemetry, not
//! durable state.

use std::fs::{self, File};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

pub const RAW_MAGIC: [u8; 8] = *b"BAMLRAW1";
pub const RAW_VERSION: u16 = 1;
pub const RAW_HEADER_LEN: usize = 64;
/// Rotation threshold per §6.2 ("rotated 64 MiB").
pub const RAW_ROTATE_BYTES: u64 = 64 << 20;
/// Bound on ranges buffered before the session dir exists (the session is
/// minted by the first non-empty flush window, ≤250 ms after first record).
/// Overflow drops whole ranges, counted in `dropped_bytes`.
pub const RAW_PENDING_CAP: usize = 64 << 20;

/// One engine's raw firehose. Ranges buffer in `pending` until the session
/// directory exists; `flush_to` follows epoch rotations by resetting when
/// the session dir changes.
#[derive(Default)]
pub struct RawSink {
    pending: Vec<u8>,
    /// Ranges dropped on `pending` overflow (bytes, pre-framing).
    pub dropped_bytes: u64,
    dir: Option<PathBuf>,
    file: Option<File>,
    seq: u32,
    written: u64,
}

impl RawSink {
    /// Frame and buffer one drained range (u32 LE length prefix).
    pub fn push_range(&mut self, bytes: &[u8]) {
        if self.pending.len() + 4 + bytes.len() > RAW_PENDING_CAP {
            self.dropped_bytes += bytes.len() as u64;
            return;
        }
        let len = u32::try_from(bytes.len()).unwrap_or(u32::MAX);
        self.pending.extend_from_slice(&len.to_le_bytes());
        self.pending.extend_from_slice(bytes);
    }

    /// Write everything buffered into `<session_dir>/raw/`, opening or
    /// rotating files as needed. A changed `session_dir` (epoch rotation)
    /// resets the sequence under the new directory.
    pub fn flush_to(
        &mut self,
        session_dir: &Path,
        process_euid: [u8; 16],
        engine_id: u64,
        clock: (u8, u8, u64, u64),
    ) -> io::Result<()> {
        let raw_dir = session_dir.join("raw");
        if self.dir.as_deref() != Some(raw_dir.as_path()) {
            self.file = None;
            self.seq = 0;
            self.written = 0;
            self.dir = Some(raw_dir.clone());
        }
        if self.pending.is_empty() {
            return Ok(());
        }
        if self.file.is_none() {
            fs::create_dir_all(&raw_dir)?;
            self.seq += 1;
            let path = raw_dir.join(format!("raw-{:06}.bamlprof", self.seq));
            let mut file = File::create(&path)?;
            file.write_all(&header(process_euid, engine_id, clock))?;
            self.written = RAW_HEADER_LEN as u64;
            self.file = Some(file);
        }
        let file = self.file.as_mut().expect("opened above");
        file.write_all(&self.pending)?;
        self.written += self.pending.len() as u64;
        self.pending.clear();
        if self.written >= RAW_ROTATE_BYTES {
            self.file = None; // next flush opens raw-{seq+1}
        }
        Ok(())
    }
}

fn header(
    process_euid: [u8; 16],
    engine_id: u64,
    clock: (u8, u8, u64, u64),
) -> [u8; RAW_HEADER_LEN] {
    let (kind, quality, numer, denom) = clock;
    let mut h = [0u8; RAW_HEADER_LEN];
    h[0..8].copy_from_slice(&RAW_MAGIC);
    h[8..10].copy_from_slice(&RAW_VERSION.to_le_bytes());
    h[10..12].copy_from_slice(&u16::try_from(RAW_HEADER_LEN).unwrap_or(0).to_le_bytes());
    // flags [12..16] zero
    h[16..32].copy_from_slice(&process_euid);
    h[32..40].copy_from_slice(&engine_id.to_le_bytes());
    h[40] = kind;
    h[41] = quality;
    // pad [42..48] zero
    h[48..56].copy_from_slice(&numer.to_le_bytes());
    h[56..64].copy_from_slice(&denom.to_le_bytes());
    h
}

/// Parsed view of one raw file, for tests and `baml doctor`.
pub struct RawFile {
    pub process_euid: [u8; 16],
    pub engine_id: u64,
    pub clock: (u8, u8, u64, u64),
    /// Complete frames, in drain order.
    pub ranges: Vec<Vec<u8>>,
    /// Bytes past the last complete frame (torn tail at crash).
    pub torn_bytes: usize,
}

pub fn read_raw_file(bytes: &[u8]) -> io::Result<RawFile> {
    let bad = |m: &str| io::Error::new(io::ErrorKind::InvalidData, m.to_string());
    if bytes.len() < RAW_HEADER_LEN || bytes[0..8] != RAW_MAGIC {
        return Err(bad("not a BAMLRAW1 file"));
    }
    let version = u16::from_le_bytes(bytes[8..10].try_into().unwrap());
    if version != RAW_VERSION {
        return Err(bad("unsupported BAMLRAW version"));
    }
    let header_len = u16::from_le_bytes(bytes[10..12].try_into().unwrap()) as usize;
    if header_len < RAW_HEADER_LEN || header_len > bytes.len() {
        return Err(bad("bad BAMLRAW header length"));
    }
    let process_euid: [u8; 16] = bytes[16..32].try_into().unwrap();
    let engine_id = u64::from_le_bytes(bytes[32..40].try_into().unwrap());
    let clock = (
        bytes[40],
        bytes[41],
        u64::from_le_bytes(bytes[48..56].try_into().unwrap()),
        u64::from_le_bytes(bytes[56..64].try_into().unwrap()),
    );
    let mut ranges = Vec::new();
    let mut at = header_len;
    loop {
        let Some(len_bytes) = bytes.get(at..at + 4) else {
            break;
        };
        let len = u32::from_le_bytes(len_bytes.try_into().unwrap()) as usize;
        let Some(range) = bytes.get(at + 4..at + 4 + len) else {
            break;
        };
        ranges.push(range.to_vec());
        at += 4 + len;
    }
    Ok(RawFile {
        process_euid,
        engine_id,
        clock,
        ranges,
        torn_bytes: bytes.len() - at,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_with_rotation_reset_on_dir_change() {
        let tmp = std::env::temp_dir().join(format!("baml-raw-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&tmp);
        let session_a = tmp.join("sess-a");
        let session_b = tmp.join("sess-b");
        fs::create_dir_all(&session_a).unwrap();
        fs::create_dir_all(&session_b).unwrap();
        let clock = (1, 2, 3, 1_000_000_007);

        let mut sink = RawSink::default();
        sink.push_range(&[0xAA; 10]);
        sink.push_range(&[0xBB; 3]);
        sink.flush_to(&session_a, [7; 16], 9, clock).unwrap();
        sink.push_range(&[0xCC; 5]);
        sink.flush_to(&session_a, [7; 16], 9, clock).unwrap();

        let file = session_a.join("raw/raw-000001.bamlprof");
        let parsed = read_raw_file(&fs::read(&file).unwrap()).unwrap();
        assert_eq!(parsed.process_euid, [7; 16]);
        assert_eq!(parsed.engine_id, 9);
        assert_eq!(parsed.clock, clock);
        assert_eq!(
            parsed.ranges,
            vec![vec![0xAA; 10], vec![0xBB; 3], vec![0xCC; 5]]
        );
        assert_eq!(parsed.torn_bytes, 0);

        // Epoch rotation: a different session dir restarts the sequence.
        sink.push_range(&[0xDD; 2]);
        sink.flush_to(&session_b, [7; 16], 9, clock).unwrap();
        let parsed_b =
            read_raw_file(&fs::read(session_b.join("raw/raw-000001.bamlprof")).unwrap()).unwrap();
        assert_eq!(parsed_b.ranges, vec![vec![0xDD; 2]]);

        // Torn tail: truncate mid-frame, reader keeps the committed prefix.
        let mut torn = fs::read(&file).unwrap();
        torn.extend_from_slice(&100u32.to_le_bytes());
        torn.extend_from_slice(&[0xEE; 10]);
        let parsed_torn = read_raw_file(&torn).unwrap();
        assert_eq!(parsed_torn.ranges.len(), 3);
        assert_eq!(parsed_torn.torn_bytes, 14);
        let _ = fs::remove_dir_all(&tmp);
    }
}
