use std::{
    path::Path,
    sync::mpsc::{Receiver, channel},
};

use anyhow::Result;
use notify::{RecommendedWatcher, RecursiveMode, Watcher as NotifyWatcher};

pub(crate) struct FileWatcher {
    #[allow(dead_code)]
    watcher: RecommendedWatcher,
    receiver: Receiver<notify::Result<notify::Event>>,
}

impl FileWatcher {
    pub(crate) fn new(path: impl AsRef<Path>) -> Result<Self> {
        let (tx, rx) = channel();

        let mut watcher = notify::recommended_watcher(move |res| {
            let _ = tx.send(res);
        })?;

        // Use recursive mode if watching a directory
        let mode = if path.as_ref().is_dir() {
            RecursiveMode::Recursive
        } else {
            RecursiveMode::NonRecursive
        };

        watcher.watch(path.as_ref(), mode)?;

        Ok(Self {
            watcher,
            receiver: rx,
        })
    }

    pub(crate) fn check_for_changes(&self) -> bool {
        // Simple event detection - just check if any events arrived
        self.receiver
            .try_recv()
            .map(|result| result.is_ok())
            .unwrap_or(false)
    }
}
