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
    /// Native: captures `tokio::runtime::Handle::current()` — must be called
    /// inside a tokio runtime context, otherwise this panics at runtime.
    /// WASM: zero-size sentinel.
    #[allow(clippy::new_without_default)]
    pub fn new() -> Self {
        #[cfg(not(target_arch = "wasm32"))]
        {
            Self {
                handle: tokio::runtime::Handle::current(),
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
