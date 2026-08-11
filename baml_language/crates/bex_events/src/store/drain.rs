//! §7.3 per-process value drain service.
//!
//! One background thread per process owns the [`Store`]: producers hand it
//! **already-encoded** value DAGs (canonicalization + hashing happened
//! producer-side), so the service thread performs hashing-free work only —
//! dedupe probes, pack appends, group commits. The §6.7 root-commit
//! ordering (pack D1 **before** the manifest append) runs entirely on the
//! service thread, in [`ValueDrainService::append_manifest_and_commit`].
//!
//! The service thread's cumulative CPU (`CLOCK_THREAD_CPUTIME_ID`, same
//! probe as `prof::stats`) is sampled after every op and exported through
//! [`ValueDrainService::cpu_ns`] — the C10 measurement hook: value-plane
//! cost is a measurement, not an inference from wall-clock deltas.

use std::{
    io,
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
        mpsc,
    },
    thread,
};

use super::{Store, canon::CanonEncoded, gc};
use crate::prof::stats::thread_cpu_ns;

pub use crate::store::ValueStoreSink;

impl ValueStoreSink for Store {
    fn put_encoded(&mut self, encoded: &CanonEncoded, created_ms: u64) -> io::Result<u64> {
        Store::put_encoded(self, encoded, created_ms)
    }
}

impl ValueStoreSink for &ValueDrainService {
    fn put_encoded(&mut self, encoded: &CanonEncoded, created_ms: u64) -> io::Result<u64> {
        ValueDrainService::put_encoded(self, encoded, created_ms)
    }
}

/// A `&CanonEncoded` smuggled to the service thread.
///
/// Sound because [`ValueDrainService::put_encoded`] is a synchronous
/// round-trip: the sending thread constructs the pointer from a live
/// borrow and then blocks on the per-op reply channel, so the borrow
/// outlives every service-side use (the service dereferences it only
/// before replying). If the op is dropped unprocessed (service stopped) or
/// the service thread dies, the pointer is never (further) dereferenced
/// and the sender's `recv` observes disconnection instead.
struct EncodedRef(*const CanonEncoded);

#[expect(
    unsafe_code,
    reason = "scoped borrow handed to the service thread; the sender \
              blocks on the reply channel for the borrow's whole use"
)]
unsafe impl Send for EncodedRef {}

enum Op {
    Put {
        encoded: EncodedRef,
        created_ms: u64,
        reply: mpsc::Sender<io::Result<u64>>,
    },
    /// §6.7 root-commit ordering: pack D1, THEN the manifest append, both
    /// on the service thread so no root can dangle at any age.
    Commit {
        boundary_dir: PathBuf,
        cids: Vec<[u8; 32]>,
        reply: mpsc::Sender<io::Result<()>>,
    },
    Sync {
        reply: mpsc::Sender<io::Result<()>>,
    },
    Seal {
        reply: mpsc::Sender<io::Result<()>>,
    },
}

/// The per-process value drain service (§7.3): a background thread owning
/// the process's [`Store`], fed over an mpsc channel. All ops are
/// synchronous round-trips, so producer-side ordering is service-side
/// ordering.
#[derive(Debug)]
pub struct ValueDrainService {
    tx: Option<mpsc::Sender<Op>>,
    thread: Option<thread::JoinHandle<()>>,
    cpu_ns: Arc<AtomicU64>,
}

impl ValueDrainService {
    /// Spawn the service thread around an already-open [`Store`].
    pub fn spawn(store: Store) -> io::Result<ValueDrainService> {
        let (tx, rx) = mpsc::channel();
        let cpu_ns = Arc::new(AtomicU64::new(0));
        let thread_cpu = Arc::clone(&cpu_ns);
        let thread = thread::Builder::new()
            .name("baml-value-drain".to_string())
            .spawn(move || service_loop(store, &rx, &thread_cpu))?;
        Ok(ValueDrainService {
            tx: Some(tx),
            thread: Some(thread),
            cpu_ns,
        })
    }

    /// Open the store at `dir` and spawn the service around it.
    pub fn open(dir: &Path, origin_euid: [u8; 16]) -> io::Result<ValueDrainService> {
        Self::spawn(Store::open(dir, origin_euid)?)
    }

    /// Persist one encoded value DAG (sync round-trip; the write happens
    /// on the service thread). Returns the number of chunks actually
    /// appended — dedupe hits skip, exactly like [`Store::put_encoded`].
    pub fn put_encoded(&self, encoded: &CanonEncoded, created_ms: u64) -> io::Result<u64> {
        let (reply, rx) = mpsc::channel();
        self.send(Op::Put {
            encoded: EncodedRef(std::ptr::from_ref(encoded)),
            created_ms,
            reply,
        })?;
        recv_reply(&rx)
    }

    /// §6.7 root commit: D1-sync the active pack, THEN append `cids` to
    /// `boundary_dir/manifest.bamlcids` — in that order, on the service
    /// thread, so a committed root never references non-durable chunks.
    pub fn append_manifest_and_commit(
        &self,
        boundary_dir: PathBuf,
        cids: Vec<[u8; 32]>,
    ) -> io::Result<()> {
        let (reply, rx) = mpsc::channel();
        self.send(Op::Commit {
            boundary_dir,
            cids,
            reply,
        })?;
        recv_reply(&rx)
    }

    /// D1 group-commit of the active pack (no manifest append).
    pub fn sync(&self) -> io::Result<()> {
        let (reply, rx) = mpsc::channel();
        self.send(Op::Sync { reply })?;
        recv_reply(&rx)
    }

    /// Graceful shutdown: seal the active pack (idx tmp+rename, lease
    /// dropped), stop the service thread, and join it.
    pub fn seal_and_stop(mut self) -> io::Result<()> {
        let (reply, rx) = mpsc::channel();
        self.send(Op::Seal { reply })?;
        let sealed = recv_reply(&rx);
        drop(self.tx.take());
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
        sealed
    }

    /// Cumulative CPU consumed by the service thread (user+sys,
    /// `CLOCK_THREAD_CPUTIME_ID`), sampled after every op — the C10
    /// measurement hook. 0 where unsupported (non-unix) or before the
    /// first op completes.
    #[must_use]
    pub fn cpu_ns(&self) -> u64 {
        self.cpu_ns.load(Ordering::Relaxed)
    }

    fn send(&self, op: Op) -> io::Result<()> {
        self.tx
            .as_ref()
            .ok_or_else(|| {
                io::Error::new(io::ErrorKind::NotConnected, "value drain service stopped")
            })?
            .send(op)
            .map_err(|_| {
                io::Error::new(
                    io::ErrorKind::BrokenPipe,
                    "value drain service thread exited",
                )
            })
    }
}

impl Drop for ValueDrainService {
    fn drop(&mut self) {
        // Disconnect; the service loop seals best-effort and exits.
        drop(self.tx.take());
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

fn recv_reply<T>(rx: &mpsc::Receiver<io::Result<T>>) -> io::Result<T> {
    rx.recv().unwrap_or_else(|_| {
        Err(io::Error::new(
            io::ErrorKind::BrokenPipe,
            "value drain service thread exited before replying",
        ))
    })
}

fn service_loop(mut store: Store, rx: &mpsc::Receiver<Op>, cpu_ns: &AtomicU64) {
    // C10: sample this thread's cumulative CPU BEFORE each reply — the
    // reply unblocks the caller, so sampling after it races callers that
    // read `cpu_ns()` as soon as their op returns.
    let sample = |cpu_ns: &AtomicU64| cpu_ns.store(thread_cpu_ns(), Ordering::Relaxed);
    while let Ok(op) = rx.recv() {
        match op {
            Op::Put {
                encoded,
                created_ms,
                reply,
            } => {
                #[expect(
                    unsafe_code,
                    reason = "see EncodedRef: the sender's borrow is pinned \
                              by its blocking recv until this reply is sent"
                )]
                let encoded = unsafe { &*encoded.0 };
                let result = store.put_encoded(encoded, created_ms);
                sample(cpu_ns);
                let _ = reply.send(result);
            }
            Op::Commit {
                boundary_dir,
                cids,
                reply,
            } => {
                // §6.7: the manifest append happens only after the pack
                // bytes referenced by these roots are durable (D1).
                let result = store
                    .sync_data()
                    .and_then(|()| gc::append_manifest(&boundary_dir, &cids));
                sample(cpu_ns);
                let _ = reply.send(result);
            }
            Op::Sync { reply } => {
                let result = store.sync_data();
                sample(cpu_ns);
                let _ = reply.send(result);
            }
            Op::Seal { reply } => {
                let result = store.seal_active();
                sample(cpu_ns);
                let _ = reply.send(result);
            }
        }
    }
    // Producer side dropped without seal_and_stop: seal best-effort so a
    // clean process exit still leaves an indexed pack (a true crash is
    // covered by the open-time pack rescan).
    let _ = store.seal_active();
    sample(cpu_ns);
}

#[cfg(test)]
mod tests {
    use super::super::canon::{CanonValue, encode};
    use super::super::{gc, pack};
    use super::*;

    fn temp_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "baml-drain-{tag}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    fn pack_paths(store_dir: &Path) -> Vec<PathBuf> {
        let mut paths: Vec<PathBuf> = std::fs::read_dir(store_dir.join("packs"))
            .unwrap()
            .filter_map(Result::ok)
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|e| e == "bamlpack"))
            .collect();
        paths.sort();
        paths
    }

    #[test]
    fn service_persists_dedupes_and_seals_on_stop() {
        let dir = temp_dir("e2e");
        let service = ValueDrainService::open(&dir, [7; 16]).unwrap();

        let value = CanonValue::List(vec![CanonValue::String("drain me ".repeat(4_000))]);
        let encoded = encode(&value);
        assert!(service.put_encoded(&encoded, 1).unwrap() > 0);
        // Same DAG again: full dedupe on the service thread.
        assert_eq!(service.put_encoded(&encoded, 2).unwrap(), 0);
        service.sync().unwrap();
        service.seal_and_stop().unwrap();

        // Graceful seal: the pack has a committed idx sidecar.
        let packs = pack_paths(&dir);
        assert_eq!(packs.len(), 1);
        assert!(
            pack::idx_path_for(&packs[0]).exists(),
            "seal_and_stop must seal the active pack"
        );

        // Reopen: the DAG root is present and readable.
        let store = Store::open(&dir, [7; 16]).unwrap();
        assert!(store.contains(&encoded.root_cid));
        assert!(store.get(&encoded.root_cid).unwrap().is_some());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn manifest_appears_only_after_put_and_commit_and_references_durable_chunks() {
        let dir = temp_dir("commit");
        let boundary = dir.join("history/run-a");
        std::fs::create_dir_all(&boundary).unwrap();
        let service = ValueDrainService::open(&dir.join("store"), [8; 16]).unwrap();

        let encoded = encode(&CanonValue::String("root evidence ".repeat(2_000)));
        service.put_encoded(&encoded, 1).unwrap();

        // §6.7 ordering, observed: the put alone must not surface a root.
        let manifest = boundary.join("manifest.bamlcids");
        assert!(
            !manifest.exists(),
            "no manifest before append_manifest_and_commit"
        );

        service
            .append_manifest_and_commit(boundary, vec![encoded.root_cid])
            .unwrap();

        // The committed root parses back...
        let lines = std::fs::read_to_string(&manifest).unwrap();
        let roots: Vec<[u8; 32]> = lines.lines().filter_map(gc::parse_cid_wire).collect();
        assert_eq!(roots, vec![encoded.root_cid]);

        // ...and every chunk it references is already in the pack bytes on
        // disk (the pack write + D1 happened before the manifest append).
        let pack_bytes = std::fs::read(&pack_paths(&dir.join("store"))[0]).unwrap();
        let scan = pack::scan_pack(&pack_bytes).unwrap();
        let on_disk: Vec<[u8; 32]> = scan.chunks.iter().map(|c| c.cid).collect();
        for (cid, _) in encoded.nodes.iter().chain(encoded.chunks.iter()) {
            assert!(
                on_disk.contains(cid),
                "manifest committed a root whose chunk is not durable"
            );
        }
        service.seal_and_stop().unwrap();
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn commit_failure_reports_and_service_stays_usable() {
        let dir = temp_dir("commit-err");
        let service = ValueDrainService::open(&dir.join("store"), [9; 16]).unwrap();
        let encoded = encode(&CanonValue::Int(42));
        service.put_encoded(&encoded, 1).unwrap();

        // Boundary dir does not exist: the manifest append fails AFTER the
        // pack sync; the error surfaces to the caller.
        let missing = dir.join("history/never-created");
        assert!(
            service
                .append_manifest_and_commit(missing, vec![encoded.root_cid])
                .is_err()
        );

        // The service thread survives a failed op.
        assert_eq!(service.put_encoded(&encoded, 2).unwrap(), 0);
        service.sync().unwrap();
        service.seal_and_stop().unwrap();
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn cpu_ns_starts_zero_and_grows_monotonically() {
        let dir = temp_dir("cpu");
        let service = ValueDrainService::open(&dir, [4; 16]).unwrap();
        assert_eq!(service.cpu_ns(), 0, "no op processed yet");

        let mut last = 0;
        for round in 0..4u64 {
            let encoded = encode(&CanonValue::String(
                format!("cpu round {round} ").repeat(9_000),
            ));
            service.put_encoded(&encoded, round).unwrap();
            let now = service.cpu_ns();
            assert!(
                now >= last,
                "service CPU must be monotonic: {last} -> {now}"
            );
            last = now;
        }
        if cfg!(unix) {
            assert!(last > 0, "service thread CPU must advance on unix");
        }
        service.seal_and_stop().unwrap();
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn drop_without_stop_joins_and_seals_best_effort() {
        let dir = temp_dir("drop");
        let encoded = encode(&CanonValue::String("drop path".to_string()));
        {
            let service = ValueDrainService::open(&dir, [5; 16]).unwrap();
            service.put_encoded(&encoded, 1).unwrap();
            // Dropped without seal_and_stop.
        }
        let packs = pack_paths(&dir);
        assert_eq!(packs.len(), 1);
        assert!(
            pack::idx_path_for(&packs[0]).exists(),
            "drop still seals the active pack best-effort"
        );
        let store = Store::open(&dir, [5; 16]).unwrap();
        assert!(store.contains(&encoded.root_cid));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
