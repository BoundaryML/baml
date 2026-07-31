use std::{
    collections::{BTreeMap, BTreeSet},
    fs::{self, File, OpenOptions},
    io::{self, Read, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
};

use super::{Cid, DagChunk, lock::WritersLockGuard};

pub const PACK_HEADER_LEN: usize = 48;
const PACK_MAGIC: &[u8; 4] = b"BPK1";
const PACK_RECORD_MAGIC: [u8; 2] = [0xb1, 0xc1];
const PACK_RECORD_HEADER_LEN: usize = 44;
const PACK_INDEX_MAGIC: &[u8; 4] = b"BPKI";
const PACK_INDEX_VERSION: u16 = 1;
const PACK_INDEX_HEADER_LEN_U16: u16 = 48;
const PACK_INDEX_HEADER_LEN: usize = 48;
const PACK_INDEX_FANOUT_LEN: usize = 256 * 4;
const PACK_INDEX_ENTRY_LEN: usize = 48;
const PACK_INDEX_CRC_LEN: usize = 4;
const PACK_CHUNK_KIND_VALUE_NODE: u8 = 1;
const PACK_STORAGE_RAW: u8 = 0;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PackPaths {
    pub pack: PathBuf,
    pub index: PathBuf,
    pub lease: PathBuf,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PackIndexEntry {
    pub cid: Cid,
    pub offset: u64,
    pub logical_len: u32,
    pub stored_len: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PackAppendOutcome {
    pub cid: Cid,
    pub appended: bool,
    /// Present only when this active pack owns the physical record.
    pub active_entry: Option<PackIndexEntry>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PackIndex {
    pub pack_header_digest: [u8; 32],
    pub fanout: [u32; 256],
    pub entries: Vec<PackIndexEntry>,
}

impl PackIndex {
    pub fn read(index_path: impl AsRef<Path>, pack_path: impl AsRef<Path>) -> io::Result<Self> {
        let bytes = fs::read(index_path)?;
        if bytes.len() < PACK_INDEX_HEADER_LEN + PACK_INDEX_FANOUT_LEN + PACK_INDEX_CRC_LEN {
            return Err(invalid_data("BPKI index is shorter than its fixed fields"));
        }
        if &bytes[..4] != PACK_INDEX_MAGIC {
            return Err(invalid_data("invalid BPKI magic"));
        }
        if get_u16(&bytes, 4) != PACK_INDEX_VERSION
            || usize::from(get_u16(&bytes, 6)) != PACK_INDEX_HEADER_LEN
        {
            return Err(invalid_data("unsupported BPKI header"));
        }
        let expected_crc_offset = bytes.len() - PACK_INDEX_CRC_LEN;
        if get_u32(&bytes, expected_crc_offset) != crc32c(&bytes[..expected_crc_offset]) {
            return Err(invalid_data("BPKI CRC mismatch"));
        }
        let entry_count = usize::try_from(get_u64(&bytes, 40))
            .map_err(|_| invalid_data("BPKI entry count does not fit usize"))?;
        let expected_len = PACK_INDEX_HEADER_LEN
            .checked_add(PACK_INDEX_FANOUT_LEN)
            .and_then(|length| length.checked_add(entry_count.checked_mul(PACK_INDEX_ENTRY_LEN)?))
            .and_then(|length| length.checked_add(PACK_INDEX_CRC_LEN))
            .ok_or_else(|| invalid_data("BPKI length overflow"))?;
        if bytes.len() != expected_len {
            return Err(invalid_data("BPKI length does not match entry count"));
        }

        let pack_header = read_pack_header(pack_path.as_ref())?;
        let actual_digest = *blake3::hash(&pack_header).as_bytes();
        let stored_digest: [u8; 32] = bytes[8..40]
            .try_into()
            .map_err(|_| invalid_data("short BPKI pack digest"))?;
        if stored_digest != actual_digest {
            return Err(invalid_data("BPKI is bound to a different pack"));
        }

        let mut fanout = [0_u32; 256];
        for (index, value) in fanout.iter_mut().enumerate() {
            *value = get_u32(&bytes, PACK_INDEX_HEADER_LEN + index * 4);
        }
        if fanout[255] != u32::try_from(entry_count).unwrap_or(u32::MAX) {
            return Err(invalid_data("BPKI fanout does not end at entry count"));
        }

        let entries_start = PACK_INDEX_HEADER_LEN + PACK_INDEX_FANOUT_LEN;
        let mut entries = Vec::with_capacity(entry_count);
        let mut previous = None;
        for bytes in bytes[entries_start..expected_crc_offset].chunks_exact(PACK_INDEX_ENTRY_LEN) {
            let cid = Cid::from_bytes(
                bytes[..32]
                    .try_into()
                    .map_err(|_| invalid_data("short BPKI CID"))?,
            );
            if previous.is_some_and(|previous| previous >= cid) {
                return Err(invalid_data("BPKI entries are not strictly CID-sorted"));
            }
            previous = Some(cid);
            entries.push(PackIndexEntry {
                cid,
                offset: get_u64(bytes, 32),
                logical_len: get_u32(bytes, 40),
                stored_len: get_u32(bytes, 44),
            });
        }
        validate_fanout(&fanout, &entries)?;
        Ok(Self {
            pack_header_digest: stored_digest,
            fanout,
            entries,
        })
    }

    #[must_use]
    pub fn find(&self, cid: Cid) -> Option<PackIndexEntry> {
        let first = usize::from(cid.as_bytes()[0]);
        let start = if first == 0 {
            0
        } else {
            self.fanout[first - 1] as usize
        };
        let end = self.fanout[first] as usize;
        self.entries[start..end]
            .binary_search_by_key(&cid, |entry| entry.cid)
            .ok()
            .map(|index| self.entries[start + index])
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PackScan {
    pub origin_euid: [u8; 16],
    pub pack_seq: u32,
    pub created_ms: u64,
    pub entries: Vec<PackIndexEntry>,
    pub committed_len: u64,
    pub torn_tail: bool,
}

#[derive(Debug)]
pub struct PackWriter {
    paths: PackPaths,
    file: File,
    entries: BTreeMap<Cid, PackIndexEntry>,
    sealed_store_cids: BTreeSet<Cid>,
    _writers_lock: Option<WritersLockGuard>,
    sealed: bool,
}

impl PackWriter {
    pub fn create(
        store_dir: impl AsRef<Path>,
        origin_euid: [u8; 16],
        pack_seq: u32,
        created_ms: u64,
    ) -> io::Result<Self> {
        let store_dir = store_dir.as_ref();
        let writers_lock = WritersLockGuard::acquire(store_dir)?;
        Self::create_inner(
            store_dir,
            origin_euid,
            pack_seq,
            created_ms,
            Some(writers_lock),
            None,
        )
    }

    /// Create a replacement pack while the caller holds the exclusive GC
    /// guard. The source being compacted is excluded from the dedupe snapshot
    /// so its sole copies are physically rewritten before it is unlinked.
    pub(super) fn create_for_gc(
        store_dir: impl AsRef<Path>,
        origin_euid: [u8; 16],
        pack_seq: u32,
        created_ms: u64,
        excluded_pack: &Path,
    ) -> io::Result<Self> {
        Self::create_inner(
            store_dir.as_ref(),
            origin_euid,
            pack_seq,
            created_ms,
            None,
            Some(excluded_pack),
        )
    }

    fn create_inner(
        store_dir: &Path,
        origin_euid: [u8; 16],
        pack_seq: u32,
        created_ms: u64,
        writers_lock: Option<WritersLockGuard>,
        excluded_pack: Option<&Path>,
    ) -> io::Result<Self> {
        let packs_dir = store_dir.join("packs");
        fs::create_dir_all(&packs_dir)?;
        // The shared writer lock prevents GC from invalidating this dedupe
        // snapshot for the writer's lifetime.
        let sealed_store_cids = read_sealed_store_cids(&packs_dir, excluded_pack)?;
        let stem = format!("pack-{}-{pack_seq:06}", hex_16(origin_euid));
        let paths = PackPaths {
            pack: packs_dir.join(format!("{stem}.bamlpack")),
            index: packs_dir.join(format!("{stem}.bamlpack.idx")),
            lease: packs_dir.join(format!("{stem}.lease")),
        };
        let mut file = OpenOptions::new()
            .read(true)
            .write(true)
            .create_new(true)
            .open(&paths.pack)?;
        file.write_all(&encode_pack_header(origin_euid, pack_seq, created_ms))?;
        file.sync_data()?;
        write_lease(&paths.lease, created_ms)?;
        Ok(Self {
            paths,
            file,
            entries: BTreeMap::new(),
            sealed_store_cids,
            _writers_lock: writers_lock,
            sealed: false,
        })
    }

    #[must_use]
    pub fn paths(&self) -> &PackPaths {
        &self.paths
    }

    #[must_use]
    pub fn entry(&self, cid: Cid) -> Option<PackIndexEntry> {
        self.entries.get(&cid).copied()
    }

    #[must_use]
    pub fn contains(&self, cid: Cid) -> bool {
        self.entries.contains_key(&cid) || self.sealed_store_cids.contains(&cid)
    }

    /// Append a raw canonical DAG node. Repeated CIDs in the active pack are
    /// deduplicated without another physical record.
    pub fn append_chunk(&mut self, chunk: &DagChunk) -> io::Result<PackAppendOutcome> {
        if self.sealed {
            return Err(io::Error::other("cannot append to a sealed BPK1 pack"));
        }
        if Cid::for_node(&chunk.canonical_bytes) != chunk.cid {
            return Err(invalid_data("DAG chunk CID does not match canonical bytes"));
        }
        if let Some(entry) = self.entries.get(&chunk.cid) {
            return Ok(PackAppendOutcome {
                cid: chunk.cid,
                appended: false,
                active_entry: Some(*entry),
            });
        }
        if self.sealed_store_cids.contains(&chunk.cid) {
            return Ok(PackAppendOutcome {
                cid: chunk.cid,
                appended: false,
                active_entry: None,
            });
        }
        let logical_len = u32::try_from(chunk.logical_len)
            .map_err(|_| invalid_data("DAG chunk logical length exceeds pack u32"))?;
        let stored_len = u32::try_from(chunk.canonical_bytes.len())
            .map_err(|_| invalid_data("DAG chunk stored length exceeds pack u32"))?;
        let offset = self.file.stream_position()?;
        let header = encode_record_header(chunk.cid, logical_len, stored_len);
        let crc = crc32c_slices(&[&header, &chunk.canonical_bytes]);
        self.file.write_all(&header)?;
        self.file.write_all(&chunk.canonical_bytes)?;
        self.file.write_all(&crc.to_le_bytes())?;
        let entry = PackIndexEntry {
            cid: chunk.cid,
            offset,
            logical_len,
            stored_len,
        };
        self.entries.insert(chunk.cid, entry);
        Ok(PackAppendOutcome {
            cid: chunk.cid,
            appended: true,
            active_entry: Some(entry),
        })
    }

    pub fn sync_data(&mut self) -> io::Result<()> {
        self.file.flush()?;
        self.file.sync_data()
    }

    pub fn heartbeat_lease(&self, now_ms: u64) -> io::Result<()> {
        write_lease(&self.paths.lease, now_ms)
    }

    /// D2-seal the rebuildable sorted index and release the writer lease.
    pub fn seal(mut self) -> io::Result<PackPaths> {
        self.sync_data()?;
        let entries = self.entries.values().copied().collect::<Vec<_>>();
        write_pack_index(&self.paths.index, &self.paths.pack, &entries)?;
        sync_parent(&self.paths.index)?;
        self.sealed = true;
        match fs::remove_file(&self.paths.lease) {
            Ok(()) => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(error),
        }
        sync_parent(&self.paths.lease)?;
        Ok(self.paths.clone())
    }
}

impl Drop for PackWriter {
    fn drop(&mut self) {
        if !self.sealed {
            let _ = self.file.flush();
        }
    }
}

pub fn scan_pack(path: impl AsRef<Path>) -> io::Result<PackScan> {
    let path = path.as_ref();
    let mut file = File::open(path)?;
    let mut header = [0_u8; PACK_HEADER_LEN];
    file.read_exact(&mut header)?;
    validate_pack_header(&header)?;
    let file_len = file.metadata()?.len();
    let mut offset = PACK_HEADER_LEN as u64;
    let mut entries = Vec::new();
    let mut torn_tail = false;

    while offset < file_len {
        let remaining = file_len - offset;
        if remaining < (PACK_RECORD_HEADER_LEN + 4) as u64 {
            torn_tail = true;
            break;
        }
        file.seek(SeekFrom::Start(offset))?;
        let mut record_header = [0_u8; PACK_RECORD_HEADER_LEN];
        file.read_exact(&mut record_header)?;
        if record_header[..2] != PACK_RECORD_MAGIC
            || record_header[2] != PACK_CHUNK_KIND_VALUE_NODE
            || record_header[3] != PACK_STORAGE_RAW
        {
            torn_tail = true;
            break;
        }
        let stored_len = get_u32(&record_header, 40);
        let record_len = (PACK_RECORD_HEADER_LEN as u64)
            .checked_add(u64::from(stored_len))
            .and_then(|length| length.checked_add(4))
            .ok_or_else(|| invalid_data("BPK1 record length overflow"))?;
        if remaining < record_len {
            torn_tail = true;
            break;
        }
        let mut payload = vec![0_u8; stored_len as usize];
        file.read_exact(&mut payload)?;
        let mut crc = [0_u8; 4];
        file.read_exact(&mut crc)?;
        if u32::from_le_bytes(crc) != crc32c_slices(&[&record_header, &payload]) {
            torn_tail = true;
            break;
        }
        let cid = Cid::from_bytes(
            record_header[4..36]
                .try_into()
                .map_err(|_| invalid_data("short BPK1 CID"))?,
        );
        if Cid::for_node(&payload) != cid {
            torn_tail = true;
            break;
        }
        entries.push(PackIndexEntry {
            cid,
            offset,
            logical_len: get_u32(&record_header, 36),
            stored_len,
        });
        offset += record_len;
    }
    Ok(PackScan {
        origin_euid: header[4..20]
            .try_into()
            .map_err(|_| invalid_data("short BPK1 origin euid"))?,
        pack_seq: get_u32(&header, 20),
        created_ms: get_u64(&header, 24),
        entries,
        committed_len: offset,
        torn_tail,
    })
}

pub fn rebuild_pack_index(
    pack_path: impl AsRef<Path>,
    index_path: impl AsRef<Path>,
) -> io::Result<PackIndex> {
    let pack_path = pack_path.as_ref();
    let index_path = index_path.as_ref();
    let scan = scan_pack(pack_path)?;
    if scan.torn_tail {
        return Err(invalid_data(
            "cannot index BPK1 pack with a torn or corrupt tail",
        ));
    }
    write_pack_index(index_path, pack_path, &scan.entries)?;
    PackIndex::read(index_path, pack_path)
}

pub fn read_pack_chunk(pack_path: impl AsRef<Path>, entry: PackIndexEntry) -> io::Result<Vec<u8>> {
    let mut file = File::open(pack_path)?;
    file.seek(SeekFrom::Start(entry.offset))?;
    let mut header = [0_u8; PACK_RECORD_HEADER_LEN];
    file.read_exact(&mut header)?;
    if header[..2] != PACK_RECORD_MAGIC
        || header[2] != PACK_CHUNK_KIND_VALUE_NODE
        || header[3] != PACK_STORAGE_RAW
    {
        return Err(invalid_data("invalid BPK1 record header"));
    }
    let cid = Cid::from_bytes(
        header[4..36]
            .try_into()
            .map_err(|_| invalid_data("short BPK1 CID"))?,
    );
    if cid != entry.cid
        || get_u32(&header, 36) != entry.logical_len
        || get_u32(&header, 40) != entry.stored_len
    {
        return Err(invalid_data("BPK1 record does not match BPKI entry"));
    }
    let mut payload = vec![0_u8; entry.stored_len as usize];
    file.read_exact(&mut payload)?;
    let mut crc = [0_u8; 4];
    file.read_exact(&mut crc)?;
    if u32::from_le_bytes(crc) != crc32c_slices(&[&header, &payload]) {
        return Err(invalid_data("BPK1 record CRC mismatch"));
    }
    if Cid::for_node(&payload) != cid {
        return Err(invalid_data("BPK1 record CID mismatch"));
    }
    Ok(payload)
}

fn encode_pack_header(origin_euid: [u8; 16], pack_seq: u32, created_ms: u64) -> [u8; 48] {
    let mut header = [0_u8; PACK_HEADER_LEN];
    header[..4].copy_from_slice(PACK_MAGIC);
    header[4..20].copy_from_slice(&origin_euid);
    header[20..24].copy_from_slice(&pack_seq.to_le_bytes());
    header[24..32].copy_from_slice(&created_ms.to_le_bytes());
    header
}

fn validate_pack_header(header: &[u8; PACK_HEADER_LEN]) -> io::Result<()> {
    if &header[..4] != PACK_MAGIC {
        return Err(invalid_data("invalid BPK1 magic"));
    }
    if header[32..].iter().any(|byte| *byte != 0) {
        return Err(invalid_data("nonzero reserved BPK1 header bytes"));
    }
    Ok(())
}

fn read_pack_header(path: &Path) -> io::Result<[u8; PACK_HEADER_LEN]> {
    let mut file = File::open(path)?;
    let mut header = [0_u8; PACK_HEADER_LEN];
    file.read_exact(&mut header)?;
    validate_pack_header(&header)?;
    Ok(header)
}

fn encode_record_header(cid: Cid, logical_len: u32, stored_len: u32) -> [u8; 44] {
    let mut header = [0_u8; PACK_RECORD_HEADER_LEN];
    header[..2].copy_from_slice(&PACK_RECORD_MAGIC);
    header[2] = PACK_CHUNK_KIND_VALUE_NODE;
    header[3] = PACK_STORAGE_RAW;
    header[4..36].copy_from_slice(cid.as_bytes());
    header[36..40].copy_from_slice(&logical_len.to_le_bytes());
    header[40..44].copy_from_slice(&stored_len.to_le_bytes());
    header
}

fn write_pack_index(
    index_path: &Path,
    pack_path: &Path,
    entries: &[PackIndexEntry],
) -> io::Result<()> {
    let mut entries = entries.to_vec();
    entries.sort_by_key(|entry| entry.cid);
    entries.dedup_by_key(|entry| entry.cid);
    let entry_count =
        u64::try_from(entries.len()).map_err(|_| invalid_data("too many BPKI entries"))?;
    let fanout = build_fanout(&entries)?;
    let pack_header_digest = *blake3::hash(&read_pack_header(pack_path)?).as_bytes();

    let mut bytes = Vec::with_capacity(
        PACK_INDEX_HEADER_LEN
            + PACK_INDEX_FANOUT_LEN
            + entries.len() * PACK_INDEX_ENTRY_LEN
            + PACK_INDEX_CRC_LEN,
    );
    bytes.extend_from_slice(PACK_INDEX_MAGIC);
    bytes.extend_from_slice(&PACK_INDEX_VERSION.to_le_bytes());
    bytes.extend_from_slice(&PACK_INDEX_HEADER_LEN_U16.to_le_bytes());
    bytes.extend_from_slice(&pack_header_digest);
    bytes.extend_from_slice(&entry_count.to_le_bytes());
    for value in fanout {
        bytes.extend_from_slice(&value.to_le_bytes());
    }
    for entry in &entries {
        bytes.extend_from_slice(entry.cid.as_bytes());
        bytes.extend_from_slice(&entry.offset.to_le_bytes());
        bytes.extend_from_slice(&entry.logical_len.to_le_bytes());
        bytes.extend_from_slice(&entry.stored_len.to_le_bytes());
    }
    bytes.extend_from_slice(&crc32c(&bytes).to_le_bytes());

    let parent = index_path
        .parent()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "BPKI path has no parent"))?;
    fs::create_dir_all(parent)?;
    let file_name = index_path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "invalid BPKI filename"))?;
    let tmp = parent.join(format!(".{file_name}.{}.tmp", uuid::Uuid::new_v4()));
    let mut file = OpenOptions::new().write(true).create_new(true).open(&tmp)?;
    file.write_all(&bytes)?;
    file.sync_all()?;
    fs::rename(&tmp, index_path)?;
    Ok(())
}

fn read_sealed_store_cids(
    packs_dir: &Path,
    excluded_pack: Option<&Path>,
) -> io::Result<BTreeSet<Cid>> {
    let mut cids = BTreeSet::new();
    for directory_entry in fs::read_dir(packs_dir)? {
        let index_path = directory_entry?.path();
        if !index_path
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.ends_with(".bamlpack.idx"))
        {
            continue;
        }
        let mut pack_path = index_path.clone();
        pack_path.set_extension("");
        if excluded_pack.is_some_and(|excluded| excluded == pack_path) {
            continue;
        }
        let index = PackIndex::read(index_path, pack_path)?;
        cids.extend(index.entries.into_iter().map(|entry| entry.cid));
    }
    Ok(cids)
}

fn build_fanout(entries: &[PackIndexEntry]) -> io::Result<[u32; 256]> {
    let mut fanout = [0_u32; 256];
    for entry in entries {
        fanout[usize::from(entry.cid.as_bytes()[0])] = fanout[usize::from(entry.cid.as_bytes()[0])]
            .checked_add(1)
            .ok_or_else(|| invalid_data("BPKI fanout overflow"))?;
    }
    let mut total = 0_u32;
    for value in &mut fanout {
        total = total
            .checked_add(*value)
            .ok_or_else(|| invalid_data("BPKI fanout overflow"))?;
        *value = total;
    }
    Ok(fanout)
}

fn validate_fanout(fanout: &[u32; 256], entries: &[PackIndexEntry]) -> io::Result<()> {
    let expected = build_fanout(entries)?;
    if *fanout != expected {
        return Err(invalid_data("BPKI fanout does not match sorted entries"));
    }
    Ok(())
}

fn write_lease(path: &Path, now_ms: u64) -> io::Result<()> {
    let mut file = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .open(path)?;
    writeln!(file, "{now_ms}")?;
    file.flush()
}

fn sync_parent(path: &Path) -> io::Result<()> {
    let parent = path
        .parent()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "path has no parent"))?;
    File::open(parent)?.sync_all()
}

fn hex_16(bytes: [u8; 16]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(32);
    for byte in bytes {
        output.push(char::from(HEX[usize::from(byte >> 4)]));
        output.push(char::from(HEX[usize::from(byte & 0x0f)]));
    }
    output
}

fn crc32c(bytes: &[u8]) -> u32 {
    crc32c_slices(&[bytes])
}

fn crc32c_slices(parts: &[&[u8]]) -> u32 {
    let mut crc = !0_u32;
    for bytes in parts {
        for byte in *bytes {
            crc ^= u32::from(*byte);
            for _ in 0..8 {
                let mask = 0_u32.wrapping_sub(crc & 1);
                crc = (crc >> 1) ^ (0x82f6_3b78 & mask);
            }
        }
    }
    !crc
}

fn get_u16(bytes: &[u8], offset: usize) -> u16 {
    u16::from_le_bytes(bytes[offset..offset + 2].try_into().expect("checked slice"))
}

fn get_u32(bytes: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes(bytes[offset..offset + 4].try_into().expect("checked slice"))
}

fn get_u64(bytes: &[u8], offset: usize) -> u64 {
    u64::from_le_bytes(bytes[offset..offset + 8].try_into().expect("checked slice"))
}

fn invalid_data(message: &'static str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}

#[cfg(test)]
mod tests {
    use std::{fs, io::Write as _};

    use super::{
        PACK_HEADER_LEN, PackIndex, PackWriter, read_pack_chunk, rebuild_pack_index, scan_pack,
    };
    use crate::value_cas::{CanonicalValue, GcGuard, encode_value_dag};

    fn temp_store() -> std::path::PathBuf {
        let path = std::env::temp_dir().join(format!(
            "baml-cas-pack-{}-{}",
            std::process::id(),
            uuid::Uuid::new_v4()
        ));
        fs::create_dir_all(&path).unwrap();
        path
    }

    #[test]
    fn pack_index_round_trip_and_dedup() {
        let store = temp_store();
        let dag = encode_value_dag(&CanonicalValue::List(vec![
            CanonicalValue::String("a".repeat(4096)),
            CanonicalValue::Int(7),
        ]))
        .unwrap();
        let mut writer = PackWriter::create(&store, [2; 16], 7, 100).unwrap();
        for chunk in &dag.chunks {
            writer.append_chunk(chunk).unwrap();
            writer.append_chunk(chunk).unwrap();
        }
        let paths = writer.seal().unwrap();
        let scan = scan_pack(&paths.pack).unwrap();
        assert!(!scan.torn_tail);
        assert_eq!(scan.entries.len(), dag.chunks.len());
        let index = PackIndex::read(&paths.index, &paths.pack).unwrap();
        for chunk in &dag.chunks {
            let entry = index.find(chunk.cid).unwrap();
            assert_eq!(
                read_pack_chunk(&paths.pack, entry).unwrap(),
                chunk.canonical_bytes
            );
        }
        fs::remove_dir_all(store).unwrap();
    }

    #[test]
    fn index_is_rebuildable_and_binds_to_pack_header() {
        let store = temp_store();
        let dag = encode_value_dag(&CanonicalValue::Int(9)).unwrap();
        let mut writer = PackWriter::create(&store, [3; 16], 1, 200).unwrap();
        writer.append_chunk(&dag.chunks[0]).unwrap();
        let paths = writer.seal().unwrap();
        fs::remove_file(&paths.index).unwrap();
        let index = rebuild_pack_index(&paths.pack, &paths.index).unwrap();
        assert!(index.find(dag.root).is_some());

        let other = store.join("packs/other.bamlpack");
        let mut bytes = fs::read(&paths.pack).unwrap();
        bytes[4] ^= 1;
        fs::write(&other, bytes).unwrap();
        let error = PackIndex::read(&paths.index, &other).unwrap_err();
        assert_eq!(error.kind(), std::io::ErrorKind::InvalidData);
        fs::remove_dir_all(store).unwrap();
    }

    #[test]
    fn scan_stops_before_torn_tail_without_mutation() {
        let store = temp_store();
        let dag = encode_value_dag(&CanonicalValue::Int(9)).unwrap();
        let mut writer = PackWriter::create(&store, [4; 16], 1, 200).unwrap();
        writer.append_chunk(&dag.chunks[0]).unwrap();
        writer.sync_data().unwrap();
        let path = writer.paths().pack.clone();
        drop(writer);
        let committed_len = fs::metadata(&path).unwrap().len();
        let mut file = fs::OpenOptions::new().append(true).open(&path).unwrap();
        file.write_all(&[0xb1, 0xc1, 1]).unwrap();
        drop(file);
        let scan = scan_pack(&path).unwrap();
        assert!(scan.torn_tail);
        assert_eq!(scan.committed_len, committed_len);
        assert_eq!(fs::metadata(&path).unwrap().len(), committed_len + 3);
        assert!(scan.committed_len >= PACK_HEADER_LEN as u64);
        fs::remove_dir_all(store).unwrap();
    }

    #[test]
    fn new_writer_dedupes_against_sealed_packs() {
        let store = temp_store();
        let dag = encode_value_dag(&CanonicalValue::String("shared".repeat(1000))).unwrap();
        let mut first = PackWriter::create(&store, [5; 16], 1, 100).unwrap();
        for chunk in &dag.chunks {
            assert!(first.append_chunk(chunk).unwrap().appended);
        }
        first.seal().unwrap();

        let mut second = PackWriter::create(&store, [5; 16], 2, 200).unwrap();
        for chunk in &dag.chunks {
            let outcome = second.append_chunk(chunk).unwrap();
            assert!(!outcome.appended);
            assert!(outcome.active_entry.is_none());
        }
        // Delete-boundary -> dedupe-old-chunk -> GC cannot sweep the old pack:
        // the deduping writer's shared lock forces the GC pass to skip.
        let gc_error = GcGuard::try_acquire(&store).unwrap_err();
        assert_eq!(gc_error.kind(), std::io::ErrorKind::WouldBlock);
        second.seal().unwrap();
        let inventory = super::super::build_pack_inventory(store.join("packs")).unwrap();
        assert_eq!(
            inventory
                .iter()
                .map(|pack| pack.entries.len())
                .sum::<usize>(),
            dag.chunks.len()
        );
        fs::remove_dir_all(store).unwrap();
    }
}
