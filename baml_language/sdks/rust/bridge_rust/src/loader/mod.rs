//! Runtime discovery and acquisition of the engine shared library.
//!
//! This is a behavioral port of the Go loader
//! (`engine/language_client_go/baml_go/lib_common.go`), which is the
//! team-designated spec for library loading. Resolution order:
//!
//! 1. an explicit path set via [`set_shared_library_path`] (invalid path
//!    = hard error, no fallback),
//! 2. the `BAML_LIBRARY_PATH` environment variable (same hard-error
//!    semantics),
//! 3. the versioned cache directory (`BAML_CACHE_DIR` override, else the
//!    platform user cache dir + `baml/libs/<version>/`),
//! 4. unless `BAML_LIBRARY_DISABLE_DOWNLOAD=true`: download from the
//!    GitHub release into the cache (see [`download`]),
//! 5. legacy system paths (with a warning),
//! 6. an error listing every attempt.
//!
//! Contract deltas inherited from the Go model, deliberately: the
//! download URL is constructed from version + target rather than resolved
//! from the release manifest, the `.sha256` sidecar comes from the same
//! origin as the artifact, and there is no per-version install lock —
//! concurrent processes race benignly on the atomic rename and at worst
//! re-download. Manifest-resolved URLs with pinned checksums remain
//! available as a later hardening.

mod download;
pub(crate) mod log;

use std::{
    path::{Path, PathBuf},
    sync::Mutex,
};

const GITHUB_REPO: &str = "boundaryml/baml";
const CACHE_DIR_ENV: &str = "BAML_CACHE_DIR";
const LIBRARY_PATH_ENV: &str = "BAML_LIBRARY_PATH";
const DISABLE_DOWNLOAD_ENV: &str = "BAML_LIBRARY_DISABLE_DOWNLOAD";
/// Overrides the release-asset base URL (final URL = `<base>/<filename>`).
/// For hermetic verification against a local server; not a user-facing
/// knob.
const DOWNLOAD_BASE_ENV: &str = "BAML_LIBRARY_DOWNLOAD_BASE";

/// Why the engine library could not be acquired or loaded. Mirrors the Go
/// loader's error taxonomy.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub enum LoaderError {
    /// The library file could not be found or loaded.
    LoadLibrary(String),
    /// This OS/architecture has no prebuilt engine library.
    NotSupportedPlatform(String),
    /// The download from the release could not be completed.
    DownloadFailed(String),
    /// The cache directory could not be determined or created.
    CacheDir(String),
    /// The downloaded artifact did not match its `.sha256` sidecar.
    ChecksumMismatch(String),
    /// The loaded library reports a different version than this crate.
    VersionMismatch(String),
}

impl std::fmt::Display for LoaderError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LoaderError::LoadLibrary(msg) => {
                write!(f, "baml: failed loading shared library: {msg}")
            }
            LoaderError::NotSupportedPlatform(msg) => write!(
                f,
                "baml: platform not supported (only Linux, macOS, and Windows on \
                 x86_64/aarch64): {msg}"
            ),
            LoaderError::DownloadFailed(msg) => {
                write!(f, "baml: failed to download shared library: {msg}")
            }
            LoaderError::CacheDir(msg) => write!(
                f,
                "baml: failed to determine or create cache directory: {msg}"
            ),
            LoaderError::ChecksumMismatch(msg) => {
                write!(f, "baml: downloaded library checksum mismatch: {msg}")
            }
            LoaderError::VersionMismatch(msg) => {
                write!(f, "baml: library version mismatch: {msg}")
            }
        }
    }
}

impl std::error::Error for LoaderError {}

static EXPLICIT_PATH: Mutex<Option<PathBuf>> = Mutex::new(None);

/// Point the loader at an engine library on disk, overriding all
/// discovery (environment, cache, download, system paths).
///
/// Must be called before the first BAML call; once the library is loaded
/// the path is ignored with a warning.
pub fn set_shared_library_path(path: impl Into<PathBuf>) {
    let path = path.into();
    // Hold the lock across the `engine_loaded()` check and the write. The load
    // path snapshots `EXPLICIT_PATH` under the same lock (`from_process`), so
    // this makes the check+write atomic with respect to that snapshot: a
    // concurrent first load cannot clone `None` in between and silently drop
    // the path.
    let mut guard = EXPLICIT_PATH
        .lock()
        .expect("explicit library path lock poisoned");
    if crate::capi::engine_loaded() {
        log::warn(&format!(
            "set_shared_library_path called after the BAML library was initialized; \
             path ignored: {}",
            path.display()
        ));
        return;
    }
    *guard = Some(path);
}

/// Initialize the engine now instead of lazily on the first BAML call —
/// resolving, downloading, loading, and version-checking the library.
/// Useful for eager, fallible startup.
pub fn preload() -> Result<(), crate::SdkError> {
    crate::capi::api().map(|_| ())
}

/// Resolve the engine library exactly as loading would — including
/// downloading it into the cache if it is not present anywhere — without
/// loading it. Lets deploy/CI steps warm the cache ahead of first use.
pub fn ensure_library_cached() -> Result<PathBuf, LoaderError> {
    resolve_library_path(&LoaderEnv::from_process())
}

/// Everything resolution consults, captured up front so the logic is
/// testable without touching process-global state.
pub(crate) struct LoaderEnv {
    pub(crate) explicit_path: Option<PathBuf>,
    pub(crate) env_path: Option<PathBuf>,
    pub(crate) cache_dir_override: Option<PathBuf>,
    /// Platform user cache base (`~/Library/Caches`, `~/.cache`, …);
    /// `None` when undeterminable.
    pub(crate) user_cache_dir: Option<PathBuf>,
    pub(crate) disable_download: bool,
    pub(crate) download_base: Option<String>,
    pub(crate) system_paths: Vec<PathBuf>,
    pub(crate) version: String,
}

impl LoaderEnv {
    pub(crate) fn from_process() -> Self {
        let version = crate::get_version().to_string();
        Self {
            explicit_path: EXPLICIT_PATH
                .lock()
                .expect("explicit library path lock poisoned")
                .clone(),
            env_path: non_empty_env(LIBRARY_PATH_ENV).map(PathBuf::from),
            cache_dir_override: non_empty_env(CACHE_DIR_ENV).map(PathBuf::from),
            user_cache_dir: platform_user_cache_dir(),
            disable_download: std::env::var(DISABLE_DOWNLOAD_ENV)
                .is_ok_and(|v| v.eq_ignore_ascii_case("true")),
            download_base: non_empty_env(DOWNLOAD_BASE_ENV)
                .map(|v| v.to_string_lossy().into_owned()),
            system_paths: default_system_paths(&version),
            version,
        }
    }

    fn download_base_url(&self) -> String {
        self.download_base.clone().unwrap_or_else(|| {
            format!(
                "https://github.com/{GITHUB_REPO}/releases/download/baml-language-{}",
                self.version
            )
        })
    }
}

fn non_empty_env(name: &str) -> Option<std::ffi::OsString> {
    std::env::var_os(name).filter(|v| !v.is_empty())
}

pub(crate) fn resolve_library_path(env: &LoaderEnv) -> Result<PathBuf, LoaderError> {
    if !is_supported_platform() {
        return Err(LoaderError::NotSupportedPlatform(format!(
            "OS={} Arch={}",
            std::env::consts::OS,
            std::env::consts::ARCH
        )));
    }

    if let Some(path) = &env.explicit_path {
        return match std::fs::metadata(path) {
            Ok(_) => {
                log::debug(&format!(
                    "Using BAML library path set via set_shared_library_path(): {}",
                    path.display()
                ));
                Ok(path.clone())
            }
            Err(e) => Err(LoaderError::LoadLibrary(format!(
                "path explicitly set via set_shared_library_path() {} is invalid: {e}",
                path.display()
            ))),
        };
    }

    if let Some(path) = &env.env_path {
        return match std::fs::metadata(path) {
            Ok(_) => {
                log::debug(&format!(
                    "Using BAML library path from {LIBRARY_PATH_ENV}: {}",
                    path.display()
                ));
                Ok(path.clone())
            }
            Err(e) => Err(LoaderError::LoadLibrary(format!(
                "path from environment variable {LIBRARY_PATH_ENV} ({}) is invalid: {e}",
                path.display()
            ))),
        };
    }

    let cache_dir = cache_dir(env)?;
    let filename = target_lib_filename()?;
    let cached_path = cache_dir.join(&filename);
    log::debug(&format!(
        "Checking for cached BAML library at {}",
        cached_path.display()
    ));
    if std::fs::metadata(&cached_path).is_ok() {
        log::info(&format!(
            "Found cached BAML library at {}",
            cached_path.display()
        ));
        return Ok(cached_path);
    }
    log::debug("Library not found in cache");

    let mut download_status = "Attempted but failed";
    if env.disable_download {
        log::warn(&format!(
            "Automatic download disabled via {DISABLE_DOWNLOAD_ENV}"
        ));
        download_status = "Disabled";
    } else {
        log::debug(&format!(
            "Attempting to download BAML library v{} for {}/{}",
            env.version,
            std::env::consts::OS,
            std::env::consts::ARCH
        ));
        match download::download_library(env, &cache_dir, &filename) {
            Ok(()) => return Ok(cached_path),
            Err(e) => log::warn(&format!("BAML library download failed: {e}")),
        }
    }

    log::debug("Checking default system library paths");
    for path in &env.system_paths {
        if std::fs::metadata(path).is_ok() {
            log::warn(&format!(
                "Found BAML library in a default system path: {}. This might lead to \
                 version/architecture mismatches; consider the cache or {LIBRARY_PATH_ENV}.",
                path.display()
            ));
            return Ok(path.clone());
        }
    }

    // The explicit/env sources are provably unset here: when either is
    // set, resolution ends above (successfully or with a hard error).
    Err(LoaderError::LoadLibrary(format!(
        "could not find BAML library v{} for {}/{}.\n       Resolution attempts failed:\n       \
         - Explicit path (set_shared_library_path): not set\n       \
         - Environment var ({LIBRARY_PATH_ENV}): not set\n       \
         - Cache path: {} (not found)\n       \
         - Download ({DISABLE_DOWNLOAD_ENV}): {download_status}\n       \
         - Default system paths: {:?} (not found)",
        env.version,
        std::env::consts::OS,
        std::env::consts::ARCH,
        cached_path.display(),
        env.system_paths
    )))
}

fn is_supported_platform() -> bool {
    cfg!(any(
        target_os = "linux",
        target_os = "macos",
        target_os = "windows"
    )) && cfg!(any(target_arch = "x86_64", target_arch = "aarch64"))
}

/// The release-asset filename for the compile-time target, matching the
/// dylib build matrix's naming: `libbaml_cffi-<triple>.{so,dylib}` on
/// Unix, `baml_cffi-<triple>.dll` on Windows.
pub(crate) fn target_lib_filename() -> Result<String, LoaderError> {
    let arch = if cfg!(target_arch = "x86_64") {
        "x86_64"
    } else if cfg!(target_arch = "aarch64") {
        "aarch64"
    } else {
        return Err(LoaderError::NotSupportedPlatform(format!(
            "unsupported architecture {}",
            std::env::consts::ARCH
        )));
    };
    let (prefix, triple_rest, ext) = if cfg!(target_os = "windows") {
        ("baml_cffi", "pc-windows-msvc", "dll")
    } else if cfg!(target_os = "macos") {
        ("libbaml_cffi", "apple-darwin", "dylib")
    } else if cfg!(target_os = "linux") {
        if cfg!(target_env = "musl") {
            ("libbaml_cffi", "unknown-linux-musl", "so")
        } else {
            ("libbaml_cffi", "unknown-linux-gnu", "so")
        }
    } else {
        return Err(LoaderError::NotSupportedPlatform(format!(
            "unsupported OS {}",
            std::env::consts::OS
        )));
    };
    Ok(format!("{prefix}-{arch}-{triple_rest}.{ext}"))
}

fn cache_dir(env: &LoaderEnv) -> Result<PathBuf, LoaderError> {
    let (dir, source) = match &env.cache_dir_override {
        Some(dir) => (dir.clone(), "environment variable BAML_CACHE_DIR"),
        None => match &env.user_cache_dir {
            Some(base) => (
                base.join("baml").join("libs").join(&env.version),
                "default user cache location",
            ),
            None => {
                return Err(LoaderError::CacheDir(
                    "could not determine the user cache directory \
                     (HOME/XDG_CACHE_HOME/LOCALAPPDATA not set)"
                        .to_string(),
                ));
            }
        },
    };
    log::debug(&format!(
        "Using cache directory from {source}: {}",
        dir.display()
    ));
    std::fs::create_dir_all(&dir).map_err(|e| {
        LoaderError::CacheDir(format!(
            "failed to create cache directory {}: {e}",
            dir.display()
        ))
    })?;
    Ok(dir)
}

/// The platform user cache base, matching Go's `os.UserCacheDir`.
fn platform_user_cache_dir() -> Option<PathBuf> {
    if cfg!(target_os = "macos") {
        non_empty_env("HOME").map(|home| PathBuf::from(home).join("Library").join("Caches"))
    } else if cfg!(target_os = "windows") {
        non_empty_env("LOCALAPPDATA").map(PathBuf::from)
    } else {
        non_empty_env("XDG_CACHE_HOME")
            .map(PathBuf::from)
            .or_else(|| non_empty_env("HOME").map(|home| PathBuf::from(home).join(".cache")))
    }
}

/// Legacy last-resort install locations (checked with a warning).
fn default_system_paths(version: &str) -> Vec<PathBuf> {
    if cfg!(target_os = "windows") {
        let mut paths = Vec::new();
        for base in ["ProgramFiles", "LOCALAPPDATA"] {
            if let Some(base) = non_empty_env(base) {
                let dir = Path::new(&base).join("baml");
                paths.push(dir.join(format!("baml_cffi-{version}.dll")));
                paths.push(dir.join("baml_cffi.dll"));
            }
        }
        paths
    } else if cfg!(target_os = "macos") {
        vec![
            PathBuf::from(format!("/usr/local/lib/libbaml-{version}.dylib")),
            PathBuf::from("/usr/local/lib/libbaml.dylib"),
        ]
    } else {
        vec![
            PathBuf::from(format!("/usr/local/lib/libbaml-{version}.so")),
            PathBuf::from("/usr/local/lib/libbaml.so"),
        ]
    }
}

#[cfg(test)]
mod tests {
    use std::io::{Read, Write};

    use super::*;

    /// A unique, self-removing temp directory (no `tempfile` dep).
    struct TempDir(PathBuf);

    impl TempDir {
        fn new() -> Self {
            static COUNTER: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
            let n = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            let dir = std::env::temp_dir().join(format!(
                "baml_bridge_loader_test_{}_{n}",
                std::process::id()
            ));
            std::fs::create_dir_all(&dir).expect("create test temp dir");
            Self(dir)
        }

        fn path(&self) -> &Path {
            &self.0
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn test_env(cache_dir: &Path) -> LoaderEnv {
        LoaderEnv {
            explicit_path: None,
            env_path: None,
            cache_dir_override: Some(cache_dir.to_path_buf()),
            user_cache_dir: None,
            disable_download: true,
            download_base: None,
            system_paths: Vec::new(),
            version: "0.0.0-test".to_string(),
        }
    }

    #[test]
    fn explicit_path_wins_when_it_exists() {
        let dir = TempDir::new();
        let lib = dir.path().join("engine.dylib");
        std::fs::write(&lib, b"x").unwrap();
        let env = LoaderEnv {
            explicit_path: Some(lib.clone()),
            ..test_env(dir.path())
        };
        assert_eq!(resolve_library_path(&env).unwrap(), lib);
    }

    #[test]
    fn invalid_explicit_path_is_a_hard_error() {
        let dir = TempDir::new();
        let env = LoaderEnv {
            explicit_path: Some(dir.path().join("missing.dylib")),
            // A valid cached library must NOT rescue an invalid explicit path.
            ..cache_env_with_library(&dir)
        };
        let err = resolve_library_path(&env).unwrap_err();
        assert!(matches!(&err, LoaderError::LoadLibrary(m) if m.contains("explicitly set")));
    }

    #[test]
    fn env_path_wins_when_it_exists() {
        let dir = TempDir::new();
        let lib = dir.path().join("engine.so");
        std::fs::write(&lib, b"x").unwrap();
        let env = LoaderEnv {
            env_path: Some(lib.clone()),
            ..test_env(dir.path())
        };
        assert_eq!(resolve_library_path(&env).unwrap(), lib);
    }

    #[test]
    fn invalid_env_path_is_a_hard_error() {
        let dir = TempDir::new();
        let env = LoaderEnv {
            env_path: Some(dir.path().join("missing.so")),
            ..cache_env_with_library(&dir)
        };
        let err = resolve_library_path(&env).unwrap_err();
        assert!(matches!(&err, LoaderError::LoadLibrary(m) if m.contains(LIBRARY_PATH_ENV)));
    }

    /// A `test_env` whose cache already holds the target library.
    fn cache_env_with_library(dir: &TempDir) -> LoaderEnv {
        let env = test_env(dir.path());
        let filename = target_lib_filename().unwrap();
        std::fs::write(dir.path().join(&filename), b"cached").unwrap();
        env
    }

    #[test]
    fn cache_hit_resolves_to_the_cached_file() {
        let dir = TempDir::new();
        let env = cache_env_with_library(&dir);
        let resolved = resolve_library_path(&env).unwrap();
        assert_eq!(resolved, dir.path().join(target_lib_filename().unwrap()));
    }

    #[test]
    fn default_cache_location_is_under_the_user_cache_dir() {
        let dir = TempDir::new();
        let filename = target_lib_filename().unwrap();
        let versioned = dir.path().join("baml").join("libs").join("0.0.0-test");
        std::fs::create_dir_all(&versioned).unwrap();
        std::fs::write(versioned.join(&filename), b"cached").unwrap();
        let env = LoaderEnv {
            cache_dir_override: None,
            user_cache_dir: Some(dir.path().to_path_buf()),
            ..test_env(dir.path())
        };
        assert_eq!(
            resolve_library_path(&env).unwrap(),
            versioned.join(&filename)
        );
    }

    #[test]
    fn cache_dir_is_created_when_missing() {
        let dir = TempDir::new();
        let nested = dir.path().join("deep").join("cache");
        let env = test_env(&nested);
        // Resolution fails (nothing to find), but the cache dir must exist.
        let _ = resolve_library_path(&env).unwrap_err();
        assert!(nested.is_dir());
    }

    #[test]
    fn system_path_is_used_as_a_last_resort() {
        let dir = TempDir::new();
        let system_lib = dir.path().join("libbaml.dylib");
        std::fs::write(&system_lib, b"system").unwrap();
        let env = LoaderEnv {
            system_paths: vec![dir.path().join("missing.dylib"), system_lib.clone()],
            ..test_env(dir.path())
        };
        assert_eq!(resolve_library_path(&env).unwrap(), system_lib);
    }

    #[test]
    fn full_miss_error_lists_every_attempt() {
        let dir = TempDir::new();
        let env = LoaderEnv {
            system_paths: vec![PathBuf::from("/nonexistent/libbaml.so")],
            ..test_env(dir.path())
        };
        let err = resolve_library_path(&env).unwrap_err();
        let msg = err.to_string();
        assert!(matches!(err, LoaderError::LoadLibrary(_)));
        for expected in [
            "set_shared_library_path",
            LIBRARY_PATH_ENV,
            DISABLE_DOWNLOAD_ENV,
            "Disabled",
            "/nonexistent/libbaml.so",
        ] {
            assert!(msg.contains(expected), "missing {expected:?} in: {msg}");
        }
        assert!(msg.contains(&target_lib_filename().unwrap()), "{msg}");
    }

    #[test]
    fn target_filename_matches_the_current_platform() {
        let filename = target_lib_filename().unwrap();
        if cfg!(target_os = "macos") {
            assert!(filename.starts_with("libbaml_cffi-"), "{filename}");
            assert!(filename.ends_with("-apple-darwin.dylib"), "{filename}");
        } else if cfg!(target_os = "linux") {
            let libc = if cfg!(target_env = "musl") {
                "musl"
            } else {
                "gnu"
            };
            assert!(filename.starts_with("libbaml_cffi-"), "{filename}");
            assert!(
                filename.ends_with(&format!("-unknown-linux-{libc}.so")),
                "{filename}"
            );
        } else if cfg!(target_os = "windows") {
            assert!(filename.starts_with("baml_cffi-"), "{filename}");
            assert!(filename.ends_with("-pc-windows-msvc.dll"), "{filename}");
        }
    }

    // ---- hermetic download tests (local HTTP server, no network) ----

    /// Serve canned responses on an ephemeral local port. Each response is
    /// `Connection: close`, so every request arrives on a new connection;
    /// the server thread exits after `expected_requests`.
    fn serve(
        routes: Vec<(String, u16, Vec<u8>)>,
        expected_requests: usize,
    ) -> (String, std::thread::JoinHandle<Vec<String>>) {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind test server");
        let base = format!("http://{}", listener.local_addr().unwrap());
        let handle = std::thread::spawn(move || {
            let mut seen = Vec::new();
            for _ in 0..expected_requests {
                let (mut stream, _) = listener.accept().expect("accept");
                let mut request = Vec::new();
                let mut byte = [0u8; 1];
                while !request.ends_with(b"\r\n\r\n") && stream.read(&mut byte).unwrap_or(0) == 1 {
                    request.push(byte[0]);
                }
                let request = String::from_utf8_lossy(&request);
                let path = request
                    .split_whitespace()
                    .nth(1)
                    .unwrap_or_default()
                    .to_string();
                let (status, body) = routes
                    .iter()
                    .find(|(route, _, _)| *route == path)
                    .map_or((404, Vec::new()), |(_, status, body)| {
                        (*status, body.clone())
                    });
                let reason = if status == 200 { "OK" } else { "Not Found" };
                let _ = stream.write_all(
                    format!(
                        "HTTP/1.1 {status} {reason}\r\nContent-Length: {}\r\n\
                         Connection: close\r\n\r\n",
                        body.len()
                    )
                    .as_bytes(),
                );
                let _ = stream.write_all(&body);
                seen.push(path);
            }
            seen
        });
        (base, handle)
    }

    fn sha256_hex(data: &[u8]) -> String {
        use sha2::Digest as _;
        hex::encode(sha2::Sha256::digest(data))
    }

    #[test]
    fn download_installs_a_checksum_verified_library() {
        let dir = TempDir::new();
        let filename = target_lib_filename().unwrap();
        let artifact = b"fake engine bytes".to_vec();
        let sidecar = format!("{}  {filename}\n", sha256_hex(&artifact));
        let (base, server) = serve(
            vec![
                (format!("/{filename}"), 200, artifact.clone()),
                (format!("/{filename}.sha256"), 200, sidecar.into_bytes()),
            ],
            2,
        );
        let env = LoaderEnv {
            disable_download: false,
            download_base: Some(base),
            ..test_env(dir.path())
        };
        let resolved = resolve_library_path(&env).unwrap();
        assert_eq!(resolved, dir.path().join(&filename));
        assert_eq!(std::fs::read(&resolved).unwrap(), artifact);
        // The temp download file is gone.
        let leftovers: Vec<_> = std::fs::read_dir(dir.path())
            .unwrap()
            .filter_map(Result::ok)
            .filter(|e| e.file_name().to_string_lossy().ends_with(".tmpdl"))
            .collect();
        assert!(leftovers.is_empty(), "{leftovers:?}");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(&resolved).unwrap().permissions().mode();
            assert_eq!(mode & 0o777, 0o755, "mode {mode:o}");
        }
        server.join().unwrap();
    }

    #[test]
    fn checksum_mismatch_rejects_the_download() {
        let dir = TempDir::new();
        let filename = target_lib_filename().unwrap();
        let sidecar = format!("{}  {filename}\n", "0".repeat(64));
        let (base, _server) = serve(
            vec![
                (format!("/{filename}"), 200, b"corrupted bytes".to_vec()),
                (format!("/{filename}.sha256"), 200, sidecar.into_bytes()),
            ],
            2,
        );
        let env = LoaderEnv {
            disable_download: false,
            download_base: Some(base),
            ..test_env(dir.path())
        };
        let err = download::download_library(&env, dir.path(), &filename).unwrap_err();
        assert!(matches!(err, LoaderError::ChecksumMismatch(_)), "{err}");
        assert!(!dir.path().join(&filename).exists());
        // No temp file may survive the rejection either.
        let leftovers: Vec<_> = std::fs::read_dir(dir.path())
            .unwrap()
            .filter_map(Result::ok)
            .filter(|e| e.file_name().to_string_lossy().ends_with(".tmpdl"))
            .collect();
        assert!(leftovers.is_empty(), "{leftovers:?}");
    }

    #[test]
    fn missing_sidecar_downloads_unverified() {
        let dir = TempDir::new();
        let filename = target_lib_filename().unwrap();
        let artifact = b"engine without sidecar".to_vec();
        let (base, _server) = serve(vec![(format!("/{filename}"), 200, artifact.clone())], 2);
        let env = LoaderEnv {
            disable_download: false,
            download_base: Some(base),
            ..test_env(dir.path())
        };
        let resolved = resolve_library_path(&env).unwrap();
        assert_eq!(std::fs::read(resolved).unwrap(), artifact);
    }

    #[test]
    fn missing_artifact_fails_the_download_step() {
        let dir = TempDir::new();
        let filename = target_lib_filename().unwrap();
        let (base, _server) = serve(Vec::new(), 2);
        let env = LoaderEnv {
            disable_download: false,
            download_base: Some(base),
            ..test_env(dir.path())
        };
        // 404 on the artifact → download step fails → full-miss error.
        let err = resolve_library_path(&env).unwrap_err();
        assert!(matches!(err, LoaderError::LoadLibrary(_)));
        assert!(!dir.path().join(&filename).exists());
    }
}
