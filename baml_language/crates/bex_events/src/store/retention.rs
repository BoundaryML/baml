//! §6.8 retention: age/size budgets per root, ordered degradation,
//! tombstoned deletions, snapshot-materialize-before-delete.
//!
//! Degradation order when a budget binds: raw firehose segments → flight
//! dumps → per-boundary full-trace segments → whole oldest boundaries
//! (releasing their CAS closure) → sealed session CCT segments last.
//! Value packs are never deleted here — only via reachability
//! ([`super::gc`]). Every deletion is tombstoned in `retention.log`.

use std::io::{self, Write as _};
use std::path::{Path, PathBuf};

/// Retention policy knobs (defaults per §6.8; overridable by the host via
/// `baml.toml [observability]` / env before calling).
#[derive(Debug, Clone)]
pub struct RetentionPolicy {
    pub history_age_ms: u64,
    pub history_size_bytes: u64,
    /// Never prune below this many newest boundaries.
    pub history_floor: usize,
    pub sessions_age_ms: u64,
    pub sessions_size_bytes: u64,
    /// Per-session raw/ firehose cap.
    pub raw_per_session_bytes: u64,
    pub profiles_age_ms: u64,
}

impl Default for RetentionPolicy {
    fn default() -> RetentionPolicy {
        RetentionPolicy {
            history_age_ms: 30 * 24 * 60 * 60 * 1000,
            history_size_bytes: 2 << 30,
            history_floor: 20,
            sessions_age_ms: 7 * 24 * 60 * 60 * 1000,
            sessions_size_bytes: 1 << 30,
            raw_per_session_bytes: 512 << 20,
            profiles_age_ms: 7 * 24 * 60 * 60 * 1000,
        }
    }
}

#[derive(Debug, Default)]
pub struct RetentionReport {
    pub raw_files_removed: usize,
    pub flight_dumps_removed: usize,
    pub trace_segments_removed: usize,
    pub boundaries_removed: usize,
    pub sessions_removed: usize,
    pub legacy_profiles_removed: usize,
    pub bytes_reclaimed: u64,
    /// Dry run: nothing deleted, counts show what WOULD go.
    pub dry_run: bool,
}

fn dir_mtime_ms(path: &Path) -> u64 {
    std::fs::metadata(path)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map_or(0, |d| u64::try_from(d.as_millis()).unwrap_or(u64::MAX))
}

fn tree_bytes(path: &Path) -> u64 {
    let mut total = 0;
    if let Ok(entries) = std::fs::read_dir(path) {
        for e in entries.filter_map(Result::ok) {
            let p = e.path();
            if p.is_dir() {
                total += tree_bytes(&p);
            } else {
                total += e.metadata().map_or(0, |m| m.len());
            }
        }
    }
    total
}

fn tombstone(baml_dir: &Path, kind: &str, path: &Path, bytes: u64, now_ms: u64) -> io::Result<()> {
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(baml_dir.join("retention.log"))?;
    let line = serde_json::json!({
        "action": "retention",
        "kind": kind,
        "path": path.strip_prefix(baml_dir).unwrap_or(path).display().to_string(),
        "bytes_reclaimed": bytes,
        "at_ms": now_ms,
    });
    writeln!(file, "{line}")
}

/// One retention pass over `<baml_dir>` (the `baml clean` engine).
/// `dry_run` reports without deleting. GC (pack reachability) is separate
/// — run [`super::gc::gc`] after boundaries are released.
pub fn clean(
    baml_dir: &Path,
    policy: &RetentionPolicy,
    now_ms: u64,
    dry_run: bool,
) -> io::Result<RetentionReport> {
    let mut report = RetentionReport {
        dry_run,
        ..RetentionReport::default()
    };

    let mut remove_file =
        |report_bytes: &mut u64, counter: &mut usize, kind: &str, path: &Path| -> io::Result<()> {
            let bytes = std::fs::metadata(path).map_or(0, |m| m.len());
            if !dry_run {
                std::fs::remove_file(path)?;
                tombstone(baml_dir, kind, path, bytes, now_ms)?;
            }
            *counter += 1;
            *report_bytes += bytes;
            Ok(())
        };

    // Degradation step 1: raw firehose over its per-session cap, oldest
    // files first (they are the first casualty by contract §6.2).
    if let Ok(sessions) = std::fs::read_dir(baml_dir.join("sessions")) {
        for session in sessions.filter_map(Result::ok).map(|e| e.path()) {
            let raw_dir = session.join("raw");
            let mut raw_files: Vec<PathBuf> = std::fs::read_dir(&raw_dir)
                .map(|e| e.filter_map(Result::ok).map(|e| e.path()).collect())
                .unwrap_or_default();
            raw_files.sort();
            let mut raw_total: u64 = raw_files
                .iter()
                .map(|p| std::fs::metadata(p).map_or(0, |m| m.len()))
                .sum();
            for file in raw_files {
                if raw_total <= policy.raw_per_session_bytes {
                    break;
                }
                let len = std::fs::metadata(&file).map_or(0, |m| m.len());
                remove_file(
                    &mut report.bytes_reclaimed,
                    &mut report.raw_files_removed,
                    "raw",
                    &file,
                )?;
                raw_total = raw_total.saturating_sub(len);
            }
        }
    }

    // history/: age + size budget, newest-floor protected, oldest first.
    let mut boundaries: Vec<(u64, PathBuf)> = std::fs::read_dir(baml_dir.join("history"))
        .map(|entries| {
            entries
                .filter_map(Result::ok)
                .map(|e| e.path())
                .filter(|p| p.is_dir() && p.file_name().is_some_and(|n| n != "_unbound"))
                .map(|p| (dir_mtime_ms(&p), p))
                .collect()
        })
        .unwrap_or_default();
    boundaries.sort();
    let mut history_total: u64 = boundaries.iter().map(|(_, p)| tree_bytes(p)).sum();
    let mut removable = boundaries.len().saturating_sub(policy.history_floor);
    for (mtime, dir) in &boundaries {
        if removable == 0 {
            break;
        }
        let too_old = mtime.saturating_add(policy.history_age_ms) <= now_ms;
        let over_budget = history_total > policy.history_size_bytes;
        if !(too_old || over_budget) {
            continue;
        }
        let bytes = tree_bytes(dir);
        if !dry_run {
            // Degradation steps 2-3 within the boundary happen implicitly:
            // the whole dir goes (step 4); its CAS closure is released for
            // the next GC by the manifest disappearing with it.
            std::fs::remove_dir_all(dir)?;
            tombstone(baml_dir, "boundary", dir, bytes, now_ms)?;
        }
        report.boundaries_removed += 1;
        report.bytes_reclaimed += bytes;
        history_total = history_total.saturating_sub(bytes);
        removable -= 1;
    }

    // sessions/: age + size budget; sealed CCT aggregates go LAST, so a
    // session dir is only removed when it is past age or the budget still
    // binds after raw pruning. (Bound sessions of kept boundaries are
    // referenced by name; v1 keeps any session younger than the age gate.)
    let mut sessions: Vec<(u64, PathBuf)> = std::fs::read_dir(baml_dir.join("sessions"))
        .map(|entries| {
            entries
                .filter_map(Result::ok)
                .map(|e| e.path())
                .filter(|p| p.is_dir())
                .map(|p| (dir_mtime_ms(&p), p))
                .collect()
        })
        .unwrap_or_default();
    sessions.sort();
    let mut sessions_total: u64 = sessions.iter().map(|(_, p)| tree_bytes(p)).sum();
    for (mtime, dir) in &sessions {
        let too_old = mtime.saturating_add(policy.sessions_age_ms) <= now_ms;
        let over_budget = sessions_total > policy.sessions_size_bytes;
        if !(too_old || over_budget) {
            continue;
        }
        let bytes = tree_bytes(dir);
        if !dry_run {
            std::fs::remove_dir_all(dir)?;
            tombstone(baml_dir, "session", dir, bytes, now_ms)?;
        }
        report.sessions_removed += 1;
        report.bytes_reclaimed += bytes;
        sessions_total = sessions_total.saturating_sub(bytes);
    }

    // Legacy profiles/: age only.
    if let Ok(entries) = std::fs::read_dir(baml_dir.join("profiles")) {
        for file in entries.filter_map(Result::ok).map(|e| e.path()) {
            if !file.is_file() {
                continue;
            }
            if dir_mtime_ms(&file).saturating_add(policy.profiles_age_ms) <= now_ms {
                remove_file(
                    &mut report.bytes_reclaimed,
                    &mut report.legacy_profiles_removed,
                    "legacy_profile",
                    &file,
                )?;
            }
        }
    }
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("baml-ret-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn raw_cap_prunes_oldest_first_and_floor_protects_history() {
        let baml = setup("basic");
        // A session with 3 raw files of 100 B against an 150 B cap.
        let raw = baml.join("sessions/1-aa-e1/raw");
        std::fs::create_dir_all(&raw).unwrap();
        for i in 0..3 {
            std::fs::write(raw.join(format!("raw-{i:06}.bamlprof")), vec![0u8; 100]).unwrap();
        }
        // Two boundaries, floor 20 → nothing removable regardless of age.
        for name in ["1-old-b1", "2-new-b2"] {
            std::fs::create_dir_all(baml.join("history").join(name)).unwrap();
            std::fs::write(
                baml.join("history").join(name).join("boundary.bamlmeta"),
                b"x",
            )
            .unwrap();
        }
        let policy = RetentionPolicy {
            raw_per_session_bytes: 150,
            // Only the raw cap binds here — the session itself must
            // survive so the newest raw file is observable.
            sessions_age_ms: u64::MAX,
            ..RetentionPolicy::default()
        };
        let now = u64::MAX / 2;
        let report = clean(&baml, &policy, now, false).unwrap();
        assert_eq!(report.raw_files_removed, 2, "{report:?}");
        assert_eq!(report.boundaries_removed, 0, "floor protects newest 20");
        assert!(raw.join("raw-000002.bamlprof").exists(), "newest raw kept");

        let log = std::fs::read_to_string(baml.join("retention.log")).unwrap();
        assert_eq!(log.lines().count(), 2);
        let _ = std::fs::remove_dir_all(&baml);
    }

    #[test]
    fn dry_run_deletes_nothing() {
        let baml = setup("dry");
        let raw = baml.join("sessions/1-aa-e1/raw");
        std::fs::create_dir_all(&raw).unwrap();
        std::fs::write(raw.join("raw-000001.bamlprof"), vec![0u8; 200]).unwrap();
        let policy = RetentionPolicy {
            raw_per_session_bytes: 50,
            ..RetentionPolicy::default()
        };
        let report = clean(&baml, &policy, u64::MAX / 2, true).unwrap();
        assert!(report.dry_run);
        assert_eq!(report.raw_files_removed, 1);
        assert!(
            raw.join("raw-000001.bamlprof").exists(),
            "dry run keeps files"
        );
        assert!(
            !baml.join("retention.log").exists(),
            "no tombstones on dry run"
        );
        let _ = std::fs::remove_dir_all(&baml);
    }

    #[test]
    fn old_boundaries_go_when_beyond_floor() {
        let baml = setup("age");
        for i in 0..25 {
            let d = baml.join("history").join(format!("{i:02}-run"));
            std::fs::create_dir_all(&d).unwrap();
            std::fs::write(d.join("boundary.bamlmeta"), vec![0u8; 10]).unwrap();
        }
        let policy = RetentionPolicy {
            history_age_ms: 0, // everything "too old"
            history_floor: 20,
            ..RetentionPolicy::default()
        };
        let report = clean(&baml, &policy, u64::MAX / 2, false).unwrap();
        assert_eq!(report.boundaries_removed, 5, "25 - floor 20");
        let left = std::fs::read_dir(baml.join("history")).unwrap().count();
        assert_eq!(left, 20);
        let _ = std::fs::remove_dir_all(&baml);
    }
}
