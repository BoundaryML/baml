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
pub struct SdkPackage {
    /// Registry the package was published to, e.g. `crates_io`.
    pub registry: String,
    /// Package name as it appears on the registry, e.g. `baml_bridge`.
    pub package: String,
    /// Registry-encoded version (the identity of the canonical version for
    /// crates.io; pypi/npm have their own encodings).
    pub version: String,
    /// Digest of the exact pre-publication package exercised by release
    /// consumers. Optional for older SDK entries whose manifest contract has
    /// not migrated yet.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub verified_package_sha256: Option<String>,
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
    /// Engine cdylib assets (`target -> {url, sha256}`), recorded for
    /// completeness. The dylib-loader SDKs construct these GitHub-release URLs
    /// directly, so the loader never consults the manifest for them; `default`
    /// keeps older manifests without this field deserializable.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cffi: Option<BTreeMap<String, Artifact>>,
    /// Generated-SDK registry coordinates (`language -> {registry, package,
    /// version}`), e.g. `rust -> crates_io/baml_bridge`. Recorded once a
    /// language's registry publisher lands; `default` keeps older manifests
    /// deserializable.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sdks: Option<BTreeMap<String, SdkPackage>>,
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
        if let Some(cffi) = &self.cffi {
            for (target, artifact) in cffi {
                validate_artifact(&format!("cffi/{target}"), artifact)?;
            }
        }
        if let Some(sdks) = &self.sdks {
            for (language, package) in sdks {
                validate_sdk(language, package)?;
            }
        }
        Ok(())
    }

    pub fn artifact_for_target(&self, target: &str) -> anyhow::Result<&Artifact> {
        self.artifacts.get(target).ok_or_else(|| {
            anyhow::anyhow!("target {target} not built for version {}", self.version)
        })
    }
}

#[cfg(all(feature = "self-update", not(feature = "no-self-update")))]
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

fn validate_sdk(language: &str, package: &SdkPackage) -> anyhow::Result<()> {
    if package.registry.is_empty() || package.package.is_empty() || package.version.is_empty() {
        anyhow::bail!("sdk {language} has an empty registry, package, or version");
    }
    if let Some(digest) = &package.verified_package_sha256 {
        validate_sha256(digest)
            .map_err(|error| anyhow::anyhow!("sdk {language} package digest: {error}"))?;
    }
    if language == "csharp" {
        if package.registry != "nuget" || package.package != "baml-bridge" {
            anyhow::bail!("sdk csharp must identify nuget/baml-bridge");
        }
        if package.verified_package_sha256.is_none() {
            anyhow::bail!("sdk csharp must record the verified NuGet package digest");
        }
    }
    if language == "swift" {
        if package.registry != "swiftpm" || package.package != "BoundaryML/baml-swift" {
            anyhow::bail!("sdk swift must identify swiftpm/BoundaryML/baml-swift");
        }
        if package.verified_package_sha256.is_none() {
            anyhow::bail!("sdk swift must record the verified XCFramework package digest");
        }
    }
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

    fn artifact() -> Artifact {
        Artifact {
            url: "https://example.com/x".to_string(),
            sha256: "a".repeat(64),
        }
    }

    fn manifest_with(cffi: Option<BTreeMap<String, Artifact>>) -> ToolchainManifest {
        ToolchainManifest {
            schema: crate::MANIFEST_SCHEMA,
            version: "0.15.0".to_string(),
            channel: Channel::Canary,
            released_at: "2026-07-14T00:00:00Z".to_string(),
            artifacts: full_target_artifacts(),
            vsix: None,
            cffi,
            sdks: None,
        }
    }

    #[test]
    fn cffi_map_round_trips_and_validates() {
        let cffi = BTreeMap::from([("aarch64-apple-darwin".to_string(), artifact())]);
        let manifest = manifest_with(Some(cffi));
        manifest.validate().unwrap();

        let json = serde_json::to_string(&manifest).unwrap();
        assert!(json.contains("\"cffi\""), "cffi should serialize: {json}");
        let back: ToolchainManifest = serde_json::from_str(&json).unwrap();
        assert_eq!(back.cffi.as_ref().unwrap().len(), 1);
        back.validate().unwrap();
    }

    #[test]
    fn cffi_is_omitted_when_absent() {
        let json = serde_json::to_string(&manifest_with(None)).unwrap();
        assert!(!json.contains("\"cffi\""), "cffi should be omitted: {json}");
    }

    #[test]
    fn older_manifest_without_cffi_deserializes() {
        // A manifest predating the cffi field must still load (serde default).
        let json =
            r#"{"schema":1,"version":"0.1.0","channel":"canary","released_at":"t","artifacts":{}}"#;
        let manifest: ToolchainManifest = serde_json::from_str(json).unwrap();
        assert!(manifest.cffi.is_none());
    }

    #[test]
    fn legacy_bridge_metadata_deserializes() {
        let json = r#"{
            "schema": 1,
            "version": "0.1.0",
            "channel": "canary",
            "released_at": "t",
            "artifacts": {},
            "baml_bridge_pypi": {"version": "0.1.0"},
            "baml_bridge_go": {
                "module": "github.com/boundaryml/baml-go",
                "version": "v0.1.0"
            }
        }"#;
        let manifest: ToolchainManifest = serde_json::from_str(json).unwrap();
        assert!(manifest.sdks.is_none());
    }

    #[test]
    fn invalid_cffi_artifact_is_rejected() {
        let bad = BTreeMap::from([(
            "aarch64-apple-darwin".to_string(),
            Artifact {
                url: "http://insecure".to_string(),
                sha256: "a".repeat(64),
            },
        )]);
        let err = manifest_with(Some(bad)).validate().unwrap_err();
        assert!(
            format!("{err}").contains("cffi/aarch64-apple-darwin"),
            "{err}"
        );
    }

    #[test]
    fn sdks_map_round_trips_and_validates() {
        let mut manifest = manifest_with(None);
        manifest.sdks = Some(BTreeMap::from([(
            "rust".to_string(),
            SdkPackage {
                registry: "crates_io".to_string(),
                package: "baml_bridge".to_string(),
                version: "0.15.0".to_string(),
                verified_package_sha256: None,
            },
        )]));
        manifest.validate().unwrap();

        let json = serde_json::to_string(&manifest).unwrap();
        assert!(json.contains("\"sdks\""), "sdks should serialize: {json}");
        let back: ToolchainManifest = serde_json::from_str(&json).unwrap();
        assert_eq!(back.sdks.as_ref().unwrap()["rust"].package, "baml_bridge");
        back.validate().unwrap();
    }

    #[test]
    fn sdks_is_omitted_when_absent() {
        let json = serde_json::to_string(&manifest_with(None)).unwrap();
        assert!(!json.contains("\"sdks\""), "sdks should be omitted: {json}");
    }

    #[test]
    fn empty_sdk_field_is_rejected() {
        let mut manifest = manifest_with(None);
        manifest.sdks = Some(BTreeMap::from([(
            "rust".to_string(),
            SdkPackage {
                registry: String::new(),
                package: "baml_bridge".to_string(),
                version: "0.15.0".to_string(),
                verified_package_sha256: None,
            },
        )]));
        let err = manifest.validate().unwrap_err();
        assert!(format!("{err}").contains("rust"), "{err}");
    }

    #[test]
    fn csharp_sdk_requires_a_valid_verified_package_digest() {
        let mut manifest = manifest_with(None);
        manifest.sdks = Some(BTreeMap::from([(
            "csharp".to_string(),
            SdkPackage {
                registry: "nuget".to_string(),
                package: "baml-bridge".to_string(),
                version: "0.15.0".to_string(),
                verified_package_sha256: Some("a".repeat(64)),
            },
        )]));
        manifest.validate().unwrap();

        manifest
            .sdks
            .as_mut()
            .unwrap()
            .get_mut("csharp")
            .unwrap()
            .verified_package_sha256 = Some("not-a-digest".to_string());
        let err = manifest.validate().unwrap_err();
        assert!(format!("{err}").contains("csharp"), "{err}");
    }

    #[test]
    fn swift_sdk_requires_the_mirror_identity_and_verified_digest() {
        let mut manifest = manifest_with(None);
        manifest.sdks = Some(BTreeMap::from([(
            "swift".to_string(),
            SdkPackage {
                registry: "swiftpm".to_string(),
                package: "BoundaryML/baml-swift".to_string(),
                version: "0.15.0".to_string(),
                verified_package_sha256: Some("a".repeat(64)),
            },
        )]));
        manifest.validate().unwrap();

        manifest
            .sdks
            .as_mut()
            .unwrap()
            .get_mut("swift")
            .unwrap()
            .package = "wrong/mirror".to_string();
        let err = manifest.validate().unwrap_err();
        assert!(format!("{err}").contains("swift"), "{err}");
    }
}
