use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub(crate) struct Config {
    /// Directory containing per-platform baseline TOML files.
    /// Default: ".ci/size-gate"
    #[serde(default = "default_baseline_dir")]
    pub baseline_dir: PathBuf,

    /// Artifact definitions keyed by name.
    #[serde(default)]
    pub artifacts: BTreeMap<String, ArtifactConfig>,
}

fn default_baseline_dir() -> PathBuf {
    PathBuf::from(".ci/size-gate")
}

#[derive(Debug, Deserialize)]
pub(crate) struct ArtifactConfig {
    /// Cargo package name (e.g., `bridge_cffi`).
    pub package: String,

    /// Target triple. None means native host target.
    pub target: Option<String>,

    /// Pass --no-default-features to cargo build.
    #[serde(default)]
    pub no_default_features: bool,

    /// Additional features to enable.
    #[serde(default)]
    pub features: Vec<String>,

    /// Whether this is a WASM artifact (skip strip, only `file_bytes` + `gzip_bytes`).
    #[serde(default)]
    pub wasm: bool,

    /// Policy thresholds.
    #[serde(default)]
    pub policy: Policy,
}

#[derive(Debug, Default, Deserialize)]
#[allow(clippy::struct_field_names)]
pub(crate) struct Policy {
    /// Maximum allowed gzip bytes (absolute ceiling).
    pub max_gzip_bytes: Option<u64>,

    /// Maximum allowed stripped file bytes.
    pub max_stripped_bytes: Option<u64>,

    /// Maximum allowed gzip delta in bytes vs baseline.
    pub max_gzip_delta_bytes: Option<i64>,

    /// Maximum allowed growth percentage vs baseline (e.g., 5.0 = 5%).
    pub max_delta_pct: Option<f64>,
}

impl Config {
    pub(crate) fn load(workspace_root: &Path) -> Result<Self> {
        let config_path = workspace_root.join(".cargo/size-gate.toml");
        let content = std::fs::read_to_string(&config_path)
            .with_context(|| format!("failed to read config: {}", config_path.display()))?;
        let config: Config =
            toml::from_str(&content).with_context(|| "failed to parse size-gate.toml")?;
        Ok(config)
    }

    /// Return the resolved platform for a given artifact config.
    /// WASM artifacts use their explicit target; native artifacts use the host triple.
    pub(crate) fn platform_for_artifact(artifact: &ArtifactConfig) -> String {
        artifact.target.clone().unwrap_or_else(host_triple)
    }
}

/// Returns the host target triple (e.g., "aarch64-apple-darwin").
/// Uses the `CARGO_BUILD_TARGET` env var if set, otherwise constructs from compile-time constants.
pub(crate) fn host_triple() -> String {
    if let Ok(target) = std::env::var("CARGO_BUILD_TARGET") {
        return target;
    }

    // Construct from compile-time arch/os/env
    let arch = std::env::consts::ARCH;
    let os = std::env::consts::OS;

    match (arch, os) {
        ("aarch64", "macos") => "aarch64-apple-darwin".to_owned(),
        ("x86_64", "macos") => "x86_64-apple-darwin".to_owned(),
        ("x86_64", "linux") => "x86_64-unknown-linux-gnu".to_owned(),
        ("aarch64", "linux") => "aarch64-unknown-linux-gnu".to_owned(),
        ("x86_64", "windows") => "x86_64-pc-windows-msvc".to_owned(),
        ("aarch64", "windows") => "aarch64-pc-windows-msvc".to_owned(),
        _ => format!("{arch}-unknown-{os}"),
    }
}
