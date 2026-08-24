use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::{human_size, measure::ArtifactMeasurement};

/// A platform baseline file containing measurements for all artifacts on that platform.
#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct PlatformBaseline {
    pub version: u32,
    pub recorded_at: String,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub git_sha: Option<String>,

    #[serde(
        default,
        serialize_with = "serialize_artifacts",
        deserialize_with = "deserialize_artifacts"
    )]
    pub artifacts: BTreeMap<String, ArtifactMeasurement>,
}

#[derive(Serialize, Deserialize)]
struct HumanReadableArtifactMeasurement {
    #[serde(with = "human_size::required")]
    file_bytes: u64,

    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "human_size::optional"
    )]
    stripped_bytes: Option<u64>,

    #[serde(with = "human_size::required")]
    gzip_bytes: u64,

    #[serde(
        default,
        skip_serializing_if = "BTreeMap::is_empty",
        with = "human_size::map"
    )]
    sections: BTreeMap<String, u64>,
}

impl From<&ArtifactMeasurement> for HumanReadableArtifactMeasurement {
    fn from(measurement: &ArtifactMeasurement) -> Self {
        Self {
            file_bytes: measurement.file_bytes,
            stripped_bytes: measurement.stripped_bytes,
            gzip_bytes: measurement.gzip_bytes,
            sections: measurement.sections.clone(),
        }
    }
}

impl From<HumanReadableArtifactMeasurement> for ArtifactMeasurement {
    fn from(measurement: HumanReadableArtifactMeasurement) -> Self {
        Self {
            file_bytes: measurement.file_bytes,
            stripped_bytes: measurement.stripped_bytes,
            gzip_bytes: measurement.gzip_bytes,
            sections: measurement.sections,
        }
    }
}

#[derive(PartialEq, Eq)]
struct DisplayedArtifactMeasurement {
    file_bytes: String,
    stripped_bytes: Option<String>,
    gzip_bytes: String,
    sections: BTreeMap<String, String>,
}

impl From<&ArtifactMeasurement> for DisplayedArtifactMeasurement {
    fn from(measurement: &ArtifactMeasurement) -> Self {
        Self {
            file_bytes: human_size::format(measurement.file_bytes),
            stripped_bytes: measurement.stripped_bytes.map(human_size::format),
            gzip_bytes: human_size::format(measurement.gzip_bytes),
            sections: measurement
                .sections
                .iter()
                .map(|(name, bytes)| (name.clone(), human_size::format(*bytes)))
                .collect(),
        }
    }
}

fn serialize_artifacts<S>(
    artifacts: &BTreeMap<String, ArtifactMeasurement>,
    serializer: S,
) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    artifacts
        .iter()
        .map(|(name, measurement)| (name, HumanReadableArtifactMeasurement::from(measurement)))
        .collect::<BTreeMap<_, _>>()
        .serialize(serializer)
}

fn deserialize_artifacts<'de, D>(
    deserializer: D,
) -> Result<BTreeMap<String, ArtifactMeasurement>, D::Error>
where
    D: Deserializer<'de>,
{
    BTreeMap::<String, HumanReadableArtifactMeasurement>::deserialize(deserializer).map(
        |artifacts| {
            artifacts
                .into_iter()
                .map(|(name, measurement)| (name, measurement.into()))
                .collect()
        },
    )
}

impl PlatformBaseline {
    /// Read a platform baseline from disk. Returns None if the file doesn't exist.
    pub(crate) fn load(path: &Path) -> Result<Option<Self>> {
        if !path.exists() {
            return Ok(None);
        }
        let content = std::fs::read_to_string(path)
            .with_context(|| format!("failed to read baseline: {}", path.display()))?;
        let baseline: PlatformBaseline = toml::from_str(&content)
            .with_context(|| format!("failed to parse {}", path.display()))?;
        Ok(Some(baseline))
    }

    /// Write the baseline to disk, creating parent directories as needed.
    pub(crate) fn save(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("failed to create dir: {}", parent.display()))?;
        }
        let content =
            toml::to_string_pretty(self).context("failed to serialize baseline to TOML")?;
        std::fs::write(path, content)
            .with_context(|| format!("failed to write baseline: {}", path.display()))?;
        Ok(())
    }

    /// Whether `measurements` would serialize to the sizes already stored in this baseline.
    pub(crate) fn measurements_match(
        &self,
        measurements: &BTreeMap<String, ArtifactMeasurement>,
    ) -> bool {
        self.artifacts.len() == measurements.len()
            && self.artifacts.iter().all(|(name, existing)| {
                measurements.get(name).is_some_and(|measurement| {
                    DisplayedArtifactMeasurement::from(existing)
                        == DisplayedArtifactMeasurement::from(measurement)
                })
            })
    }
}

/// Resolve the baseline file path for a given platform.
pub(crate) fn baseline_path(workspace_root: &Path, baseline_dir: &Path, platform: &str) -> PathBuf {
    workspace_root
        .join(baseline_dir)
        .join(format!("{platform}.toml"))
}

/// Get the current git SHA (short), or None if not in a git repo.
pub(crate) fn current_git_sha(workspace_root: &Path) -> Option<String> {
    let output = std::process::Command::new("git")
        .args(["rev-parse", "--short", "HEAD"])
        .current_dir(workspace_root)
        .output()
        .ok()?;
    if output.status.success() {
        Some(String::from_utf8_lossy(&output.stdout).trim().to_owned())
    } else {
        None
    }
}

/// Get the current timestamp in ISO 8601 format.
pub(crate) fn now_iso8601() -> String {
    // Simple approach without pulling in chrono: use `date` command
    let output = std::process::Command::new("date")
        .arg("-u")
        .arg("+%Y-%m-%dT%H:%M:%SZ")
        .output();
    match output {
        Ok(o) if o.status.success() => String::from_utf8_lossy(&o.stdout).trim().to_owned(),
        _ => "unknown".to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn baseline_round_trips_human_readable_sizes() {
        let baseline = PlatformBaseline {
            version: 1,
            recorded_at: "2026-07-29T10:12:16Z".to_owned(),
            git_sha: Some("abc123".to_owned()),
            artifacts: BTreeMap::from([(
                "baml-cli".to_owned(),
                ArtifactMeasurement {
                    file_bytes: 22_990_336,
                    stripped_bytes: Some(22_990_336),
                    gzip_bytes: 10_413_994,
                    sections: BTreeMap::from([(".text".to_owned(), 1_024)]),
                },
            )]),
        };

        let toml = toml::to_string_pretty(&baseline).unwrap();
        assert!(toml.contains(r#"file_bytes = "21.9 MiB""#));
        assert!(toml.contains(r#"stripped_bytes = "21.9 MiB""#));
        assert!(toml.contains(r#"gzip_bytes = "9.9 MiB""#));
        assert!(toml.contains(r#"".text" = "1.0 KiB""#));

        let parsed: PlatformBaseline = toml::from_str(&toml).unwrap();
        let artifact = &parsed.artifacts["baml-cli"];
        assert_eq!(artifact.file_bytes, 22_963_814);
        assert_eq!(artifact.stripped_bytes, Some(22_963_814));
        assert_eq!(artifact.gzip_bytes, 10_380_902);
        assert_eq!(artifact.sections[".text"], 1_024);
        assert_ne!(parsed.artifacts, baseline.artifacts);
        assert!(parsed.measurements_match(&baseline.artifacts));
    }

    #[test]
    fn checked_in_baselines_use_human_readable_sizes() {
        for content in [
            include_str!("../../../.ci/size-gate/aarch64-apple-darwin.toml"),
            include_str!("../../../.ci/size-gate/wasm32-unknown-unknown.toml"),
            include_str!("../../../.ci/size-gate/x86_64-pc-windows-msvc.toml"),
            include_str!("../../../.ci/size-gate/x86_64-unknown-linux-gnu.toml"),
        ] {
            toml::from_str::<PlatformBaseline>(content).unwrap();
        }
    }
}
