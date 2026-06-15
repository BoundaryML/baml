//! `.bamlprof` file writing and reading (v2 §4 framing).
//!
//! Framing: one length-delimited [`pb::EventFileHeaderV1`], then a stream of
//! length-delimited [`pb::DiskEventV1`] messages. One file per engine per
//! process: `<process_id>-<started_at>-<engine>.bamlprof`.

use std::{
    fs::{self, File},
    io::{self, Read, Write},
    path::{Path, PathBuf},
};

use prost::Message;

use crate::prof::pb;

/// A single engine's open profile file.
pub(crate) struct ProfileWriter {
    file: File,
    path: PathBuf,
    /// Reused length-delimited encode buffer. One allocation that grows to
    /// the largest message, then stays — avoids a `Vec` allocation (and the
    /// extra `encoded_len` walk a pre-sized `with_capacity` would cost) on
    /// every event written. The consumer transcodes one event per record on
    /// its hot path, so this is the difference between zero and one malloc
    /// per traced event.
    scratch: Vec<u8>,
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
            file,
            path,
            scratch: Vec::new(),
        };
        writer.write_message(header)?;
        Ok(writer)
    }

    /// Appends one length-delimited event to the in-flight range buffer
    /// (`scratch`). The consumer accumulates a whole drained range with
    /// repeated `encode_event`, then issues a single [`flush_buffered`] — one
    /// `write` per ~256 KiB drained segment instead of one per event. There is
    /// no userspace `BufWriter`: the range buffer *is* the buffer.
    ///
    /// The two hot records — `CallFunction`/`EndFunction`, the per-call pair
    /// that is essentially all of the event volume — are serialized by a
    /// hand-rolled single-pass protobuf encoder rather than prost.
    ///
    /// Why: prost serializes this nested-oneof message by walking it ~3×
    /// (`encoded_len`, then `encode_raw`, whose embedded-message path recomputes
    /// the inner message's length a third time) before writing a byte. Measured
    /// at ~40-55 ns/record — roughly a third of the consumer's per-record cost.
    /// A direct encoder with back-patched 1-byte length prefixes does it in a
    /// single pass. This is the same approach Perfetto takes with its trace
    /// protobufs — its "protozero" streaming/zero-copy encoder, adopted there
    /// for exactly this hot-path-serialization reason
    /// (<https://perfetto.dev/docs/design-docs/protozero>).
    ///
    /// Correctness: `to_disk_event` still constructs the prost structs, so a
    /// proto field add/remove/rename/type-change is a *compile* error (the
    /// exhaustive struct literal + the field reads in the encoders below); the
    /// differential test `hand_encoder_matches_prost` pins byte-for-byte
    /// equality with prost over a range of inputs, covering even a field
    /// renumber. Every other (cold) record stays on prost.
    pub(crate) fn encode_event(&mut self, event: &pb::DiskEventV1) {
        use pb::disk_event_v1::Event;
        match &event.event {
            Some(Event::CallFunction(cf)) => encode_call_function_event(&mut self.scratch, cf),
            Some(Event::EndFunction(ef)) => encode_end_function_event(&mut self.scratch, ef),
            // Cold/rare records (StartThread, SetFunctionId, EndThread,
            // Heartbeat, and any future variant) go through prost. Encoding
            // into a growable `Vec` cannot run out of capacity.
            _ => {
                event
                    .encode_length_delimited(&mut self.scratch)
                    .expect("protobuf encode into a Vec never runs out of capacity");
            }
        }
    }

    /// Write the accumulated range buffer to the file in one syscall and reset
    /// it. No-op when nothing is buffered.
    pub(crate) fn flush_buffered(&mut self) -> io::Result<()> {
        if !self.scratch.is_empty() {
            self.file.write_all(&self.scratch)?;
            self.scratch.clear();
        }
        Ok(())
    }

    fn write_message(&mut self, msg: &impl Message) -> io::Result<()> {
        // One-shot header write at file creation: encode + a single write.
        self.scratch.clear();
        msg.encode_length_delimited(&mut self.scratch)
            .map_err(io::Error::other)?;
        self.file.write_all(&self.scratch)?;
        self.scratch.clear();
        Ok(())
    }

    /// Flush buffered range bytes to the OS (no `fsync`). The cadence flush.
    pub(crate) fn flush(&mut self) -> io::Result<()> {
        self.flush_buffered()
    }

    /// Flush + `fsync`: survives power loss, not just process exit. Used for
    /// explicit flush requests and engine closes.
    pub(crate) fn sync(&mut self) -> io::Result<()> {
        self.flush_buffered()?;
        self.file.sync_all()
    }

    pub(crate) fn path(&self) -> &Path {
        &self.path
    }
}

// ── Hand-rolled protobuf for the two hot events (see `write_event`) ──────────
//
// These emit a length-delimited `DiskEventV1{ call_function | end_function }`
// byte-for-byte as prost would, in one pass. Both messages are < 128 bytes
// (≤ 5 small scalar fields), so the outer (`DiskEventV1`) and inner length
// prefixes are each a single varint byte we back-patch — minimal varints,
// identical to prost's output. proto3 implicit-presence rules are replicated
// explicitly: zero-valued scalars are omitted; `optional parent_call_id` is
// emitted iff `Some`. Field numbers/wire-types are from `bamlprof.proto`
// (`DiskEventV1.call_function = 3`, `.end_function = 5`; inner fields 1..=5).

/// Append `v` as a protobuf LEB128 varint.
#[inline]
#[expect(
    clippy::cast_possible_truncation,
    reason = "varint extracts the low 7 bits per byte"
)]
fn put_varint(out: &mut Vec<u8>, mut v: u64) {
    while v >= 0x80 {
        out.push((v as u8) | 0x80);
        v >>= 7;
    }
    out.push(v as u8);
}

/// proto3 implicit-presence scalar: key byte + varint, omitted when the value
/// is the default (0) — matching prost.
#[inline]
fn put_scalar(out: &mut Vec<u8>, key: u8, v: u64) {
    if v != 0 {
        out.push(key);
        put_varint(out, v);
    }
}

/// Back-patch the inner-message and outer-`DiskEventV1` single-byte length
/// prefixes once the body is written.
#[inline]
#[expect(
    clippy::cast_possible_truncation,
    reason = "messages are < 128 bytes; asserted"
)]
fn patch_lengths(out: &mut [u8], outer_at: usize, inner_len_at: usize, inner_start: usize) {
    let inner_len = out.len() - inner_start;
    debug_assert!(
        inner_len < 128,
        "event message exceeds single-byte length prefix"
    );
    out[inner_len_at] = inner_len as u8;
    let outer_len = out.len() - outer_at - 1;
    debug_assert!(
        outer_len < 128,
        "DiskEventV1 exceeds single-byte length prefix"
    );
    out[outer_at] = outer_len as u8;
}

fn encode_call_function_event(out: &mut Vec<u8>, cf: &pb::CallFunction) {
    let outer_at = out.len();
    out.push(0); // DiskEventV1 length (patched)
    out.push(0x1A); // field 3 (call_function), wire-type 2 (LEN)
    let inner_len_at = out.len();
    out.push(0); // CallFunction length (patched)
    let inner_start = out.len();
    put_scalar(out, 0x08, cf.thread_id); // field 1
    put_scalar(out, 0x10, cf.call_id); // field 2
    if let Some(p) = cf.parent_call_id {
        // field 3, optional (explicit presence): emitted iff Some.
        out.push(0x18);
        put_varint(out, p);
    }
    put_scalar(out, 0x20, u64::from(cf.function_id)); // field 4 (uint32)
    put_scalar(out, 0x28, cf.timestamp_ns); // field 5
    patch_lengths(out, outer_at, inner_len_at, inner_start);
}

fn encode_end_function_event(out: &mut Vec<u8>, ef: &pb::EndFunction) {
    let outer_at = out.len();
    out.push(0); // DiskEventV1 length (patched)
    out.push(0x2A); // field 5 (end_function), wire-type 2 (LEN)
    let inner_len_at = out.len();
    out.push(0); // EndFunction length (patched)
    let inner_start = out.len();
    put_scalar(out, 0x08, ef.thread_id); // field 1
    put_scalar(out, 0x10, ef.call_id); // field 2
    // field 3, status enum (implicit presence): omit default (0 = OK).
    if ef.status != 0 {
        out.push(0x18);
        put_varint(out, u64::try_from(ef.status).unwrap_or(0));
    }
    put_scalar(out, 0x20, ef.timestamp_ns); // field 4
    patch_lengths(out, outer_at, inner_len_at, inner_start);
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
    clock: &crate::prof::clock::TickConverter,
) -> pb::EventFileHeaderV1 {
    use crate::prof::clock::{ClockKind, ClockQuality};
    let (tick_ns_numer, tick_ns_denom) = clock.rate();
    pb::EventFileHeaderV1 {
        process_id: process_id.to_vec(),
        engine_id,
        program_id: meta.map(|m| m.program_id.clone()).unwrap_or_default(),
        started_at_epoch_ns: started_at_epoch_ns.to_le_bytes().to_vec(),
        clock_kind: match clock.kind() {
            ClockKind::Tsc => pb::ClockKind::Tsc,
            ClockKind::Cntvct => pb::ClockKind::Cntvct,
            ClockKind::Instant => pb::ClockKind::Instant,
            ClockKind::Stub => pb::ClockKind::Stub,
        } as i32,
        tick_ns_numer,
        tick_ns_denom,
        clock_quality: match clock.quality() {
            ClockQuality::Exact => pb::ClockQuality::Exact,
            ClockQuality::Calibrated => pb::ClockQuality::Calibrated,
            ClockQuality::Coarse => pb::ClockQuality::Coarse,
        } as i32,
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

#[cfg(test)]
mod hand_encoder_tests {
    use prost::Message;

    use super::{encode_call_function_event, encode_end_function_event};
    use crate::prof::pb;

    /// The hand-rolled encoders must produce byte-for-byte the same output as
    /// prost's `encode_length_delimited` for every input — that equality is
    /// the wire contract. This also catches a proto field *renumber* that the
    /// struct literal in `to_disk_event` would not (the gap noted in
    /// `write_event`'s docs).
    #[test]
    fn hand_encoder_matches_prost() {
        let call_fns = [
            // includes zero scalars (omitted), max-width varints, and the
            // optional parent present (Some) / absent (None).
            (1u64, 1u64, None, 0u32, 0u64),
            (5, 7, Some(3), 9, 12_345),
            (u64::MAX, 1, Some(2), u32::MAX, u64::MAX),
            (0, 0, None, 0, 0),
            (128, 16_384, Some(2_097_152), 100_000, 1 << 40),
            (9, 7, Some(0), 4, 1), // optional present with value 0
        ];
        for &(thread_id, call_id, parent_call_id, function_id, timestamp_ns) in &call_fns {
            let cf = pb::CallFunction {
                thread_id,
                call_id,
                parent_call_id,
                function_id,
                timestamp_ns,
            };
            let event = pb::DiskEventV1 {
                event: Some(pb::disk_event_v1::Event::CallFunction(cf)),
            };
            let mut prost_bytes = Vec::new();
            event.encode_length_delimited(&mut prost_bytes).unwrap();
            let mut hand = Vec::new();
            encode_call_function_event(&mut hand, &cf);
            assert_eq!(hand, prost_bytes, "CallFunction {cf:?}");
        }

        for status in 0..4i32 {
            for &(thread_id, call_id, timestamp_ns) in
                &[(1u64, 1u64, 0u64), (9, 7, 12_345), (u64::MAX, 1, u64::MAX)]
            {
                let ef = pb::EndFunction {
                    thread_id,
                    call_id,
                    status,
                    timestamp_ns,
                };
                let event = pb::DiskEventV1 {
                    event: Some(pb::disk_event_v1::Event::EndFunction(ef)),
                };
                let mut prost_bytes = Vec::new();
                event.encode_length_delimited(&mut prost_bytes).unwrap();
                let mut hand = Vec::new();
                encode_end_function_event(&mut hand, &ef);
                assert_eq!(hand, prost_bytes, "EndFunction {ef:?}");
            }
        }
    }
}
