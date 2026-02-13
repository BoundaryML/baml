use std::{
    collections::BTreeMap,
    io::Write,
    path::{Path, PathBuf},
    process::Command,
};

use anyhow::{Context, Result, bail};
use flate2::{Compression, write::GzEncoder};
use serde::{Deserialize, Serialize};

use crate::config::ArtifactConfig;

/// Measurements for a single artifact.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct ArtifactMeasurement {
    pub file_bytes: u64,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub stripped_bytes: Option<u64>,

    pub gzip_bytes: u64,

    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub sections: BTreeMap<String, u64>,
}

/// Build a single artifact and return the path to the output file.
pub(crate) fn build_artifact(workspace_root: &Path, config: &ArtifactConfig) -> Result<PathBuf> {
    let mut cmd = Command::new("cargo");
    cmd.arg("build")
        .arg("--release")
        .arg("-p")
        .arg(&config.package);

    if let Some(target) = &config.target {
        cmd.arg("--target").arg(target);
    }

    if config.no_default_features {
        cmd.arg("--no-default-features");
    }

    for feat in &config.features {
        cmd.arg("--features").arg(feat);
    }

    cmd.current_dir(workspace_root);

    eprintln!(
        "  building {} ({})",
        config.package,
        if config.no_default_features {
            "no-default-features"
        } else {
            "default features"
        }
    );

    let status = cmd.status().context("failed to run cargo build")?;
    if !status.success() {
        bail!("cargo build failed for {}", config.package);
    }

    locate_artifact(workspace_root, config)
}

/// Find the built artifact in the target directory (public for --no-build mode).
pub(crate) fn locate_artifact_public(
    workspace_root: &Path,
    config: &ArtifactConfig,
) -> Result<PathBuf> {
    locate_artifact(workspace_root, config)
}

/// Find the built artifact in the target directory.
fn locate_artifact(workspace_root: &Path, config: &ArtifactConfig) -> Result<PathBuf> {
    let lib_name = resolve_lib_name(workspace_root, &config.package)?;

    let (dir, filename) =
        if config.wasm || config.target.as_deref() == Some("wasm32-unknown-unknown") {
            let dir = workspace_root.join("target/wasm32-unknown-unknown/release");
            // WASM cdylib uses the package name with underscores (Cargo convention)
            let wasm_name = config.package.replace('-', "_");
            (dir, format!("{wasm_name}.wasm"))
        } else if let Some(target) = &config.target {
            let dir = workspace_root.join(format!("target/{target}/release"));
            (dir, native_lib_filename(&lib_name))
        } else {
            let dir = workspace_root.join("target/release");
            (dir, native_lib_filename(&lib_name))
        };

    let path = dir.join(&filename);
    if !path.exists() {
        bail!(
            "artifact not found: {} (expected at {})",
            filename,
            path.display()
        );
    }
    Ok(path)
}

/// Resolve the lib name for a package using cargo metadata.
fn resolve_lib_name(workspace_root: &Path, package_name: &str) -> Result<String> {
    let output = Command::new("cargo")
        .args(["metadata", "--no-deps", "--format-version", "1"])
        .current_dir(workspace_root)
        .output()
        .context("failed to run cargo metadata")?;

    if !output.status.success() {
        bail!("cargo metadata failed");
    }

    let json: serde_json::Value =
        serde_json::from_slice(&output.stdout).context("failed to parse cargo metadata")?;

    let packages = json["packages"]
        .as_array()
        .context("no packages in metadata")?;

    for pkg in packages {
        if pkg["name"].as_str() == Some(package_name) {
            if let Some(targets) = pkg["targets"].as_array() {
                for target in targets {
                    let kinds = target["kind"].as_array();
                    let is_cdylib = kinds
                        .map(|k| k.iter().any(|v| v.as_str() == Some("cdylib")))
                        .unwrap_or(false);
                    if is_cdylib {
                        if let Some(name) = target["name"].as_str() {
                            return Ok(name.to_owned());
                        }
                    }
                }
            }
            // Fallback: use package name with hyphens replaced
            return Ok(package_name.replace('-', "_"));
        }
    }

    bail!("package '{package_name}' not found in cargo metadata");
}

/// Platform-specific dynamic library filename.
fn native_lib_filename(lib_name: &str) -> String {
    if cfg!(target_os = "macos") {
        format!("lib{lib_name}.dylib")
    } else if cfg!(target_os = "windows") {
        format!("{lib_name}.dll")
    } else {
        format!("lib{lib_name}.so")
    }
}

/// Measure a built artifact.
pub(crate) fn measure_artifact(artifact_path: &Path, is_wasm: bool) -> Result<ArtifactMeasurement> {
    let file_bytes = std::fs::metadata(artifact_path)
        .with_context(|| format!("failed to stat {}", artifact_path.display()))?
        .len();

    let (stripped_bytes, gzip_source) = if is_wasm {
        // WASM: no strip needed (Cargo profile already strips), gzip the original
        (None, artifact_path.to_path_buf())
    } else {
        // Native: strip to a temp copy, then gzip the stripped version
        let stripped_path = strip_artifact(artifact_path)?;
        let stripped_size = std::fs::metadata(&stripped_path)
            .with_context(|| format!("failed to stat stripped {}", stripped_path.display()))?
            .len();
        (Some(stripped_size), stripped_path)
    };

    let gzip_bytes = gzip_size(&gzip_source)?;

    // Clean up stripped temp file if it exists and is different from the original
    if gzip_source != artifact_path {
        let _ = std::fs::remove_file(&gzip_source);
    }

    let sections = parse_sections(artifact_path).unwrap_or_default();

    Ok(ArtifactMeasurement {
        file_bytes,
        stripped_bytes,
        gzip_bytes,
        sections,
    })
}

/// Strip a native artifact, returning the path to the stripped copy.
fn strip_artifact(path: &Path) -> Result<PathBuf> {
    let stripped = path.with_extension("stripped");
    std::fs::copy(path, &stripped)
        .with_context(|| format!("failed to copy {} for stripping", path.display()))?;

    let (program, args): (&str, &[&str]) = if cfg!(target_os = "macos") {
        ("strip", &["-x"])
    } else if cfg!(target_os = "windows") {
        ("llvm-strip", &["--strip-unneeded"])
    } else {
        ("strip", &["--strip-unneeded"])
    };

    let status = Command::new(program)
        .args(args)
        .arg(&stripped)
        .status()
        .with_context(|| format!("failed to run {program}"))?;

    if !status.success() {
        // Try llvm-strip as fallback on Linux
        if cfg!(target_os = "linux") {
            let fallback = Command::new("llvm-strip").arg("-x").arg(&stripped).status();
            if let Ok(s) = fallback {
                if s.success() {
                    return Ok(stripped);
                }
            }
        }
        eprintln!("  warning: strip failed, using unstripped size");
        // Return the copy as-is (unstripped)
    }

    Ok(stripped)
}

/// Compute gzip size of a file in memory (no temp file).
fn gzip_size(path: &Path) -> Result<u64> {
    let data = std::fs::read(path).with_context(|| format!("failed to read {}", path.display()))?;
    let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
    encoder
        .write_all(&data)
        .context("failed to gzip compress")?;
    let compressed = encoder.finish().context("failed to finish gzip")?;
    Ok(compressed.len() as u64)
}

/// Parse section sizes from the binary (best-effort).
fn parse_sections(path: &Path) -> Result<BTreeMap<String, u64>> {
    if cfg!(target_os = "macos") {
        parse_macos_sections(path)
    } else if cfg!(target_os = "linux") {
        parse_linux_sections(path)
    } else {
        Ok(BTreeMap::new())
    }
}

/// Parse macOS Mach-O sections via `size -m`.
fn parse_macos_sections(path: &Path) -> Result<BTreeMap<String, u64>> {
    let output = Command::new("size")
        .arg("-m")
        .arg(path)
        .output()
        .context("failed to run size -m")?;

    if !output.status.success() {
        return Ok(BTreeMap::new());
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut sections = BTreeMap::new();

    // Parse lines like "Segment __TEXT: 3145728"
    for line in stdout.lines() {
        let line = line.trim();
        if let Some(rest) = line.strip_prefix("Segment ") {
            if let Some((name, size_str)) = rest.split_once(": ") {
                if let Ok(size) = size_str.trim().parse::<u64>() {
                    sections.insert(name.trim().to_owned(), size);
                }
            }
        }
    }

    Ok(sections)
}

/// Parse Linux ELF sections via `size -A`.
fn parse_linux_sections(path: &Path) -> Result<BTreeMap<String, u64>> {
    let output = Command::new("size")
        .arg("-A")
        .arg(path)
        .output()
        .context("failed to run size -A")?;

    if !output.status.success() {
        return Ok(BTreeMap::new());
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut sections = BTreeMap::new();

    // Parse lines like ".text    3145728    0"
    for line in stdout.lines() {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() >= 2 && parts[0].starts_with('.') {
            if let Ok(size) = parts[1].parse::<u64>() {
                if size > 0 {
                    sections.insert(parts[0].to_owned(), size);
                }
            }
        }
    }

    Ok(sections)
}
