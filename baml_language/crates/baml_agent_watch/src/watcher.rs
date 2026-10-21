//! Watches every file under the root and records settled writes.
//!
//! Editors and agents rarely write a file in one event: an atomic save is a
//! temp file plus a rename, a streamed write is several modify events. Events
//! are collected until the directory has been quiet for a moment (or a burst
//! has run long enough), then each touched path is read once and recorded.

use std::{
    collections::BTreeSet,
    path::{Path, PathBuf},
    sync::{Arc, mpsc},
    time::{Duration, Instant},
};

use notify::{EventKind, RecursiveMode, Watcher};

use crate::{content::read_path, git::IgnoreFilter, session::Session};

/// Settle once no event has arrived for this long.
const QUIET: Duration = Duration::from_millis(120);
/// ... or once a burst has gone on this long.
const MAX_BATCH: Duration = Duration::from_millis(800);

/// Keep the returned watcher alive for as long as the directory is watched.
pub(crate) fn spawn(
    session: Arc<Session>,
    git: bool,
    on_baml_change: impl Fn(Vec<PathBuf>) + Send + 'static,
) -> anyhow::Result<notify::RecommendedWatcher> {
    let root = session.root().to_path_buf();
    let (tx, rx) = mpsc::channel::<PathBuf>();
    let mut watcher = notify::recommended_watcher(move |event: notify::Result<notify::Event>| {
        let Ok(event) = event else { return };
        if matches!(event.kind, EventKind::Access(_)) {
            return;
        }
        for path in event.paths {
            let _ = tx.send(path);
        }
    })?;
    watcher.watch(&root, RecursiveMode::Recursive)?;

    std::thread::Builder::new()
        .name("agent-watch-files".into())
        .spawn(move || {
            let mut filter = IgnoreFilter::new(&root, git);
            while let Ok(first) = rx.recv() {
                let started = Instant::now();
                let mut batch = BTreeSet::from([first]);
                loop {
                    let left = MAX_BATCH.saturating_sub(started.elapsed());
                    if left.is_zero() {
                        break;
                    }
                    match rx.recv_timeout(QUIET.min(left)) {
                        Ok(path) => {
                            batch.insert(path);
                        }
                        Err(mpsc::RecvTimeoutError::Timeout) => break,
                        Err(mpsc::RecvTimeoutError::Disconnected) => return,
                    }
                }
                let mut baml = Vec::new();
                for path in batch {
                    let Some(rel) = relative(&root, &path) else {
                        continue;
                    };
                    if filter.is_ignored(&rel) {
                        continue;
                    }
                    let Some(read) = read_path(&path) else {
                        continue; // a directory
                    };
                    if session.record(&rel, read) && is_baml(&rel) {
                        baml.push(path);
                    }
                }
                if !baml.is_empty() {
                    on_baml_change(baml);
                }
            }
        })?;
    Ok(watcher)
}

/// `path` relative to `root`, `/`-separated; `None` outside the root.
pub(crate) fn relative(root: &Path, path: &Path) -> Option<String> {
    let rel = path.strip_prefix(root).ok()?;
    if rel.as_os_str().is_empty() {
        return None;
    }
    Some(
        rel.components()
            .map(|c| c.as_os_str().to_string_lossy())
            .collect::<Vec<_>>()
            .join("/"),
    )
}

fn is_baml(rel: &str) -> bool {
    Path::new(rel)
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("baml"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn relative_paths_use_forward_slashes_and_exclude_the_root() {
        let root = Path::new("/w");
        assert_eq!(
            relative(root, Path::new("/w/a/b.rs")).as_deref(),
            Some("a/b.rs")
        );
        assert_eq!(relative(root, Path::new("/w")), None);
        assert_eq!(relative(root, Path::new("/elsewhere/x")), None);
    }
}
