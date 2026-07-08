use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::{MANIFEST_SCHEMA, SUPPORTED_RELEASE_TARGETS, validate_sha256};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Channel {
    Canary,
    Nightly,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Artifact {
    pub url: String,
    pub sha256: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VsixArtifact {
    pub url: String,
    pub sha256: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BamlBridgePypi {
    pub version: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolchainManifest {
    pub schema: u32,
    pub version: String,
    pub channel: Channel,
    pub released_at: String,
    pub artifacts: BTreeMap<String, Artifact>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub vsix: Option<VsixArtifact>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub baml_bridge_pypi: Option<BamlBridgePypi>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WrapperManifest {
    pub schema: u32,
    pub version: String,
    pub released_at: String,
    pub artifacts: BTreeMap<String, Artifact>,
}

impl ToolchainManifest {
    pub fn validate(&self) -> anyhow::Result<()> {
        validate_schema(self.schema)?;
        validate_artifacts(&self.version, &self.artifacts)?;
        if let Some(vsix) = &self.vsix {
            validate_artifact(
                "vsix",
                &Artifact {
                    url: vsix.url.clone(),
                    sha256: vsix.sha256.clone(),
                },
            )?;
        }
        Ok(())
    }

    pub fn artifact_for_target(&self, target: &str) -> anyhow::Result<&Artifact> {
        self.artifacts.get(target).ok_or_else(|| {
            anyhow::anyhow!("target {target} not built for version {}", self.version)
        })
    }
}

impl WrapperManifest {
    pub fn validate(&self) -> anyhow::Result<()> {
        validate_schema(self.schema)?;
        validate_artifacts(&self.version, &self.artifacts)
    }

    pub fn artifact_for_target(&self, target: &str) -> anyhow::Result<&Artifact> {
        self.artifacts.get(target).ok_or_else(|| {
            anyhow::anyhow!("target {target} not built for wrapper {}", self.version)
        })
    }
}

fn validate_schema(schema: u32) -> anyhow::Result<()> {
    if schema > MANIFEST_SCHEMA {
        anyhow::bail!(
            "manifest schema {schema} is newer than this wrapper; run `baml self-update`"
        );
    }
    if schema != MANIFEST_SCHEMA {
        anyhow::bail!("unsupported manifest schema {schema}");
    }
    Ok(())
}

fn validate_artifacts(version: &str, artifacts: &BTreeMap<String, Artifact>) -> anyhow::Result<()> {
    let expected: std::collections::BTreeSet<_> =
        SUPPORTED_RELEASE_TARGETS.iter().copied().collect();
    let actual: std::collections::BTreeSet<_> = artifacts.keys().map(String::as_str).collect();
    if actual != expected {
        anyhow::bail!("manifest for {version} has target set {actual:?}; expected {expected:?}");
    }
    for (target, artifact) in artifacts {
        validate_artifact(target, artifact)?;
    }
    Ok(())
}

fn validate_artifact(name: &str, artifact: &Artifact) -> anyhow::Result<()> {
    if !artifact.url.starts_with("https://") {
        anyhow::bail!("artifact {name} URL must use HTTPS");
    }
    validate_sha256(&artifact.sha256)?;
    Ok(())
}
