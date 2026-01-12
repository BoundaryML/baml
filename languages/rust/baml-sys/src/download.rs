//! Library download from GitHub releases.

// Allow stderr output for download progress messages - this is expected behavior
// for a library that downloads files and provides user feedback.
#![allow(clippy::print_stderr)]

use std::{
    fs::File,
    io::{Read, Write},
    path::Path,
};

use sha2::{Digest, Sha256};

use crate::error::{BamlSysError, Result};

/// Download the library from GitHub releases.
pub(crate) fn download_library(
    dest_dir: &Path,
    filename: &str,
    version: &str,
    github_repo: &str,
) -> Result<()> {
    let download_url =
        format!("https://github.com/{github_repo}/releases/download/{version}/{filename}");
    let checksum_url = format!("{download_url}.sha256");
    let dest_path = dest_dir.join(filename);

    eprintln!("Downloading BAML library from {download_url}...");

    // Try to get checksum (optional)
    let expected_checksum = download_checksum(&checksum_url, filename).ok();

    // Download the library
    let response = ureq::get(&download_url)
        .set(
            "User-Agent",
            &format!(
                "baml-sys/{} ({}/{})",
                version,
                std::env::consts::OS,
                std::env::consts::ARCH
            ),
        )
        .call()
        .map_err(|e| match e {
            ureq::Error::Status(404, _) => BamlSysError::DownloadFailed(format!(
                "Library not found at {download_url} (HTTP 404). Check release tag '{version}'"
            )),
            ureq::Error::Status(code, _) => {
                BamlSysError::DownloadFailed(format!("HTTP error {code} fetching {download_url}"))
            }
            ureq::Error::Transport(e) => {
                BamlSysError::DownloadFailed(format!("Network error: {e}"))
            }
        })?;

    // Create temp file in same directory (for atomic rename)
    let temp_path = dest_dir.join(format!("{filename}.tmp"));
    let mut temp_file = File::create(&temp_path)?;
    let mut hasher = Sha256::new();

    // Read and hash simultaneously
    let mut reader = response.into_reader();
    let mut buffer = [0u8; 8192];
    loop {
        let bytes_read = reader.read(&mut buffer)?;
        if bytes_read == 0 {
            break;
        }
        temp_file.write_all(&buffer[..bytes_read])?;
        hasher.update(&buffer[..bytes_read]);
    }
    temp_file.sync_all()?;
    drop(temp_file);

    // Verify checksum if available
    let actual_checksum = hex::encode(hasher.finalize());
    if let Some(expected) = expected_checksum {
        if actual_checksum != expected {
            // Clean up temp file
            let _ = std::fs::remove_file(&temp_path);
            return Err(BamlSysError::ChecksumMismatch {
                expected,
                actual: actual_checksum,
            });
        }
        eprintln!("Checksum verified: {}", &actual_checksum[..8]);
    }

    // Atomic rename to final location
    std::fs::rename(&temp_path, &dest_path).or_else(|_| {
        // Fallback: copy if rename fails (cross-filesystem)
        std::fs::copy(&temp_path, &dest_path)?;
        std::fs::remove_file(&temp_path)?;
        Ok::<_, std::io::Error>(())
    })?;

    // Set executable permissions on Unix
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&dest_path, std::fs::Permissions::from_mode(0o755))?;
    }

    eprintln!(
        "Successfully downloaded BAML library to {}",
        dest_path.display()
    );
    Ok(())
}

/// Download and parse the checksum file.
fn download_checksum(checksum_url: &str, target_filename: &str) -> Result<String> {
    let response = ureq::get(checksum_url)
        .call()
        .map_err(|e| BamlSysError::DownloadFailed(format!("Failed to fetch checksum: {e}")))?;

    let mut body = String::new();
    response
        .into_reader()
        .take(4096)
        .read_to_string(&mut body)?;

    // Parse checksum file (format: "CHECKSUM *FILENAME" or "CHECKSUM FILENAME")
    for line in body.lines() {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() >= 2 {
            let checksum = parts[0];
            let filename = parts[1].trim_start_matches('*');
            if filename == target_filename && checksum.len() == 64 && is_hex(checksum) {
                return Ok(checksum.to_lowercase());
            }
        }
    }

    Err(BamlSysError::DownloadFailed(format!(
        "Checksum for '{target_filename}' not found in checksum file"
    )))
}

/// Check if a string is valid hex.
fn is_hex(s: &str) -> bool {
    !s.is_empty() && s.chars().all(|c| c.is_ascii_hexdigit())
}
