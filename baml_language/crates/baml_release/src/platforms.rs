//! The repository-owned platform contract (`release/platforms.json`): the
//! single machine-readable source for the release target set, each target's
//! per-artifact support, and the `bridge_cffi` cdylib build config.
//!
//! The cffi, toolchain, and python build matrices are generated from this file
//! (each in its own workflow); the Rust target list is kept in sync with
//! [`crate::SUPPORTED_RELEASE_TARGETS`] by the tests below. The nodejs matrix
//! keeps its bespoke per-target build recipes in its own workflow.
//!
//! Support is encoded structurally: an artifact that is `None` in [`Artifacts`]
//! is not built for the target (unsupported), so "unsupported but configured"
//! is unrepresentable; and each artifact owns its single `experimental` flag,
//! so the support tier and the build config can never disagree.

use serde::Deserialize;

/// The committed platform contract, embedded at build time.
const PLATFORMS_JSON: &str = include_str!("../../../../release/platforms.json");

#[derive(Debug, Clone, Deserialize)]
pub struct Platforms {
    pub schema: u32,
    pub targets: Vec<Platform>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Platform {
    /// Canonical Rust target triple.
    pub triple: String,
    pub os: String,
    pub arch: String,
    /// C runtime where the platform has a choice (`gnu`/`musl`, `msvc`); `None`
    /// for single-ABI platforms (Apple).
    pub libc: Option<String>,
    /// Release-archive extension for the toolchain artifact (`.tar.gz`/`.zip`).
    pub archive_suffix: String,
    /// Per-artifact build config; a `None` field means that artifact is not
    /// built for this target.
    pub artifacts: Artifacts,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Artifacts {
    #[serde(default)]
    pub toolchain: Option<ToolchainArtifact>,
    #[serde(default)]
    pub python: Option<PythonArtifact>,
    #[serde(default)]
    pub nodejs: Option<SdkArtifact>,
    #[serde(default)]
    pub java: Option<JavaArtifact>,
    #[serde(default)]
    pub cffi: Option<CffiArtifact>,
}

/// The CLI toolchain artifact (baml-cli + pack host, archived per target).
#[derive(Debug, Clone, Deserialize)]
pub struct ToolchainArtifact {
    /// GitHub Actions runner label (distinct from the target's `os` family).
    pub runner: String,
    /// Built best-effort: a build failure must not block the release.
    #[serde(default)]
    pub experimental: bool,
}

/// The python wheel artifact built per target.
#[derive(Debug, Clone, Deserialize)]
pub struct PythonArtifact {
    /// GitHub Actions runner label (distinct from the target's `os` family).
    pub runner: String,
    /// `manylinux`/`musllinux` platform tag for maturin (Linux targets only).
    #[serde(default)]
    pub manylinux: Option<String>,
    /// Python-setup architecture override (arm64-Windows only).
    #[serde(default)]
    pub architecture: Option<String>,
    /// Built best-effort: a build failure must not block the release.
    #[serde(default)]
    pub experimental: bool,
}

/// A registry-packaged SDK artifact (nodejs). Carries only the support tier for
/// now; per-target runner + setup config move here when its matrix migrates
/// onto the contract.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct SdkArtifact {
    /// Built best-effort: a build failure must not block the release.
    #[serde(default)]
    pub experimental: bool,
}

/// The per-target `bridge_java` native jar (`baml_bridge`'s `natives-<platform>`
/// classifier artifact carrying the `bridge_java` cdylib).
#[derive(Debug, Clone, Deserialize)]
pub struct JavaArtifact {
    /// `<os>-<arch>` classifier token for the natives jar
    /// (`baml-bridge-<version>-natives-<platform>.jar`), also passed to Gradle
    /// as `-PbamlNativePlatform` and used as the `/native/<platform>/` resource
    /// dir the loader reads. musl targets carry a `-musl` suffix so their jar
    /// does not collide with the gnu jar (the runtime loader resolves only the
    /// bare `<os>-<arch>` token today; the `-musl` jars are for explicit-
    /// classifier consumers and are experimental).
    pub platform: String,
    /// GitHub Actions runner label (native-arch, distinct from the `os` family):
    /// the cdylib is built natively per target so `cargo build --target` needs
    /// no cross C toolchain except the musl linker (`setup-musl-cross`).
    pub runner: String,
    /// Built best-effort: a build failure must not block the release.
    #[serde(default)]
    pub experimental: bool,
}

/// The standalone `bridge_cffi` engine cdylib built per target.
#[derive(Debug, Clone, Deserialize)]
pub struct CffiArtifact {
    /// Engine cdylib asset filename the dylib loader constructs for this target.
    pub asset: String,
    /// GitHub Actions runner label (distinct from the target's `os` family).
    pub runner: String,
    /// Build via `cross` (Linux cross-compile targets).
    #[serde(default)]
    pub use_cross: bool,
    /// A native dlopen + `version()` smoke runs on this target.
    #[serde(default)]
    pub smoke: bool,
    /// Built best-effort: a build failure must not block the release.
    #[serde(default)]
    pub experimental: bool,
}

/// Parse the embedded platform contract. Panics if the committed file is
/// malformed — that is a build-blocking repository error the tests guard.
pub fn platforms() -> Platforms {
    serde_json::from_str(PLATFORMS_JSON).expect("release/platforms.json is valid")
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::*;
    use crate::SUPPORTED_RELEASE_TARGETS;

    #[test]
    fn platform_contract_parses() {
        let p = platforms();
        assert_eq!(p.schema, 1);
        assert_eq!(p.targets.len(), SUPPORTED_RELEASE_TARGETS.len());
    }

    #[test]
    fn platform_contract_matches_supported_release_targets() {
        let p = platforms();
        let from_json: BTreeSet<&str> = p.targets.iter().map(|t| t.triple.as_str()).collect();
        let from_const: BTreeSet<&str> = SUPPORTED_RELEASE_TARGETS.iter().copied().collect();
        assert_eq!(
            from_json, from_const,
            "release/platforms.json target set must match SUPPORTED_RELEASE_TARGETS"
        );
    }

    #[test]
    fn every_target_has_a_toolchain_runner() {
        for t in platforms().targets {
            let toolchain = t
                .artifacts
                .toolchain
                .as_ref()
                .unwrap_or_else(|| panic!("{}: toolchain artifact missing", t.triple));
            assert!(
                !toolchain.runner.is_empty(),
                "{}: empty toolchain runner",
                t.triple
            );
        }
    }

    #[test]
    fn every_target_has_a_python_runner() {
        for t in platforms().targets {
            let python = t
                .artifacts
                .python
                .as_ref()
                .unwrap_or_else(|| panic!("{}: python artifact missing", t.triple));
            assert!(
                !python.runner.is_empty(),
                "{}: empty python runner",
                t.triple
            );
        }
    }

    /// The denormalized `os`/`arch`/`libc`/`archive_suffix` fields are
    /// convenience documentation but derivable from the triple; keep them from
    /// drifting out of sync with it.
    #[test]
    fn target_fields_are_consistent_with_the_triple() {
        for t in platforms().targets {
            assert!(
                t.triple.starts_with(&t.arch),
                "{}: arch {} not in triple",
                t.triple,
                t.arch
            );
            let os_marker = match t.os.as_str() {
                "macos" => "apple-darwin",
                "linux" => "unknown-linux",
                "windows" => "pc-windows",
                other => panic!("{}: unknown os {other}", t.triple),
            };
            assert!(
                t.triple.contains(os_marker),
                "{}: os {} inconsistent with triple",
                t.triple,
                t.os
            );
            if let Some(libc) = &t.libc {
                assert!(
                    t.triple.ends_with(libc),
                    "{}: libc {} inconsistent with triple",
                    t.triple,
                    libc
                );
            }
            let expected_suffix = if t.os == "windows" { ".zip" } else { ".tar.gz" };
            assert_eq!(
                t.archive_suffix, expected_suffix,
                "{}: unexpected archive_suffix",
                t.triple
            );
        }
    }

    /// Every target ships a Java native jar, on a native-arch runner.
    #[test]
    fn every_target_has_a_java_runner() {
        for t in platforms().targets {
            let java = t
                .artifacts
                .java
                .as_ref()
                .unwrap_or_else(|| panic!("{}: java artifact missing", t.triple));
            assert!(!java.runner.is_empty(), "{}: empty java runner", t.triple);
        }
    }

    /// The Java `platform` token is the `<os>-<arch>` classifier the natives jar
    /// and the `NativeLibraryLoader` resource path are built from (musl gets a
    /// `-musl` suffix so its jar does not collide with the gnu jar).
    #[test]
    fn java_platform_tokens_follow_the_natives_classifier_convention() {
        for t in platforms().targets {
            let Some(java) = &t.artifacts.java else {
                continue;
            };
            let os_token = match t.os.as_str() {
                "macos" => "macos",
                "linux" => "linux",
                "windows" => "windows",
                other => panic!("{}: unknown os {other}", t.triple),
            };
            let base = format!("{os_token}-{}", t.arch);
            let expected = if t.libc.as_deref() == Some("musl") {
                format!("{base}-musl")
            } else {
                base
            };
            assert_eq!(
                java.platform, expected,
                "{}: unexpected java platform token",
                t.triple
            );
        }
    }

    /// The recorded cffi asset name must match the loader's filename convention
    /// (`libbaml_cffi-<triple>.{so,dylib}` / `baml_cffi-<triple>.dll`).
    #[test]
    fn cffi_asset_names_follow_the_loader_convention() {
        for t in platforms().targets {
            let Some(cffi) = &t.artifacts.cffi else {
                continue;
            };
            let expected = match t.os.as_str() {
                "windows" => format!("baml_cffi-{}.dll", t.triple),
                "macos" => format!("libbaml_cffi-{}.dylib", t.triple),
                _ => format!("libbaml_cffi-{}.so", t.triple),
            };
            assert_eq!(cffi.asset, expected, "{}: unexpected cffi asset", t.triple);
        }
    }

    /// The linchpin of the schema: a missing artifact key deserializes to
    /// `None`, i.e. "not built for this target" — the only way to express
    /// unsupported, so it can never carry a contradictory build config.
    #[test]
    fn a_missing_artifact_key_means_unsupported() {
        let json = r#"{
            "triple": "x86_64-unknown-linux-gnu", "os": "linux", "arch": "x86_64",
            "libc": "gnu", "archive_suffix": ".tar.gz",
            "artifacts": { "toolchain": { "runner": "ubuntu-latest" } }
        }"#;
        let p: Platform = serde_json::from_str(json).unwrap();
        assert!(p.artifacts.toolchain.is_some(), "present key is supported");
        assert!(p.artifacts.cffi.is_none(), "absent cffi is unsupported");
        assert!(p.artifacts.python.is_none());
        assert!(p.artifacts.nodejs.is_none());
        assert!(p.artifacts.java.is_none());
    }
}
