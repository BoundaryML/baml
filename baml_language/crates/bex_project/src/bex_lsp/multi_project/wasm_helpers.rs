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

#[allow(dead_code)]
pub(super) fn run_async_in_background(f: impl Future<Output = ()> + Send + 'static) {
    #[cfg(not(target_arch = "wasm32"))]
    drop(tokio::spawn(f));
    #[cfg(target_arch = "wasm32")]
    wasm_bindgen_futures::spawn_local(f);
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
