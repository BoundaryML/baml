use std::{
    fs::{File, OpenOptions},
    io::{self, Read, Write},
    path::Path,
};

use borsh::{BorshDeserialize, BorshSerialize};

use super::format::{crc32c_slices, get_u32, invalid_data};
use super::layout::{create_dir_all_anchored, sync_parent_directory};

const META_MAGIC: &[u8; 4] = b"BMET";
const META_PREFIX_LEN: usize = 8;
const META_CRC_LEN: usize = 4;
pub const META_MAX_RECORD_LEN: usize = 16 * 1024 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum SessionMetaKind {
    Begin = 1,
    Heartbeat = 2,
    EpochClose = 3,
    End = 4,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum BoundaryMetaKind {
    Begin = 1,
    Bound = 2,
    Complete = 3,
    Trigger = 4,
    Loss = 5,
}

/// Stable v1 payload for `session.bamlmeta` begin records.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct SessionBeginMeta {
    pub process_euid: [u8; 16],
    pub engine_id: u64,
    pub pid: u32,
    pub started_epoch_ns: u64,
    pub revision_id: [u8; 32],
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct SessionHeartbeatMeta {
    pub pid: u32,
    pub wall_epoch_ns: u64,
    pub durable_block_seq: u32,
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct SessionEpochCloseMeta {
    pub closed_epoch_ns: u64,
    pub last_seg_seq: u32,
    pub reason: String,
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct SessionEndMeta {
    pub ended_epoch_ns: u64,
    pub last_seg_seq: u32,
    pub reason: String,
}

/// Host-authored boundary begin payload. `capture_defaults` is a stable bitset
/// whose individual policy bits are interpreted by the capture-plane version.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct BoundaryBeginMeta {
    pub boundary_id: [u8; 16],
    pub target: String,
    pub source: String,
    pub created_ms: u64,
    pub project_id: String,
    pub revision_id: [u8; 32],
    pub capture_defaults: u32,
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct BoundaryBoundMeta {
    pub session_dir: String,
    pub first_seg_seq: u32,
    pub partition_id: u32,
    pub boundary_local_id: u32,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct BoundaryCounts {
    pub events: u64,
    pub nodes: u64,
    pub calls: u64,
    pub errors: u64,
    pub captures: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct BoundaryCompleteMeta {
    pub status: String,
    pub completed_ms: u64,
    pub last_seg_seq: u32,
    pub counts: BoundaryCounts,
    pub diagnostics: Vec<String>,
    pub dump_refs: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct BoundaryTriggerMeta {
    pub trigger: String,
    pub timestamp_ns: u64,
    pub dump_ref: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct BoundaryLossMeta {
    pub timestamp_ns: u64,
    pub kind: String,
    pub count: u64,
    pub detail: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TypedSessionMeta {
    Begin(SessionBeginMeta),
    Heartbeat(SessionHeartbeatMeta),
    EpochClose(SessionEpochCloseMeta),
    End(SessionEndMeta),
    Unknown(MetaRecord),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TypedBoundaryMeta {
    Begin(BoundaryBeginMeta),
    Bound(BoundaryBoundMeta),
    Complete(BoundaryCompleteMeta),
    Trigger(BoundaryTriggerMeta),
    Loss(BoundaryLossMeta),
    Unknown(MetaRecord),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MetaRecord {
    pub kind: u8,
    pub payload: Vec<u8>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct MetaScan {
    pub records: Vec<MetaRecord>,
    pub committed_len: u64,
    pub torn_tail: bool,
}

pub fn encode_typed_session_meta(value: &TypedSessionMeta) -> io::Result<(u8, Vec<u8>)> {
    match value {
        TypedSessionMeta::Begin(value) => encode(SessionMetaKind::Begin as u8, value),
        TypedSessionMeta::Heartbeat(value) => encode(SessionMetaKind::Heartbeat as u8, value),
        TypedSessionMeta::EpochClose(value) => encode(SessionMetaKind::EpochClose as u8, value),
        TypedSessionMeta::End(value) => encode(SessionMetaKind::End as u8, value),
        TypedSessionMeta::Unknown(value) => Ok((value.kind, value.payload.clone())),
    }
}

pub fn decode_typed_session_meta(record: &MetaRecord) -> io::Result<TypedSessionMeta> {
    Ok(match record.kind {
        kind if kind == SessionMetaKind::Begin as u8 => {
            TypedSessionMeta::Begin(decode(&record.payload)?)
        }
        kind if kind == SessionMetaKind::Heartbeat as u8 => {
            TypedSessionMeta::Heartbeat(decode(&record.payload)?)
        }
        kind if kind == SessionMetaKind::EpochClose as u8 => {
            TypedSessionMeta::EpochClose(decode(&record.payload)?)
        }
        kind if kind == SessionMetaKind::End as u8 => {
            TypedSessionMeta::End(decode(&record.payload)?)
        }
        _ => TypedSessionMeta::Unknown(record.clone()),
    })
}

pub fn encode_typed_boundary_meta(value: &TypedBoundaryMeta) -> io::Result<(u8, Vec<u8>)> {
    match value {
        TypedBoundaryMeta::Begin(value) => encode(BoundaryMetaKind::Begin as u8, value),
        TypedBoundaryMeta::Bound(value) => encode(BoundaryMetaKind::Bound as u8, value),
        TypedBoundaryMeta::Complete(value) => encode(BoundaryMetaKind::Complete as u8, value),
        TypedBoundaryMeta::Trigger(value) => encode(BoundaryMetaKind::Trigger as u8, value),
        TypedBoundaryMeta::Loss(value) => encode(BoundaryMetaKind::Loss as u8, value),
        TypedBoundaryMeta::Unknown(value) => Ok((value.kind, value.payload.clone())),
    }
}

pub fn decode_typed_boundary_meta(record: &MetaRecord) -> io::Result<TypedBoundaryMeta> {
    Ok(match record.kind {
        kind if kind == BoundaryMetaKind::Begin as u8 => {
            TypedBoundaryMeta::Begin(decode(&record.payload)?)
        }
        kind if kind == BoundaryMetaKind::Bound as u8 => {
            TypedBoundaryMeta::Bound(decode(&record.payload)?)
        }
        kind if kind == BoundaryMetaKind::Complete as u8 => {
            TypedBoundaryMeta::Complete(decode(&record.payload)?)
        }
        kind if kind == BoundaryMetaKind::Trigger as u8 => {
            TypedBoundaryMeta::Trigger(decode(&record.payload)?)
        }
        kind if kind == BoundaryMetaKind::Loss as u8 => {
            TypedBoundaryMeta::Loss(decode(&record.payload)?)
        }
        _ => TypedBoundaryMeta::Unknown(record.clone()),
    })
}

fn encode<T: BorshSerialize>(kind: u8, value: &T) -> io::Result<(u8, Vec<u8>)> {
    borsh::to_vec(value)
        .map(|payload| (kind, payload))
        .map_err(io::Error::other)
}

fn decode<T: BorshDeserialize>(payload: &[u8]) -> io::Result<T> {
    borsh::from_slice(payload).map_err(|error| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("invalid typed BMET payload: {error}"),
        )
    })
}

pub struct MetaWriter<W> {
    sink: W,
    offset: u64,
}

impl<W: Write> MetaWriter<W> {
    #[must_use]
    pub fn new(sink: W) -> Self {
        Self { sink, offset: 0 }
    }

    #[must_use]
    pub fn bytes_written(&self) -> u64 {
        self.offset
    }

    pub fn append(&mut self, kind: u8, payload: &[u8]) -> io::Result<u64> {
        let record_len = payload
            .len()
            .checked_add(1)
            .ok_or_else(|| invalid_data("BMET record length overflow"))?;
        if record_len > META_MAX_RECORD_LEN {
            return Err(invalid_data("BMET record exceeds size limit"));
        }
        let record_len_u32 = u32::try_from(record_len)
            .map_err(|_| invalid_data("BMET record length exceeds u32"))?;
        let mut prefix = [0_u8; META_PREFIX_LEN];
        prefix[..4].copy_from_slice(META_MAGIC);
        prefix[4..8].copy_from_slice(&record_len_u32.to_le_bytes());
        let kind_bytes = [kind];
        let crc = crc32c_slices(&[&prefix, &kind_bytes, payload]);

        let offset = self.offset;
        self.sink.write_all(&prefix)?;
        self.sink.write_all(&[kind])?;
        self.sink.write_all(payload)?;
        self.sink.write_all(&crc.to_le_bytes())?;
        self.offset = self.offset.saturating_add(
            u64::try_from(META_PREFIX_LEN + record_len + META_CRC_LEN)
                .expect("BMET maximum fits u64"),
        );
        Ok(offset)
    }

    pub fn flush(&mut self) -> io::Result<()> {
        self.sink.flush()
    }

    pub fn into_inner(self) -> W {
        self.sink
    }
}

impl MetaWriter<File> {
    pub fn open_append(path: &Path) -> io::Result<Self> {
        let existing = match std::fs::read(path) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == io::ErrorKind::NotFound => Vec::new(),
            Err(error) => return Err(error),
        };
        let scan = scan_meta_bytes(&existing);
        if scan.torn_tail {
            return Err(invalid_data("refusing to append after a torn BMET record"));
        }
        let sink = OpenOptions::new().create(true).append(true).open(path)?;
        Ok(Self {
            sink,
            offset: scan.committed_len,
        })
    }

    pub fn sync_data(&mut self) -> io::Result<()> {
        self.flush()?;
        self.sink.sync_data()
    }

    pub fn sync_all(&mut self) -> io::Result<()> {
        self.flush()?;
        self.sink.sync_all()
    }
}

#[must_use]
pub fn scan_meta_bytes(bytes: &[u8]) -> MetaScan {
    let mut scan = MetaScan::default();
    let mut offset = 0_usize;
    while offset < bytes.len() {
        let Some(prefix_end) = offset.checked_add(META_PREFIX_LEN) else {
            scan.torn_tail = true;
            break;
        };
        let Some(prefix) = bytes.get(offset..prefix_end) else {
            scan.torn_tail = true;
            break;
        };
        if &prefix[..4] != META_MAGIC {
            scan.torn_tail = true;
            break;
        }
        let record_len = get_u32(prefix, 4) as usize;
        if record_len == 0 || record_len > META_MAX_RECORD_LEN {
            scan.torn_tail = true;
            break;
        }
        let Some(record_end) = prefix_end.checked_add(record_len) else {
            scan.torn_tail = true;
            break;
        };
        let Some(frame_end) = record_end.checked_add(META_CRC_LEN) else {
            scan.torn_tail = true;
            break;
        };
        let Some(record) = bytes.get(prefix_end..record_end) else {
            scan.torn_tail = true;
            break;
        };
        let Some(crc_bytes) = bytes.get(record_end..frame_end) else {
            scan.torn_tail = true;
            break;
        };
        if get_u32(crc_bytes, 0) != crc32c_slices(&[prefix, record]) {
            scan.torn_tail = true;
            break;
        }
        scan.records.push(MetaRecord {
            kind: record[0],
            payload: record[1..].to_vec(),
        });
        offset = frame_end;
    }
    scan.committed_len = offset as u64;
    scan
}

pub fn scan_meta_reader(mut reader: impl Read) -> io::Result<MetaScan> {
    let mut bytes = Vec::new();
    reader.read_to_end(&mut bytes)?;
    Ok(scan_meta_bytes(&bytes))
}

/// Appends a D0 meta record. The payload encoding is owned by the record kind;
/// BMET only provides append framing, CRC, and torn-tail recovery.
pub fn append_meta_d0(path: &Path, kind: u8, payload: &[u8]) -> io::Result<u64> {
    ensure_parent(path)?;
    let mut writer = MetaWriter::open_append(path)?;
    let offset = writer.append(kind, payload)?;
    writer.flush()?;
    Ok(offset)
}

/// Appends a D2 milestone record and anchors first-file visibility.
pub fn append_meta_d2(path: &Path, kind: u8, payload: &[u8]) -> io::Result<u64> {
    ensure_parent(path)?;
    let mut writer = MetaWriter::open_append(path)?;
    let offset = writer.append(kind, payload)?;
    writer.sync_all()?;
    if let Some(parent) = path.parent() {
        sync_parent_directory(parent)?;
    }
    Ok(offset)
}

fn ensure_parent(path: &Path) -> io::Result<()> {
    let parent = path
        .parent()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "meta path has no parent"))?;
    create_dir_all_anchored(parent)
}
