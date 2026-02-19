use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::measure::ArtifactMeasurement;

/// A platform baseline file containing measurements for all artifacts on that platform.
#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct PlatformBaseline {
    pub version: u32,
    pub recorded_at: String,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub git_sha: Option<String>,

    #[serde(default)]
    pub artifacts: BTreeMap<String, ArtifactMeasurement>,
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
