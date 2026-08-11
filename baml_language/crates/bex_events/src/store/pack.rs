//! §6.7 pack format (`.bamlpack`): the project-level content-addressed
//! chunk store's physical layer.
//!
//! ```text
//! [48 B header]
//!   magic         4B  "BPK1"
//!   version       u16 LE (1)
//!   header_len    u16 LE (48)
//!   origin_euid   [u8; 16]
//!   pack_seq      u32 LE
//!   reserved      u32 LE (0)
//!   created_ms    u64 LE
//!   reserved2     u64 LE (0)
//! [chunk records]*
//!   rec_magic     u16 LE ("CK" = 0x4B43)
//!   kind          u8       (ChunkKind)
//!   storage       u8       (0 raw | 1 zstd — v1 writes raw only)
//!   cid           [u8; 32]
//!   logical_len   u32 LE
//!   stored_len    u32 LE
//!   payload       [u8; stored_len]
//!   crc32c        u32 LE   (over rec_magic..payload)
//! ```
//!
//! Append-only; one active pack per writing process (no cross-process file
//! contention); sealed at 64 MiB or process exit. Physical packing is
//! independent of logical CIDs — repack/compress/compact never changes
//! identity.

use std::io::{self, Write as _};
use std::path::{Path, PathBuf};

use crate::prof::cct::crc32c;

pub const PACK_MAGIC: [u8; 4] = *b"BPK1";
pub const PACK_VERSION: u16 = 1;
pub const PACK_HEADER_LEN: usize = 48;
pub const REC_MAGIC: u16 = 0x4B43; // "CK"
pub const REC_FIXED_LEN: usize = 2 + 1 + 1 + 32 + 4 + 4; // before payload
/// §6.7: seal the active pack at this size.
pub const PACK_SEAL_BYTES: u64 = 64 << 20;

/// DAG node kinds (§7.4) as stored in chunk records.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum ChunkKind {
    /// A canonical value DAG node.
    Node = 1,
    /// A raw string/bytes chunk (128 KiB fixed chunking).
    Chunk = 2,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChunkMeta {
    pub kind: u8,
    pub storage: u8,
    pub cid: [u8; 32],
    pub logical_len: u32,
    pub stored_len: u32,
    /// Byte offset of the record header in the pack.
    pub offset: u64,
}

#[must_use]
pub fn encode_header(
    origin_euid: [u8; 16],
    pack_seq: u32,
    created_ms: u64,
) -> [u8; PACK_HEADER_LEN] {
    let mut h = [0u8; PACK_HEADER_LEN];
    h[0..4].copy_from_slice(&PACK_MAGIC);
    h[4..6].copy_from_slice(&PACK_VERSION.to_le_bytes());
    h[6..8].copy_from_slice(&u16::try_from(PACK_HEADER_LEN).unwrap_or(0).to_le_bytes());
    h[8..24].copy_from_slice(&origin_euid);
    h[24..28].copy_from_slice(&pack_seq.to_le_bytes());
    // 28..32 reserved
    h[32..40].copy_from_slice(&created_ms.to_le_bytes());
    // 40..48 reserved
    h
}

/// Encode one chunk record (storage raw).
#[must_use]
pub fn encode_record(kind: ChunkKind, cid: [u8; 32], payload: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(REC_FIXED_LEN + payload.len() + 4);
    out.extend_from_slice(&REC_MAGIC.to_le_bytes());
    out.push(kind as u8);
    out.push(0); // storage: raw
    out.extend_from_slice(&cid);
    let len = u32::try_from(payload.len()).unwrap_or(u32::MAX);
    out.extend_from_slice(&len.to_le_bytes()); // logical
    out.extend_from_slice(&len.to_le_bytes()); // stored (raw)
    out.extend_from_slice(payload);
    let crc = crc32c::crc32c(&out);
    out.extend_from_slice(&crc.to_le_bytes());
    out
}

/// Scan a pack's committed records. A torn tail (partial record / bad CRC)
/// ends the scan at the last whole record — crash-consistent by
/// construction.
pub struct PackScan {
    pub origin_euid: [u8; 16],
    pub pack_seq: u32,
    pub created_ms: u64,
    pub chunks: Vec<ChunkMeta>,
    pub torn_bytes: usize,
}

pub fn scan_pack(bytes: &[u8]) -> io::Result<PackScan> {
    let bad = |m: &str| io::Error::new(io::ErrorKind::InvalidData, m.to_string());
    if bytes.len() < PACK_HEADER_LEN || bytes[0..4] != PACK_MAGIC {
        return Err(bad("not a BPK1 pack"));
    }
    if u16::from_le_bytes(bytes[4..6].try_into().unwrap()) != PACK_VERSION {
        return Err(bad("unsupported BPK1 version"));
    }
    let origin_euid: [u8; 16] = bytes[8..24].try_into().unwrap();
    let pack_seq = u32::from_le_bytes(bytes[24..28].try_into().unwrap());
    let created_ms = u64::from_le_bytes(bytes[32..40].try_into().unwrap());
    let mut chunks = Vec::new();
    let mut at = PACK_HEADER_LEN;
    loop {
        let Some(fixed) = bytes.get(at..at + REC_FIXED_LEN) else {
            break;
        };
        if u16::from_le_bytes(fixed[0..2].try_into().unwrap()) != REC_MAGIC {
            break;
        }
        let stored_len =
            u32::from_le_bytes(fixed[REC_FIXED_LEN - 4..].try_into().unwrap()) as usize;
        let Some(rec) = bytes.get(at..at + REC_FIXED_LEN + stored_len + 4) else {
            break;
        };
        let crc = u32::from_le_bytes(rec[REC_FIXED_LEN + stored_len..].try_into().unwrap());
        if crc32c::crc32c(&rec[..REC_FIXED_LEN + stored_len]) != crc {
            break;
        }
        chunks.push(ChunkMeta {
            kind: rec[2],
            storage: rec[3],
            cid: rec[4..36].try_into().unwrap(),
            logical_len: u32::from_le_bytes(rec[36..40].try_into().unwrap()),
            stored_len: u32::try_from(stored_len).unwrap_or(u32::MAX),
            offset: at as u64,
        });
        at += REC_FIXED_LEN + stored_len + 4;
    }
    Ok(PackScan {
        origin_euid,
        pack_seq,
        created_ms,
        chunks,
        torn_bytes: bytes.len() - at,
    })
}

/// Read one chunk's payload out of pack bytes by its [`ChunkMeta`],
/// CRC-verified.
#[must_use]
pub fn read_chunk(bytes: &[u8], meta: &ChunkMeta) -> Option<Vec<u8>> {
    let at = usize::try_from(meta.offset).ok()?;
    let stored = meta.stored_len as usize;
    let rec = bytes.get(at..at + REC_FIXED_LEN + stored + 4)?;
    let crc = u32::from_le_bytes(rec[REC_FIXED_LEN + stored..].try_into().ok()?);
    if crc32c::crc32c(&rec[..REC_FIXED_LEN + stored]) != crc {
        return None;
    }
    Some(rec[REC_FIXED_LEN..REC_FIXED_LEN + stored].to_vec())
}

/// The active pack writer: one per process (§6.7). Holds a shared flock on
/// `writers.lock` for its lifetime (GC takes it exclusive), plus a
/// `.lease` heartbeat file next to the active pack.
pub struct PackWriter {
    file: std::fs::File,
    path: PathBuf,
    lease_path: PathBuf,
    /// Shared writers.lock guard — dropped (unlocked) with the writer.
    _writers_lock: std::fs::File,
    bytes: u64,
    /// In-memory index of the active pack (§6.7: "the active pack is
    /// indexed in its owner's memory").
    index: Vec<ChunkMeta>,
    origin_euid: [u8; 16],
    pack_seq: u32,
    /// The pack's directory entry has been fsynced. Data durability is
    /// meaningless while the name itself can vanish, so the first group
    /// commit also syncs `packs/` (once per pack lifetime).
    dir_synced: bool,
}

impl PackWriter {
    /// Create `store/packs/pack-<euid>-<seq6>.bamlpack` (+ `.lease`),
    /// taking the shared writers.lock.
    pub fn create(
        store_dir: &Path,
        origin_euid: [u8; 16],
        pack_seq: u32,
        created_ms: u64,
    ) -> io::Result<PackWriter> {
        std::fs::create_dir_all(store_dir.join("packs"))?;
        let writers_lock = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(store_dir.join("writers.lock"))?;
        flock_shared(&writers_lock)?;
        let euid_hex: String = origin_euid.iter().map(|b| format!("{b:02x}")).collect();
        let name = format!("pack-{euid_hex}-{pack_seq:06}");
        let path = store_dir.join("packs").join(format!("{name}.bamlpack"));
        let lease_path = store_dir.join("packs").join(format!("{name}.lease"));
        let mut file = std::fs::OpenOptions::new()
            .create_new(true)
            .append(true)
            .open(&path)?;
        file.write_all(&encode_header(origin_euid, pack_seq, created_ms))?;
        std::fs::write(&lease_path, format!("{}\n", std::process::id()))?;
        active_packs().insert(path.clone());
        Ok(PackWriter {
            file,
            path,
            lease_path,
            _writers_lock: writers_lock,
            bytes: PACK_HEADER_LEN as u64,
            index: Vec::new(),
            origin_euid,
            pack_seq,
            dir_synced: false,
        })
    }

    /// Append one chunk (no dedupe here — the [`super::Store`] checks
    /// existence first). Returns its record offset.
    pub fn append(&mut self, kind: ChunkKind, cid: [u8; 32], payload: &[u8]) -> io::Result<u64> {
        let record = encode_record(kind, cid, payload);
        let offset = self.bytes;
        self.file.write_all(&record)?;
        self.bytes += record.len() as u64;
        self.index.push(ChunkMeta {
            kind: kind as u8,
            storage: 0,
            cid,
            logical_len: u32::try_from(payload.len()).unwrap_or(u32::MAX),
            stored_len: u32::try_from(payload.len()).unwrap_or(u32::MAX),
            offset,
        });
        Ok(offset)
    }

    /// D1 group commit hook: fsync the pack, plus (once) the `packs/`
    /// directory so the pack's own name is as durable as its bytes.
    pub fn sync_data(&mut self) -> io::Result<()> {
        self.file.sync_data()?;
        if !self.dir_synced {
            if let Some(dir) = self.path.parent() {
                crate::fsutil::fsync_dir(dir)?;
            }
            self.dir_synced = true;
        }
        Ok(())
    }

    #[must_use]
    pub fn bytes(&self) -> u64 {
        self.bytes
    }

    #[must_use]
    pub fn should_seal(&self) -> bool {
        self.bytes >= PACK_SEAL_BYTES
    }

    #[must_use]
    pub fn active_index(&self) -> &[ChunkMeta] {
        &self.index
    }

    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    #[must_use]
    pub fn pack_seq(&self) -> u32 {
        self.pack_seq
    }

    #[must_use]
    pub fn origin_euid(&self) -> [u8; 16] {
        self.origin_euid
    }

    /// Seal: fsync pack + name, write `.bamlpack.idx` durably
    /// (tmp fsync → rename → dir fsync), drop the lease. Consumes the
    /// writer; the caller opens a fresh pack for new writes.
    pub fn seal(mut self) -> io::Result<()> {
        self.sync_data()?;
        let idx_bytes = super::index::encode_index(&self.index);
        let idx_path = idx_path_for(&self.path);
        let tmp = self.path.with_extension("bamlpack.idx.tmp");
        crate::fsutil::write_replace_durable(&tmp, &idx_path, &idx_bytes)?;
        let _ = std::fs::remove_file(&self.lease_path);
        Ok(())
    }
}

impl Drop for PackWriter {
    fn drop(&mut self) {
        // In-memory ownership release only — the lease FILE stays unless
        // seal removed it, which is what makes an unsealed drop look like
        // (and recover like) a crash.
        active_packs().remove(&self.path);
    }
}

/// `pack-...bamlpack` → `pack-...bamlpack.idx` (design §6.1 naming).
#[must_use]
pub fn idx_path_for(pack_path: &Path) -> PathBuf {
    let mut name = pack_path
        .file_name()
        .map_or_else(String::new, |n| n.to_string_lossy().into_owned());
    name.push_str(".idx");
    pack_path.with_file_name(name)
}

/// `pack-...bamlpack` → its sibling `.lease` heartbeat file.
#[must_use]
pub fn lease_path_for(pack_path: &Path) -> PathBuf {
    pack_path.with_extension("lease")
}

/// Parse a pack's sequence number from its 48-byte header without
/// reading the (possibly 64 MiB) body.
pub fn read_header_seq(pack_path: &Path) -> io::Result<u32> {
    use std::io::Read as _;
    let mut header = [0u8; PACK_HEADER_LEN];
    let mut file = std::fs::File::open(pack_path)?;
    file.read_exact(&mut header)?;
    Ok(scan_pack(&header)?.pack_seq)
}

/// Pack paths owned by live [`PackWriter`]s in THIS process. A lease
/// naming our own pid proves nothing by itself — the writer may have been
/// dropped without seal (recoverable orphan) or may be live in another
/// `Store` instance. This registry resolves that ambiguity exactly.
static ACTIVE_PACKS: std::sync::LazyLock<std::sync::Mutex<std::collections::HashSet<PathBuf>>> =
    std::sync::LazyLock::new(|| std::sync::Mutex::new(std::collections::HashSet::new()));

fn active_packs() -> std::sync::MutexGuard<'static, std::collections::HashSet<PathBuf>> {
    ACTIVE_PACKS
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// Whether the pack's `.lease` names a live owner. Used by open-time
/// recovery to distinguish a live writer (bytes readable but not provably
/// durable) from a crashed one (recoverable). Same-process ownership is
/// answered exactly via the active-writer registry; for foreign pids the
/// probe errs on the side of "alive": an unreadable lease, an unparseable
/// pid, or a platform without a liveness probe all report `true`, which
/// only costs harmless duplicate writes — never a durability claim.
#[must_use]
pub fn lease_holder_alive(pack_path: &Path) -> bool {
    let Ok(lease) = std::fs::read_to_string(lease_path_for(pack_path)) else {
        // No lease at all: a sealed or already-recovered pack. The caller
        // only asks for idx-less packs, where a missing lease means the
        // owner is gone.
        return false;
    };
    let Ok(pid) = lease.trim().parse::<i32>() else {
        return true;
    };
    if pid == i32::try_from(std::process::id()).unwrap_or(-1) {
        return active_packs().contains(pack_path);
    }
    #[cfg(unix)]
    {
        // SAFETY: kill(pid, 0) probes existence without side effects.
        #[expect(unsafe_code, reason = "libc kill(pid, 0) liveness probe")]
        let rc = unsafe { libc::kill(pid, 0) };
        // ESRCH = definitely gone; anything else (0, EPERM) = assume live.
        rc == 0 || std::io::Error::last_os_error().raw_os_error() != Some(libc::ESRCH)
    }
    #[cfg(not(unix))]
    {
        let _ = pid;
        true
    }
}

fn flock_shared(file: &std::fs::File) -> io::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::io::AsRawFd as _;
        // SAFETY: flock on an owned open fd; LOCK_SH | LOCK_NB.
        #[expect(unsafe_code, reason = "libc flock FFI on an owned fd")]
        let rc = unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_SH | libc::LOCK_NB) };
        if rc != 0 {
            return Err(io::Error::last_os_error());
        }
    }
    Ok(())
}

/// Take `writers.lock` exclusively (GC / `baml clean`). `None` when any
/// writer holds it shared — the §6.7 contract: GC never runs concurrently
/// with writers; it skips with a notice.
pub fn try_exclusive_writers_lock(store_dir: &Path) -> io::Result<Option<std::fs::File>> {
    let file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(store_dir.join("writers.lock"))?;
    #[cfg(unix)]
    {
        use std::os::unix::io::AsRawFd as _;
        // SAFETY: flock on an owned open fd; LOCK_EX | LOCK_NB.
        #[expect(unsafe_code, reason = "libc flock FFI on an owned fd")]
        let rc = unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) };
        if rc != 0 {
            return Ok(None);
        }
    }
    Ok(Some(file))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pack_round_trip_and_torn_tail() {
        let dir = std::env::temp_dir().join(format!("baml-pack-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        let mut writer = PackWriter::create(&dir, [3; 16], 1, 1_700_000_000_000).unwrap();
        let cid_a = [0xAA; 32];
        let cid_b = [0xBB; 32];
        writer
            .append(ChunkKind::Node, cid_a, b"node-bytes")
            .unwrap();
        writer
            .append(ChunkKind::Chunk, cid_b, &vec![7u8; 1000])
            .unwrap();
        let path = writer.path().to_path_buf();
        writer.seal().unwrap();

        let bytes = std::fs::read(&path).unwrap();
        let scan = scan_pack(&bytes).unwrap();
        assert_eq!(scan.pack_seq, 1);
        assert_eq!(scan.chunks.len(), 2);
        assert_eq!(scan.torn_bytes, 0);
        assert_eq!(scan.chunks[0].cid, cid_a);
        assert_eq!(
            read_chunk(&bytes, &scan.chunks[0]).unwrap(),
            b"node-bytes".to_vec()
        );
        assert_eq!(
            read_chunk(&bytes, &scan.chunks[1]).unwrap(),
            vec![7u8; 1000]
        );

        // Idx sealed alongside; lease dropped.
        assert!(idx_path_for(&path).exists());
        assert!(!path.with_extension("lease").exists());

        // Torn tail: cut into the second record — first survives.
        let cut = usize::try_from(scan.chunks[1].offset).unwrap() + 10;
        let torn = scan_pack(&bytes[..cut]).unwrap();
        assert_eq!(torn.chunks.len(), 1);
        assert!(torn.torn_bytes > 0);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn exclusive_lock_excludes_writers() {
        let dir = std::env::temp_dir().join(format!("baml-lock-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        let writer = PackWriter::create(&dir, [4; 16], 1, 1).unwrap();
        assert!(
            try_exclusive_writers_lock(&dir).unwrap().is_none(),
            "GC must not get the lock while a writer is live"
        );
        drop(writer);
        assert!(try_exclusive_writers_lock(&dir).unwrap().is_some());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
