//! The §6.4 meta record streams (`session.bamlmeta` / `boundary.bamlmeta`):
//! append-only `BMET` framing (§6.9), never rewrite-in-place. Each record
//! is length-prefixed and CRC'd individually, so a crash tears at most the
//! record being written; readers serve the committed prefix and report
//! `truncated` instead of erroring.
//!
//! Crash detection needs no per-boundary heartbeat rewrites (§6.4 ruling):
//! the *session* stream's heartbeat (one writer, coarse 10 s cadence, D0)
//! plus pid liveness answers "is this session alive"; a boundary `begin`
//! without `complete` under a dead session heartbeat reads as **crashed
//! (partial, readable)**.
//!
//! Payloads are `serde_json` for now: meta records are small (~200 B/run)
//! and cold (milestones plus the heartbeat), so JSON's self-describing
//! forward compat is worth more than the bytes; a binary codec can replace
//! it later without changing the framing, because the framing never looks
//! inside the payload.
//!
//! The durability POLICY lives with callers (§6.6: begin/bound/complete/
//! end milestones D2, heartbeats D0) — [`MetaWriter`] only exposes
//! [`MetaWriter::sync_data`]. Encode/decode is target-neutral (bytes in,
//! bytes out); only the thin file-append helper is native.

use super::crc32c;

/// The 8-byte v1 stream header: 5-byte magic `BMET\0`, then the format
/// version as LE u16 (currently 1), then one reserved byte.
pub const META_MAGIC: [u8; 8] = *b"BMET\0\x01\0\0";

/// The version encoded in [`META_MAGIC`] bytes 5..7.
const FORMAT_VERSION: u16 = 1;

/// Bytes of the 8-byte stream header.
const HEADER_LEN: usize = META_MAGIC.len();

/// Per-record framing overhead: u32 LE `payload_len` + u8 kind + u32 LE
/// crc32c over (kind byte + payload).
const RECORD_OVERHEAD: usize = 4 + 1 + 4;

// §6.4 record kinds. Session kinds sit in 1..=15, boundary kinds in
// 16..=31, so a misfiled record is recognizably foreign. Readers skip
// unknown kinds (forward compat, §6.9: additive change = new record
// variant, skipped by old readers).
const KIND_SESSION_BEGIN: u8 = 1;
const KIND_SESSION_HEARTBEAT: u8 = 2;
const KIND_SESSION_EPOCH_CLOSE: u8 = 3;
const KIND_SESSION_END: u8 = 4;
const KIND_BOUNDARY_BEGIN: u8 = 16;
const KIND_BOUNDARY_BOUND: u8 = 17;
const KIND_BOUNDARY_COMPLETE: u8 = 18;
const KIND_BOUNDARY_TRIGGER: u8 = 19;
const KIND_BOUNDARY_LOSS: u8 = 20;

/// One §6.4 meta record. `Session*` variants belong to `session.bamlmeta`
/// (one writer: the consumer thread); `Boundary*` variants belong to
/// `boundary.bamlmeta` (`begin` from the host, `bound`/`complete` from the
/// consumer — the host↔consumer handshake).
#[derive(Debug, Clone, PartialEq)]
pub enum MetaRecord {
    /// Session milestone (D2): identifies the writing process and engine.
    SessionBegin {
        process_euid: [u8; 16],
        engine_id: u64,
        pid: u32,
        started_epoch_ns: u64,
        revision_id: String,
    },
    /// Liveness beacon (D0, coarse cadence) — the crash detector, together
    /// with pid liveness.
    SessionHeartbeat { wall_epoch_ns: u64 },
    /// A CCT epoch closed (segment budget reached, engine reset, ...).
    SessionEpochClose { reason: String, cct_bytes: u64 },
    /// Session milestone (D2): clean shutdown, with why.
    SessionEnd { reason: String },
    /// Boundary milestone (D2), written by the **host** at run start.
    /// `boundary_id` is the `baml_id_1_` wire form; `project_id` rides
    /// along so exported boundary dirs carry project identity with zero
    /// project state (§6.4).
    BoundaryBegin {
        boundary_id: String,
        target: String,
        /// `cli` | `playground` | `sdk` | `test`.
        source: String,
        created_ms: u64,
        project_id: String,
        revision_id: String,
        capture_defaults: String,
    },
    /// Written by the **consumer** once the boundary is bound to a session
    /// partition — the host does not know segment sequences (§6.4).
    BoundaryBound {
        session_dir: String,
        first_seg_seq: u32,
        partition_id: u32,
        boundary_local_id: u32,
    },
    /// Boundary milestone (D2), consumer: `begin` without this record plus
    /// a dead session heartbeat ⇒ crashed (partial, readable).
    BoundaryComplete {
        status: String,
        completed_ms: u64,
        last_seg_seq: u32,
        counts: serde_json::Value,
        diagnostics: Vec<String>,
        dump_refs: Vec<String>,
    },
    /// Optional: a capture trigger fired (flight-recorder dump, ...).
    BoundaryTrigger {
        trigger: String,
        at_ms: u64,
        detail: String,
    },
    /// Optional: declared data loss (shed, budget, ...), so readers can
    /// surface it instead of silent absence.
    BoundaryLoss { kind: String, detail: String },
}

impl MetaRecord {
    /// The framing kind byte for this record.
    #[must_use]
    pub fn kind(&self) -> u8 {
        match self {
            MetaRecord::SessionBegin { .. } => KIND_SESSION_BEGIN,
            MetaRecord::SessionHeartbeat { .. } => KIND_SESSION_HEARTBEAT,
            MetaRecord::SessionEpochClose { .. } => KIND_SESSION_EPOCH_CLOSE,
            MetaRecord::SessionEnd { .. } => KIND_SESSION_END,
            MetaRecord::BoundaryBegin { .. } => KIND_BOUNDARY_BEGIN,
            MetaRecord::BoundaryBound { .. } => KIND_BOUNDARY_BOUND,
            MetaRecord::BoundaryComplete { .. } => KIND_BOUNDARY_COMPLETE,
            MetaRecord::BoundaryTrigger { .. } => KIND_BOUNDARY_TRIGGER,
            MetaRecord::BoundaryLoss { .. } => KIND_BOUNDARY_LOSS,
        }
    }

    /// The JSON payload (framing never looks inside it — see module docs
    /// on why JSON and how it can be replaced).
    fn payload_json(&self) -> serde_json::Value {
        match self {
            MetaRecord::SessionBegin {
                process_euid,
                engine_id,
                pid,
                started_epoch_ns,
                revision_id,
            } => serde_json::json!({
                // Hex, not a JSON byte array: readable in `jq`/logs and
                // matches how euids appear in dir names.
                "process_euid": euid_to_hex(process_euid),
                "engine_id": engine_id,
                "pid": pid,
                "started_epoch_ns": started_epoch_ns,
                "revision_id": revision_id,
            }),
            MetaRecord::SessionHeartbeat { wall_epoch_ns } => serde_json::json!({
                "wall_epoch_ns": wall_epoch_ns,
            }),
            MetaRecord::SessionEpochClose { reason, cct_bytes } => serde_json::json!({
                "reason": reason,
                "cct_bytes": cct_bytes,
            }),
            MetaRecord::SessionEnd { reason } => serde_json::json!({
                "reason": reason,
            }),
            MetaRecord::BoundaryBegin {
                boundary_id,
                target,
                source,
                created_ms,
                project_id,
                revision_id,
                capture_defaults,
            } => serde_json::json!({
                "boundary_id": boundary_id,
                "target": target,
                "source": source,
                "created_ms": created_ms,
                "project_id": project_id,
                "revision_id": revision_id,
                "capture_defaults": capture_defaults,
            }),
            MetaRecord::BoundaryBound {
                session_dir,
                first_seg_seq,
                partition_id,
                boundary_local_id,
            } => serde_json::json!({
                "session_dir": session_dir,
                "first_seg_seq": first_seg_seq,
                "partition_id": partition_id,
                "boundary_local_id": boundary_local_id,
            }),
            MetaRecord::BoundaryComplete {
                status,
                completed_ms,
                last_seg_seq,
                counts,
                diagnostics,
                dump_refs,
            } => serde_json::json!({
                "status": status,
                "completed_ms": completed_ms,
                "last_seg_seq": last_seg_seq,
                "counts": counts,
                "diagnostics": diagnostics,
                "dump_refs": dump_refs,
            }),
            MetaRecord::BoundaryTrigger {
                trigger,
                at_ms,
                detail,
            } => serde_json::json!({
                "trigger": trigger,
                "at_ms": at_ms,
                "detail": detail,
            }),
            MetaRecord::BoundaryLoss { kind, detail } => serde_json::json!({
                "kind": kind,
                "detail": detail,
            }),
        }
    }
}

/// Encode the 8-byte stream header (written once, at file creation).
#[must_use]
pub fn encode_header() -> [u8; 8] {
    META_MAGIC
}

/// Encode one framed record, ready to append: u32 LE `payload_len` + kind
/// byte + payload + u32 LE crc32c over (kind byte + payload). Appending
/// this as a single write is what keeps records whole under concurrent
/// `O_APPEND` writers and torn only at the tail under a crash.
#[must_use]
pub fn encode_record(record: &MetaRecord) -> Vec<u8> {
    let payload = record.payload_json().to_string().into_bytes();
    let kind = record.kind();
    let mut out = Vec::with_capacity(RECORD_OVERHEAD + payload.len());
    // Meta payloads are ~200 B; a >4 GiB payload is unrepresentable.
    out.extend_from_slice(&u32::try_from(payload.len()).unwrap().to_le_bytes());
    out.push(kind);
    out.extend_from_slice(&payload);
    let crc = crc32c::extend(crc32c::extend(0, &[kind]), &payload);
    out.extend_from_slice(&crc.to_le_bytes());
    out
}

/// A read stream: every committed record, in order.
#[derive(Debug, Clone, PartialEq)]
pub struct MetaContents {
    pub records: Vec<MetaRecord>,
    /// True iff the byte stream ends in a torn/corrupt record. Not an
    /// error: the committed prefix above is still served (§6.4 — a torn
    /// tail costs at most the record being written).
    pub truncated: bool,
    /// Records with a valid CRC but an unknown kind (or a payload this
    /// reader cannot interpret): skipped, not fatal (§6.9 forward compat).
    pub unknown_records: u32,
}

/// Why a stream could not be opened at all. Torn *tails* are never an
/// error (see [`MetaContents::truncated`]); a bad *header* is — there is
/// no committed prefix to serve.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MetaError {
    TruncatedHeader,
    BadMagic,
    UnsupportedVersion(u16),
}

/// Read a meta stream: header, then committed records until the end of
/// the bytes or the first record whose length runs past the buffer or
/// whose CRC fails. Never mutates, never errors on a torn tail.
pub fn read_meta(bytes: &[u8]) -> Result<MetaContents, MetaError> {
    if bytes.len() < HEADER_LEN {
        return Err(MetaError::TruncatedHeader);
    }
    if bytes[0..5] != META_MAGIC[0..5] {
        return Err(MetaError::BadMagic);
    }
    let version = u16::from_le_bytes([bytes[5], bytes[6]]);
    if version != FORMAT_VERSION {
        return Err(MetaError::UnsupportedVersion(version));
    }
    // Byte 7 is reserved: ignored on read so it can be claimed additively.
    let mut records = Vec::new();
    let mut unknown_records = 0u32;
    let mut offset = HEADER_LEN;
    let truncated = loop {
        if offset == bytes.len() {
            break false; // clean end on a record boundary
        }
        if bytes.len() - offset < RECORD_OVERHEAD {
            break true; // torn inside the fixed framing
        }
        let payload_len =
            u32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap()) as usize;
        // checked_add: a torn length field can hold garbage near usize::MAX.
        let end = match offset
            .checked_add(RECORD_OVERHEAD)
            .and_then(|end| end.checked_add(payload_len))
        {
            Some(end) if end <= bytes.len() => end,
            _ => break true, // length runs past the buffer
        };
        let kind = bytes[offset + 4];
        let payload = &bytes[offset + 5..end - 4];
        let stored_crc = u32::from_le_bytes(bytes[end - 4..end].try_into().unwrap());
        if crc32c::extend(crc32c::extend(0, &[kind]), payload) != stored_crc {
            break true; // torn or corrupt: stop, serve the prefix
        }
        match decode_payload(kind, payload) {
            Some(record) => records.push(record),
            None => unknown_records += 1,
        }
        offset = end;
    };
    Ok(MetaContents {
        records,
        truncated,
        unknown_records,
    })
}

/// Decode one committed payload. `None` ⇒ skipped and counted in
/// [`MetaContents::unknown_records`]: after the CRC passed, an unknown
/// kind or an uninterpretable payload is a newer writer's data, not
/// corruption.
fn decode_payload(kind: u8, payload: &[u8]) -> Option<MetaRecord> {
    let value: serde_json::Value = serde_json::from_slice(payload).ok()?;
    Some(match kind {
        KIND_SESSION_BEGIN => MetaRecord::SessionBegin {
            process_euid: euid_from_hex(value.get("process_euid")?.as_str()?)?,
            engine_id: get_u64(&value, "engine_id")?,
            pid: get_u32(&value, "pid")?,
            started_epoch_ns: get_u64(&value, "started_epoch_ns")?,
            revision_id: get_string(&value, "revision_id")?,
        },
        KIND_SESSION_HEARTBEAT => MetaRecord::SessionHeartbeat {
            wall_epoch_ns: get_u64(&value, "wall_epoch_ns")?,
        },
        KIND_SESSION_EPOCH_CLOSE => MetaRecord::SessionEpochClose {
            reason: get_string(&value, "reason")?,
            cct_bytes: get_u64(&value, "cct_bytes")?,
        },
        KIND_SESSION_END => MetaRecord::SessionEnd {
            reason: get_string(&value, "reason")?,
        },
        KIND_BOUNDARY_BEGIN => MetaRecord::BoundaryBegin {
            boundary_id: get_string(&value, "boundary_id")?,
            target: get_string(&value, "target")?,
            source: get_string(&value, "source")?,
            created_ms: get_u64(&value, "created_ms")?,
            project_id: get_string(&value, "project_id")?,
            revision_id: get_string(&value, "revision_id")?,
            capture_defaults: get_string(&value, "capture_defaults")?,
        },
        KIND_BOUNDARY_BOUND => MetaRecord::BoundaryBound {
            session_dir: get_string(&value, "session_dir")?,
            first_seg_seq: get_u32(&value, "first_seg_seq")?,
            partition_id: get_u32(&value, "partition_id")?,
            boundary_local_id: get_u32(&value, "boundary_local_id")?,
        },
        KIND_BOUNDARY_COMPLETE => MetaRecord::BoundaryComplete {
            status: get_string(&value, "status")?,
            completed_ms: get_u64(&value, "completed_ms")?,
            last_seg_seq: get_u32(&value, "last_seg_seq")?,
            counts: value.get("counts")?.clone(),
            diagnostics: get_strings(&value, "diagnostics")?,
            dump_refs: get_strings(&value, "dump_refs")?,
        },
        KIND_BOUNDARY_TRIGGER => MetaRecord::BoundaryTrigger {
            trigger: get_string(&value, "trigger")?,
            at_ms: get_u64(&value, "at_ms")?,
            detail: get_string(&value, "detail")?,
        },
        KIND_BOUNDARY_LOSS => MetaRecord::BoundaryLoss {
            kind: get_string(&value, "kind")?,
            detail: get_string(&value, "detail")?,
        },
        _ => return None,
    })
}

fn get_u64(value: &serde_json::Value, key: &str) -> Option<u64> {
    value.get(key)?.as_u64()
}

fn get_u32(value: &serde_json::Value, key: &str) -> Option<u32> {
    u32::try_from(value.get(key)?.as_u64()?).ok()
}

fn get_string(value: &serde_json::Value, key: &str) -> Option<String> {
    Some(value.get(key)?.as_str()?.to_owned())
}

fn get_strings(value: &serde_json::Value, key: &str) -> Option<Vec<String>> {
    value
        .get(key)?
        .as_array()?
        .iter()
        .map(|item| item.as_str().map(str::to_owned))
        .collect()
}

fn euid_to_hex(euid: &[u8; 16]) -> String {
    use std::fmt::Write as _;
    euid.iter()
        .fold(String::with_capacity(32), |mut out, byte| {
            // write! into a String is infallible.
            let _ = write!(out, "{byte:02x}");
            out
        })
}

fn euid_from_hex(hex: &str) -> Option<[u8; 16]> {
    let digits = hex.as_bytes();
    if digits.len() != 32 {
        return None;
    }
    let mut out = [0u8; 16];
    for (slot, pair) in out.iter_mut().zip(digits.chunks_exact(2)) {
        *slot = u8::from_str_radix(std::str::from_utf8(pair).ok()?, 16).ok()?;
    }
    Some(out)
}

/// Thin native append helper for one `.bamlmeta` stream (wasm callers hand
/// [`encode_record`] bytes to their host sink instead).
///
/// Opened create+append (`O_APPEND`), and [`MetaWriter::append`] issues a
/// single `write_all` per record: the kernel serializes `O_APPEND` writes,
/// so the §6.4 concurrent appenders (host writes `begin`, consumer writes
/// `bound`/`complete`) never interleave mid-record, and a crash tears at
/// most the tail record — exactly what [`read_meta`] tolerates.
///
/// Durability is the CALLER's policy (§6.6: milestones D2, heartbeats D0);
/// this type only exposes [`MetaWriter::sync_data`].
#[cfg(not(target_arch = "wasm32"))]
pub struct MetaWriter {
    file: std::fs::File,
}

#[cfg(not(target_arch = "wasm32"))]
impl MetaWriter {
    /// Open (creating if absent) for appending. The header is written only
    /// when the file is empty — an existing stream is never rewritten
    /// (append-only ruling, §6.4). The empty-file check-then-write is safe
    /// because stream *creation* has a single owner (the host creates
    /// `boundary.bamlmeta` with `begin`; the consumer opens it only after
    /// the `BindBoundary` handshake).
    pub fn create(path: impl AsRef<std::path::Path>) -> std::io::Result<MetaWriter> {
        use std::io::Write as _;
        let file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)?;
        let mut writer = MetaWriter { file };
        if writer.file.metadata()?.len() == 0 {
            writer.file.write_all(&encode_header())?;
        }
        Ok(writer)
    }

    /// Append one record as a single `write_all` (keeps records whole —
    /// see the type docs).
    pub fn append(&mut self, record: &MetaRecord) -> std::io::Result<()> {
        use std::io::Write as _;
        self.file.write_all(&encode_record(record))
    }

    /// `fdatasync` the stream — callers apply the §6.6 ladder (D2 for
    /// milestones means this plus a parent-dir fsync on first create).
    pub fn sync_data(&mut self) -> std::io::Result<()> {
        self.file.sync_data()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_records() -> Vec<MetaRecord> {
        vec![
            MetaRecord::SessionBegin {
                process_euid: *b"\x00\x11\x22\x33\x44\x55\x66\x77\x88\x99\xaa\xbb\xcc\xdd\xee\xff",
                engine_id: 0xD0A1_0001,
                pid: 4242,
                started_epoch_ns: 1_700_000_000_000_000_000,
                revision_id: "rev-abc123".to_string(),
            },
            MetaRecord::SessionHeartbeat {
                wall_epoch_ns: 1_700_000_010_000_000_000,
            },
            MetaRecord::SessionEpochClose {
                reason: "segment-budget".to_string(),
                cct_bytes: 1_048_576,
            },
            MetaRecord::SessionEnd {
                reason: "clean".to_string(),
            },
            MetaRecord::BoundaryBegin {
                boundary_id: "baml_id_1_9f8e7d6c5b4a".to_string(),
                target: "pkg.Main".to_string(),
                source: "cli".to_string(),
                created_ms: 1_700_000_000_123,
                project_id: "proj-1".to_string(),
                revision_id: "rev-abc123".to_string(),
                capture_defaults: "on".to_string(),
            },
            MetaRecord::BoundaryBound {
                session_dir: "sessions/2026-07-31-abcd".to_string(),
                first_seg_seq: 3,
                partition_id: 7,
                boundary_local_id: 1,
            },
            MetaRecord::BoundaryComplete {
                status: "ok".to_string(),
                completed_ms: 1_700_000_009_999,
                last_seg_seq: 5,
                counts: serde_json::json!({"calls": 12, "llm_calls": 3}),
                diagnostics: vec!["shed:none".to_string()],
                dump_refs: vec!["dumps/0001".to_string()],
            },
            MetaRecord::BoundaryTrigger {
                trigger: "provider-error".to_string(),
                at_ms: 1_700_000_005_000,
                detail: "provider 500".to_string(),
            },
            MetaRecord::BoundaryLoss {
                kind: "ring-shed".to_string(),
                detail: "42 events".to_string(),
            },
        ]
    }

    fn stream_of(records: &[MetaRecord]) -> Vec<u8> {
        let mut bytes = encode_header().to_vec();
        for record in records {
            bytes.extend_from_slice(&encode_record(record));
        }
        bytes
    }

    #[test]
    fn header_and_every_kind_roundtrip() {
        let records = sample_records();
        let contents = read_meta(&stream_of(&records)).unwrap();
        assert_eq!(contents.records, records);
        assert!(!contents.truncated);
        assert_eq!(contents.unknown_records, 0);
        // Header alone is a valid, empty, non-truncated stream.
        let empty = read_meta(&encode_header()).unwrap();
        assert!(empty.records.is_empty());
        assert!(!empty.truncated);
    }

    #[test]
    fn header_failures_are_errors() {
        assert_eq!(read_meta(b"").unwrap_err(), MetaError::TruncatedHeader);
        assert_eq!(
            read_meta(&META_MAGIC[..7]).unwrap_err(),
            MetaError::TruncatedHeader
        );
        assert_eq!(
            read_meta(b"XMET\0\x01\0\0").unwrap_err(),
            MetaError::BadMagic
        );
        assert_eq!(
            read_meta(b"BMET\0\x02\0\0").unwrap_err(),
            MetaError::UnsupportedVersion(2)
        );
    }

    #[test]
    fn torn_tail_at_every_offset_keeps_committed_prefix() {
        let records = vec![
            sample_records()[0].clone(),
            sample_records()[1].clone(),
            sample_records()[3].clone(),
        ];
        let bytes = stream_of(&records);
        // Committed boundaries: end of header, then end of each record.
        let mut boundaries = vec![HEADER_LEN];
        for record in &records {
            boundaries.push(boundaries.last().unwrap() + encode_record(record).len());
        }
        assert_eq!(*boundaries.last().unwrap(), bytes.len());
        for cut in 0..=bytes.len() {
            if cut < HEADER_LEN {
                assert_eq!(
                    read_meta(&bytes[..cut]).unwrap_err(),
                    MetaError::TruncatedHeader,
                    "cut {cut}"
                );
                continue;
            }
            let contents = read_meta(&bytes[..cut]).unwrap();
            let committed = boundaries.iter().filter(|end| **end <= cut).count() - 1;
            assert_eq!(contents.records, records[..committed], "cut {cut}");
            // Truncated iff the cut is not on a record boundary.
            assert_eq!(contents.truncated, !boundaries.contains(&cut), "cut {cut}");
            assert_eq!(contents.unknown_records, 0);
        }
    }

    #[test]
    fn unknown_kind_is_skipped_and_counted() {
        let records = sample_records();
        let known = [records[1].clone(), records[3].clone()];
        let mut bytes = encode_header().to_vec();
        bytes.extend_from_slice(&encode_record(&known[0]));
        // Hand-craft a kind-99 record with a valid CRC: a newer writer's
        // additive variant, which this reader must skip, not reject.
        let payload = br#"{"future":"field"}"#;
        bytes.extend_from_slice(&u32::try_from(payload.len()).unwrap().to_le_bytes());
        bytes.push(99);
        bytes.extend_from_slice(payload);
        let crc = crc32c::extend(crc32c::extend(0, &[99]), payload);
        bytes.extend_from_slice(&crc.to_le_bytes());
        bytes.extend_from_slice(&encode_record(&known[1]));

        let contents = read_meta(&bytes).unwrap();
        assert_eq!(contents.records, known);
        assert_eq!(contents.unknown_records, 1);
        assert!(!contents.truncated, "records after the skip are still read");
    }

    #[test]
    fn crc_corrupt_record_ends_read_with_prefix() {
        let records = vec![
            sample_records()[0].clone(),
            sample_records()[1].clone(),
            sample_records()[3].clone(),
        ];
        let mut bytes = stream_of(&records);
        // Flip one payload byte of the second record.
        let second_payload = HEADER_LEN + encode_record(&records[0]).len() + 5;
        bytes[second_payload] ^= 0xFF;
        let contents = read_meta(&bytes).unwrap();
        assert_eq!(contents.records, records[..1]);
        assert!(contents.truncated);
        assert_eq!(contents.unknown_records, 0);
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn writer_appends_survive_reopen() {
        let dir = std::env::temp_dir().join(format!("baml-meta-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("session.bamlmeta");
        let first = sample_records()[0].clone();
        let second = sample_records()[3].clone();
        {
            let mut writer = MetaWriter::create(&path).unwrap();
            writer.append(&first).unwrap();
            writer.sync_data().unwrap();
        }
        {
            // Reopen: the existing header must NOT be rewritten.
            let mut writer = MetaWriter::create(&path).unwrap();
            writer.append(&second).unwrap();
            writer.sync_data().unwrap();
        }
        let bytes = std::fs::read(&path).unwrap();
        assert_eq!(bytes[..HEADER_LEN], META_MAGIC, "exactly one header");
        let contents = read_meta(&bytes).unwrap();
        assert_eq!(contents.records, vec![first, second]);
        assert!(!contents.truncated);
        std::fs::remove_dir_all(&dir).ok();
    }
}
