//! Download-and-install of the engine shared library into the cache.
//!
//! Mirrors the Go loader's flow: fetch the `.sha256` sidecar first (a
//! missing sidecar is a warning, not a failure — the artifact and its
//! checksum come from the same origin either way), stream the artifact to
//! a temp file in the destination directory while hashing, verify, then
//! atomically rename into place (copy fallback for filesystems that
//! reject the rename). A checksum *mismatch* is always a hard failure.

use std::{
    io::{IsTerminal, Read, Write},
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

use sha2::{Digest, Sha256};

use super::{LoaderEnv, LoaderError, log};

pub(super) fn download_library(
    env: &LoaderEnv,
    dest_dir: &Path,
    filename: &str,
) -> Result<(), LoaderError> {
    let base = env.download_base_url();
    let download_url = format!("{base}/{filename}");
    let checksum_url = format!("{base}/{filename}.sha256");
    let dest_path = dest_dir.join(filename);
    log::debug(&format!(
        "Downloading BAML library from {download_url} to {}",
        dest_path.display()
    ));

    let agent = http_agent(&env.version);

    let expected_checksum = match fetch_checksum(&agent, &checksum_url, filename) {
        Ok(checksum) => {
            log::debug("Checksum found. Will verify after download.");
            Some(checksum)
        }
        Err(e) => {
            log::warn(&format!(
                "Could not get checksum from {checksum_url}: {e}. \
                 Download will proceed without verification."
            ));
            None
        }
    };

    let mut response = agent.get(&download_url).call().map_err(|e| {
        LoaderError::DownloadFailed(format!("network error fetching {download_url}: {e}"))
    })?;
    let status = response.status();
    if status == 404 {
        return Err(LoaderError::DownloadFailed(format!(
            "library file not found at {download_url} (HTTP 404); \
             check release tag baml-language-{} and filename {filename}",
            env.version
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
    let (tmp_path, mut tmp_file) = create_temp_in(dest_dir, filename)?;
    // Remove the temp file on every exit path; after a successful rename
    // the removal quietly finds nothing.
    let _cleanup = RemoveOnDrop(tmp_path.clone());

    let mut hasher = Sha256::new();
    let mut progress = Progress::new(content_length, filename);
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
    if let Some(expected) = &expected_checksum {
        log::debug("Verifying checksum");
        if !actual_checksum.eq_ignore_ascii_case(expected) {
            return Err(LoaderError::ChecksumMismatch(format!(
                "checksum mismatch for {filename}: expected {expected}, got {actual_checksum}; \
                 the downloaded file may be corrupt"
            )));
        }
        log::info(&format!(
            "Checksum verified successfully ({})",
            &actual_checksum[..8]
        ));
    } else if content_length.is_some_and(|len| len > 0) {
        log::warn("Checksum verification skipped (checksum file not found or download failed)");
    }

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
    if let Err(rename_err) = std::fs::rename(&tmp_path, &dest_path) {
        log::warn(&format!(
            "Atomic rename failed ({rename_err}), attempting copy fallback"
        ));
        copy_file(&tmp_path, &dest_path).map_err(|copy_err| {
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
        if let Err(e) = std::fs::set_permissions(&dest_path, std::fs::Permissions::from_mode(0o755))
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

/// Fetch and parse the `.sha256` sidecar. All failures are strings for the
/// caller to downgrade to a warning (Go parity: a missing checksum does
/// not block the download).
fn fetch_checksum(
    agent: &ureq::Agent,
    checksum_url: &str,
    target_filename: &str,
) -> Result<String, String> {
    let mut response = agent
        .get(checksum_url)
        .call()
        .map_err(|e| format!("network error fetching checksum: {e}"))?;
    let status = response.status();
    if status == 404 {
        return Err("checksum file not found (404)".to_string());
    }
    if !status.is_success() {
        return Err(format!("unexpected status {status} fetching checksum"));
    }
    let mut body = String::new();
    response
        .body_mut()
        .as_reader()
        .take(4096)
        .read_to_string(&mut body)
        .map_err(|e| format!("error reading checksum body: {e}"))?;
    parse_checksum_file(&body, target_filename)
}

/// Parse `sha256sum`-style sidecar content: one `<hex> <filename>` pair
/// per line (a leading `*` on the filename marks binary mode). The first
/// line naming the target decides: valid 64-digit hex wins, anything else
/// is a format error.
fn parse_checksum_file(content: &str, target_filename: &str) -> Result<String, String> {
    for line in content.lines() {
        let mut parts = line.split_whitespace();
        if let (Some(checksum), Some(name)) = (parts.next(), parts.next()) {
            let name = name.strip_prefix('*').unwrap_or(name);
            if name == target_filename {
                if checksum.len() == 64 && checksum.bytes().all(|b| b.is_ascii_hexdigit()) {
                    return Ok(checksum.to_ascii_lowercase());
                }
                return Err(format!(
                    "invalid checksum format '{checksum}' for {target_filename}"
                ));
            }
        }
    }
    Err(format!(
        "checksum for '{target_filename}' not found within the checksum file"
    ))
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

    #[test]
    fn parses_sha256sum_style_lines() {
        let hash = "a".repeat(64);
        let content = format!(
            "{hash}  libbaml_cffi-x.dylib\n{}  other.so\n",
            "b".repeat(64)
        );
        assert_eq!(
            parse_checksum_file(&content, "libbaml_cffi-x.dylib").unwrap(),
            hash
        );
        assert_eq!(
            parse_checksum_file(&content, "other.so").unwrap(),
            "b".repeat(64)
        );
    }

    #[test]
    fn strips_binary_mode_marker_and_lowercases() {
        let content = format!("{}  *lib.so\n", "AB".repeat(32));
        assert_eq!(
            parse_checksum_file(&content, "lib.so").unwrap(),
            "ab".repeat(32)
        );
    }

    #[test]
    fn rejects_invalid_hex_for_the_target() {
        let content = "not-a-checksum  lib.so\n";
        let err = parse_checksum_file(content, "lib.so").unwrap_err();
        assert!(err.contains("invalid checksum format"), "{err}");
    }

    #[test]
    fn reports_missing_entry() {
        let content = format!("{}  other.so\n", "c".repeat(64));
        let err = parse_checksum_file(&content, "lib.so").unwrap_err();
        assert!(err.contains("not found"), "{err}");
    }

    #[test]
    fn formats_bytes_in_binary_units() {
        assert_eq!(format_bytes(512), "512 B");
        assert_eq!(format_bytes(2048), "2.0 KiB");
        assert_eq!(format_bytes(5 * 1024 * 1024 + 512 * 1024), "5.5 MiB");
    }
}
