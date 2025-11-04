use anyhow::Result;
use notify::{RecommendedWatcher, RecursiveMode, Watcher as NotifyWatcher};
use std::path::Path;
use std::sync::mpsc::{channel, Receiver};
use std::time::Duration;

pub struct FileWatcher {
    #[allow(dead_code)]
    watcher: RecommendedWatcher,
    receiver: Receiver<notify::Result<notify::Event>>,
}

impl FileWatcher {
    pub fn new(path: impl AsRef<Path>) -> Result<Self> {
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

    pub fn check_for_changes(&self) -> bool {
        // Simple event detection - just check if any events arrived
        self.receiver
            .try_recv()
            .map(|result| result.is_ok())
            .unwrap_or(false)
    }

    pub fn wait_for_change(&self, timeout: Duration) -> bool {
        match self.receiver.recv_timeout(timeout) {
            Ok(result) => result.is_ok(),
            Err(_) => false,
        }
    }
}

