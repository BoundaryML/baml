//! Download-and-install of the engine shared library into the cache.
//!
//! The artifact is resolved through the release manifest
//! (`<manifest base>/version/<version>.json`, schema 1, a `cffi` map from
//! target triple to `{url, sha256}`), the same manifest the Go loader and
//! the `baml` wrapper read. The artifact is streamed to a temp file in the
//! destination directory while hashing, verified against the manifest's
//! sha256 (a mismatch is always a hard failure), then atomically renamed
//! into place (copy fallback for filesystems that reject the rename).

use std::{
    io::{IsTerminal, Read, Write},
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

use sha2::{Digest, Sha256};

use super::{LoaderEnv, LoaderError, log};

const MANIFEST_SCHEMA: u64 = 1;
const MAX_MANIFEST_BYTES: u64 = 4 << 20;

/// One `cffi` manifest entry for the current target.
#[derive(Debug)]
struct ManifestEntry {
    url: String,
    sha256: String,
}

pub(super) fn download_library(
    env: &LoaderEnv,
    target: &str,
    dest_path: &Path,
) -> Result<(), LoaderError> {
    let dest_dir = dest_path.parent().ok_or_else(|| {
        LoaderError::CacheDir(format!("{} has no parent directory", dest_path.display()))
    })?;
    std::fs::create_dir_all(dest_dir).map_err(|e| {
        LoaderError::CacheDir(format!(
            "failed to create cache directory {}: {e}",
            dest_dir.display()
        ))
    })?;
    let filename = dest_path
        .file_name()
        .map_or_else(String::new, |n| n.to_string_lossy().into_owned());

    let agent = http_agent(&env.version);
    let entry = resolve_manifest_entry(env, &agent, target)?;
    let download_url = entry.url;
    let expected = entry.sha256;
    log::debug(&format!(
        "Downloading BAML library from {download_url} to {}",
        dest_path.display()
    ));

    let mut response = agent.get(&download_url).call().map_err(|e| {
        LoaderError::DownloadFailed(format!("network error fetching {download_url}: {e}"))
    })?;
    let status = response.status();
    if status == 404 {
        return Err(LoaderError::DownloadFailed(format!(
            "library file not found at {download_url} (HTTP 404)"
        )));
    }
    if !status.is_success() {
        let mut snippet = String::new();
        let _ = response
            .body_mut()
            .as_reader()
            .take(512)
            .read_to_string(&mut snippet);
        return Err(LoaderError::DownloadFailed(format!(
            "unexpected HTTP status {status} fetching {download_url}. Server response: {snippet}"
        )));
    }

    let content_length = response.body().content_length();
    let (tmp_path, mut tmp_file) = create_temp_in(dest_dir, &filename)?;
    // Remove the temp file on every exit path; after a successful rename
    // the removal quietly finds nothing.
    let _cleanup = RemoveOnDrop(tmp_path.clone());

    let mut hasher = Sha256::new();
    let mut progress = Progress::new(content_length, &filename);
    let mut reader = response.body_mut().as_reader();
    let mut buf = [0u8; 64 * 1024];
    loop {
        let n = reader.read(&mut buf).map_err(|e| {
            LoaderError::DownloadFailed(format!(
                "download interrupted writing to {}: {e}",
                tmp_path.display()
            ))
        })?;
        if n == 0 {
            break;
        }
        tmp_file.write_all(&buf[..n]).map_err(|e| {
            LoaderError::DownloadFailed(format!(
                "failed writing temporary file {}: {e}",
                tmp_path.display()
            ))
        })?;
        hasher.update(&buf[..n]);
        progress.advance(n as u64);
    }
    progress.finish();

    let actual_checksum = hex::encode(hasher.finalize());
    log::debug("Verifying checksum");
    if !actual_checksum.eq_ignore_ascii_case(&expected) {
        return Err(LoaderError::ChecksumMismatch(format!(
            "checksum mismatch for {download_url}: expected {expected}, got {actual_checksum}; \
             the downloaded file may be corrupt"
        )));
    }
    log::info(&format!(
        "Checksum verified successfully ({})",
        &actual_checksum[..8]
    ));

    tmp_file.sync_all().map_err(|e| {
        LoaderError::DownloadFailed(format!(
            "failed syncing temporary file {}: {e}",
            tmp_path.display()
        ))
    })?;
    // Close the handle before renaming (required on Windows).
    drop(tmp_file);

    log::debug(&format!(
        "Moving downloaded file to final location {}",
        dest_path.display()
    ));
    if let Err(rename_err) = std::fs::rename(&tmp_path, dest_path) {
        log::warn(&format!(
            "Atomic rename failed ({rename_err}), attempting copy fallback"
        ));
        copy_file(&tmp_path, dest_path).map_err(|copy_err| {
            LoaderError::DownloadFailed(format!(
                "failed moving temp file {} to {}: rename failed ({rename_err}) \
                 and copy failed ({copy_err})",
                tmp_path.display(),
                dest_path.display()
            ))
        })?;
        log::info("Copy fallback succeeded");
    }

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if let Err(e) = std::fs::set_permissions(dest_path, std::fs::Permissions::from_mode(0o755))
        {
            log::warn(&format!(
                "Failed to set permissions (chmod 0755) on {}: {e}",
                dest_path.display()
            ));
        }
    }

    log::info(&format!(
        "Successfully downloaded and cached BAML library at {}",
        dest_path.display()
    ));
    Ok(())
}

/// Fetch the version manifest and pick the `cffi` entry for `target`.
fn resolve_manifest_entry(
    env: &LoaderEnv,
    agent: &ureq::Agent,
    target: &str,
) -> Result<ManifestEntry, LoaderError> {
    let url = format!("{}/version/{}.json", env.manifest_base_url, env.version);
    log::debug(&format!("Fetching BAML release manifest from {url}"));
    let body = get_bytes(agent, &url, MAX_MANIFEST_BYTES)?;
    let text = String::from_utf8(body)
        .map_err(|_| LoaderError::DownloadFailed(format!("manifest at {url} is not UTF-8")))?;
    parse_manifest(&text, &env.version, target)
        .map_err(|e| LoaderError::DownloadFailed(format!("invalid manifest at {url}: {e}")))
}

/// Validate the manifest document and return the entry for `target`.
fn parse_manifest(text: &str, version: &str, target: &str) -> Result<ManifestEntry, String> {
    let doc: serde_json::Value =
        serde_json::from_str(text).map_err(|e| format!("not valid JSON: {e}"))?;
    let schema = doc.get("schema").and_then(serde_json::Value::as_u64);
    if schema != Some(MANIFEST_SCHEMA) {
        return Err(format!("unsupported manifest schema {schema:?}"));
    }
    let manifest_version = doc.get("version").and_then(serde_json::Value::as_str);
    if manifest_version != Some(version) {
        return Err(format!(
            "manifest version mismatch: got {manifest_version:?}, want {version:?}"
        ));
    }
    let entry = doc
        .get("cffi")
        .and_then(|cffi| cffi.get(target))
        .ok_or_else(|| format!("manifest {version} has no CFFI artifact for {target}"))?;
    let url = entry
        .get("url")
        .and_then(serde_json::Value::as_str)
        .filter(|url| !url.trim().is_empty())
        .ok_or_else(|| format!("manifest entry for {target} has no url"))?;
    let sha256 = entry
        .get("sha256")
        .and_then(serde_json::Value::as_str)
        .filter(|sha| sha.len() == 64 && sha.bytes().all(|b| b.is_ascii_hexdigit()))
        .ok_or_else(|| {
            format!("manifest entry for {target} needs a sha256 of exactly 64 hex characters")
        })?;
    Ok(ManifestEntry {
        url: url.to_string(),
        sha256: sha256.to_ascii_lowercase(),
    })
}

fn get_bytes(agent: &ureq::Agent, url: &str, limit: u64) -> Result<Vec<u8>, LoaderError> {
    let mut response = agent
        .get(url)
        .call()
        .map_err(|e| LoaderError::DownloadFailed(format!("network error fetching {url}: {e}")))?;
    let status = response.status();
    if !status.is_success() {
        return Err(LoaderError::DownloadFailed(format!(
            "unexpected HTTP status {status} fetching {url}"
        )));
    }
    let mut body = Vec::new();
    response
        .body_mut()
        .as_reader()
        .take(limit + 1)
        .read_to_end(&mut body)
        .map_err(|e| LoaderError::DownloadFailed(format!("error reading {url}: {e}")))?;
    if body.len() as u64 > limit {
        return Err(LoaderError::DownloadFailed(format!(
            "{url} exceeds {limit} bytes"
        )));
    }
    Ok(body)
}

fn http_agent(version: &str) -> ureq::Agent {
    let config = ureq::Agent::config_builder()
        // Statuses are handled manually (404 vs other failures differ).
        .http_status_as_error(false)
        .timeout_global(Some(Duration::from_secs(300)))
        .timeout_connect(Some(Duration::from_secs(30)))
        .proxy(ureq::Proxy::try_from_env())
        .user_agent(format!(
            "baml-bridge-rust/{version} ({}/{})",
            std::env::consts::OS,
            std::env::consts::ARCH
        ))
        .build();
    ureq::Agent::new_with_config(config)
}

/// Create a uniquely named temp file in `dir` (same filesystem as the
/// destination, so the final rename is atomic).
fn create_temp_in(dir: &Path, filename: &str) -> Result<(PathBuf, std::fs::File), LoaderError> {
    static COUNTER: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
    let pid = std::process::id();
    for _ in 0..100 {
        let n = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let path = dir.join(format!("{filename}.{pid}-{n}.tmpdl"));
        match std::fs::File::create_new(&path) {
            Ok(file) => return Ok((path, file)),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(e) => {
                return Err(LoaderError::DownloadFailed(format!(
                    "failed to create temporary download file in {}: {e}",
                    dir.display()
                )));
            }
        }
    }
    Err(LoaderError::DownloadFailed(format!(
        "failed to create a unique temporary download file in {}",
        dir.display()
    )))
}

struct RemoveOnDrop(PathBuf);

impl Drop for RemoveOnDrop {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

fn copy_file(src: &Path, dst: &Path) -> std::io::Result<()> {
    std::fs::copy(src, dst)?;
    std::fs::File::options().write(true).open(dst)?.sync_all()?;
    Ok(())
}

/// Terminal download progress (Go loader parity: 40-column bar, 200ms
/// refresh, byte/rate formatting). Inert when stderr is not a terminal.
struct Progress {
    enabled: bool,
    total: Option<u64>,
    current: u64,
    start: Instant,
    last_update: Instant,
    description: String,
}

const PROGRESS_UPDATE_INTERVAL: Duration = Duration::from_millis(200);
const PROGRESS_WIDTH: u64 = 40;

impl Progress {
    fn new(total: Option<u64>, filename: &str) -> Self {
        let start = Instant::now();
        Self {
            enabled: std::io::stderr().is_terminal(),
            total,
            current: 0,
            start,
            last_update: start,
            description: format!("Downloading {filename}"),
        }
    }

    fn advance(&mut self, n: u64) {
        self.current += n;
        if !self.enabled {
            return;
        }
        let now = Instant::now();
        if now.duration_since(self.last_update) > PROGRESS_UPDATE_INTERVAL
            || Some(self.current) == self.total
        {
            self.print();
            self.last_update = now;
        }
    }

    fn finish(&mut self) {
        if !self.enabled {
            return;
        }
        if Some(self.current) != self.total {
            self.print();
        }
        let _ = writeln!(std::io::stderr().lock());
    }

    #[expect(
        clippy::cast_precision_loss,
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "display-only progress math; sizes far exceed f64-exact range only cosmetically"
    )]
    fn print(&self) {
        const WIDTH: usize = PROGRESS_WIDTH as usize;
        let (bar, percent_str, total_str) = match self.total {
            Some(total) if total > 0 => {
                let ratio = self.current as f64 / total as f64;
                let filled = ((ratio * WIDTH as f64).round() as usize).min(WIDTH);
                (
                    format!("{}{}", "=".repeat(filled), " ".repeat(WIDTH - filled)),
                    format!(" {:3.0}%", ratio * 100.0),
                    format!(" / {}", format_bytes(total)),
                )
            }
            _ => (" ".repeat(WIDTH), String::new(), " / ???".to_string()),
        };
        let elapsed = self.start.elapsed().as_secs_f64();
        let speed_str = if elapsed > 0.5 {
            format!(
                " ({}/s)",
                format_bytes((self.current as f64 / elapsed) as u64)
            )
        } else {
            String::new()
        };
        let _ = write!(
            std::io::stderr().lock(),
            "\r{} [{bar}] {}{total_str}{percent_str}{speed_str}    ",
            self.description,
            format_bytes(self.current),
        );
    }
}

#[expect(
    clippy::cast_precision_loss,
    reason = "display-only byte formatting to one decimal place"
)]
fn format_bytes(bytes: u64) -> String {
    const UNIT: u64 = 1024;
    if bytes < UNIT {
        return format!("{bytes} B");
    }
    let mut div = UNIT;
    let mut exp = 0;
    let mut n = bytes / UNIT;
    while n >= UNIT {
        div *= UNIT;
        exp += 1;
        n /= UNIT;
    }
    format!(
        "{:.1} {}iB",
        bytes as f64 / div as f64,
        ['K', 'M', 'G', 'T', 'P', 'E'][exp]
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn manifest(version: &str, target: &str, sha: &str) -> String {
        format!(
            r#"{{"schema":1,"version":"{version}","cffi":{{"{target}":{{"url":"https://x/lib","sha256":"{sha}"}}}}}}"#
        )
    }

    #[test]
    fn parses_a_valid_manifest_entry() {
        let sha = "AB".repeat(32);
        let entry = parse_manifest(&manifest("1.2.3", "t-a", &sha), "1.2.3", "t-a").unwrap();
        assert_eq!(entry.url, "https://x/lib");
        assert_eq!(entry.sha256, "ab".repeat(32));
    }

    #[test]
    fn rejects_bad_manifests() {
        let sha = "a".repeat(64);
        let good = manifest("1.2.3", "t-a", &sha);
        assert!(parse_manifest("not json", "1.2.3", "t-a").is_err());
        assert!(
            parse_manifest(
                &good.replace("\"schema\":1", "\"schema\":2"),
                "1.2.3",
                "t-a"
            )
            .unwrap_err()
            .contains("schema")
        );
        assert!(
            parse_manifest(&good, "9.9.9", "t-a")
                .unwrap_err()
                .contains("version mismatch")
        );
        assert!(
            parse_manifest(&good, "1.2.3", "t-b")
                .unwrap_err()
                .contains("no CFFI artifact")
        );
        assert!(
            parse_manifest(&manifest("1.2.3", "t-a", "short"), "1.2.3", "t-a")
                .unwrap_err()
                .contains("sha256")
        );
    }

    #[test]
    fn formats_bytes_in_binary_units() {
        assert_eq!(format_bytes(512), "512 B");
        assert_eq!(format_bytes(2048), "2.0 KiB");
        assert_eq!(format_bytes(5 * 1024 * 1024 + 512 * 1024), "5.5 MiB");
    }
}
