//! §6.7/§7.4 project-level content-addressed value store:
//! `.baml/store/packs/pack-*.bamlpack(.idx)` + `writers.lock`/`gc.lock`.
//!
//! - [`canon`] — the C9 canonical value encoding + BLAKE3 CIDs (frozen)
//! - [`pack`] — the physical chunk container + active-writer locking
//! - [`index`] — the per-pack fanout index (`.bamlpack.idx`)
//!
//! [`Store`] is the write facade: dedupe-before-append against every
//! sealed index plus the active pack, seal at 64 MiB, one writer per
//! process. Reads go newest-first (active pack, then sealed idx files).

pub mod canon;

/// Anything the value drain path can persist encoded DAGs into: a borrowed
/// `Store` (inline writes on the calling thread) or a shared
/// `ValueDrainService` (writes on the per-process service thread).
/// Target-neutral by shape — wasm callers simply never have an impl to
/// pass (§7.3: the CAS is native-only in v1).
pub trait ValueStoreSink {
    /// Persist one encoded value DAG; returns the number of chunks
    /// actually appended (dedupe hits skip).
    fn put_encoded(
        &mut self,
        encoded: &canon::CanonEncoded,
        created_ms: u64,
    ) -> std::io::Result<u64>;
}
#[cfg(not(target_arch = "wasm32"))]
pub mod drain;
#[cfg(not(target_arch = "wasm32"))]
pub mod gc;
#[cfg(not(target_arch = "wasm32"))]
pub mod index;
#[cfg(not(target_arch = "wasm32"))]
pub mod pack;
#[cfg(not(target_arch = "wasm32"))]
pub mod retention;

#[cfg(not(target_arch = "wasm32"))]
use std::io;
#[cfg(not(target_arch = "wasm32"))]
use std::path::{Path, PathBuf};

#[cfg(not(target_arch = "wasm32"))]
use rustc_hash::FxHashMap;

#[cfg(not(target_arch = "wasm32"))]
use pack::{ChunkKind, PackWriter};

/// Read exactly one record's bytes out of a pack and CRC-verify it.
/// A record that no longer matches its index entry (torn tail after a
/// crash, concurrent compaction) reads as absent, never as garbage.
#[cfg(not(target_arch = "wasm32"))]
fn read_record_range(path: &Path, meta: &pack::ChunkMeta) -> io::Result<Option<Vec<u8>>> {
    use std::io::{Read as _, Seek as _, SeekFrom};
    let mut file = match std::fs::File::open(path) {
        Ok(file) => file,
        Err(err) if err.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(err) => return Err(err),
    };
    let len = pack::REC_FIXED_LEN + meta.stored_len as usize + 4;
    let mut record = vec![0u8; len];
    if file.seek(SeekFrom::Start(meta.offset)).is_err() || file.read_exact(&mut record).is_err() {
        return Ok(None);
    }
    let mut local = *meta;
    local.offset = 0;
    Ok(pack::read_chunk(&record, &local))
}

/// The write/read facade over one project store dir (`<baml>/store`).
#[cfg(not(target_arch = "wasm32"))]
pub struct Store {
    dir: PathBuf,
    origin_euid: [u8; 16],
    writer: Option<PackWriter>,
    next_seq: u32,
    /// Sealed indexes, newest-first: (pack path, resident index).
    sealed: Vec<(PathBuf, index::PackIndex)>,
    /// Fast existence probe across sealed + active.
    known: FxHashMap<[u8; 32], ()>,
}

#[cfg(not(target_arch = "wasm32"))]
impl Store {
    /// Open the store, loading every sealed index (rebuilding a missing
    /// idx from its pack scan — the idx is always derivable).
    pub fn open(dir: &Path, origin_euid: [u8; 16]) -> io::Result<Store> {
        std::fs::create_dir_all(dir.join("packs"))?;
        let mut sealed = Vec::new();
        let mut known = FxHashMap::default();
        let mut next_seq = 0u32;
        let mut packs: Vec<PathBuf> = std::fs::read_dir(dir.join("packs"))?
            .filter_map(Result::ok)
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|e| e == "bamlpack"))
            .collect();
        packs.sort();
        for pack_path in packs.iter().rev() {
            let idx_path = pack::idx_path_for(pack_path);
            // Dedupe-trust rule: a CID enters `known` (and can therefore
            // absorb a future `put`) only when its bytes are provably
            // durable. A sealed idx proves it — `PackWriter::seal` fsyncs
            // the pack before publishing the idx. Anything else must be
            // made durable here first or stay out of `known`.
            let (idx, durable) = match std::fs::read(&idx_path)
                .ok()
                .and_then(|b| index::PackIndex::decode(&b).ok())
            {
                Some(idx) => {
                    if let Ok(seq) = pack::read_header_seq(pack_path) {
                        next_seq = next_seq.max(seq + 1);
                    }
                    // A crash between seal's idx publish and lease removal
                    // leaves a dead lease that would pin this pack against
                    // GC forever. The idx proves durability; clear it.
                    if pack::lease_path_for(pack_path).exists()
                        && !pack::lease_holder_alive(pack_path)
                    {
                        let _ = std::fs::remove_file(pack::lease_path_for(pack_path));
                    }
                    (idx, true)
                }
                None => {
                    // Unsealed (crashed or live foreign writer) or corrupt
                    // idx: rebuild from the pack's committed records.
                    let bytes = std::fs::read(pack_path)?;
                    let scan = pack::scan_pack(&bytes)?;
                    next_seq = next_seq.max(scan.pack_seq + 1);
                    let idx = index::PackIndex::decode(&index::encode_index(&scan.chunks))
                        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
                    if pack::lease_holder_alive(pack_path) {
                        // A live writer still owns this pack. Its bytes are
                        // readable but not provably durable — excluded from
                        // dedupe (we write our own durable copies; §6.7
                        // duplicates are semantically harmless).
                        (idx, false)
                    } else {
                        // Crashed-writer recovery: fsync the committed
                        // prefix, then seal the rebuilt idx durably so the
                        // recovery happens once. If durability cannot be
                        // established, degrade to dedupe-off instead of
                        // failing the open.
                        let durable = std::fs::File::open(pack_path)
                            .and_then(|f| f.sync_data())
                            .and_then(|()| {
                                pack_path.parent().map_or(Ok(()), crate::fsutil::fsync_dir)
                            })
                            .is_ok();
                        if durable {
                            let tmp = pack_path.with_extension("bamlpack.idx.tmp");
                            let _ = crate::fsutil::write_replace_durable(
                                &tmp,
                                &idx_path,
                                &index::encode_index(&scan.chunks),
                            );
                            let _ = std::fs::remove_file(pack::lease_path_for(pack_path));
                        }
                        (idx, durable)
                    }
                }
            };
            if durable {
                for (cid, ..) in idx.iter() {
                    known.insert(*cid, ());
                }
            }
            sealed.push((pack_path.clone(), idx));
        }
        Ok(Store {
            dir: dir.to_path_buf(),
            origin_euid,
            writer: None,
            next_seq,
            sealed,
            known,
        })
    }

    /// True when the CID already exists (sealed or active).
    #[must_use]
    pub fn contains(&self, cid: &[u8; 32]) -> bool {
        self.known.contains_key(cid)
    }

    /// Write one encoded value DAG: every node/chunk not already present
    /// appends to the active pack. Returns the number of chunks actually
    /// written (dedupe hits skip).
    pub fn put_encoded(
        &mut self,
        encoded: &canon::CanonEncoded,
        created_ms: u64,
    ) -> io::Result<u64> {
        let mut written = 0;
        for (cid, bytes) in &encoded.chunks {
            written += u64::from(self.put(ChunkKind::Chunk, *cid, bytes, created_ms)?);
        }
        for (cid, bytes) in &encoded.nodes {
            written += u64::from(self.put(ChunkKind::Node, *cid, bytes, created_ms)?);
        }
        Ok(written)
    }

    /// Append one chunk if absent. Returns true when it was written.
    pub fn put(
        &mut self,
        kind: ChunkKind,
        cid: [u8; 32],
        payload: &[u8],
        created_ms: u64,
    ) -> io::Result<bool> {
        if self.contains(&cid) {
            return Ok(false);
        }
        if self.writer.as_ref().is_some_and(PackWriter::should_seal) {
            self.seal_active()?;
        }
        if self.writer.is_none() {
            let seq = self.next_seq;
            self.next_seq += 1;
            self.writer = Some(PackWriter::create(
                &self.dir,
                self.origin_euid,
                seq,
                created_ms,
            )?);
        }
        let writer = self.writer.as_mut().expect("opened above");
        writer.append(kind, cid, payload)?;
        self.known.insert(cid, ());
        Ok(true)
    }

    /// D1 group-commit hook.
    pub fn sync_data(&mut self) -> io::Result<()> {
        if let Some(writer) = &mut self.writer {
            writer.sync_data()?;
        }
        Ok(())
    }

    /// Seal the active pack (idx tmp+rename, lease dropped) and load its
    /// index into the sealed set.
    pub fn seal_active(&mut self) -> io::Result<()> {
        if let Some(writer) = self.writer.take() {
            let path = writer.path().to_path_buf();
            writer.seal()?;
            let idx_bytes = std::fs::read(pack::idx_path_for(&path))?;
            let idx = index::PackIndex::decode(&idx_bytes)
                .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
            self.sealed.insert(0, (path, idx));
        }
        Ok(())
    }

    /// Read one chunk's payload by CID: active pack first, then sealed
    /// packs newest-first. CRC-verified. Reads only the record's byte
    /// range — never the whole pack — so hydration cost tracks value
    /// size, not pack size.
    pub fn get(&self, cid: &[u8; 32]) -> io::Result<Option<Vec<u8>>> {
        if let Some(writer) = &self.writer
            && let Some(meta) = writer.active_index().iter().find(|m| m.cid == *cid)
        {
            return read_record_range(writer.path(), meta);
        }
        for (path, idx) in &self.sealed {
            if let Some((offset, logical_len, stored_len)) = idx.lookup(cid) {
                let meta = pack::ChunkMeta {
                    kind: 0,
                    storage: 0,
                    cid: *cid,
                    logical_len,
                    stored_len,
                    offset,
                };
                return read_record_range(path, &meta);
            }
        }
        Ok(None)
    }
}

#[cfg(test)]
mod tests {
    use super::canon::{CanonValue, encode};
    use super::*;

    #[test]
    fn live_writer_pack_is_readable_but_not_dedupe_trusted() {
        let dir = std::env::temp_dir().join(format!("baml-store-live-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);

        let encoded = encode(&CanonValue::String("live writer bytes".repeat(200)));
        let mut owner = Store::open(&dir, [1; 16]).unwrap();
        owner.put_encoded(&encoded, 1).unwrap();
        // No sync, no seal — the owner's bytes are not provably durable.

        let mut other = Store::open(&dir, [2; 16]).unwrap();
        assert!(
            !other.contains(&encoded.root_cid),
            "a live writer's unsealed bytes must not absorb another writer's put"
        );
        assert!(
            other.get(&encoded.root_cid).unwrap().is_some(),
            "the live pack's committed records stay readable"
        );
        // The second writer stores its own durable copy (harmless duplicate).
        assert!(other.put_encoded(&encoded, 2).unwrap() > 0);
        other.sync_data().unwrap();
        other.seal_active().unwrap();
        drop(owner);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn crashed_writer_recovery_is_durable_and_single_shot() {
        let dir = std::env::temp_dir().join(format!("baml-store-recover-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);

        let encoded = encode(&CanonValue::String("crash recovery".repeat(300)));
        {
            let mut store = Store::open(&dir, [3; 16]).unwrap();
            store.put_encoded(&encoded, 1).unwrap();
            // Dropped without seal: lease left behind, no idx — a crash.
        }
        let packs: Vec<PathBuf> = std::fs::read_dir(dir.join("packs"))
            .unwrap()
            .filter_map(Result::ok)
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|e| e == "bamlpack"))
            .collect();
        assert_eq!(packs.len(), 1);
        assert!(
            pack::lease_path_for(&packs[0]).exists(),
            "crash leaves lease"
        );
        assert!(
            !pack::idx_path_for(&packs[0]).exists(),
            "crash leaves no idx"
        );

        // Recovery: the committed prefix becomes durable, the idx seals,
        // the dead lease clears, and the chunks re-enter dedupe.
        let store = Store::open(&dir, [3; 16]).unwrap();
        assert!(store.contains(&encoded.root_cid));
        assert!(
            pack::idx_path_for(&packs[0]).exists(),
            "recovery seals the rebuilt idx"
        );
        assert!(
            !pack::lease_path_for(&packs[0]).exists(),
            "recovery clears the dead lease"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn store_dedupes_and_reads_back_across_reopen() {
        let dir = std::env::temp_dir().join(format!("baml-store-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);

        let transcript =
            CanonValue::List(vec![CanonValue::String("system prompt ".repeat(12_000))]);
        let encoded = encode(&transcript);

        let mut store = Store::open(&dir, [9; 16]).unwrap();
        let first = store.put_encoded(&encoded, 1).unwrap();
        assert!(first > 0);
        // Same value again: full dedupe.
        assert_eq!(store.put_encoded(&encoded, 2).unwrap(), 0);
        assert!(store.contains(&encoded.root_cid));
        let root = store
            .get(&encoded.root_cid)
            .unwrap()
            .expect("root readable");
        assert!(!root.is_empty());
        store.seal_active().unwrap();

        // Reopen: sealed idx serves lookups; dedupe still holds.
        let mut store = Store::open(&dir, [9; 16]).unwrap();
        assert!(store.contains(&encoded.root_cid));
        assert_eq!(store.put_encoded(&encoded, 3).unwrap(), 0);
        assert_eq!(
            store
                .get(&encoded.root_cid)
                .unwrap()
                .expect("still readable"),
            root
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// C5 (§7.4): transcript-append goes from N(N+1)/2 to ~N bytes.
    /// N=64 rounds over a 64 KiB prompt must dedupe ≥20× vs naive storage,
    /// with growth staying near-linear.
    #[test]
    fn transcript_append_dedupes_20x_at_n64() {
        let dir = std::env::temp_dir().join(format!("baml-c5-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let mut store = Store::open(&dir, [0xC5; 16]).unwrap();

        let prompt = "s".repeat(64 * 1024);
        let mut naive: u64 = 0;
        let mut stored: u64 = 0;
        let mut messages: Vec<CanonValue> = vec![CanonValue::String(prompt)];
        for round in 0..64 {
            messages.push(CanonValue::String(format!(
                "user message {round}: {}",
                "m".repeat(400)
            )));
            let transcript = CanonValue::List(messages.clone());
            let encoded = encode(&transcript);
            naive += encoded.logical_len;
            // Count actual bytes appended this round (dedupe skips).
            let before = store
                .writer
                .as_ref()
                .map_or(0, super::pack::PackWriter::bytes);
            store.put_encoded(&encoded, 1).unwrap();
            let after = store
                .writer
                .as_ref()
                .map_or(0, super::pack::PackWriter::bytes);
            stored += after - before;
        }
        assert!(
            naive >= stored * 20,
            "C5 gate: naive {naive} vs stored {stored} = {:.1}x (need ≥20x)",
            naive as f64 / stored as f64
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn crashed_writer_pack_rebuilds_index_on_open() {
        let dir = std::env::temp_dir().join(format!("baml-store-crash-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);

        let value = CanonValue::String("crash evidence".repeat(500));
        let encoded = encode(&value);
        {
            let mut store = Store::open(&dir, [8; 16]).unwrap();
            store.put_encoded(&encoded, 1).unwrap();
            store.sync_data().unwrap();
            // Dropped WITHOUT seal: no idx, lease left behind — the crash.
        }
        let store = Store::open(&dir, [8; 16]).unwrap();
        assert!(
            store.contains(&encoded.root_cid),
            "idx rebuilt from pack scan"
        );
        assert!(
            store.get(&encoded.root_cid).unwrap().is_some(),
            "chunk readable after crash"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
