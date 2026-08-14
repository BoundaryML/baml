#![allow(clippy::print_stdout, clippy::print_stderr)]

pub mod manifest;
pub mod platforms;
pub mod skills;

use std::{
    fs::{self, OpenOptions},
    io::{Cursor, Read},
    path::{Path, PathBuf},
    thread,
    time::Duration,
};

use anyhow::{Context, Result};
pub use manifest::{Artifact, Channel, ToolchainManifest, WrapperManifest};
use sha2::{Digest, Sha256};

/// Resolve the BAML home directory (`~/.baml`), the root under which the
/// toolchain stores installed releases, config, and other per-user state.
///
/// Resolution order:
///   1. `$BAML_HOME`, if set.
///   2. `$HOME` (or `$USERPROFILE` on Windows) joined with `.baml`.
///   3. A relative `.baml` as a last resort when no home directory is known.
///
/// This is the single source of truth shared by the `baml` wrapper and the
/// `baml-cli` toolchain binary; don't reimplement it.
pub fn baml_home() -> PathBuf {
    std::env::var_os("BAML_HOME")
        .map(PathBuf::from)
        .or_else(|| {
            std::env::var_os("HOME")
                .map(PathBuf::from)
                .or_else(|| std::env::var_os("USERPROFILE").map(PathBuf::from))
                .map(|home| home.join(".baml"))
        })
        .unwrap_or_else(|| PathBuf::from(".baml"))
}

pub const MANIFEST_SCHEMA: u32 = 1;
pub const DEFAULT_MANIFEST_BASE_URL: &str = "https://pkg.boundaryml.com/manifest/v1";
pub const DEFAULT_RELEASE_REPO: &str = "BoundaryML/baml";

pub const SUPPORTED_RELEASE_TARGETS: &[&str] = &[
    "aarch64-apple-darwin",
    "x86_64-apple-darwin",
    "aarch64-unknown-linux-gnu",
    "aarch64-unknown-linux-musl",
    "x86_64-unknown-linux-gnu",
    "x86_64-unknown-linux-musl",
    "x86_64-pc-windows-msvc",
    "aarch64-pc-windows-msvc",
];

#[derive(Debug, Clone)]
pub struct ReleaseSpec {
    pub version: String,
    pub target: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Product {
    Toolchain,
    Wrapper,
}

impl Product {
    pub fn tag_prefix(self) -> &'static str {
        match self {
            Product::Toolchain => "baml-language",
            Product::Wrapper => "baml-wrapper",
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum FetchError {
    #[error("network error fetching {url}: {source}")]
    Network { url: String, source: reqwest::Error },
    #[error("HTTP {status} fetching {url}")]
    HttpStatus {
        url: String,
        status: reqwest::StatusCode,
    },
    #[error("manifest 404 for version {version} (not released yet?)")]
    ManifestNotFound { version: String },
    #[error("manifest schema {got} not supported (max {max}); run `baml self-update`")]
    ManifestSchemaTooNew { got: u32, max: u32 },
    #[error("target {target} not built for version {version}")]
    TargetNotInManifest { target: String, version: String },
    #[error("sha256 mismatch for {url}: expected {expected}, got {got}")]
    ChecksumMismatch {
        url: String,
        expected: String,
        got: String,
    },
    #[error("archive missing expected binary {name}")]
    BinaryNotInArchive { name: String },
    #[error("archive contains unsafe path {path}")]
    UnsafeArchivePath { path: String },
    #[error("disk error: {0}")]
    Io(#[from] std::io::Error),
    #[error("zip archive error: {0}")]
    Zip(#[from] zip::result::ZipError),
}

#[derive(Debug, Clone)]
pub struct Fetcher {
    pub spec: ReleaseSpec,
    pub product: Product,
    pub manifest_base_url: String,
    pub release_repo: String,
    artifact: Option<Artifact>,
}

impl Fetcher {
    pub fn default_for(spec: ReleaseSpec, product: Product) -> Self {
        Self {
            spec,
            product,
            manifest_base_url: manifest_base_url(),
            release_repo: release_repo(),
            artifact: None,
        }
    }

    pub fn from_artifact(spec: ReleaseSpec, product: Product, artifact: Artifact) -> Self {
        Self {
            spec,
            product,
            manifest_base_url: manifest_base_url(),
            release_repo: release_repo(),
            artifact: Some(artifact),
        }
    }

    pub fn artifact_url(&self) -> String {
        if let Some(artifact) = &self.artifact {
            return artifact.url.clone();
        }
        if let Ok(base_url) = std::env::var("BAML_PACK_HOST_RELEASE_BASE_URL") {
            let base = base_url.trim_end_matches('/');
            return format!(
                "{base}/{}",
                release_archive_filename(self.product, &self.spec.version, &self.spec.target)
            );
        }
        release_archive_url_for_repo(
            self.product,
            &self.spec.version,
            &self.spec.target,
            &self.release_repo,
        )
    }

    /// Download the release archive and verify its checksum, *without*
    /// validating the full product layout. Callers that only need a single
    /// binary out of the archive (`fetch_binary`) don't care whether the
    /// archive also ships the vsix or playground assets: extracting one entry
    /// by name into memory is safe regardless of the surrounding layout, and
    /// the checksum is what guarantees authenticity.
    fn download_and_verify(&self) -> Result<Vec<u8>, FetchError> {
        let url = self.artifact_url();
        let archive = download_bytes(&url)?;
        if let Some(artifact) = &self.artifact {
            verify_sha256(&archive, &url, &artifact.sha256)?;
        } else {
            let checksum_url = format!("{url}.sha256");
            let checksum = download_bytes(&checksum_url)?;
            let checksum_text =
                std::str::from_utf8(&checksum).map_err(|_| FetchError::UnsafeArchivePath {
                    path: format!("{checksum_url} was not valid UTF-8"),
                })?;
            verify_release_archive_checksum_text(&archive, &url, checksum_text)?;
        }
        Ok(archive)
    }

    pub fn fetch_archive(&self) -> Result<Vec<u8>, FetchError> {
        let archive = self.download_and_verify()?;
        validate_archive_layout(
            self.product,
            &self.spec.target,
            &archive,
            &self.artifact_url(),
        )?;
        Ok(archive)
    }

    /// Pull a single binary out of the release archive. Used by `baml pack`
    /// (to embed `baml-pack-host`) and wrapper self-update (to fetch `baml`).
    /// Deliberately skips `validate_archive_layout` — requiring the playground
    /// assets / vsix here would make `baml pack` fail on an archive that has a
    /// perfectly good host binary. Extraction itself errors with
    /// `BinaryNotInArchive` if the requested binary is absent.
    pub fn fetch_binary(&self, binary_name: &str) -> Result<Vec<u8>, FetchError> {
        let url = self.artifact_url();
        let archive = self.download_and_verify()?;
        extract_binary_from_archive(&archive, &url, binary_name)
    }

    pub fn install_to_toolchain_root(&self, root: &Path) -> Result<PathBuf, FetchError> {
        fs::create_dir_all(root)?;
        let _lock = InstallLock::acquire(root.join(format!(".{}.lock", self.spec.version)))?;
        let install_dir = root.join(&self.spec.version);
        if fs::read_to_string(install_dir.join("VERSION"))
            .map(|version| version.trim() == self.spec.version)
            .unwrap_or(false)
        {
            return Ok(install_dir);
        }

        let archive = self.fetch_archive()?;
        let tmp_dir = root.join(format!(".{}.tmp", self.spec.version));
        if tmp_dir.exists() {
            fs::remove_dir_all(&tmp_dir)?;
        }
        fs::create_dir_all(&tmp_dir)?;
        extract_archive_to_dir(&archive, &self.artifact_url(), &tmp_dir)?;
        fs::write(tmp_dir.join("VERSION"), format!("{}\n", self.spec.version))?;
        fs::write(
            tmp_dir.join("install.json"),
            serde_json::json!({
                "version": self.spec.version,
                "target": self.spec.target,
                "archive_url": self.artifact_url(),
            })
            .to_string(),
        )?;

        if install_dir.exists() {
            fs::remove_dir_all(&install_dir)?;
        }
        fs::rename(&tmp_dir, &install_dir)?;
        Ok(install_dir)
    }
}

struct InstallLock {
    path: PathBuf,
}

impl InstallLock {
    fn acquire(path: PathBuf) -> Result<Self, FetchError> {
        for _ in 0..60 {
            match OpenOptions::new().write(true).create_new(true).open(&path) {
                Ok(_) => return Ok(Self { path }),
                Err(err) if err.kind() == std::io::ErrorKind::AlreadyExists => {
                    thread::sleep(Duration::from_secs(1));
                }
                Err(err) => return Err(FetchError::Io(err)),
            }
        }
        Err(FetchError::Io(std::io::Error::new(
            std::io::ErrorKind::TimedOut,
            format!("timed out waiting for install lock {}", path.display()),
        )))
    }
}

impl Drop for InstallLock {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

pub fn manifest_base_url() -> String {
    std::env::var("BAML_MANIFEST_BASE_URL")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| DEFAULT_MANIFEST_BASE_URL.to_string())
        .trim_end_matches('/')
        .to_string()
}

pub fn release_repo() -> String {
    std::env::var("BAML_PACK_HOST_RELEASE_REPO")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| DEFAULT_RELEASE_REPO.to_string())
}

pub fn release_host_target_triple() -> Result<&'static str> {
    #[cfg(target_env = "musl")]
    const IS_MUSL: bool = true;
    #[cfg(not(target_env = "musl"))]
    const IS_MUSL: bool = false;

    match (std::env::consts::OS, std::env::consts::ARCH, IS_MUSL) {
        ("macos", "aarch64", _) => Ok("aarch64-apple-darwin"),
        ("macos", "x86_64", _) => Ok("x86_64-apple-darwin"),
        ("linux", "aarch64", true) => Ok("aarch64-unknown-linux-musl"),
        ("linux", "aarch64", false) => Ok("aarch64-unknown-linux-gnu"),
        ("linux", "x86_64", true) => Ok("x86_64-unknown-linux-musl"),
        ("linux", "x86_64", false) => Ok("x86_64-unknown-linux-gnu"),
        ("windows", "x86_64", _) => Ok("x86_64-pc-windows-msvc"),
        ("windows", "aarch64", _) => Ok("aarch64-pc-windows-msvc"),
        (os, arch, _) => anyhow::bail!(
            "no released BAML artifact is available for {arch}-{os}. \
             Install the matching toolchain via `baml toolchain install <version>`."
        ),
    }
}

pub fn validate_release_target_triple(target: &str) -> Result<&str> {
    if SUPPORTED_RELEASE_TARGETS.contains(&target) {
        Ok(target)
    } else {
        anyhow::bail!(
            "unsupported release target `{target}`. Supported targets: {}",
            SUPPORTED_RELEASE_TARGETS.join(", ")
        )
    }
}

pub fn release_archive_filename(product: Product, version: &str, target: &str) -> String {
    let ext = if target.ends_with("windows-msvc") {
        "zip"
    } else {
        "tar.gz"
    };
    format!("{}-{version}-{target}.{ext}", product.tag_prefix())
}

pub fn release_archive_url_for_repo(
    product: Product,
    version: &str,
    target: &str,
    repo: &str,
) -> String {
    format!(
        "https://github.com/{repo}/releases/download/{}-{version}/{}",
        product.tag_prefix(),
        release_archive_filename(product, version, target)
    )
}

pub fn validate_sha256(hash: &str) -> Result<String> {
    if hash.len() == 64 && hash.chars().all(|c| c.is_ascii_hexdigit()) {
        Ok(hash.to_ascii_lowercase())
    } else {
        anyhow::bail!("invalid SHA-256 checksum `{hash}`")
    }
}

pub fn verify_release_archive_checksum_text(
    archive_bytes: &[u8],
    archive_url: &str,
    checksum_text: &str,
) -> Result<(), FetchError> {
    let archive_name = archive_url.rsplit('/').next().unwrap_or(archive_url);
    let expected = parse_release_checksum(checksum_text, archive_name).map_err(|_| {
        FetchError::BinaryNotInArchive {
            name: archive_name.to_string(),
        }
    })?;
    let got = format!("{:x}", Sha256::digest(archive_bytes));
    compare_sha256(archive_url, &expected, &got)
}

pub fn verify_sha256(
    archive_bytes: &[u8],
    archive_url: &str,
    expected: &str,
) -> Result<(), FetchError> {
    let expected = validate_sha256(expected).map_err(|_| FetchError::ChecksumMismatch {
        url: archive_url.to_string(),
        expected: expected.to_string(),
        got: "invalid sha256".to_string(),
    })?;
    let got = format!("{:x}", Sha256::digest(archive_bytes));
    compare_sha256(archive_url, &expected, &got)
}

fn compare_sha256(archive_url: &str, expected: &str, got: &str) -> Result<(), FetchError> {
    if got != expected {
        return Err(FetchError::ChecksumMismatch {
            url: archive_url.to_string(),
            expected: expected.to_string(),
            got: got.to_string(),
        });
    }
    Ok(())
}

pub fn parse_release_checksum(checksum_text: &str, archive_name: &str) -> Result<String> {
    for line in checksum_text.lines() {
        let mut parts = line.split_whitespace();
        let Some(hash) = parts.next() else {
            continue;
        };
        let Some(name) = parts.next() else {
            continue;
        };
        if name == archive_name {
            return validate_sha256(hash);
        }
    }
    anyhow::bail!("Checksum file did not contain an entry for {archive_name}")
}

fn download_bytes(url: &str) -> Result<Vec<u8>, FetchError> {
    let client = reqwest::blocking::Client::builder()
        .connect_timeout(Duration::from_secs(10))
        .timeout(Duration::from_mins(10))
        .user_agent("baml-release/1")
        .build()
        .map_err(|source| FetchError::Network {
            url: url.to_string(),
            source,
        })?;

    let mut last_error = None;
    for attempt in 1..=3 {
        match client.get(url).send() {
            Ok(response) if response.status() == reqwest::StatusCode::NOT_FOUND => {
                return Err(FetchError::ManifestNotFound {
                    version: url.to_string(),
                });
            }
            Ok(response) if response.status().is_success() => {
                return response
                    .bytes()
                    .map(|bytes| bytes.to_vec())
                    .map_err(|source| FetchError::Network {
                        url: url.to_string(),
                        source,
                    });
            }
            Ok(response)
                if response.status().is_server_error() || response.status().is_redirection() =>
            {
                if response.status().is_redirection() {
                    return Err(FetchError::HttpStatus {
                        url: url.to_string(),
                        status: response.status(),
                    });
                }
                last_error = response.error_for_status().err();
            }
            Ok(response) => {
                return response
                    .error_for_status()
                    .map(|_| Vec::new())
                    .map_err(|source| FetchError::Network {
                        url: url.to_string(),
                        source,
                    });
            }
            Err(source) => {
                last_error = Some(source);
            }
        }
        if attempt < 3 {
            thread::sleep(Duration::from_secs(2));
        }
    }
    if let Some(source) = last_error {
        Err(FetchError::Network {
            url: url.to_string(),
            source,
        })
    } else {
        Err(FetchError::HttpStatus {
            url: url.to_string(),
            status: reqwest::StatusCode::INTERNAL_SERVER_ERROR,
        })
    }
}

pub fn extract_binary_from_archive(
    archive_bytes: &[u8],
    url: &str,
    binary_name: &str,
) -> Result<Vec<u8>, FetchError> {
    if archive_url_is_zip(url) {
        extract_binary_from_zip(archive_bytes, binary_name)
    } else {
        extract_binary_from_tar_gz(archive_bytes, binary_name)
    }
}

fn extract_binary_from_tar_gz(
    archive_bytes: &[u8],
    binary_name: &str,
) -> Result<Vec<u8>, FetchError> {
    let decoder = flate2::read::GzDecoder::new(Cursor::new(archive_bytes));
    let mut archive = tar::Archive::new(decoder);
    for entry in archive.entries()? {
        let mut entry = entry?;
        if entry
            .path()
            .ok()
            .and_then(|path| path.file_name().map(|name| name == binary_name))
            .unwrap_or(false)
        {
            let mut bytes = Vec::new();
            entry.read_to_end(&mut bytes)?;
            return Ok(bytes);
        }
    }
    Err(FetchError::BinaryNotInArchive {
        name: binary_name.to_string(),
    })
}

fn extract_binary_from_zip(archive_bytes: &[u8], binary_name: &str) -> Result<Vec<u8>, FetchError> {
    let mut archive = zip::ZipArchive::new(Cursor::new(archive_bytes))?;
    for i in 0..archive.len() {
        let mut file = archive.by_index(i)?;
        if Path::new(file.name())
            .file_name()
            .map(|name| name == binary_name)
            .unwrap_or(false)
        {
            let mut bytes = Vec::new();
            file.read_to_end(&mut bytes)?;
            return Ok(bytes);
        }
    }
    Err(FetchError::BinaryNotInArchive {
        name: binary_name.to_string(),
    })
}

pub fn extract_archive_to_dir(
    archive_bytes: &[u8],
    url: &str,
    dest: &Path,
) -> Result<(), FetchError> {
    if archive_url_is_zip(url) {
        extract_zip_to_dir(archive_bytes, dest)
    } else {
        extract_tar_gz_to_dir(archive_bytes, dest)
    }
}

fn safe_join(dest: &Path, entry_path: &Path) -> Result<PathBuf, FetchError> {
    if entry_path.is_absolute()
        || entry_path
            .components()
            .any(|c| matches!(c, std::path::Component::ParentDir))
    {
        return Err(FetchError::UnsafeArchivePath {
            path: entry_path.display().to_string(),
        });
    }
    Ok(dest.join(entry_path))
}

fn extract_tar_gz_to_dir(archive_bytes: &[u8], dest: &Path) -> Result<(), FetchError> {
    let decoder = flate2::read::GzDecoder::new(Cursor::new(archive_bytes));
    let mut archive = tar::Archive::new(decoder);
    for entry in archive.entries()? {
        let mut entry = entry?;
        let path = entry.path()?.into_owned();
        let out = safe_join(dest, &path)?;
        if entry.header().entry_type().is_dir() {
            fs::create_dir_all(out)?;
        } else if entry.header().entry_type().is_file() {
            if let Some(parent) = out.parent() {
                fs::create_dir_all(parent)?;
            }
            entry
                .unpack(out)
                .context("extract tar entry")
                .map_err(|err| FetchError::Io(std::io::Error::other(err)))?;
        }
    }
    Ok(())
}

fn validate_archive_layout(
    product: Product,
    target: &str,
    archive_bytes: &[u8],
    url: &str,
) -> Result<(), FetchError> {
    let paths = archive_paths(archive_bytes, url)?;
    let exe_suffix = if target.ends_with("windows-msvc") {
        ".exe"
    } else {
        ""
    };
    let required = match product {
        Product::Toolchain => vec![
            format!("bin/baml-cli{exe_suffix}"),
            format!("bin/baml-pack-host{exe_suffix}"),
            "assets/baml-vscode.vsix".to_string(),
            "assets/playground/index.html".to_string(),
            "assets/playground/assets/index.js".to_string(),
            "assets/playground/assets/index.css".to_string(),
        ],
        Product::Wrapper => vec![format!("bin/baml{exe_suffix}")],
    };
    for path in &required {
        if !paths.iter().any(|entry| entry == path) {
            return Err(FetchError::BinaryNotInArchive { name: path.clone() });
        }
    }
    let forbidden = match product {
        Product::Toolchain => vec![format!("bin/baml{exe_suffix}")],
        Product::Wrapper => vec![
            format!("bin/baml-cli{exe_suffix}"),
            format!("bin/baml-pack-host{exe_suffix}"),
            "assets/baml-vscode.vsix".to_string(),
            "assets/playground/index.html".to_string(),
            "assets/playground/assets/index.js".to_string(),
            "assets/playground/assets/index.css".to_string(),
        ],
    };
    for path in &forbidden {
        if paths.iter().any(|entry| entry == path) {
            return Err(FetchError::UnsafeArchivePath {
                path: format!("archive unexpectedly contains {path}"),
            });
        }
    }
    Ok(())
}

fn normalize_archive_path(path: &Path) -> Result<String, FetchError> {
    let mut parts = Vec::new();
    for component in path.components() {
        match component {
            std::path::Component::Normal(part) => parts.push(part.to_string_lossy().to_string()),
            std::path::Component::CurDir => {}
            std::path::Component::ParentDir
            | std::path::Component::RootDir
            | std::path::Component::Prefix(_) => {
                return Err(FetchError::UnsafeArchivePath {
                    path: path.display().to_string(),
                });
            }
        }
    }
    Ok(parts.join("/"))
}

fn archive_paths(archive_bytes: &[u8], url: &str) -> Result<Vec<String>, FetchError> {
    if archive_url_is_zip(url) {
        archive_paths_zip(archive_bytes)
    } else {
        archive_paths_tar_gz(archive_bytes)
    }
}

fn archive_paths_tar_gz(archive_bytes: &[u8]) -> Result<Vec<String>, FetchError> {
    let decoder = flate2::read::GzDecoder::new(Cursor::new(archive_bytes));
    let mut archive = tar::Archive::new(decoder);
    let mut paths = Vec::new();
    for entry in archive.entries()? {
        let entry = entry?;
        let entry_type = entry.header().entry_type();
        if entry_type.is_symlink() || entry_type.is_hard_link() {
            return Err(FetchError::UnsafeArchivePath {
                path: entry.path()?.display().to_string(),
            });
        }
        if entry_type.is_file() {
            paths.push(normalize_archive_path(&entry.path()?)?);
        }
    }
    Ok(paths)
}

fn archive_paths_zip(archive_bytes: &[u8]) -> Result<Vec<String>, FetchError> {
    let mut archive = zip::ZipArchive::new(Cursor::new(archive_bytes))?;
    let mut paths = Vec::new();
    for i in 0..archive.len() {
        let file = archive.by_index(i)?;
        let Some(path) = file.enclosed_name() else {
            return Err(FetchError::UnsafeArchivePath {
                path: file.name().to_string(),
            });
        };
        #[cfg(unix)]
        if file
            .unix_mode()
            .is_some_and(|mode| mode & 0o170_000 == 0o120_000)
        {
            return Err(FetchError::UnsafeArchivePath {
                path: file.name().to_string(),
            });
        }
        if file.is_file() {
            paths.push(normalize_archive_path(&path)?);
        }
    }
    Ok(paths)
}

fn archive_url_is_zip(url: &str) -> bool {
    Path::new(url)
        .extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("zip"))
}

fn extract_zip_to_dir(archive_bytes: &[u8], dest: &Path) -> Result<(), FetchError> {
    let mut archive = zip::ZipArchive::new(Cursor::new(archive_bytes))?;
    for i in 0..archive.len() {
        let mut file = archive.by_index(i)?;
        let Some(path) = file.enclosed_name() else {
            return Err(FetchError::UnsafeArchivePath {
                path: file.name().to_string(),
            });
        };
        let out = safe_join(dest, &path)?;
        if file.is_dir() {
            fs::create_dir_all(out)?;
        } else {
            if let Some(parent) = out.parent() {
                fs::create_dir_all(parent)?;
            }
            let mut output = fs::File::create(out)?;
            std::io::copy(&mut file, &mut output)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_release_archive_filename_uses_platform_extension() {
        assert_eq!(
            release_archive_filename(
                Product::Toolchain,
                "1.2.3-nightly.20260602.a",
                "x86_64-unknown-linux-gnu"
            ),
            "baml-language-1.2.3-nightly.20260602.a-x86_64-unknown-linux-gnu.tar.gz"
        );
        assert_eq!(
            release_archive_filename(Product::Wrapper, "0.1.0", "aarch64-pc-windows-msvc"),
            "baml-wrapper-0.1.0-aarch64-pc-windows-msvc.zip"
        );
    }

    #[test]
    fn test_release_archive_url_defaults_to_github_release_asset() {
        let url = release_archive_url_for_repo(
            Product::Toolchain,
            "1.2.3-nightly.20260602.a",
            "x86_64-unknown-linux-gnu",
            "BoundaryML/baml",
        );
        assert_eq!(
            url,
            "https://github.com/BoundaryML/baml/releases/download/baml-language-1.2.3-nightly.20260602.a/baml-language-1.2.3-nightly.20260602.a-x86_64-unknown-linux-gnu.tar.gz"
        );
    }

    #[test]
    fn test_verify_release_archive_checksum_text() {
        let archive_name = "baml-language-1.2.3-nightly.20260602.a-x86_64-unknown-linux-gnu.tar.gz";
        let archive_url = format!("https://example.com/releases/{archive_name}");
        let archive_bytes = b"fake archive bytes";
        let digest = format!("{:x}", Sha256::digest(archive_bytes));
        let checksum_text = format!("{digest}  {archive_name}\n");

        verify_release_archive_checksum_text(archive_bytes, &archive_url, &checksum_text).unwrap();

        let err =
            verify_release_archive_checksum_text(b"different bytes", &archive_url, &checksum_text)
                .unwrap_err();
        assert!(format!("{err}").contains("sha256 mismatch"));
    }

    #[test]
    fn test_validate_release_target_triple_accepts_supported_targets() {
        for target in SUPPORTED_RELEASE_TARGETS {
            assert_eq!(validate_release_target_triple(target).unwrap(), *target);
        }
    }

    #[test]
    fn test_validate_release_target_triple_rejects_unknown_target() {
        let err = validate_release_target_triple("wasm32-unknown-unknown").unwrap_err();
        let msg = format!("{err}");
        assert!(msg.contains("unsupported release target"), "got: {msg}");
        assert!(msg.contains("aarch64-pc-windows-msvc"), "got: {msg}");
    }

    #[test]
    fn test_extract_binary_from_tar_gz() {
        let mut gzip = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
        {
            let mut builder = tar::Builder::new(&mut gzip);
            let bytes = b"fake host";
            let mut header = tar::Header::new_gnu();
            header.set_size(bytes.len() as u64);
            header.set_mode(0o755);
            header.set_cksum();
            builder
                .append_data(&mut header, "nested/baml-pack-host", &bytes[..])
                .unwrap();
            builder.finish().unwrap();
        }
        let archive = gzip.finish().unwrap();
        assert_eq!(
            extract_binary_from_archive(
                &archive,
                "https://example.com/baml-language-1.2.3-x86_64-unknown-linux-gnu.tar.gz",
                "baml-pack-host"
            )
            .unwrap(),
            b"fake host"
        );
    }

    #[test]
    fn test_extract_binary_from_zip() {
        use std::io::Write;

        let mut archive = Cursor::new(Vec::new());
        {
            let mut zip = zip::ZipWriter::new(&mut archive);
            zip.start_file(
                "nested/baml-pack-host.exe",
                zip::write::SimpleFileOptions::default(),
            )
            .unwrap();
            zip.write_all(b"fake windows host").unwrap();
            zip.finish().unwrap();
        }
        assert_eq!(
            extract_binary_from_archive(
                &archive.into_inner(),
                "https://example.com/baml-language-1.2.3-x86_64-pc-windows-msvc.zip",
                "baml-pack-host.exe"
            )
            .unwrap(),
            b"fake windows host"
        );
    }

    #[test]
    fn test_validate_toolchain_archive_layout_rejects_wrapper_binary() {
        let mut gzip = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
        {
            let mut builder = tar::Builder::new(&mut gzip);
            for path in [
                "bin/baml-cli",
                "bin/baml-pack-host",
                "assets/baml-vscode.vsix",
                "assets/playground/index.html",
                "assets/playground/assets/index.js",
                "assets/playground/assets/index.css",
                "bin/baml",
            ] {
                let bytes = b"fake";
                let mut header = tar::Header::new_gnu();
                header.set_size(bytes.len() as u64);
                header.set_mode(0o755);
                header.set_cksum();
                builder.append_data(&mut header, path, &bytes[..]).unwrap();
            }
            builder.finish().unwrap();
        }
        let archive = gzip.finish().unwrap();
        let err = validate_archive_layout(
            Product::Toolchain,
            "x86_64-unknown-linux-gnu",
            &archive,
            "https://example.com/archive.tar.gz",
        )
        .unwrap_err();
        assert!(format!("{err}").contains("bin/baml"));
    }

    #[test]
    fn test_validate_toolchain_archive_layout_accepts_playground_payload() {
        let mut gzip = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
        {
            let mut builder = tar::Builder::new(&mut gzip);
            for path in [
                "bin/baml-cli",
                "bin/baml-pack-host",
                "assets/baml-vscode.vsix",
                "assets/playground/index.html",
                "assets/playground/assets/index.js",
                "assets/playground/assets/index.css",
            ] {
                let bytes = b"fake";
                let mut header = tar::Header::new_gnu();
                header.set_size(bytes.len() as u64);
                header.set_mode(0o755);
                header.set_cksum();
                builder.append_data(&mut header, path, &bytes[..]).unwrap();
            }
            builder.finish().unwrap();
        }
        let archive = gzip.finish().unwrap();
        validate_archive_layout(
            Product::Toolchain,
            "x86_64-unknown-linux-gnu",
            &archive,
            "https://example.com/archive.tar.gz",
        )
        .unwrap();
    }

    #[test]
    fn test_validate_windows_toolchain_archive_layout_accepts_playground_payload() {
        use std::io::Write;

        let mut archive = Cursor::new(Vec::new());
        {
            let mut zip = zip::ZipWriter::new(&mut archive);
            for path in [
                "bin/baml-cli.exe",
                "bin/baml-pack-host.exe",
                "assets/baml-vscode.vsix",
                "assets/playground/index.html",
                "assets/playground/assets/index.js",
                "assets/playground/assets/index.css",
            ] {
                zip.start_file(path, zip::write::SimpleFileOptions::default())
                    .unwrap();
                zip.write_all(b"fake").unwrap();
            }
            zip.finish().unwrap();
        }
        validate_archive_layout(
            Product::Toolchain,
            "x86_64-pc-windows-msvc",
            &archive.into_inner(),
            "https://example.com/archive.zip",
        )
        .unwrap();
    }

    #[test]
    fn test_validate_toolchain_archive_layout_rejects_incomplete_playground_payload() {
        let mut gzip = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
        {
            let mut builder = tar::Builder::new(&mut gzip);
            for path in [
                "bin/baml-cli",
                "bin/baml-pack-host",
                "assets/baml-vscode.vsix",
                "assets/playground/index.html",
                "assets/playground/assets/index.css",
            ] {
                let bytes = b"fake";
                let mut header = tar::Header::new_gnu();
                header.set_size(bytes.len() as u64);
                header.set_mode(0o755);
                header.set_cksum();
                builder.append_data(&mut header, path, &bytes[..]).unwrap();
            }
            builder.finish().unwrap();
        }
        let archive = gzip.finish().unwrap();
        let err = validate_archive_layout(
            Product::Toolchain,
            "x86_64-unknown-linux-gnu",
            &archive,
            "https://example.com/archive.tar.gz",
        )
        .unwrap_err();
        assert!(format!("{err}").contains("assets/playground/assets/index.js"));
    }

    #[test]
    fn test_validate_wrapper_archive_layout_rejects_toolchain_payload() {
        use std::io::Write;

        let mut archive = Cursor::new(Vec::new());
        {
            let mut zip = zip::ZipWriter::new(&mut archive);
            for path in ["bin/baml.exe", "bin/baml-cli.exe"] {
                zip.start_file(path, zip::write::SimpleFileOptions::default())
                    .unwrap();
                zip.write_all(b"fake").unwrap();
            }
            zip.finish().unwrap();
        }
        let err = validate_archive_layout(
            Product::Wrapper,
            "x86_64-pc-windows-msvc",
            &archive.into_inner(),
            "https://example.com/archive.zip",
        )
        .unwrap_err();
        assert!(format!("{err}").contains("baml-cli"));
    }
}
