//! Helpers for WASM bindings.

// lsp_types::Url::from_file_path is not available in WASM bindings, so we need to implement it manually.
// This is a workaround to get the file path as a string.

#[allow(dead_code)]
pub(super) fn from_vfs_file_path(path: &vfs::VfsPath) -> Result<lsp_types::Url, ()> {
    from_file_path(std::path::Path::new(path.as_str()))
}

#[cfg(not(target_arch = "wasm32"))]
pub(super) fn from_file_path(path: &std::path::Path) -> Result<lsp_types::Url, ()> {
    let url = lsp_types::Url::from_file_path(path)?;
    Ok(url)
}

#[cfg(not(target_arch = "wasm32"))]
pub(super) fn to_file_path(url: &lsp_types::Url) -> Result<std::path::PathBuf, ()> {
    url.to_file_path()
}

/// A handle for spawning background async tasks.
///
/// On native: wraps a [`tokio::runtime::Handle`] and uses `handle.spawn()`.
/// On WASM: zero-size sentinel that delegates to `wasm_bindgen_futures::spawn_local()`.
#[derive(Clone)]
pub struct BackgroundSpawner {
    #[cfg(not(target_arch = "wasm32"))]
    handle: tokio::runtime::Handle,
}

impl BackgroundSpawner {
    /// Create a spawner using the current async runtime.
    ///
    /// Native: captures the current tokio handle when one is active, otherwise
    /// falls back to a process-local runtime. This keeps callers that are
    /// constructed from synchronous LSP startup code from panicking.
    /// WASM: zero-size sentinel.
    #[allow(clippy::new_without_default)]
    pub fn new() -> Self {
        #[cfg(not(target_arch = "wasm32"))]
        {
            Self {
                handle: tokio::runtime::Handle::try_current()
                    .unwrap_or_else(|_| fallback_runtime().handle().clone()),
            }
        }
        #[cfg(target_arch = "wasm32")]
        {
            Self {}
        }
    }

    /// Create a spawner with an explicit tokio runtime handle (native only).
    ///
    /// Use this when constructing a spawner outside a runtime context, e.g.
    /// before entering the runtime, by passing `runtime.handle().clone()`.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn with_handle(handle: tokio::runtime::Handle) -> Self {
        Self { handle }
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub fn spawn(&self, f: impl Future<Output = ()> + Send + 'static) {
        drop(self.handle.spawn(f));
    }

    #[cfg(target_arch = "wasm32")]
    pub fn spawn(&self, f: impl Future<Output = ()> + 'static) {
        wasm_bindgen_futures::spawn_local(f);
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn fallback_runtime() -> &'static tokio::runtime::Runtime {
    static RUNTIME: std::sync::OnceLock<tokio::runtime::Runtime> = std::sync::OnceLock::new();
    RUNTIME.get_or_init(|| {
        tokio::runtime::Runtime::new()
            .expect("failed to create background tokio runtime for BAML LSP")
    })
}

#[cfg(target_arch = "wasm32")]
pub(super) fn from_file_path(path: &std::path::Path) -> Result<lsp_types::Url, ()> {
    let path_str = path.to_str().ok_or(())?;
    // Ensure the path starts with '/' so we get a valid file:// URL.
    let url_string = if path_str.starts_with('/') {
        format!("file://{path_str}")
    } else {
        format!("file:///{path_str}")
    };
    lsp_types::Url::parse(&url_string).map_err(|_| ())
}

#[cfg(target_arch = "wasm32")]
pub(super) fn to_file_path(url: &lsp_types::Url) -> Result<std::path::PathBuf, ()> {
    if url.scheme() != "file" {
        return Err(());
    }
    // Url::path() returns the percent-encoded path; decode it for the filesystem path.
    let decoded = percent_encoding::percent_decode_str(url.path())
        .decode_utf8()
        .map_err(|_| ())?;
    Ok(std::path::PathBuf::from(decoded.as_ref()))
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use super::BackgroundSpawner;

    #[test]
    fn background_spawner_new_works_without_current_tokio_runtime() {
        let spawner = BackgroundSpawner::new();
        let (tx, rx) = std::sync::mpsc::channel();

        spawner.spawn(async move {
            tx.send(()).expect("test receiver should still be alive");
        });

        rx.recv_timeout(std::time::Duration::from_secs(2))
            .expect("background task should run on fallback runtime");
    }
}
