use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result};
use serde::{Deserialize, Deserializer, de};

use crate::human_size;

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

/// Which measured size an artifact is *gated* on: the absolute ceiling
/// and the baseline delta both use this metric, and the report flags it.
///
/// Binaries gate on `File` (the installed/shipped on-disk size); WASM
/// gates on `Gzip` (what's actually shipped over the wire). The other
/// size is still measured and shown, just for information.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum GateMetric {
    /// On-disk file size. Default.
    #[default]
    File,
    /// Gzip-compressed size (download size).
    Gzip,
}

impl GateMetric {
    /// Lowercase label used in reports and messages (`file` / `gzip`).
    pub(crate) fn label(self) -> &'static str {
        match self {
            GateMetric::File => "file",
            GateMetric::Gzip => "gzip",
        }
    }
}

#[derive(Debug, Deserialize)]
pub(crate) struct ArtifactConfig {
    /// Artifact kind. Defaults to `cdylib` for backwards compatibility.
    #[serde(default)]
    pub kind: ArtifactKind,

    /// Which measured size this artifact is gated on. Defaults to `file`
    /// (the shipped on-disk size); WASM sets `gzip` (the download size).
    #[serde(default)]
    pub gate: GateMetric,

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
    #[serde(default, deserialize_with = "deserialize_optional_size")]
    pub max_file_bytes: Option<u64>,

    /// Maximum allowed gzip bytes (absolute ceiling). Optional secondary
    /// gate; gzip is recorded and displayed for visibility either way.
    #[serde(default, deserialize_with = "deserialize_optional_size")]
    pub max_gzip_bytes: Option<u64>,

    /// Maximum allowed stripped file bytes (absolute ceiling).
    #[serde(default, deserialize_with = "deserialize_optional_size")]
    pub max_stripped_bytes: Option<u64>,

    /// Maximum allowed file-size delta in bytes vs baseline.
    pub max_delta_bytes: Option<i64>,

    /// Maximum allowed file-size growth percentage vs baseline
    /// (e.g., 3.0 = 3%).
    pub max_delta_pct: Option<f64>,
}

fn deserialize_optional_size<'de, D>(deserializer: D) -> std::result::Result<Option<u64>, D::Error>
where
    D: Deserializer<'de>,
{
    Option::<String>::deserialize(deserializer)?
        .map(|value| parse_size(&value).map_err(de::Error::custom))
        .transpose()
}

fn parse_size(value: &str) -> Result<u64> {
    human_size::parse(value)
        .map_err(anyhow::Error::msg)
        .with_context(|| format!("invalid size threshold `{value}`"))
}

impl Config {
    pub(crate) fn load(workspace_root: &Path) -> Result<Self> {
        let config_path = workspace_root.join(".cargo/size-gate.toml");
        let content = std::fs::read_to_string(&config_path)
            .with_context(|| format!("failed to read config: {}", config_path.display()))?;
        let config: Config =
            toml::from_str(&content).with_context(|| "failed to parse size-gate.toml")?;
        config.validate()?;
        Ok(config)
    }

    /// Reject configurations the build path would silently ignore.
    fn validate(&self) -> Result<()> {
        for (name, artifact) in &self.artifacts {
            if artifact.kind == ArtifactKind::Pack {
                // `pack` builds the CLI + host from its [pack] table on the
                // host target; the generic cargo build flags don't apply and
                // would be silently ignored, so reject them up front rather
                // than measure an artifact that diverges from the config.
                let ignored = [
                    ("package", artifact.package.is_some()),
                    ("target", artifact.target.is_some()),
                    ("no_default_features", artifact.no_default_features),
                    ("features", !artifact.features.is_empty()),
                ];
                if let Some((field, _)) = ignored.iter().find(|(_, set)| *set) {
                    anyhow::bail!(
                        "artifact `{name}` (kind = \"pack\") sets `{field}`, which is ignored \
                         for pack artifacts — configure the build via [artifacts.{name}.pack] \
                         instead"
                    );
                }
                // Surface a missing [pack] table at load time, not mid-build.
                artifact
                    .pack
                    .as_ref()
                    .with_context(|| format!("artifact `{name}` (kind = \"pack\") is missing a [artifacts.{name}.pack] table"))?;
            }
        }
        Ok(())
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_human_readable_size_thresholds() {
        assert_eq!(parse_size("1.5 KiB").unwrap(), 1536);
        assert_eq!(parse_size("20.94 MiB").unwrap(), 21_957_181);
        assert_eq!(parse_size("2 GiB").unwrap(), 2_147_483_648);
        assert_eq!(parse_size("1 MB").unwrap(), 1_000_000);
        assert_eq!(parse_size("1024").unwrap(), 1024);
    }

    #[test]
    fn rejects_invalid_size_thresholds() {
        assert!(parse_size("-1 MiB").is_err());
        assert!(parse_size("not a size").is_err());
    }

    #[test]
    fn policy_accepts_human_readable_thresholds() {
        let human: Policy = toml::from_str(r#"max_file_bytes = "1.5 MiB""#).unwrap();
        assert_eq!(human.max_file_bytes, Some(1_572_864));
    }

    #[test]
    fn policy_rejects_integer_thresholds() {
        assert!(toml::from_str::<Policy>("max_file_bytes = 1_572_864").is_err());
    }

    #[test]
    fn checked_in_config_uses_valid_human_readable_thresholds() {
        let config: Config =
            toml::from_str(include_str!("../../../.cargo/size-gate.toml")).unwrap();
        config.validate().unwrap();

        let macos = &config.artifacts["baml-cli"].platform["aarch64-apple-darwin"];
        assert_eq!(macos.max_file_bytes, Some(26_633_830));
    }
}
