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

/// What kind of artifact we build and measure.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum ArtifactKind {
    /// A `cdylib` (native `.so`/`.dylib`/`.dll`, or a `.wasm` when
    /// `wasm = true`). The historical default.
    #[default]
    Cdylib,

    /// An executable produced by `cargo build --bin`.
    Bin,

    /// A standalone executable produced by running `baml pack` on a
    /// fixture — i.e. the size a user actually ships with `baml pack`.
    Pack,
}

#[derive(Debug, Deserialize)]
pub(crate) struct ArtifactConfig {
    /// Artifact kind. Defaults to `cdylib` for backwards compatibility.
    #[serde(default)]
    pub kind: ArtifactKind,

    /// Cargo package name (e.g., `bridge_wasm`). Required for `cdylib`
    /// and `bin`; ignored for `pack` (which uses `[artifacts.x.pack]`).
    pub package: Option<String>,

    /// Binary name for `kind = "bin"`. Defaults to the `package` name.
    pub bin: Option<String>,

    /// Pack configuration. Required for `kind = "pack"`.
    pub pack: Option<PackConfig>,

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

    /// Base policy thresholds, applied to every platform.
    #[serde(default)]
    pub policy: Policy,

    /// Per-platform policy overrides, keyed by target triple. Any field
    /// set here wins over `policy` for that platform. Absolute ceilings
    /// (`max_gzip_bytes`) live here so each platform can be capped tight
    /// to its own baseline rather than to the largest platform.
    #[serde(default)]
    pub platform: BTreeMap<String, Policy>,
}

/// Configuration for a `kind = "pack"` artifact: build the CLI and the
/// pack host, then run `baml pack <function>` on `fixture`.
#[derive(Debug, Deserialize)]
pub(crate) struct PackConfig {
    /// Cargo package providing the `baml` CLI used to run `baml pack`.
    pub cli_package: String,
    /// Binary name of the CLI within `cli_package`.
    pub cli_bin: String,
    /// Cargo package providing the pack host binary. The built host must
    /// sit next to the CLI binary so `baml pack` finds it without a
    /// network download.
    pub host_package: String,
    /// Binary name of the pack host within `host_package`.
    pub host_bin: String,
    /// `.baml` source to pack, relative to the workspace root.
    pub fixture: PathBuf,
    /// Function name to pack as the binary's only entry point.
    pub function: String,
}

impl ArtifactConfig {
    /// True if this artifact is a WASM build (explicit `wasm = true` or a
    /// `wasm32-*` target). WASM artifacts are never stripped.
    pub(crate) fn is_wasm(&self) -> bool {
        self.wasm || self.target.as_deref() == Some("wasm32-unknown-unknown")
    }

    /// Whether the measured artifact should be stripped before sizing.
    /// Native cdylibs and binaries are stripped; WASM and `pack` outputs
    /// are measured as-is (stripping a packed binary would also drop its
    /// appended bytecode section).
    pub(crate) fn should_strip(&self) -> bool {
        match self.kind {
            ArtifactKind::Bin => true,
            ArtifactKind::Cdylib => !self.is_wasm(),
            ArtifactKind::Pack => false,
        }
    }

    /// The cargo package name, erroring if absent (required for non-pack
    /// kinds).
    pub(crate) fn require_package(&self) -> Result<&str> {
        self.package
            .as_deref()
            .context("artifact is missing required `package` field")
    }

    /// The pack configuration, erroring if absent (required for `pack`).
    pub(crate) fn require_pack(&self) -> Result<&PackConfig> {
        self.pack
            .as_ref()
            .context("artifact with kind = \"pack\" is missing a [artifacts.*.pack] table")
    }

    /// The effective policy for `platform`: the base `policy` with any
    /// per-platform overrides layered on top (set fields win).
    pub(crate) fn effective_policy(&self, platform: &str) -> Policy {
        let mut effective = self.policy.clone();
        if let Some(over) = self.platform.get(platform) {
            if over.max_file_bytes.is_some() {
                effective.max_file_bytes = over.max_file_bytes;
            }
            if over.max_gzip_bytes.is_some() {
                effective.max_gzip_bytes = over.max_gzip_bytes;
            }
            if over.max_stripped_bytes.is_some() {
                effective.max_stripped_bytes = over.max_stripped_bytes;
            }
            if over.max_delta_bytes.is_some() {
                effective.max_delta_bytes = over.max_delta_bytes;
            }
            if over.max_delta_pct.is_some() {
                effective.max_delta_pct = over.max_delta_pct;
            }
        }
        effective
    }
}

#[derive(Debug, Default, Clone, Deserialize)]
#[allow(clippy::struct_field_names)]
pub(crate) struct Policy {
    /// Maximum allowed on-disk file bytes (absolute ceiling). This is the
    /// primary gate — the actual size a user installs/ships.
    pub max_file_bytes: Option<u64>,

    /// Maximum allowed gzip bytes (absolute ceiling). Optional secondary
    /// gate; gzip is recorded and displayed for visibility either way.
    pub max_gzip_bytes: Option<u64>,

    /// Maximum allowed stripped file bytes (absolute ceiling).
    pub max_stripped_bytes: Option<u64>,

    /// Maximum allowed file-size delta in bytes vs baseline.
    pub max_delta_bytes: Option<i64>,

    /// Maximum allowed file-size growth percentage vs baseline
    /// (e.g., 3.0 = 3%).
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
