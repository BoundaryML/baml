//! §6.7 GC: coarse exclusive locking, mark → sweep, tombstoned deletions.
//!
//! - Writers hold `writers.lock` SHARED; GC takes it EXCLUSIVE. If any
//!   writer is live, GC **skips with a notice** — the delete→dedupe→sweep
//!   adversarial interleaving reduces to "GC waits".
//! - `gc.lock` (exclusive) serializes concurrent GC invocations.
//! - Mark = union of `history/*/manifest.bamlcids` roots +
//!   `sessions/*/flight/*.bamlcids` pins + `uploads.pin`, closed over the
//!   canonical DAG (node → child CIDs via [`super::canon::node_refs`]).
//! - Sweep = packs older than the grace window whose chunks are all
//!   unmarked are unlinked whole; partially-live packs are compacted
//!   (live records rewritten to a fresh pack, old pack unlinked). Young
//!   packs are untouched. Every deletion appends a tombstone to
//!   `<baml>/retention.log` (jsonl).

use std::io::{self, Write as _};
use std::path::Path;

use rustc_hash::FxHashMap;

use super::{canon, index, pack};

/// 24 h default grace before unreferenced chunks become sweepable.
pub const DEFAULT_GRACE_MS: u64 = 24 * 60 * 60 * 1000;

#[derive(Debug, Default)]
pub struct GcReport {
    /// GC did not run (live writers or a concurrent GC).
    pub skipped: Option<String>,
    pub roots: usize,
    pub marked: usize,
    pub packs_kept: usize,
    pub packs_unlinked: usize,
    pub packs_compacted: usize,
    pub bytes_reclaimed: u64,
}

/// Append CIDs to a boundary's `manifest.bamlcids` (§6.7 root commit step
/// 3 — call inside the same group-commit barrier as the pack sync, AFTER
/// the pack fsync). Wire form, one per line, O_APPEND.
///
/// The append is durable before this returns: the manifest bytes are
/// fsynced and, because the file may have just been created, so is the
/// boundary directory. This closes the §6.7 barrier — a root pin either
/// survives a crash together with its pack bytes or is absent, in which
/// case the (durable, unpinned) chunks age out through the grace window.
pub fn append_manifest(boundary_dir: &Path, cids: &[[u8; 32]]) -> io::Result<()> {
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(boundary_dir.join("manifest.bamlcids"))?;
    let mut buf = String::new();
    for cid in cids {
        buf.push_str(&canon::cid_wire(cid));
        buf.push('\n');
    }
    file.write_all(buf.as_bytes())?;
    file.sync_data()?;
    crate::fsutil::fsync_dir(boundary_dir)
}

/// Parse one `bamlv_1_...` line back to raw CID bytes.
#[must_use]
pub fn parse_cid_wire(line: &str) -> Option<[u8; 32]> {
    let b64 = line.trim().strip_prefix("bamlv_1_")?;
    if b64.len() != 43 {
        return None;
    }
    let val = |c: u8| -> Option<u32> {
        Some(match c {
            b'A'..=b'Z' => u32::from(c - b'A'),
            b'a'..=b'z' => u32::from(c - b'a') + 26,
            b'0'..=b'9' => u32::from(c - b'0') + 52,
            b'-' => 62,
            b'_' => 63,
            _ => return None,
        })
    };
    let mut out = Vec::with_capacity(32);
    let bytes = b64.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        let n = bytes.len() - i;
        let a = val(bytes[i])?;
        let b = val(*bytes.get(i + 1)?)?;
        let c = match bytes.get(i + 2) {
            Some(&x) => val(x)?,
            None => 0,
        };
        let d = match bytes.get(i + 3) {
            Some(&x) => val(x)?,
            None => 0,
        };
        let word = a << 18 | b << 12 | c << 6 | d;
        out.push((word >> 16) as u8);
        if n > 2 {
            out.push((word >> 8) as u8);
        }
        if n > 3 {
            out.push(word as u8);
        }
        i += 4;
    }
    out.try_into().ok()
}

fn collect_root_lines(path: &Path, roots: &mut Vec<[u8; 32]>) {
    if let Ok(text) = std::fs::read_to_string(path) {
        for line in text.lines() {
            if let Some(cid) = parse_cid_wire(line) {
                roots.push(cid);
            }
        }
    }
}

/// Run one GC pass over `<baml_dir>`. Read-only until sweep; never runs
/// concurrently with writers.
pub fn gc(baml_dir: &Path, now_ms: u64, grace_ms: u64) -> io::Result<GcReport> {
    let store_dir = baml_dir.join("store");
    let mut report = GcReport::default();
    if !store_dir.join("packs").is_dir() {
        report.skipped = Some("no store".to_string());
        return Ok(report);
    }
    // §6.7 locks: writers.lock exclusive (else skip), gc.lock serializes.
    let Some(_writers) = pack::try_exclusive_writers_lock(&store_dir)? else {
        report.skipped = Some("live writers hold writers.lock".to_string());
        return Ok(report);
    };
    let gc_lock = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(store_dir.join("gc.lock"))?;
    #[cfg(unix)]
    {
        use std::os::unix::io::AsRawFd as _;
        // SAFETY: flock on an owned open fd; LOCK_EX | LOCK_NB.
        #[expect(unsafe_code, reason = "libc flock FFI on an owned fd")]
        let rc = unsafe { libc::flock(gc_lock.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) };
        if rc != 0 {
            report.skipped = Some("concurrent GC holds gc.lock".to_string());
            return Ok(report);
        }
    }

    // Roots: boundary manifests + flight pins + upload pins.
    let mut roots = Vec::new();
    for sub in ["history", "sessions"] {
        if let Ok(entries) = std::fs::read_dir(baml_dir.join(sub)) {
            for dir in entries.filter_map(Result::ok).map(|e| e.path()) {
                collect_root_lines(&dir.join("manifest.bamlcids"), &mut roots);
                if let Ok(flight) = std::fs::read_dir(dir.join("flight")) {
                    for f in flight.filter_map(Result::ok).map(|e| e.path()) {
                        if f.extension().is_some_and(|e| e == "bamlcids") {
                            collect_root_lines(&f, &mut roots);
                        }
                    }
                }
            }
        }
    }
    collect_root_lines(&baml_dir.join("uploads.pin"), &mut roots);
    report.roots = roots.len();

    // Load pack scans once (payload access for the closure walk).
    let mut packs: Vec<std::path::PathBuf> = std::fs::read_dir(store_dir.join("packs"))?
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|e| e == "bamlpack"))
        .collect();
    packs.sort();
    let mut loaded: Vec<(std::path::PathBuf, Vec<u8>, pack::PackScan, bool)> = Vec::new();
    for path in packs {
        let has_lease = path
            .with_file_name(format!(
                "{}.lease",
                path.file_stem()
                    .map(|s| s.to_string_lossy())
                    .unwrap_or_default()
            ))
            .exists()
            || {
                // Active packs are named pack-...bamlpack with a sibling
                // pack-....lease (no .bamlpack in the lease name).
                let mut lease = path.clone();
                lease.set_extension("lease");
                lease.exists()
            };
        let bytes = std::fs::read(&path)?;
        let scan = pack::scan_pack(&bytes)?;
        loaded.push((path, bytes, scan, has_lease));
    }

    // Chunk lookup across every pack (newest last wins is irrelevant —
    // contents are content-addressed).
    let mut where_is: FxHashMap<[u8; 32], (usize, usize)> = FxHashMap::default();
    for (pi, (_, _, scan, _)) in loaded.iter().enumerate() {
        for (ci, chunk) in scan.chunks.iter().enumerate() {
            where_is.entry(chunk.cid).or_insert((pi, ci));
        }
    }

    // Mark: BFS closure over node refs.
    let mut marked: FxHashMap<[u8; 32], ()> = FxHashMap::default();
    let mut queue = roots;
    while let Some(cid) = queue.pop() {
        if marked.insert(cid, ()).is_some() {
            continue;
        }
        let Some(&(pi, ci)) = where_is.get(&cid) else {
            continue; // dangling root: nothing to mark (doctor reports it)
        };
        let (_, bytes, scan, _) = &loaded[pi];
        let meta = &scan.chunks[ci];
        if meta.kind == pack::ChunkKind::Node as u8
            && let Some(payload) = pack::read_chunk(bytes, meta)
            && let Some(refs) = canon::node_refs(&payload)
        {
            queue.extend(refs.nodes);
            queue.extend(refs.chunks);
        }
    }
    report.marked = marked.len();

    // Sweep, pack by pack.
    let retention_log = baml_dir.join("retention.log");
    for (path, bytes, scan, has_lease) in &loaded {
        let sweepable_age = scan.created_ms.saturating_add(grace_ms) <= now_ms;
        if *has_lease || !sweepable_age {
            report.packs_kept += 1;
            continue;
        }
        let live: Vec<&pack::ChunkMeta> = scan
            .chunks
            .iter()
            .filter(|c| marked.contains_key(&c.cid))
            .collect();
        if live.len() == scan.chunks.len() {
            report.packs_kept += 1;
            continue;
        }
        if live.is_empty() {
            let reclaimed = bytes.len() as u64;
            std::fs::remove_file(path)?;
            let _ = std::fs::remove_file(pack::idx_path_for(path));
            tombstone(&retention_log, path, "unlinked", reclaimed, now_ms)?;
            report.packs_unlinked += 1;
            report.bytes_reclaimed += reclaimed;
            continue;
        }
        // Compact: rewrite live records into a `.compact` sibling, swap in,
        // rebuild idx. (GC holds writers.lock exclusive; the pack files are
        // rewritten directly rather than via PackWriter, which would try to
        // re-take the shared lock.)
        let fresh = encode_compacted(scan, &live, bytes);
        let compacted_metas = match pack::scan_pack(&fresh) {
            Ok(s) => s.chunks,
            Err(_) => {
                report.packs_kept += 1;
                continue;
            }
        };
        // The rename replaces a LIVE pack, so the temporary must be
        // durable first — otherwise a crash can surface the pack's name
        // with truncated content and lose the live records being kept.
        let tmp = path.with_extension("bamlpack.compact");
        crate::fsutil::write_replace_durable(&tmp, path, &fresh)?;
        let idx_bytes = index::encode_index(&compacted_metas);
        let idx_tmp = path.with_extension("bamlpack.idx.tmp");
        crate::fsutil::write_replace_durable(&idx_tmp, &pack::idx_path_for(path), &idx_bytes)?;
        let reclaimed = (bytes.len() as u64).saturating_sub(fresh.len() as u64);
        tombstone(&retention_log, path, "compacted", reclaimed, now_ms)?;
        report.packs_compacted += 1;
        report.bytes_reclaimed += reclaimed;
    }
    Ok(report)
}

fn encode_compacted(scan: &pack::PackScan, live: &[&pack::ChunkMeta], bytes: &[u8]) -> Vec<u8> {
    let mut out = pack::encode_header(scan.origin_euid, scan.pack_seq, scan.created_ms).to_vec();
    for meta in live {
        if let Some(payload) = pack::read_chunk(bytes, meta) {
            let kind = if meta.kind == pack::ChunkKind::Chunk as u8 {
                pack::ChunkKind::Chunk
            } else {
                pack::ChunkKind::Node
            };
            out.extend_from_slice(&pack::encode_record(kind, meta.cid, &payload));
        }
    }
    out
}

fn tombstone(log: &Path, pack_path: &Path, action: &str, bytes: u64, at_ms: u64) -> io::Result<()> {
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(log)?;
    let line = serde_json::json!({
        "action": action,
        "pack": pack_path.file_name().map(|n| n.to_string_lossy().into_owned()),
        "bytes_reclaimed": bytes,
        "at_ms": at_ms,
    });
    writeln!(file, "{line}")
}

#[cfg(test)]
mod tests {
    use super::super::Store;
    use super::super::canon::{CanonValue, encode};
    use super::*;

    fn setup(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("baml-gc-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("history/run-a")).unwrap();
        dir
    }

    #[test]
    fn cid_wire_round_trips() {
        let cid = {
            let mut c = [0u8; 32];
            for (i, b) in c.iter_mut().enumerate() {
                *b = u8::try_from(i * 7 % 251).unwrap();
            }
            c
        };
        assert_eq!(parse_cid_wire(&canon::cid_wire(&cid)), Some(cid));
        assert_eq!(parse_cid_wire("bamlv_1_short"), None);
        assert_eq!(parse_cid_wire("not-a-cid"), None);
    }

    #[test]
    fn gc_skips_with_live_writers_and_sweeps_after() {
        let baml = setup("sweep");
        let store_dir = baml.join("store");

        // Two values: one referenced by the boundary manifest, one orphan.
        let kept = encode(&CanonValue::String("kept ".repeat(3000)));
        let orphan = encode(&CanonValue::String("orphan ".repeat(3000)));
        {
            let mut store = Store::open(&store_dir, [5; 16]).unwrap();
            store.put_encoded(&kept, 1_000).unwrap();
            store.put_encoded(&orphan, 1_000).unwrap();

            append_manifest(&baml.join("history/run-a"), &[kept.root_cid]).unwrap();

            // Adversarial ruling: with the writer live, GC waits (skips).
            let report = gc(&baml, DEFAULT_GRACE_MS * 2, DEFAULT_GRACE_MS).unwrap();
            assert!(report.skipped.is_some(), "GC must skip under live writers");
            store.seal_active().unwrap();
        }

        // Young pack: grace not elapsed → kept untouched.
        let report = gc(&baml, 2_000, DEFAULT_GRACE_MS).unwrap();
        assert!(report.skipped.is_none());
        assert_eq!(report.packs_kept, 1);
        assert_eq!(report.packs_compacted + report.packs_unlinked, 0);

        // Past grace: orphan chunks compact away; kept root survives.
        let report = gc(&baml, DEFAULT_GRACE_MS * 2, DEFAULT_GRACE_MS).unwrap();
        assert!(report.skipped.is_none());
        assert_eq!(report.packs_compacted, 1);
        assert!(report.bytes_reclaimed > 0);
        assert!(report.marked >= 1);

        let store = Store::open(&store_dir, [5; 16]).unwrap();
        assert!(
            store.get(&kept.root_cid).unwrap().is_some(),
            "no readable root ever references a sweepable CID"
        );
        assert!(
            store.get(&orphan.root_cid).unwrap().is_none(),
            "orphan swept"
        );

        // Tombstoned.
        let log = std::fs::read_to_string(baml.join("retention.log")).unwrap();
        assert!(log.contains("compacted"), "{log}");
        let _ = std::fs::remove_dir_all(&baml);
    }

    #[test]
    fn fully_dead_pack_is_unlinked_whole() {
        let baml = setup("unlink");
        let store_dir = baml.join("store");
        let orphan = encode(&CanonValue::Bytes(vec![9u8; 50_000]));
        {
            let mut store = Store::open(&store_dir, [6; 16]).unwrap();
            store.put_encoded(&orphan, 1_000).unwrap();
            store.seal_active().unwrap();
        }
        let report = gc(&baml, DEFAULT_GRACE_MS * 2, DEFAULT_GRACE_MS).unwrap();
        assert_eq!(report.packs_unlinked, 1);
        let packs = std::fs::read_dir(store_dir.join("packs"))
            .unwrap()
            .filter_map(Result::ok)
            .filter(|e| e.path().extension().is_some_and(|x| x == "bamlpack"))
            .count();
        assert_eq!(packs, 0);
        let _ = std::fs::remove_dir_all(&baml);
    }

    #[test]
    fn closure_marks_chunks_through_nodes() {
        let baml = setup("closure");
        let store_dir = baml.join("store");
        // A chunked string: the manifest holds only the ROOT; chunks must
        // survive via the closure walk.
        let value = encode(&CanonValue::String("c".repeat(300_000)));
        {
            let mut store = Store::open(&store_dir, [7; 16]).unwrap();
            store.put_encoded(&value, 1_000).unwrap();
            store.seal_active().unwrap();
        }
        append_manifest(&baml.join("history/run-a"), &[value.root_cid]).unwrap();
        let report = gc(&baml, DEFAULT_GRACE_MS * 2, DEFAULT_GRACE_MS).unwrap();
        assert_eq!(report.packs_kept, 1, "everything reachable: {report:?}");
        assert_eq!(report.packs_compacted + report.packs_unlinked, 0);

        let store = Store::open(&store_dir, [7; 16]).unwrap();
        for (cid, _) in value.chunks.iter().chain(value.nodes.iter()) {
            assert!(store.get(cid).unwrap().is_some());
        }
        let _ = std::fs::remove_dir_all(&baml);
    }
}
