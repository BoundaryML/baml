//! Engine-owned native body tasks, independent of telemetry and VM ownership.
use std::sync::Mutex;

use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;

#[derive(Default, Debug)]
pub struct HttpReaders(Mutex<Vec<Reader>>);

#[derive(Debug)]
struct Reader {
    release: CancellationToken,
    handle: JoinHandle<()>,
}

impl HttpReaders {
    pub fn register(&self, release: CancellationToken, handle: JoinHandle<()>) {
        let mut readers = self
            .0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        readers.retain(|reader| !reader.handle.is_finished());
        readers.push(Reader { release, handle });
    }

    /// All drains share one deadline. Abort every straggler before joining any
    /// of them, so no task can access recording after shutdown returns.
    #[cfg(not(target_arch = "wasm32"))]
    pub async fn shutdown(&self, budget: std::time::Duration) {
        let mut readers = std::mem::take(
            &mut *self
                .0
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner),
        );
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

impl Drop for HttpReaders {
    fn drop(&mut self) {
        for reader in self
            .0
            .get_mut()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
        {
            reader.handle.abort();
        }
    }
}
