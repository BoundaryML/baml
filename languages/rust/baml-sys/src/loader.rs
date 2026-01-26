//! Library resolution and loading.

// Allow stderr output for download warning messages.
#![allow(clippy::print_stderr)]

use std::{
    path::{Path, PathBuf},
    sync::Mutex,
};

use libloading::Library;
use once_cell::sync::OnceCell;

use crate::{
    download::download_library,
    error::{BamlSysError, Result},
};

/// Package version from Cargo.toml (workspace).
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// GitHub repository for releases.
const GITHUB_REPO: &str = "boundaryml/baml";

/// Environment variable for explicit library path.
pub const ENV_LIBRARY_PATH: &str = "BAML_LIBRARY_PATH";

/// Environment variable for cache directory override.
pub const ENV_CACHE_DIR: &str = "BAML_CACHE_DIR";

/// Environment variable to disable automatic download.
pub const ENV_DISABLE_DOWNLOAD: &str = "BAML_LIBRARY_DISABLE_DOWNLOAD";

/// Global library instance.
static LIBRARY: OnceCell<LoadedLibrary> = OnceCell::new();

/// Mutex for explicit path setting (before initialization).
static EXPLICIT_PATH: Mutex<Option<PathBuf>> = Mutex::new(None);

/// A loaded dynamic library with its path.
pub(crate) struct LoadedLibrary {
    pub(crate) library: Library,
    pub(crate) path: PathBuf,
}

// Safety: libloading::Library is Send + Sync when the underlying
// library's functions are thread-safe (which baml_cffi is).
#[allow(unsafe_code)]
unsafe impl Send for LoadedLibrary {}
#[allow(unsafe_code)]
unsafe impl Sync for LoadedLibrary {}

/// Set an explicit library path before initialization.
///
/// Must be called before any FFI functions are used.
/// Returns an error if the library is already loaded.
pub fn set_library_path(path: impl AsRef<Path>) -> Result<()> {
    let path = path.as_ref().to_path_buf();

    if LIBRARY.get().is_some() {
        let existing = LIBRARY.get().unwrap();
        return Err(BamlSysError::AlreadyInitialized {
            existing_path: existing.path.clone(),
            requested_path: path,
        });
    }

    let mut explicit = EXPLICIT_PATH.lock().unwrap();
    *explicit = Some(path);
    Ok(())
}

/// Ensure the library is available (for use in build.rs).
///
/// This function will:
/// 1. Check if library exists at configured/default paths
/// 2. Download if necessary and enabled
/// 3. Return the path to the library
///
/// Does NOT load the library - that happens at runtime.
pub fn ensure_library() -> Result<PathBuf> {
    find_or_download_library()
}

/// Get the loaded library, initializing if necessary.
pub(crate) fn get_library() -> Result<&'static LoadedLibrary> {
    LIBRARY.get_or_try_init(|| {
        let path = find_or_download_library()?;
        load_library(&path)
    })
}

/// Find or download the library, returning its path.
fn find_or_download_library() -> Result<PathBuf> {
    let mut searched_paths = Vec::new();

    // 1. Check explicit path set via API
    {
        let explicit = EXPLICIT_PATH.lock().unwrap();
        if let Some(path) = explicit.as_ref() {
            if path.exists() {
                return Ok(path.clone());
            }
            searched_paths.push(path.clone());
        }
    }

    // 2. Check environment variable
    if let Ok(env_path) = std::env::var(ENV_LIBRARY_PATH) {
        // Env vars can be wrapped in quotes and spaces, so we need to unwrap them
        let env_path = env_path.trim().trim_matches('"').trim();
        let path = PathBuf::from(&env_path);
        if path.exists() {
            return Ok(path);
        }
        searched_paths.push(path);
    }

    // 3. Check cache directory
    let cache_dir = get_cache_dir()?;
    let lib_filename = get_library_filename()?;
    let cached_path = cache_dir.join(&lib_filename);

    if cached_path.exists() {
        return Ok(cached_path);
    }
    searched_paths.push(cached_path.clone());

    // 4. Try to download (if enabled)
    #[cfg(feature = "download")]
    if std::env::var(ENV_DISABLE_DOWNLOAD).map(|v| v.to_lowercase()) != Ok("true".to_string()) {
        match download_library(&cache_dir, &lib_filename, VERSION, GITHUB_REPO) {
            Ok(()) => return Ok(cached_path),
            Err(e) => {
                // Log warning but continue to system paths
                eprintln!("Warning: Failed to download BAML library: {e}");
            }
        }
    }

    // 5. Check system default paths
    for path in get_system_paths(&lib_filename) {
        if path.exists() {
            return Ok(path);
        }
        searched_paths.push(path);
    }

    Err(BamlSysError::LibraryNotFound { searched_paths })
}

/// Load the library from a path.
fn load_library(path: &Path) -> Result<LoadedLibrary> {
    // Safety: We're loading a dynamic library. The library must be
    // compatible with our expected ABI.
    #[allow(unsafe_code)]
    let library = unsafe { Library::new(path) }.map_err(|e| BamlSysError::LoadFailed {
        path: path.to_path_buf(),
        source: e,
    })?;

    Ok(LoadedLibrary {
        library,
        path: path.to_path_buf(),
    })
}

/// Get the cache directory for libraries.
fn get_cache_dir() -> Result<PathBuf> {
    // Check environment variable override
    if let Ok(cache_dir) = std::env::var(ENV_CACHE_DIR) {
        let path = PathBuf::from(cache_dir);
        std::fs::create_dir_all(&path)?;
        return Ok(path);
    }

    // Use platform-specific user cache directory
    let base = dirs_cache_dir().ok_or_else(|| {
        BamlSysError::CacheDir("Could not determine user cache directory".to_string())
    })?;

    // Structure: {cache}/baml/libs/{VERSION}/
    let cache_dir = base.join("baml").join("libs").join(VERSION);
    std::fs::create_dir_all(&cache_dir)?;

    Ok(cache_dir)
}

/// Get platform-specific cache directory.
fn dirs_cache_dir() -> Option<PathBuf> {
    #[cfg(target_os = "macos")]
    {
        std::env::var_os("HOME").map(|h| PathBuf::from(h).join("Library/Caches"))
    }

    #[cfg(target_os = "linux")]
    {
        std::env::var_os("XDG_CACHE_HOME")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".cache")))
    }

    #[cfg(target_os = "windows")]
    {
        std::env::var_os("LOCALAPPDATA").map(PathBuf::from)
    }

    #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
    {
        None
    }
}

/// Get the library filename for the current platform.
fn get_library_filename() -> Result<String> {
    let (prefix, ext, target_triple) = get_platform_info()?;
    Ok(format!("{prefix}baml_cffi-{target_triple}.{ext}"))
}

/// Get platform-specific library info: (prefix, extension, `target_triple`).
#[allow(clippy::unnecessary_wraps)] // Result needed for unsupported platform cfg fallback
fn get_platform_info() -> Result<(&'static str, &'static str, &'static str)> {
    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    return Ok(("lib", "dylib", "aarch64-apple-darwin"));

    #[cfg(all(target_os = "macos", target_arch = "x86_64"))]
    return Ok(("lib", "dylib", "x86_64-apple-darwin"));

    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
    return Ok(("lib", "so", "x86_64-unknown-linux-gnu"));

    #[cfg(all(target_os = "linux", target_arch = "aarch64"))]
    return Ok(("lib", "so", "aarch64-unknown-linux-gnu"));

    #[cfg(all(target_os = "windows", target_arch = "x86_64"))]
    return Ok(("", "dll", "x86_64-pc-windows-msvc"));

    #[cfg(all(target_os = "windows", target_arch = "aarch64"))]
    return Ok(("", "dll", "aarch64-pc-windows-msvc"));

    #[cfg(not(any(
        all(target_os = "macos", target_arch = "aarch64"),
        all(target_os = "macos", target_arch = "x86_64"),
        all(target_os = "linux", target_arch = "x86_64"),
        all(target_os = "linux", target_arch = "aarch64"),
        all(target_os = "windows", target_arch = "x86_64"),
        all(target_os = "windows", target_arch = "aarch64"),
    )))]
    Err(BamlSysError::UnsupportedPlatform {
        os: std::env::consts::OS,
        arch: std::env::consts::ARCH,
    })
}

/// Get system default library paths.
fn get_system_paths(lib_filename: &str) -> Vec<PathBuf> {
    let mut paths = Vec::new();

    #[cfg(target_os = "macos")]
    {
        paths.push(PathBuf::from("/usr/local/lib").join(lib_filename));
        paths.push(PathBuf::from("/usr/local/lib/libbaml_cffi.dylib"));
    }

    #[cfg(target_os = "linux")]
    {
        paths.push(PathBuf::from("/usr/local/lib").join(lib_filename));
        paths.push(PathBuf::from("/usr/local/lib/libbaml_cffi.so"));
        paths.push(PathBuf::from("/usr/lib").join(lib_filename));
    }

    #[cfg(target_os = "windows")]
    {
        if let Ok(program_files) = std::env::var("ProgramFiles") {
            paths.push(
                PathBuf::from(&program_files)
                    .join("baml")
                    .join(lib_filename),
            );
            paths.push(
                PathBuf::from(&program_files)
                    .join("baml")
                    .join("baml_cffi.dll"),
            );
        }
        if let Ok(local_app_data) = std::env::var("LOCALAPPDATA") {
            paths.push(
                PathBuf::from(&local_app_data)
                    .join("baml")
                    .join(lib_filename),
            );
            paths.push(
                PathBuf::from(&local_app_data)
                    .join("baml")
                    .join("baml_cffi.dll"),
            );
        }
    }

    paths
}
