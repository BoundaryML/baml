//! Engine-owned native body tasks, independent of telemetry and VM ownership.
#[cfg(not(target_arch = "wasm32"))]
use std::{future::Future, sync::Mutex};

#[cfg(not(target_arch = "wasm32"))]
use tokio::task::JoinHandle;
#[cfg(not(target_arch = "wasm32"))]
use tokio_util::sync::CancellationToken;

#[derive(Default, Debug)]
pub struct HttpReaders(#[cfg(not(target_arch = "wasm32"))] Mutex<Registry>);

#[cfg(not(target_arch = "wasm32"))]
#[derive(Default, Debug)]
struct Registry {
    closed: bool,
    readers: Vec<Reader>,
}

#[cfg(not(target_arch = "wasm32"))]
#[derive(Debug)]
struct Reader {
    release: CancellationToken,
    handle: JoinHandle<()>,
}

#[cfg(not(target_arch = "wasm32"))]
impl HttpReaders {
    /// Admission and spawning share the shutdown lock: late sys-op completions
    /// cannot start an untracked reader after the shutdown sweep.
    pub fn spawn(
        &self,
        release: CancellationToken,
        future: impl Future<Output = ()> + Send + 'static,
    ) -> bool {
        let mut registry = self
            .0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if registry.closed {
            return false;
        }
        registry
            .readers
            .retain(|reader| !reader.handle.is_finished());
        registry.readers.push(Reader {
            release,
            handle: tokio::spawn(future),
        });
        true
    }

    /// All drains share one deadline. Abort every straggler before joining any
    /// of them, so no task can access recording after shutdown returns.
    pub async fn shutdown(&self, budget: std::time::Duration) {
        let mut readers = {
            let mut registry = self
                .0
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            registry.closed = true;
            std::mem::take(&mut registry.readers)
        };
        let deadline = tokio::time::Instant::now() + budget;
        for reader in &readers {
            reader.release.cancel();
        }
        for reader in &mut readers {
            if tokio::time::timeout_at(deadline, &mut reader.handle)
                .await
                .is_err()
            {
                break;
            }
        }
        for reader in &readers {
            if !reader.handle.is_finished() {
                reader.handle.abort();
            }
        }
        for reader in readers {
            if !reader.handle.is_finished() {
                let _ = reader.handle.await;
            }
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl Drop for HttpReaders {
    fn drop(&mut self) {
        for reader in &self
            .0
            .get_mut()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .readers
        {
            reader.release.cancel();
            reader.handle.abort();
        }
    }
}
