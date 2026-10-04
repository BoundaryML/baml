//! Runtime discovery and acquisition of the engine shared library.
//!
//! Resolution order:
//!
//! 1. an explicit path set via [`set_shared_library_path`] (invalid path
//!    = hard error, no fallback),
//! 2. the `BAML_BRIDGE_PATH` environment variable (an absolute path; same
//!    hard-error semantics, and no network),
//! 3. the versioned cache, `$BAML_HOME/bridges/<version>/<target>/<lib>`
//!    (`BAML_HOME` defaults to `~/.baml`; there is no other cache
//!    location),
//! 4. unless `BAML_BRIDGE_DISABLE_DOWNLOAD` is true: download from the
//!    release manifest (`BAML_MANIFEST_BASE_URL`, default
//!    `https://pkg.boundaryml.com/manifest/v1`) into the cache, verifying
//!    the manifest's sha256 (see [`download`]),
//! 5. an error listing every attempt.
//!
//! A bridge always uses the version compiled into it. The cache is
//! only ever populated by a checksum-verified atomic install, so a file
//! found there is trusted. Concurrent processes race benignly on the
//! atomic rename and at worst re-download.
//!
//! `DEV_BAML_BRIDGE_SKIP_VERSION_CHECK` (dev only, requires
//! `BAML_BRIDGE_PATH`) skips the match between the loaded library's
//! version and this crate's toolchain version; the ABI table check always
//! applies.

mod download;
pub(crate) mod log;

use std::{
    ffi::OsString,
    path::{Path, PathBuf},
    sync::Mutex,
};

const LIBRARY_PATH_ENV: &str = "BAML_BRIDGE_PATH";
const DISABLE_DOWNLOAD_ENV: &str = "BAML_BRIDGE_DISABLE_DOWNLOAD";
const MANIFEST_BASE_URL_ENV: &str = "BAML_MANIFEST_BASE_URL";
const SKIP_VERSION_CHECK_ENV: &str = "DEV_BAML_BRIDGE_SKIP_VERSION_CHECK";
const DEFAULT_MANIFEST_BASE_URL: &str = "https://pkg.boundaryml.com/manifest/v1";

/// Why the engine library could not be acquired or loaded. Mirrors the Go
/// loader's error taxonomy.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub enum LoaderError {
    /// The library file could not be found or loaded.
    LoadLibrary(String),
    /// This OS/architecture has no prebuilt engine library.
    NotSupportedPlatform(String),
    /// The download from the release manifest could not be completed.
    DownloadFailed(String),
    /// The cache directory could not be determined or created.
    CacheDir(String),
    /// The downloaded artifact did not match the manifest's sha256.
    ChecksumMismatch(String),
    /// A loader environment variable held an invalid value.
    Config(String),
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
            LoaderError::Config(msg) => write!(f, "baml: invalid loader configuration: {msg}"),
            LoaderError::VersionMismatch(msg) => {
                write!(f, "baml: library version mismatch: {msg}")
            }
        }
    }
}

impl std::error::Error for LoaderError {}

static EXPLICIT_PATH: Mutex<Option<PathBuf>> = Mutex::new(None);

/// Point the loader at an engine library on disk, overriding all
/// discovery (environment, cache, download).
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
    resolve_library_path(&LoaderEnv::from_process()?)
}

/// Everything resolution consults, captured up front so the logic is
/// testable without touching process-global state.
pub(crate) struct LoaderEnv {
    pub(crate) explicit_path: Option<PathBuf>,
    pub(crate) env_path: Option<PathBuf>,
    /// The root of all BAML state (`BAML_HOME`, default `~/.baml`).
    pub(crate) baml_home: PathBuf,
    pub(crate) disable_download: bool,
    pub(crate) skip_version_check: bool,
    /// Manifest base URL without a trailing slash.
    pub(crate) manifest_base_url: String,
    pub(crate) version: String,
}

impl LoaderEnv {
    pub(crate) fn from_process() -> Result<Self, LoaderError> {
        let explicit_path = EXPLICIT_PATH
            .lock()
            .expect("explicit library path lock poisoned")
            .clone();
        Self::from_lookup(explicit_path, &|name| std::env::var_os(name))
    }

    /// Build from an arbitrary variable source; `from_process` passes the
    /// real environment.
    pub(crate) fn from_lookup(
        explicit_path: Option<PathBuf>,
        lookup: &dyn Fn(&str) -> Option<OsString>,
    ) -> Result<Self, LoaderError> {
        let string_var = |name: &str| -> Result<Option<String>, LoaderError> {
            let Some(raw) = lookup(name) else {
                return Ok(None);
            };
            let value = raw
                .into_string()
                .map_err(|_| LoaderError::Config(format!("{name} is not valid unicode")))?;
            let value = value.trim();
            Ok((!value.is_empty()).then(|| value.to_string()))
        };
        let bool_var = |name: &str| -> Result<bool, LoaderError> {
            let Some(value) = string_var(name)? else {
                return Ok(false);
            };
            match value.to_ascii_lowercase().as_str() {
                "1" | "true" | "yes" | "on" => Ok(true),
                "0" | "false" | "no" | "off" => Ok(false),
                _ => Err(LoaderError::Config(format!(
                    "{name} must be a boolean (1/true/yes/on or 0/false/no/off), got {value:?}"
                ))),
            }
        };

        let env_path = match string_var(LIBRARY_PATH_ENV)? {
            Some(value) if !Path::new(&value).is_absolute() => {
                return Err(LoaderError::Config(format!(
                    "{LIBRARY_PATH_ENV} must be an absolute path, got {value:?}"
                )));
            }
            value => value.map(PathBuf::from),
        };
        let skip_version_check = bool_var(SKIP_VERSION_CHECK_ENV)?;
        if skip_version_check && env_path.is_none() && explicit_path.is_none() {
            return Err(LoaderError::Config(format!(
                "{SKIP_VERSION_CHECK_ENV} requires {LIBRARY_PATH_ENV}"
            )));
        }
        let manifest_base_url = string_var(MANIFEST_BASE_URL_ENV)?
            .unwrap_or_else(|| DEFAULT_MANIFEST_BASE_URL.to_string())
            .trim_end_matches('/')
            .to_string();
        let home_var = if cfg!(target_os = "windows") {
            "USERPROFILE"
        } else {
            "HOME"
        };
        // Same rule as `baml_release::baml_home`: `BAML_HOME` when non-empty,
        // else `<home>/.baml`, else a relative `.baml`. Deliberately duplicated
        // (as is the env parsing in this module): this crate is published to
        // crates.io and cannot depend on the workspace-only `baml_release` or
        // `baml_env`. Keep in sync with those and the Go and C++ copies.
        let baml_home = string_var("BAML_HOME")?
            .map(PathBuf::from)
            .or_else(|| {
                string_var(home_var)
                    .ok()
                    .flatten()
                    .map(|h| Path::new(&h).join(".baml"))
            })
            .unwrap_or_else(|| PathBuf::from(".baml"));
        Ok(Self {
            explicit_path,
            env_path,
            baml_home,
            disable_download: bool_var(DISABLE_DOWNLOAD_ENV)?,
            skip_version_check,
            manifest_base_url,
            version: crate::get_version().to_string(),
        })
    }

    /// `$BAML_HOME/bridges/<version>`.
    pub(crate) fn version_dir(&self) -> PathBuf {
        self.baml_home.join("bridges").join(&self.version)
    }
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

    let target = target_triple()?;
    let cached_path = env.version_dir().join(target).join(lib_filename());
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

    if env.disable_download {
        log::warn(&format!(
            "Automatic download disabled via {DISABLE_DOWNLOAD_ENV}"
        ));
        // The explicit/env sources are provably unset here: when either is
        // set, resolution ends above (successfully or with a hard error).
        return Err(LoaderError::LoadLibrary(format!(
            "could not find BAML library v{} for {}/{}.\n       Resolution attempts failed:\n       \
             - Explicit path (set_shared_library_path): not set\n       \
             - Environment var ({LIBRARY_PATH_ENV}): not set\n       \
             - Cache path: {} (not found)\n       \
             - Download ({DISABLE_DOWNLOAD_ENV}): Disabled",
            env.version,
            std::env::consts::OS,
            std::env::consts::ARCH,
            cached_path.display(),
        )));
    }

    log::debug(&format!(
        "Attempting to download BAML library v{} for {}/{}",
        env.version,
        std::env::consts::OS,
        std::env::consts::ARCH
    ));
    download::download_library(env, target, &cached_path)?;
    Ok(cached_path)
}

fn is_supported_platform() -> bool {
    cfg!(any(
        target_os = "linux",
        target_os = "macos",
        target_os = "windows"
    )) && cfg!(any(target_arch = "x86_64", target_arch = "aarch64"))
}

/// The canonical target triple for the compile-time target, as used in
/// the manifest's `cffi` map and the cache path.
pub(crate) fn target_triple() -> Result<&'static str, LoaderError> {
    let triple = match (
        cfg!(target_arch = "x86_64"),
        cfg!(target_arch = "aarch64"),
        cfg!(target_os = "macos"),
        cfg!(target_os = "windows"),
        cfg!(target_os = "linux"),
        cfg!(target_env = "musl"),
    ) {
        (true, _, true, _, _, _) => "x86_64-apple-darwin",
        (_, true, true, _, _, _) => "aarch64-apple-darwin",
        (true, _, _, true, _, _) => "x86_64-pc-windows-msvc",
        (_, true, _, true, _, _) => "aarch64-pc-windows-msvc",
        (true, _, _, _, true, false) => "x86_64-unknown-linux-gnu",
        (true, _, _, _, true, true) => "x86_64-unknown-linux-musl",
        (_, true, _, _, true, false) => "aarch64-unknown-linux-gnu",
        (_, true, _, _, true, true) => "aarch64-unknown-linux-musl",
        _ => {
            return Err(LoaderError::NotSupportedPlatform(format!(
                "OS={} Arch={}",
                std::env::consts::OS,
                std::env::consts::ARCH
            )));
        }
    };
    Ok(triple)
}

/// The library filename shared by every language's cache:
/// `libbridge_cffi.{so,dylib}` on Unix, `bridge_cffi.dll` on Windows.
pub(crate) fn lib_filename() -> &'static str {
    if cfg!(target_os = "windows") {
        "bridge_cffi.dll"
    } else if cfg!(target_os = "macos") {
        "libbridge_cffi.dylib"
    } else {
        "libbridge_cffi.so"
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

    fn test_env(home: &Path) -> LoaderEnv {
        LoaderEnv {
            explicit_path: None,
            env_path: None,
            baml_home: home.to_path_buf(),
            disable_download: true,
            skip_version_check: false,
            manifest_base_url: DEFAULT_MANIFEST_BASE_URL.to_string(),
            version: "0.0.0-test".to_string(),
        }
    }

    fn cached_lib_path(home: &Path) -> PathBuf {
        home.join("bridges")
            .join("0.0.0-test")
            .join(target_triple().unwrap())
            .join(lib_filename())
    }

    /// A `test_env` whose cache already holds the target library.
    fn cache_env_with_library(dir: &TempDir) -> LoaderEnv {
        let path = cached_lib_path(dir.path());
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, b"cached").unwrap();
        test_env(dir.path())
    }

    /// `LoaderEnv::from_lookup` over a fixed variable set.
    fn env_from(vars: &[(&str, &str)]) -> Result<LoaderEnv, LoaderError> {
        let vars: Vec<(String, String)> = vars
            .iter()
            .map(|(k, v)| ((*k).to_string(), (*v).to_string()))
            .collect();
        LoaderEnv::from_lookup(None, &|name| {
            vars.iter()
                .find(|(k, _)| k == name)
                .map(|(_, v)| OsString::from(v))
        })
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

    #[test]
    fn cache_hit_resolves_to_the_cached_file() {
        let dir = TempDir::new();
        let env = cache_env_with_library(&dir);
        assert_eq!(
            resolve_library_path(&env).unwrap(),
            cached_lib_path(dir.path())
        );
    }

    #[test]
    fn cache_layout_is_home_bridges_version_target_lib() {
        let dir = TempDir::new();
        let env = cache_env_with_library(&dir);
        let expected = dir
            .path()
            .join("bridges")
            .join("0.0.0-test")
            .join(target_triple().unwrap())
            .join(lib_filename());
        assert_eq!(resolve_library_path(&env).unwrap(), expected);
    }

    #[test]
    fn full_miss_error_lists_every_attempt() {
        let dir = TempDir::new();
        let err = resolve_library_path(&test_env(dir.path())).unwrap_err();
        let msg = err.to_string();
        assert!(matches!(err, LoaderError::LoadLibrary(_)));
        for expected in [
            "set_shared_library_path",
            LIBRARY_PATH_ENV,
            DISABLE_DOWNLOAD_ENV,
            "Disabled",
        ] {
            assert!(msg.contains(expected), "missing {expected:?} in: {msg}");
        }
        assert!(msg.contains(lib_filename()), "{msg}");
    }

    #[test]
    fn target_and_filename_match_the_current_platform() {
        let triple = target_triple().unwrap();
        let filename = lib_filename();
        if cfg!(target_os = "macos") {
            assert!(triple.ends_with("-apple-darwin"), "{triple}");
            assert_eq!(filename, "libbridge_cffi.dylib");
        } else if cfg!(target_os = "linux") {
            let libc = if cfg!(target_env = "musl") {
                "musl"
            } else {
                "gnu"
            };
            assert!(
                triple.ends_with(&format!("-unknown-linux-{libc}")),
                "{triple}"
            );
            assert_eq!(filename, "libbridge_cffi.so");
        } else if cfg!(target_os = "windows") {
            assert!(triple.ends_with("-pc-windows-msvc"), "{triple}");
            assert_eq!(filename, "bridge_cffi.dll");
        }
    }

    // ---- environment parsing ----

    #[test]
    fn environment_defaults() {
        let env = env_from(&[("HOME", "/home/tester"), ("USERPROFILE", "/home/tester")]).unwrap();
        assert!(!env.disable_download);
        assert!(!env.skip_version_check);
        assert_eq!(env.manifest_base_url, DEFAULT_MANIFEST_BASE_URL);
        assert_eq!(env.baml_home, Path::new("/home/tester").join(".baml"));
        assert!(env.env_path.is_none());
    }

    #[test]
    fn baml_home_prefers_a_non_empty_env_value() {
        let env = env_from(&[("BAML_HOME", "/custom/baml"), ("HOME", "/home/tester")]).unwrap();
        assert_eq!(env.baml_home, PathBuf::from("/custom/baml"));
        let env = env_from(&[("BAML_HOME", ""), ("HOME", "/home/tester")]).unwrap();
        assert_eq!(env.baml_home, Path::new("/home/tester").join(".baml"));
        let env = env_from(&[]).unwrap();
        assert_eq!(env.baml_home, PathBuf::from(".baml"));
    }

    #[test]
    fn bools_use_the_shared_spelling() {
        for value in ["1", "true", "TRUE", "Yes", "on"] {
            let env = env_from(&[(DISABLE_DOWNLOAD_ENV, value)]).unwrap();
            assert!(env.disable_download, "{value}");
        }
        for value in ["0", "false", "No", "OFF", ""] {
            let env = env_from(&[(DISABLE_DOWNLOAD_ENV, value)]).unwrap();
            assert!(!env.disable_download, "{value}");
        }
        let err = env_from(&[(DISABLE_DOWNLOAD_ENV, "maybe")]).err().unwrap();
        assert!(matches!(err, LoaderError::Config(m) if m.contains(DISABLE_DOWNLOAD_ENV)));
    }

    #[test]
    fn bridge_path_must_be_absolute() {
        let abs = std::env::temp_dir().join("libbridge_cffi.so");
        let env = env_from(&[(LIBRARY_PATH_ENV, abs.to_str().unwrap())]).unwrap();
        assert_eq!(env.env_path, Some(abs));
        let err = env_from(&[(LIBRARY_PATH_ENV, "relative/lib.so")])
            .err()
            .unwrap();
        assert!(matches!(err, LoaderError::Config(m) if m.contains("absolute")));
    }

    #[test]
    fn skip_version_check_requires_a_bridge_path() {
        let err = env_from(&[(SKIP_VERSION_CHECK_ENV, "1")]).err().unwrap();
        assert!(matches!(err, LoaderError::Config(m) if m.contains(LIBRARY_PATH_ENV)));
        let abs = std::env::temp_dir().join("libbridge_cffi.so");
        let env = env_from(&[
            (SKIP_VERSION_CHECK_ENV, "true"),
            (LIBRARY_PATH_ENV, abs.to_str().unwrap()),
        ])
        .unwrap();
        assert!(env.skip_version_check);
        let env = env_from(&[(SKIP_VERSION_CHECK_ENV, "0")]).unwrap();
        assert!(!env.skip_version_check);
    }

    #[test]
    fn manifest_base_url_trims_trailing_slashes() {
        let env = env_from(&[(MANIFEST_BASE_URL_ENV, "https://example.invalid/m//")]).unwrap();
        assert_eq!(env.manifest_base_url, "https://example.invalid/m");
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
        serve_on(listener, routes, expected_requests)
    }

    fn serve_on(
        listener: std::net::TcpListener,
        routes: Vec<(String, u16, Vec<u8>)>,
        expected_requests: usize,
    ) -> (String, std::thread::JoinHandle<Vec<String>>) {
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

    fn manifest_json(base: &str, sha256: &str) -> Vec<u8> {
        format!(
            r#"{{"schema":1,"version":"0.0.0-test","cffi":{{"{}":{{"url":"{base}/lib","sha256":"{sha256}"}}}}}}"#,
            target_triple().unwrap()
        )
        .into_bytes()
    }

    fn download_env(home: &Path, base: String) -> LoaderEnv {
        LoaderEnv {
            disable_download: false,
            manifest_base_url: base,
            ..test_env(home)
        }
    }

    fn leftovers(dir: &Path) -> Vec<PathBuf> {
        let mut found = Vec::new();
        let mut stack = vec![dir.to_path_buf()];
        while let Some(current) = stack.pop() {
            for entry in std::fs::read_dir(current).unwrap().filter_map(Result::ok) {
                let path = entry.path();
                if path.is_dir() {
                    stack.push(path);
                } else if path.to_string_lossy().ends_with(".tmpdl") {
                    found.push(path);
                }
            }
        }
        found
    }

    /// Bind the server first so the manifest can point back at it.
    fn serve_with_manifest(
        artifact: &[u8],
        manifest_sha256: &str,
        expected_requests: usize,
    ) -> (String, std::thread::JoinHandle<Vec<String>>) {
        // The manifest must point back at the server, so bind first.
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let routes = vec![
            (
                "/version/0.0.0-test.json".to_string(),
                200,
                manifest_json(&base, manifest_sha256),
            ),
            ("/lib".to_string(), 200, artifact.to_vec()),
        ];
        let (_, handle) = serve_on(listener, routes, expected_requests);
        (base, handle)
    }

    #[test]
    fn download_installs_a_checksum_verified_library() {
        let dir = TempDir::new();
        let artifact = b"fake engine bytes".to_vec();
        let (base, server) = serve_with_manifest(&artifact, &sha256_hex(&artifact), 2);
        let env = download_env(dir.path(), base);
        let resolved = resolve_library_path(&env).unwrap();
        assert_eq!(resolved, cached_lib_path(dir.path()));
        assert_eq!(std::fs::read(&resolved).unwrap(), artifact);
        assert!(leftovers(dir.path()).is_empty());
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(&resolved).unwrap().permissions().mode();
            assert_eq!(mode & 0o777, 0o755, "mode {mode:o}");
        }
        assert_eq!(server.join().unwrap().len(), 2);
        // A second resolution is served from the cache with no requests (the
        // server has exited, so any request would fail).
        assert_eq!(resolve_library_path(&env).unwrap(), resolved);
    }

    #[test]
    fn checksum_mismatch_rejects_the_download() {
        let dir = TempDir::new();
        let (base, server) = serve_with_manifest(b"corrupted bytes", &"0".repeat(64), 2);
        let env = download_env(dir.path(), base);
        let err = resolve_library_path(&env).unwrap_err();
        assert!(matches!(err, LoaderError::ChecksumMismatch(_)), "{err}");
        assert!(!cached_lib_path(dir.path()).exists());
        assert!(leftovers(dir.path()).is_empty());
        server.join().unwrap();
    }

    #[test]
    fn manifest_without_this_target_fails() {
        let dir = TempDir::new();
        let (base, server) = serve(
            vec![(
                "/version/0.0.0-test.json".to_string(),
                200,
                br#"{"schema":1,"version":"0.0.0-test","cffi":{}}"#.to_vec(),
            )],
            1,
        );
        let env = download_env(dir.path(), base);
        let err = resolve_library_path(&env).unwrap_err();
        assert!(
            matches!(&err, LoaderError::DownloadFailed(m) if m.contains(target_triple().unwrap())),
            "{err}"
        );
        server.join().unwrap();
    }

    #[test]
    fn manifest_version_mismatch_fails() {
        let dir = TempDir::new();
        let (base, server) = serve(
            vec![(
                "/version/0.0.0-test.json".to_string(),
                200,
                br#"{"schema":1,"version":"9.9.9","cffi":{}}"#.to_vec(),
            )],
            1,
        );
        let err = resolve_library_path(&download_env(dir.path(), base)).unwrap_err();
        assert!(
            matches!(&err, LoaderError::DownloadFailed(m) if m.contains("9.9.9")),
            "{err}"
        );
        server.join().unwrap();
    }

    #[test]
    fn missing_manifest_fails_the_download_step() {
        let dir = TempDir::new();
        let (base, server) = serve(Vec::new(), 1);
        let err = resolve_library_path(&download_env(dir.path(), base)).unwrap_err();
        assert!(matches!(err, LoaderError::DownloadFailed(_)), "{err}");
        assert!(!cached_lib_path(dir.path()).exists());
        server.join().unwrap();
    }
}
