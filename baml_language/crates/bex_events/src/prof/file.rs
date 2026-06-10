//! `.bamlprof` file writing and reading (v2 §4 framing).
//!
//! Framing: one length-delimited [`pb::EventFileHeaderV1`], then a stream of
//! length-delimited [`pb::DiskEventV1`] messages. One file per engine per
//! process: `<process_id>-<started_at>-<engine>.bamlprof`.

use std::{
    fs::{self, File},
    io::{self, BufWriter, Read, Write},
    path::{Path, PathBuf},
};

use prost::Message;

use crate::prof::pb;

/// A single engine's open profile file.
pub(crate) struct ProfileWriter {
    writer: BufWriter<File>,
    path: PathBuf,
}

impl ProfileWriter {
    /// Creates the profiles directory if needed, the file, and writes the
    /// header.
    pub(crate) fn create(
        dir: &Path,
        process_id: [u8; 16],
        started_at_epoch_ns: u128,
        engine_id: u64,
        header: &pb::EventFileHeaderV1,
    ) -> io::Result<ProfileWriter> {
        fs::create_dir_all(dir)?;
        let started_at_secs = started_at_epoch_ns / 1_000_000_000;
        let name = format!(
            "{}-{started_at_secs}-{engine_id}.bamlprof",
            hex_uuid(process_id)
        );
        let path = dir.join(name);
        let file = File::create(&path)?;
        let mut writer = ProfileWriter {
            writer: BufWriter::new(file),
            path,
        };
        writer.write_message(header)?;
        Ok(writer)
    }

    pub(crate) fn write_event(&mut self, event: &pb::DiskEventV1) -> io::Result<()> {
        self.write_message(event)
    }

    fn write_message(&mut self, msg: &impl Message) -> io::Result<()> {
        let mut buf = Vec::with_capacity(msg.encoded_len() + 4);
        msg.encode_length_delimited(&mut buf)
            .map_err(io::Error::other)?;
        self.writer.write_all(&buf)
    }

    pub(crate) fn flush(&mut self) -> io::Result<()> {
        self.writer.flush()
    }

    /// Flush + `fsync`: survives power loss, not just process exit. Used for
    /// explicit flush requests and engine closes; the cadence flushes stay
    /// cheap.
    pub(crate) fn sync(&mut self) -> io::Result<()> {
        self.writer.flush()?;
        self.writer.get_ref().sync_all()
    }

    pub(crate) fn path(&self) -> &Path {
        &self.path
    }
}

fn hex_uuid(bytes: [u8; 16]) -> String {
    let mut s = String::with_capacity(32);
    for b in bytes {
        use std::fmt::Write as _;
        let _ = write!(s, "{b:02x}");
    }
    s
}

/// Builds the header message from registered engine metadata.
pub(crate) fn build_header(
    process_id: [u8; 16],
    engine_id: u64,
    started_at_epoch_ns: u128,
    meta: Option<&crate::prof::EngineProfileMetadata>,
) -> pb::EventFileHeaderV1 {
    pb::EventFileHeaderV1 {
        process_id: process_id.to_vec(),
        engine_id,
        program_id: meta.map(|m| m.program_id.clone()).unwrap_or_default(),
        started_at_epoch_ns: started_at_epoch_ns.to_le_bytes().to_vec(),
        function_table: Some(pb::FunctionMetadataTable {
            functions: meta
                .map(|m| {
                    m.functions
                        .iter()
                        .map(|f| pb::FunctionMetadata {
                            function_id: f.function_id,
                            fqn: f.fqn.clone(),
                            source_file: f.source_file.clone(),
                            span_start: f.span_start,
                            span_end: f.span_end,
                            kind: f.kind.clone(),
                        })
                        .collect()
                })
                .unwrap_or_default(),
        }),
    }
}

/// A parsed `.bamlprof` (see [`read_bamlprof`]).
pub struct BamlprofContents {
    /// The file header.
    pub header: pb::EventFileHeaderV1,
    /// Every whole event, in file order (NOT event order — sort by
    /// `timestamp_ns` within each `thread_id`; see the .proto header).
    pub events: Vec<pb::DiskEventV1>,
    /// The file ended mid-message: a live writer's heartbeat append caught
    /// in flight, or a crashed process's torn tail. `events` holds the
    /// whole-message prefix — partial-parseability is a design goal (v2
    /// core bet 3); a torn tail must not reject the good prefix.
    pub truncated: bool,
}

/// Reads a `.bamlprof` back: the header and every whole event, tolerating a
/// torn trailing message. Errors only when the file or its header is
/// unreadable. The reader for tests, gates, and ad-hoc tooling — the M5
/// renderer supersedes it for real consumption.
pub fn read_bamlprof(path: &Path) -> io::Result<BamlprofContents> {
    let mut bytes = Vec::new();
    File::open(path)?.read_to_end(&mut bytes)?;
    let mut buf = bytes.as_slice();
    let header = pb::EventFileHeaderV1::decode_length_delimited(&mut buf)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
    let mut events = Vec::new();
    let mut truncated = false;
    while !buf.is_empty() {
        match pb::DiskEventV1::decode_length_delimited(&mut buf) {
            Ok(event) => events.push(event),
            Err(_) => {
                truncated = true;
                break;
            }
        }
    }
    Ok(BamlprofContents {
        header,
        events,
        truncated,
    })
}

/// Decodes the header's 16-byte little-endian wall anchor.
#[must_use]
pub fn header_started_at_epoch_ns(header: &pb::EventFileHeaderV1) -> Option<u128> {
    let bytes: [u8; 16] = header.started_at_epoch_ns.as_slice().try_into().ok()?;
    Some(u128::from_le_bytes(bytes))
}
