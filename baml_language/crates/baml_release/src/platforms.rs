//! The repository-owned platform contract (`release/platforms.json`): the
//! single machine-readable source for the release target set, each target's
//! per-artifact support, and the `bridge_cffi` cdylib build config.
//!
//! The cffi, toolchain, python, Java, and C# build matrices are generated from
//! this file (each in its own workflow); the Rust target list is kept in sync
//! with [`crate::SUPPORTED_RELEASE_TARGETS`] by the tests below. The nodejs
//! matrix keeps its bespoke per-target build recipes in its own workflow.
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
    #[serde(default)]
    pub csharp: Option<CSharpArtifact>,
}

/// The CLI toolchain artifact (baml-cli + pack host, archived per target).
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ToolchainArtifact {
    /// GitHub Actions runner label (distinct from the target's `os` family).
    pub runner: String,
    /// Native job container used for the entire build.
    #[serde(default)]
    pub container: Option<String>,
    /// Container image invoked by `cross` for a cross-compiled build.
    #[serde(default)]
    pub cross_image: Option<String>,
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
    /// Explicit maturin build container (otherwise the action selects its default).
    #[serde(default)]
    pub container: Option<String>,
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

/// The C# projection of a shared `bridge_cffi` producer artifact.
#[derive(Debug, Clone, Deserialize)]
pub struct CSharpArtifact {
    /// Portable .NET Runtime Identifier used below `runtimes/{rid}/native/`.
    pub rid: String,
    /// Canonical native filename installed in the RID directory.
    pub native_asset: String,
    /// Native GitHub Actions runner for executing the assembled package.
    pub consumer_runner: String,
    /// Packaging contract used by this target.
    pub package_policy: String,
    /// Built best-effort: a build failure must not block the release.
    #[serde(default)]
    pub experimental: bool,
}

/// Parse the embedded platform contract. Panics if the committed file is
/// malformed — that is a build-blocking repository error the tests guard.
pub fn platforms() -> Platforms {
    let platforms: Platforms =
        serde_json::from_str(PLATFORMS_JSON).expect("release/platforms.json is valid JSON");
    validate_platforms(&platforms).expect("release/platforms.json has valid artifact dependencies");
    platforms
}

fn validate_platforms(platforms: &Platforms) -> Result<(), String> {
    for target in &platforms.targets {
        if let Some(toolchain) = &target.artifacts.toolchain
            && toolchain.container.is_some()
            && toolchain.cross_image.is_some()
        {
            return Err(format!(
                "{}: toolchain container and cross_image are mutually exclusive",
                target.triple
            ));
        }

        let Some(csharp) = &target.artifacts.csharp else {
            continue;
        };
        let Some(cffi) = &target.artifacts.cffi else {
            return Err(format!(
                "{}: C# requires a CFFI source artifact",
                target.triple
            ));
        };
        if !csharp.experimental && cffi.experimental {
            return Err(format!(
                "{}: required C# cannot consume experimental CFFI",
                target.triple
            ));
        }
    }
    Ok(())
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
    fn python_musl_images_are_explicit_without_changing_wheel_policy() {
        for target in platforms().targets {
            let Some(python) = target.artifacts.python else {
                continue;
            };
            if target.libc.as_deref() == Some("musl") {
                assert_eq!(python.manylinux.as_deref(), Some("musllinux_1_1"));
                let image = python
                    .container
                    .expect("musl must override maturin's GHCR default");
                assert!(image.starts_with(&format!(
                    "us-central1-docker.pkg.dev/baml-infra/ghcr-cache/rust-cross/rust-musl-cross:{}-musl@sha256:",
                    target.arch
                )));
            } else {
                assert!(
                    python.container.is_none(),
                    "preserve non-musl image defaults"
                );
            }
        }
    }

    #[test]
    fn toolchain_build_modes_are_preserved_and_mutually_exclusive() {
        let p = platforms();
        let arm = p
            .targets
            .iter()
            .find(|target| target.triple == "aarch64-unknown-linux-gnu")
            .unwrap()
            .artifacts
            .toolchain
            .as_ref()
            .unwrap();
        assert_eq!(
            arm.container.as_deref(),
            Some(
                "us-central1-docker.pkg.dev/baml-infra/ghcr-cache/rust-cross/manylinux_2_28-cross:aarch64@sha256:ca53fa07ecf1c3e6408c51fbca64c036d9d29af832d3f8bb954910e89097f275"
            )
        );
        assert!(arm.cross_image.is_none());

        let json = r#"{
            "schema": 1,
            "targets": [{
                "triple": "aarch64-unknown-linux-gnu", "os": "linux", "arch": "aarch64",
                "libc": "gnu", "archive_suffix": ".tar.gz",
                "artifacts": { "toolchain": {
                    "runner": "ubuntu-24.04-arm",
                    "container": "native-image",
                    "cross_image": "cross-image"
                } }
            }]
        }"#;
        let p: Platforms = serde_json::from_str(json).unwrap();
        assert!(validate_platforms(&p).is_err());
    }

    #[test]
    fn toolchain_artifact_rejects_unknown_fields() {
        let json = r#"{
            "runner": "ubuntu-latest",
            "cross_iamge": "misspelled-image"
        }"#;
        assert!(serde_json::from_str::<ToolchainArtifact>(json).is_err());
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

    #[test]
    fn csharp_runtime_assets_follow_the_platform_contract() {
        let expected_rids = BTreeSet::from([
            "linux-arm64".to_string(),
            "linux-musl-arm64".to_string(),
            "linux-musl-x64".to_string(),
            "linux-x64".to_string(),
            "osx-arm64".to_string(),
            "osx-x64".to_string(),
            "win-arm64".to_string(),
            "win-x64".to_string(),
        ]);
        let mut actual_rids = BTreeSet::new();

        for t in platforms().targets {
            let Some(csharp) = &t.artifacts.csharp else {
                continue;
            };
            assert!(t.artifacts.cffi.is_some(), "{}: C# requires CFFI", t.triple);
            assert!(
                !csharp.experimental,
                "{}: the eight-RID C# package is all-required",
                t.triple
            );
            let arch = match t.arch.as_str() {
                "aarch64" => "arm64",
                "x86_64" => "x64",
                other => panic!("{}: unsupported .NET architecture {other}", t.triple),
            };
            let rid = match (t.os.as_str(), t.libc.as_deref()) {
                ("macos", _) => format!("osx-{arch}"),
                ("linux", Some("musl")) => format!("linux-musl-{arch}"),
                ("linux", _) => format!("linux-{arch}"),
                ("windows", _) => format!("win-{arch}"),
                (other, _) => panic!("{}: unsupported .NET OS {other}", t.triple),
            };
            let native_asset = match t.os.as_str() {
                "macos" => "libbridge_cffi.dylib",
                "windows" => "bridge_cffi.dll",
                _ => "libbridge_cffi.so",
            };
            assert_eq!(csharp.rid, rid, "{}: unexpected .NET RID", t.triple);
            assert!(
                actual_rids.insert(csharp.rid.clone()),
                "{}: duplicate .NET RID {}",
                t.triple,
                csharp.rid
            );
            assert_eq!(
                csharp.native_asset, native_asset,
                "{}: unexpected .NET native filename",
                t.triple
            );
            assert!(
                !csharp.consumer_runner.is_empty(),
                "{}: empty .NET consumer runner",
                t.triple
            );
            assert_eq!(csharp.package_policy, "rid-native");
        }

        assert_eq!(actual_rids, expected_rids);
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
        assert!(p.artifacts.csharp.is_none());
    }

    #[test]
    fn cffi_support_does_not_imply_csharp_support() {
        let json = r#"{
            "schema": 1,
            "targets": [{
                "triple": "x86_64-unknown-linux-gnu", "os": "linux", "arch": "x86_64",
                "libc": "gnu", "archive_suffix": ".tar.gz",
                "artifacts": { "cffi": {
                    "asset": "libbaml_cffi-x86_64-unknown-linux-gnu.so",
                    "runner": "ubuntu-22.04"
                } }
            }]
        }"#;
        let p: Platforms = serde_json::from_str(json).unwrap();
        assert!(validate_platforms(&p).is_ok());
        assert!(p.targets[0].artifacts.cffi.is_some());
        assert!(p.targets[0].artifacts.csharp.is_none());
    }

    #[test]
    fn csharp_without_cffi_is_rejected() {
        let json = r#"{
            "schema": 1,
            "targets": [{
                "triple": "x86_64-unknown-linux-gnu", "os": "linux", "arch": "x86_64",
                "libc": "gnu", "archive_suffix": ".tar.gz",
                "artifacts": { "csharp": {
                    "rid": "linux-x64",
                    "native_asset": "libbridge_cffi.so",
                    "consumer_runner": "ubuntu-24.04",
                    "package_policy": "rid-native"
                } }
            }]
        }"#;
        let p: Platforms = serde_json::from_str(json).unwrap();
        assert!(validate_platforms(&p).is_err());
    }

    #[test]
    fn required_csharp_cannot_consume_experimental_cffi() {
        let json = r#"{
            "schema": 1,
            "targets": [{
                "triple": "x86_64-unknown-linux-gnu", "os": "linux", "arch": "x86_64",
                "libc": "gnu", "archive_suffix": ".tar.gz",
                "artifacts": {
                    "cffi": {
                        "asset": "libbaml_cffi-x86_64-unknown-linux-gnu.so",
                        "runner": "ubuntu-22.04",
                        "experimental": true
                    },
                    "csharp": {
                        "rid": "linux-x64",
                        "native_asset": "libbridge_cffi.so",
                        "consumer_runner": "ubuntu-24.04",
                        "package_policy": "rid-native"
                    }
                }
            }]
        }"#;
        let p: Platforms = serde_json::from_str(json).unwrap();
        assert!(validate_platforms(&p).is_err());
    }
}
