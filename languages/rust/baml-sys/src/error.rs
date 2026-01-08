//! Error types for baml-sys library loading.

use std::path::PathBuf;

/// Errors that can occur during library loading.
#[derive(Debug, thiserror::Error)]
#[allow(missing_docs)] // Error variants are self-documenting via #[error(...)]
pub enum BamlSysError {
    /// Library file not found at any search path.
    #[error("BAML library not found. Searched paths: {searched_paths:?}")]
    LibraryNotFound { searched_paths: Vec<PathBuf> },

    /// Failed to load the dynamic library.
    #[error("Failed to load BAML library from {path}: {source}")]
    LoadFailed {
        path: PathBuf,
        #[source]
        source: libloading::Error,
    },

    /// Symbol not found in loaded library.
    #[error("Symbol '{symbol}' not found in BAML library: {source}")]
    SymbolNotFound {
        symbol: &'static str,
        #[source]
        source: libloading::Error,
    },

    /// Version mismatch between Rust package and loaded library.
    #[error("Version mismatch: Rust package expects {expected}, but library reports {actual}")]
    VersionMismatch { expected: String, actual: String },

    /// Platform not supported.
    #[error("Platform not supported: {os}/{arch}")]
    UnsupportedPlatform {
        os: &'static str,
        arch: &'static str,
    },

    /// Failed to determine cache directory.
    #[error("Failed to determine cache directory: {0}")]
    CacheDir(String),

    /// Download failed.
    #[error("Failed to download library: {0}")]
    DownloadFailed(String),

    /// Checksum mismatch after download.
    #[error("Checksum mismatch: expected {expected}, got {actual}")]
    ChecksumMismatch { expected: String, actual: String },

    /// IO error.
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    /// Library already initialized with different path.
    #[error("Library already initialized from {existing_path}, cannot change to {requested_path}")]
    AlreadyInitialized {
        existing_path: PathBuf,
        requested_path: PathBuf,
    },
}

/// Result type for baml-sys operations.
pub type Result<T> = std::result::Result<T, BamlSysError>;
