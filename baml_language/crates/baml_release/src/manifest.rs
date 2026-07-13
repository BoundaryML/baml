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
    /// Per-target C++ SDK tarballs (prebuilt `bridge_cffi` cdylib + headers),
    /// attached to the same baml-language release tag as `artifacts`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub baml_cpp: Option<BTreeMap<String, Artifact>>,
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
        if let Some(baml_cpp) = &self.baml_cpp {
            validate_artifacts(&self.version, baml_cpp)?;
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

#[cfg(test)]
mod tests {
    use super::*;

    fn full_target_artifacts() -> BTreeMap<String, Artifact> {
        SUPPORTED_RELEASE_TARGETS
            .iter()
            .map(|target| {
                (
                    (*target).to_string(),
                    Artifact {
                        url: format!("https://example.com/{target}.tar.gz"),
                        sha256: "a".repeat(64),
                    },
                )
            })
            .collect()
    }

    fn manifest_with_baml_cpp(baml_cpp: Option<BTreeMap<String, Artifact>>) -> ToolchainManifest {
        ToolchainManifest {
            schema: MANIFEST_SCHEMA,
            version: "1.2.3".to_string(),
            channel: Channel::Nightly,
            released_at: "2026-07-13T00:00:00Z".to_string(),
            artifacts: full_target_artifacts(),
            vsix: None,
            baml_bridge_pypi: None,
            baml_cpp,
        }
    }

    #[test]
    fn test_baml_cpp_absent_is_valid_and_omitted_from_json() {
        let manifest = manifest_with_baml_cpp(None);
        manifest.validate().unwrap();
        let json = serde_json::to_string(&manifest).unwrap();
        assert!(!json.contains("baml_cpp"));
    }

    #[test]
    fn test_baml_cpp_with_full_target_set_is_valid() {
        let manifest = manifest_with_baml_cpp(Some(full_target_artifacts()));
        manifest.validate().unwrap();
        let json = serde_json::to_string(&manifest).unwrap();
        let parsed: ToolchainManifest = serde_json::from_str(&json).unwrap();
        parsed.validate().unwrap();
        assert_eq!(
            parsed.baml_cpp.unwrap().len(),
            SUPPORTED_RELEASE_TARGETS.len()
        );
    }

    #[test]
    fn test_baml_cpp_with_missing_target_is_rejected() {
        let mut baml_cpp = full_target_artifacts();
        baml_cpp.remove(SUPPORTED_RELEASE_TARGETS[0]);
        let manifest = manifest_with_baml_cpp(Some(baml_cpp));
        assert!(manifest.validate().is_err());
    }

    #[test]
    fn test_manifest_without_baml_cpp_key_still_parses() {
        let manifest = manifest_with_baml_cpp(None);
        let json = serde_json::to_string(&manifest).unwrap();
        let parsed: ToolchainManifest = serde_json::from_str(&json).unwrap();
        assert!(parsed.baml_cpp.is_none());
    }
}
