//! Disk-backed telemetry queue. This is the piece that makes `record()`
//! truly fire-and-forget: events are appended to a file the moment they're
//! recorded, and HTTP happens in a detached child process after the CLI
//! has already exited. Vercel's `flushDetached` (used for `next dev`),
//! generalized to every invocation and made crash-safe by writing events
//! to disk as they happen instead of buffering them in memory.
//!
//! File lifecycle inside `<baml_home>/telemetry/`:
//!
//! ```text
//! live_<pid>_<rand8>.jsonl      appended to by the owning process, one
//!    |                          complete PostHog request body per line
//!    |  sealed on: process drop, 10-min rotation, or orphan sweep
//!    v
//! sealed_<pid>_<rand8>.jsonl    ready to send; any flush child may claim it
//!    |
//!    |  claimed via atomic rename (exactly one child wins)
//!    v
//! sending_<childpid>_sealed_<pid>_<rand8>.jsonl
//!    |
//!    +-- POST ok  -> deleted
//!    +-- POST err -> renamed back to sealed_* for retry on a later run
//! ```
//!
//! Failure-mode coverage:
//!
//! - Process crash / SIGKILL: its `live_*` file stays on disk. A later
//!   flush child seals any live file untouched for [`ORPHAN_AGE`]
//!   (owners rotate every [`ROTATE_INTERVAL`], so a quiet live file means
//!   a dead owner) and sends it. No PID liveness check needed, which
//!   also sidesteps PID-reuse ambiguity entirely.
//! - PID reuse: filenames carry a random suffix, so a recycled PID can
//!   never append into a predecessor's file.
//! - Network down: the POST fails, the file returns to `sealed_*`, and
//!   the next `baml` invocation's child retries. Backlog drains itself.
//! - Child crash mid-send: the `sending_*` file is left behind and
//!   removed by the [`STALE_AGE`] purge. Bounded loss, no growth.

use std::{
    fs,
    io::Write as _,
    path::{Path, PathBuf},
    time::Duration,
};

/// How often a long-running process (LSP, playground) seals and ships its
/// live file so events don't wait for process exit. Bounded wire-latency
/// for any event is roughly this interval.
pub(super) const ROTATE_INTERVAL: Duration = Duration::from_secs(10 * 60);

/// A `live_*` file untouched for this long is treated as owned by a dead
/// process and sealed by the sweep. Must comfortably exceed
/// [`ROTATE_INTERVAL`]: a healthy owner either rotates the file away or
/// keeps appending to it (both update mtime) well within this window.
const ORPHAN_AGE: Duration = Duration::from_secs(30 * 60);

/// Queue files older than this are deleted without sending. If an event
/// couldn't be shipped for a full day (offline machine, wedged child),
/// dropping it is better than letting the directory grow forever.
const STALE_AGE: Duration = Duration::from_secs(24 * 60 * 60);

/// The queue directory, alongside (not inside) the config file.
pub(super) fn queue_dir() -> PathBuf {
    baml_release::baml_home().join("telemetry")
}

/// Mint a live-file path for this process: `live_<pid>_<rand8>.jsonl`.
/// The random suffix guarantees uniqueness even across PID reuse; the PID
/// is informational (nice for debugging a queue directory by eye).
pub(super) fn new_live_path_in(dir: &Path) -> PathBuf {
    let suffix = &uuid::Uuid::new_v4().simple().to_string()[..8];
    dir.join(format!("live_{}_{suffix}.jsonl", std::process::id()))
}

/// Append one serialized event line. A single `write_all` on an
/// `O_APPEND` handle is atomic for our payload sizes (~1 KB, below
/// `PIPE_BUF`), so concurrent appenders can't interleave partial lines.
pub(super) fn append_line(path: &Path, line: &str) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
        set_owner_only_dir_permissions(parent)?;
    }
    let mut options = fs::OpenOptions::new();
    options.append(true).create(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt as _;
        options.mode(0o600);
    }
    let mut file = options.open(path)?;
    set_owner_only_file_permissions(&file)?;
    file.write_all(format!("{line}\n").as_bytes())
}

#[cfg(unix)]
/// Restrict an existing or newly created queue directory to its owner.
fn set_owner_only_dir_permissions(path: &Path) -> std::io::Result<()> {
    use std::os::unix::fs::PermissionsExt as _;

    fs::set_permissions(path, fs::Permissions::from_mode(0o700))
}

#[cfg(not(unix))]
/// Rely on platform ACL inheritance where Unix permission bits are unavailable.
fn set_owner_only_dir_permissions(_path: &Path) -> std::io::Result<()> {
    Ok(())
}

#[cfg(unix)]
/// Restrict an existing or newly created queue file to its owner.
fn set_owner_only_file_permissions(file: &fs::File) -> std::io::Result<()> {
    use std::os::unix::fs::PermissionsExt as _;

    file.set_permissions(fs::Permissions::from_mode(0o600))
}

#[cfg(not(unix))]
/// Rely on platform ACL inheritance where Unix permission bits are unavailable.
fn set_owner_only_file_permissions(_file: &fs::File) -> std::io::Result<()> {
    Ok(())
}

/// Open a queue file without reading it and normalize its permissions in place.
fn normalize_queue_file_permissions(path: &Path) -> std::io::Result<()> {
    let file = fs::File::open(path)?;
    set_owner_only_file_permissions(&file)
}

/// Seal a live file (rename `live_*` → `sealed_*`) if it has any content.
/// Returns the sealed path, or `None` if the file was missing/empty (in
/// which case an empty leftover is removed). Rename is atomic on POSIX
/// and NTFS, so appenders and claimers can never see a half-state.
pub(super) fn seal(live: &Path) -> Option<PathBuf> {
    let len = fs::metadata(live).ok()?.len();
    if len == 0 {
        let _ = fs::remove_file(live);
        return None;
    }
    let name = live.file_name()?.to_str()?;
    let sealed_name = name
        .strip_prefix("live_")
        .map(|rest| format!("sealed_{rest}"))?;
    let sealed = live.with_file_name(sealed_name);
    fs::rename(live, &sealed).ok()?;
    Some(sealed)
}

/// Whether any sealed files are waiting in `dir`. Used at drop time to
/// decide if a flush child is worth spawning even when this invocation
/// recorded nothing (drains backlog left by earlier failed sends).
pub(super) fn has_sealed_work_in(dir: &Path) -> bool {
    let Ok(entries) = fs::read_dir(dir) else {
        return false;
    };
    entries
        .flatten()
        .any(|e| e.file_name().to_string_lossy().starts_with("sealed_"))
}

/// Spawn the detached flush child (`baml __flush-telemetry`). Fully
/// detached: null stdio, own process group (Unix) / no console window
/// (Windows), never waited on. The parent exits immediately; the child
/// does the HTTP work on its own time. Every error is swallowed — a
/// telemetry child we couldn't spawn just means the backlog waits for
/// the next invocation.
pub(super) fn spawn_flush_child() {
    let Ok(exe) = std::env::current_exe() else {
        return;
    };

    let mut cmd = std::process::Command::new(exe);
    cmd.arg("__flush-telemetry")
        // The child is our own binary invoked directly; suppress the
        // "use `baml` instead" wrapper warning it would otherwise print
        // (to /dev/null, but still).
        .env("BAML_CLI_ALLOW_DIRECT", "1")
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());

    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        // New process group: the child no longer receives the terminal's
        // SIGINT (Ctrl-C) and survives the parent's exit. It's ~200ms of
        // work, so full setsid()-style session detachment isn't worth
        // the unsafe libc call.
        cmd.process_group(0);
    }

    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const DETACHED_PROCESS: u32 = 0x0000_0008;
        const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP | CREATE_NO_WINDOW);
    }

    let _ = cmd.spawn();
}

/// The flush child's whole job: sweep the queue directory and send what's
/// ready. `disabled` (user opted out between record and send) deletes the
/// backlog instead of sending it — opt-out wins retroactively.
///
/// Runs in the detached child, after the parent CLI has already exited,
/// so nothing here is latency-sensitive.
pub(super) fn drain(dir: &Path, disabled: bool) {
    drain_with_dir_permissions(dir, disabled, set_owner_only_dir_permissions);
}

/// Drain with an injectable directory-permission operation for failure testing.
fn drain_with_dir_permissions(
    dir: &Path,
    disabled: bool,
    set_dir_permissions: impl FnOnce(&Path) -> std::io::Result<()>,
) {
    if disabled {
        // Opt-out wins retroactively and completely: remove *every* queue
        // file — `live_*` (possibly still being written by a long-running
        // process with a stale enabled config), `sealed_*`, and
        // `sending_*` — not just the claimable sealed ones. This ensures
        // no pre-opt-out data lingers on disk to be sent if telemetry is
        // later re-enabled. A running writer whose config snapshot is
        // still stale may append a fresh `live_*` after this, but its own
        // rotation (≤10 min) refreshes the config and stops it, and the
        // next flush child purges anything left — so lingering data is
        // bounded to a single rotation window.
        // Opt-out deletion must not depend on chmod succeeding. Attempt to
        // tighten the surviving directory only after all queue files are gone.
        purge_all_in(dir);
        let _ = set_dir_permissions(dir);
        return;
    }

    // A detached flush may encounter a queue created by an older CLI without
    // calling `append_line` first. Tighten the directory before scanning it.
    if set_dir_permissions(dir).is_err() {
        return;
    }

    purge_stale_in(dir);
    seal_orphans_in(dir);

    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    let sealed: Vec<PathBuf> = entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            p.file_name()
                .map(|n| n.to_string_lossy().starts_with("sealed_"))
                .unwrap_or(false)
        })
        .collect();

    for path in sealed {
        // Claim via atomic rename; exactly one concurrent child wins.
        let Some(claimed) = claim(&path) else {
            continue;
        };
        if disabled {
            let _ = fs::remove_file(&claimed);
            continue;
        }
        if send_file(&claimed) {
            let _ = fs::remove_file(&claimed);
        } else {
            // Put it back for a retry on a later invocation.
            let _ = unclaim(&claimed);
        }
    }
}

/// Atomically claim a sealed file for this process:
/// `sealed_X.jsonl` → `sending_<pid>_sealed_X.jsonl`.
fn claim(sealed: &Path) -> Option<PathBuf> {
    let name = sealed.file_name()?.to_str()?;
    let claimed = sealed.with_file_name(format!("sending_{}_{name}", std::process::id()));
    fs::rename(sealed, &claimed).ok()?;
    if normalize_queue_file_permissions(&claimed).is_err() {
        let _ = unclaim(&claimed);
        return None;
    }
    Some(claimed)
}

/// Undo a claim after a failed send: strip the `sending_<pid>_` prefix,
/// restoring the original `sealed_*` name.
fn unclaim(claimed: &Path) -> Option<()> {
    let name = claimed.file_name()?.to_str()?;
    let rest = name.strip_prefix("sending_")?;
    let sealed_name = rest.split_once('_').map(|(_pid, rest)| rest)?;
    let sealed = claimed.with_file_name(sealed_name);
    fs::rename(claimed, sealed).ok()
}

/// POST every line of a claimed file. Each line is a complete PostHog
/// request body, serialized at `record()` time by the process that owned
/// the event (so session IDs and metadata reflect the recording process,
/// not this child). All-or-nothing: any failed line marks the file for
/// retry. Duplicate delivery of already-sent lines on retry is possible
/// and acceptable — PostHog analytics tolerate rare dupes far better
/// than we'd tolerate the bookkeeping to prevent them.
fn send_file(path: &Path) -> bool {
    let Ok(contents) = fs::read_to_string(path) else {
        // Unreadable (corrupt write during a crash?) — claim it lost.
        return true;
    };
    for line in contents.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let Ok(body) = serde_json::from_str::<serde_json::Value>(line) else {
            // A torn/corrupt line (crash mid-append). Skip it; the rest
            // of the file is still good.
            continue;
        };
        if !super::post::send_body(&body) {
            return false;
        }
    }
    true
}

/// Seal `live_*` files that haven't been touched for [`ORPHAN_AGE`] —
/// their owners are gone (healthy owners rotate or append well within
/// the window). Freshly-active live files are left alone.
fn seal_orphans_in(dir: &Path) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name();
        if !name.to_string_lossy().starts_with("live_") {
            continue;
        }
        if older_than(&path, ORPHAN_AGE) {
            let _ = seal(&path);
        }
    }
}

/// Delete any queue file older than [`STALE_AGE`], sent or not.
fn purge_stale_in(dir: &Path) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if older_than(&path, STALE_AGE) {
            let _ = fs::remove_file(&path);
        }
    }
}

/// Delete every queue file in any state (`live_*`, `sealed_*`,
/// `sending_*`). Used when the user has opted out — see [`drain`].
fn purge_all_in(dir: &Path) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if name.starts_with("live_") || name.starts_with("sealed_") || name.starts_with("sending_")
        {
            let _ = fs::remove_file(entry.path());
        }
    }
}

/// Whether `path`'s mtime is older than `age`. Unreadable metadata or a
/// clock anomaly reports `false` (i.e. "not old"), so we never delete a
/// file we can't reason about.
fn older_than(path: &Path, age: Duration) -> bool {
    fs::metadata(path)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|mtime| mtime.elapsed().ok())
        .map(|elapsed| elapsed > age)
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Appended lines land in the live file; sealing renames it to the
    /// `sealed_*` twin with content intact.
    #[test]
    fn append_then_seal_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let live = new_live_path_in(dir.path());

        append_line(&live, r#"{"event":"a"}"#).unwrap();
        append_line(&live, r#"{"event":"b"}"#).unwrap();

        let sealed = seal(&live).expect("non-empty file should seal");
        assert!(!live.exists());
        assert!(sealed.exists());
        assert!(
            sealed
                .file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with("sealed_")
        );

        let contents = fs::read_to_string(&sealed).unwrap();
        assert_eq!(contents.lines().count(), 2);
    }

    #[cfg(unix)]
    #[test]
    fn queue_directory_and_file_are_owner_only() {
        use std::os::unix::fs::PermissionsExt as _;

        let root = tempfile::tempdir().unwrap();
        let queue = root.path().join("telemetry");
        let live = new_live_path_in(&queue);

        append_line(&live, r#"{"event":"a"}"#).unwrap();

        assert_eq!(
            fs::metadata(&queue).unwrap().permissions().mode() & 0o777,
            0o700
        );
        assert_eq!(
            fs::metadata(&live).unwrap().permissions().mode() & 0o777,
            0o600
        );

        let existing_live = new_live_path_in(&queue);
        fs::write(&existing_live, "").unwrap();
        fs::set_permissions(&existing_live, fs::Permissions::from_mode(0o644)).unwrap();

        append_line(&existing_live, r#"{"event":"b"}"#).unwrap();

        assert_eq!(
            fs::metadata(&existing_live).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }

    #[cfg(unix)]
    #[test]
    fn drain_and_claim_normalize_preexisting_queue_permissions() {
        use std::os::unix::fs::PermissionsExt as _;

        let root = tempfile::tempdir().unwrap();
        let queue = root.path().join("telemetry");
        fs::create_dir(&queue).unwrap();
        fs::set_permissions(&queue, fs::Permissions::from_mode(0o755)).unwrap();

        drain(&queue, false);

        assert_eq!(
            fs::metadata(&queue).unwrap().permissions().mode() & 0o777,
            0o700
        );

        let sealed = queue.join("sealed_1_existing.jsonl");
        fs::write(&sealed, r#"{"event":"a"}"#).unwrap();
        fs::set_permissions(&sealed, fs::Permissions::from_mode(0o644)).unwrap();

        let claimed = claim(&sealed).expect("existing sealed file should be claimed");

        assert_eq!(
            fs::metadata(&claimed).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }

    /// Sealing a missing or empty live file is a no-op (`None`), and any
    /// empty leftover is cleaned up rather than shipped.
    #[test]
    fn seal_skips_missing_and_empty() {
        let dir = tempfile::tempdir().unwrap();
        let live = new_live_path_in(dir.path());
        assert!(seal(&live).is_none(), "missing file");

        fs::write(&live, "").unwrap();
        assert!(seal(&live).is_none(), "empty file");
        assert!(!live.exists(), "empty leftover should be removed");
    }

    /// Claim/unclaim are inverse renames: exactly one claimer wins, and a
    /// failed send restores the original sealed name for retry.
    #[test]
    fn claim_and_unclaim_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let live = new_live_path_in(dir.path());
        append_line(&live, r#"{"event":"a"}"#).unwrap();
        let sealed = seal(&live).unwrap();
        let original_name = sealed.file_name().unwrap().to_os_string();

        let claimed = claim(&sealed).expect("first claim wins");
        assert!(!sealed.exists());
        assert!(claimed.exists());
        // A second claimer loses: the sealed file is gone.
        assert!(claim(&sealed).is_none());

        unclaim(&claimed).expect("unclaim restores the sealed name");
        assert!(!claimed.exists());
        let restored = dir.path().join(&original_name);
        assert!(restored.exists(), "restored to original sealed name");
    }

    /// Two live paths minted by the same process differ (random suffix),
    /// so even full PID reuse can't collide filenames.
    #[test]
    fn live_paths_are_unique_per_mint() {
        let dir = tempfile::tempdir().unwrap();
        let a = new_live_path_in(dir.path());
        let b = new_live_path_in(dir.path());
        assert_ne!(a, b);
    }

    /// When opted out, `drain` removes every queue file in every state —
    /// `live_*` (not yet sealed), `sealed_*`, and `sending_*` — so no
    /// pre-opt-out data can survive to be sent on a later re-enable.
    #[test]
    fn drain_disabled_purges_all_states() {
        let dir = tempfile::tempdir().unwrap();
        let d = dir.path();
        fs::write(d.join("live_1_aaaa.jsonl"), r#"{"event":"a"}"#).unwrap();
        fs::write(d.join("sealed_2_bbbb.jsonl"), r#"{"event":"b"}"#).unwrap();
        fs::write(d.join("sending_3_sealed_4_cccc.jsonl"), r#"{"event":"c"}"#).unwrap();

        drain(d, true);

        let remaining = fs::read_dir(d).unwrap().flatten().count();
        assert_eq!(remaining, 0, "disabled drain must purge the whole queue");
    }

    #[test]
    fn disabled_drain_purges_when_directory_permission_update_fails() {
        let dir = tempfile::tempdir().unwrap();
        let queued = dir.path().join("sealed_1_existing.jsonl");
        fs::write(&queued, r#"{"event":"a"}"#).unwrap();

        drain_with_dir_permissions(dir.path(), true, |_| {
            Err(std::io::Error::new(
                std::io::ErrorKind::PermissionDenied,
                "injected chmod failure",
            ))
        });

        assert!(!queued.exists(), "opt-out purge must not depend on chmod");
    }

    /// Fresh files survive both sweeps: too young to be orphans, too
    /// young to be stale.
    #[test]
    fn sweeps_leave_fresh_files_alone() {
        let dir = tempfile::tempdir().unwrap();
        let live = new_live_path_in(dir.path());
        append_line(&live, r#"{"event":"a"}"#).unwrap();

        seal_orphans_in(dir.path());
        purge_stale_in(dir.path());

        assert!(live.exists(), "fresh live file must not be swept");
    }
}
