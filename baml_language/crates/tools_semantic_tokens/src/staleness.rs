//! Self-restart on source change.
//!
//! The classifier (`baml_lsp2_actions`) and this viewer are compiled into the
//! binary, so editing them leaves a running viewer stale until rebuilt. A
//! background watcher compares the running binary's mtime against those source
//! trees; when newer, it rebuilds and re-execs itself. The frontend polls
//! [`build_id`] and reloads when it changes.

use std::{
    path::Path,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, SystemTime, UNIX_EPOCH},
};

/// On-disk mtime of the running binary.
pub(crate) fn exe_mtime() -> Option<SystemTime> {
    std::env::current_exe()
        .ok()
        .and_then(|p| std::fs::metadata(p).ok())
        .and_then(|m| m.modified().ok())
}

/// A stable id for the running build (its binary mtime in epoch millis), so the
/// frontend can detect a restart and reload.
pub(crate) fn build_id(started: Option<SystemTime>) -> u64 {
    started
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map_or(0, |d| u64::try_from(d.as_millis()).unwrap_or(u64::MAX))
}

/// Newest mtime among the viewer, the classifier (`baml_lsp2_actions`), and the
/// compiler crates the classifier depends on for token output.
fn newest_source_mtime() -> Option<SystemTime> {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    let roots = [
        manifest.join("src"),
        manifest.join("../baml_lsp2_actions/src"),
        manifest.join("../baml_compiler_syntax/src"),
    ];
    let mut newest = None;
    for root in roots {
        newest_under(&root, &mut newest);
    }
    newest
}

/// Recurse `dir`, folding the newest `.rs`/`.html` mtime into `newest`.
fn newest_under(dir: &Path, newest: &mut Option<SystemTime>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            newest_under(&path, newest);
        } else if matches!(
            path.extension().and_then(|e| e.to_str()),
            Some("rs" | "html")
        ) {
            if let Ok(m) = entry.metadata().and_then(|md| md.modified()) {
                if newest.is_none_or(|n| m > n) {
                    *newest = Some(m);
                }
            }
        }
    }
}

/// Whether watched source has changed since this binary was built (or a newer
/// binary has since been built).
fn is_stale(started: Option<SystemTime>) -> bool {
    started.is_some_and(|s| {
        newest_source_mtime().is_some_and(|m| m > s) || exe_mtime().is_some_and(|m| m > s)
    })
}

/// Spawn a watcher that rebuilds and re-execs the viewer when its source
/// changes. `rebuilding` is flipped while a rebuild is in flight so the frontend
/// can show a banner. Unix-only (uses `exec`); a no-op elsewhere.
pub(crate) fn spawn_watcher(started: Option<SystemTime>, rebuilding: Arc<AtomicBool>) {
    #[cfg(unix)]
    std::thread::Builder::new()
        .name("viewer-rebuild-watcher".into())
        .spawn(move || {
            loop {
                std::thread::sleep(Duration::from_secs(2));
                if !is_stale(started) {
                    continue;
                }
                rebuilding.store(true, Ordering::Relaxed);
                println!("[viewer] source changed -> rebuilding...");
                let built = std::process::Command::new("cargo")
                    .args(["build", "-p", "tools_semantic_tokens"])
                    .current_dir(env!("CARGO_MANIFEST_DIR"))
                    .status()
                    .is_ok_and(|s| s.success());
                if built {
                    println!("[viewer] rebuilt -> restarting");
                    if let Ok(exe) = std::env::current_exe() {
                        use std::os::unix::process::CommandExt;
                        let args: Vec<String> = std::env::args().skip(1).collect();
                        // `exec` replaces this process; it only returns on error.
                        let err = std::process::Command::new(exe).args(args).exec();
                        eprintln!("[viewer] re-exec failed: {err}");
                    }
                } else {
                    eprintln!("[viewer] rebuild failed; serving previous build");
                }
                rebuilding.store(false, Ordering::Relaxed);
            }
        })
        .expect("spawn rebuild watcher");

    #[cfg(not(unix))]
    {
        let _ = (started, rebuilding);
    }
}
