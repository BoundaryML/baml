//! The repository-owned platform contract (`release/platforms.json`): the
//! single machine-readable source for the release target set, each target's
//! per-artifact support, and the `bridge_cffi`/C# package config.
//!
//! The cffi, toolchain, python, and C# build/verification matrices are generated
//! from this file (each in its own workflow); the Rust target list is kept in
//! sync with [`crate::SUPPORTED_RELEASE_TARGETS`] by the tests below. The nodejs
//! matrix keeps its bespoke per-target build recipes in its own workflow.
//!
//! Support is encoded structurally: an artifact that is `None` in [`Artifacts`]
//! is not built for the target (unsupported), so "unsupported but configured"
//! is unrepresentable. Artifacts that permit best-effort support own their
//! single `experimental` flag; C# is instead required by its atomic-package
//! policy, so the support tier and build config cannot disagree.

use serde::Deserialize;

/// The committed platform contract, embedded at build time.
const PLATFORMS_JSON: &str = include_str!("../../../../release/platforms.json");

#[derive(Debug, Clone, Deserialize)]
pub struct Platforms {
    pub schema: u32,
    pub csharp_package: CsharpPackage,
    pub targets: Vec<Platform>,
}

/// Policy for the single NuGet package containing every claimed C# RID.
#[derive(Debug, Clone, Deserialize)]
pub struct CsharpPackage {
    pub package_id: String,
    pub atomic_all_rids: bool,
    /// Every selected cffi artifact is release-blocking for this package,
    /// including targets whose standalone cffi support tier is experimental.
    pub cffi_inputs_required: bool,
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
    pub cffi: Option<CffiArtifact>,
    #[serde(default)]
    pub csharp: Option<CsharpArtifact>,
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

/// The target-specific native payload inside the atomic C# NuGet package.
#[derive(Debug, Clone, Deserialize)]
pub struct CsharpArtifact {
    /// Portable .NET runtime identifier used under `runtimes/<rid>/native`.
    pub rid: String,
    /// Canonical native filename as packaged for this RID.
    pub native_asset: String,
    /// MSBuild property used to pass this native input to `dotnet pack`.
    pub pack_property: String,
    /// Native runner used to execute the clean NuGet consumer smoke.
    pub verify_runner: String,
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

    const CSHARP_PROJECT: &str =
        include_str!("../../../sdks/csharp/bridge_csharp/src/Baml.Bridge/Baml.Bridge.csproj");
    const CSHARP_BUILD_TRANSITIVE: &str =
        include_str!("../../../sdks/csharp/bridge_csharp/buildTransitive/baml-bridge.targets");
    const CSHARP_BRIDGE_PLATFORM: &str =
        include_str!("../../../sdks/csharp/bridge_csharp/src/Baml.Bridge/Bridge/BridgePlatform.cs");
    const CSHARP_NATIVE_API: &str =
        include_str!("../../../sdks/csharp/bridge_csharp/src/Baml.Bridge/Bridge/NativeApi.cs");

    fn between<'a>(source: &'a str, start: &str, end: &str) -> &'a str {
        let (_, tail) = source
            .split_once(start)
            .unwrap_or_else(|| panic!("missing start marker {start:?}"));
        let (value, _) = tail
            .split_once(end)
            .unwrap_or_else(|| panic!("missing end marker {end:?} after {start:?}"));
        value
    }

    fn quoted_strings(source: &str) -> Vec<&str> {
        let mut values = Vec::new();
        let mut remaining = source;
        while let Some((_, after_quote)) = remaining.split_once('"') {
            let (value, tail) = after_quote
                .split_once('"')
                .expect("unterminated quoted string in checked C# declaration");
            values.push(value);
            remaining = tail;
        }
        values
    }

    fn csharp_project_pack_mappings() -> Vec<(String, String)> {
        CSHARP_PROJECT
            .lines()
            .filter(|line| {
                line.contains(r#"<None Include="$(BamlNative"#)
                    && line.contains(r#"PackagePath="runtimes/"#)
            })
            .map(|line| {
                let property = between(line, r#"Include="$("#, r#")""#);
                let rid = between(line, r#"PackagePath="runtimes/"#, r#"/native/""#);
                (rid.to_owned(), property.to_owned())
            })
            .collect()
    }

    fn msbuild_accepted_rids(property: &str) -> Vec<&str> {
        let marker = format!("IsMatch('$({property})', '");
        let pattern = between(CSHARP_BUILD_TRANSITIVE, &marker, "')");
        let members = between(pattern, "^(", ")").split('|').collect::<Vec<_>>();
        let joined = members.join("|");
        let expected_pattern = if property == "RuntimeIdentifier" {
            format!("^({joined})$")
        } else {
            format!("^({joined})(;({joined}))*$")
        };
        assert_eq!(
            pattern, expected_pattern,
            "{property} validation must be composed only from its declared RID set"
        );
        members
    }

    fn bridge_platform_supported_rids() -> Vec<&'static str> {
        let declaration = between(
            CSHARP_BRIDGE_PLATFORM,
            "private static readonly HashSet<string> Supported",
            "};",
        );
        quoted_strings(declaration)
    }

    #[test]
    fn platform_contract_parses() {
        let p = platforms();
        assert_eq!(p.schema, 1);
        assert_eq!(p.csharp_package.package_id, "baml-bridge");
        assert!(p.csharp_package.atomic_all_rids);
        assert!(p.csharp_package.cffi_inputs_required);
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
    fn csharp_atomic_package_maps_every_cffi_target_once() {
        let p = platforms();
        assert!(
            p.csharp_package.atomic_all_rids && p.csharp_package.cffi_inputs_required,
            "the atomic C# package must make all selected cffi inputs release-blocking"
        );

        let cffi_targets: BTreeSet<&str> = p
            .targets
            .iter()
            .filter(|t| t.artifacts.cffi.is_some())
            .map(|t| t.triple.as_str())
            .collect();
        let csharp_targets: BTreeSet<&str> = p
            .targets
            .iter()
            .filter(|t| t.artifacts.csharp.is_some())
            .map(|t| t.triple.as_str())
            .collect();
        assert_eq!(
            csharp_targets, cffi_targets,
            "the atomic C# package must include every cffi target, including experimental cffi targets"
        );

        let mut rids = BTreeSet::new();
        let mut pack_properties = BTreeSet::new();
        for target in &p.targets {
            let Some(csharp) = &target.artifacts.csharp else {
                continue;
            };
            assert!(
                target.artifacts.cffi.is_some(),
                "{}: C# target has no cffi input",
                target.triple
            );
            assert!(
                rids.insert(csharp.rid.as_str()),
                "{}: duplicate C# RID {}",
                target.triple,
                csharp.rid
            );
            assert!(
                pack_properties.insert(csharp.pack_property.as_str()),
                "{}: duplicate C# pack property {}",
                target.triple,
                csharp.pack_property
            );

            let arch = match target.arch.as_str() {
                "aarch64" => "arm64",
                "x86_64" => "x64",
                other => panic!("{}: unsupported C# architecture {other}", target.triple),
            };
            let rid_prefix = match (target.os.as_str(), target.libc.as_deref()) {
                ("macos", _) => "osx",
                ("linux", Some("gnu")) => "linux",
                ("linux", Some("musl")) => "linux-musl",
                ("windows", Some("msvc")) => "win",
                (os, libc) => panic!(
                    "{}: unsupported C# os/libc combination {os}/{libc:?}",
                    target.triple
                ),
            };
            assert_eq!(
                csharp.rid,
                format!("{rid_prefix}-{arch}"),
                "{}: C# RID does not match target fields",
                target.triple
            );

            let expected_native = match target.os.as_str() {
                "macos" => "libbridge_cffi.dylib",
                "linux" => "libbridge_cffi.so",
                "windows" => "bridge_cffi.dll",
                _ => unreachable!(),
            };
            assert_eq!(
                csharp.native_asset, expected_native,
                "{}: unexpected C# native asset name",
                target.triple
            );

            let expected_property = match csharp.rid.as_str() {
                "linux-x64" => "BamlNativeLinuxX64",
                "linux-arm64" => "BamlNativeLinuxArm64",
                "linux-musl-x64" => "BamlNativeLinuxMuslX64",
                "linux-musl-arm64" => "BamlNativeLinuxMuslArm64",
                "osx-x64" => "BamlNativeOsxX64",
                "osx-arm64" => "BamlNativeOsxArm64",
                "win-x64" => "BamlNativeWinX64",
                "win-arm64" => "BamlNativeWinArm64",
                other => panic!("unsupported C# RID {other}"),
            };
            assert_eq!(
                csharp.pack_property, expected_property,
                "{}: unexpected C# pack property",
                target.triple
            );
            let expected_runner = match (target.os.as_str(), target.arch.as_str()) {
                ("macos", "aarch64") => "macos-14",
                ("macos", "x86_64") => "macos-15-intel",
                ("linux", "aarch64") => "ubuntu-24.04-arm",
                ("linux", "x86_64") => "ubuntu-24.04",
                ("windows", "aarch64") => "windows-11-arm",
                ("windows", "x86_64") => "windows-2022",
                _ => unreachable!(),
            };
            assert_eq!(
                csharp.verify_runner, expected_runner,
                "{}: C# verification must run natively for its target",
                target.triple
            );
        }
    }

    #[test]
    fn csharp_static_package_and_runtime_declarations_match_the_contract() {
        let p = platforms();
        let csharp = p
            .targets
            .iter()
            .filter_map(|target| target.artifacts.csharp.as_ref())
            .collect::<Vec<_>>();
        let expected_rids = csharp
            .iter()
            .map(|artifact| artifact.rid.clone())
            .collect::<BTreeSet<_>>();
        let expected_pack_mappings = csharp
            .iter()
            .map(|artifact| (artifact.rid.clone(), artifact.pack_property.clone()))
            .collect::<BTreeSet<_>>();
        let expected_native_assets = csharp
            .iter()
            .map(|artifact| artifact.native_asset.clone())
            .collect::<BTreeSet<_>>();

        assert_eq!(
            between(CSHARP_PROJECT, "<PackageId>", "</PackageId>"),
            p.csharp_package.package_id,
            "the C# project PackageId must match release/platforms.json"
        );

        let project_pack_mappings = csharp_project_pack_mappings();
        assert_eq!(
            project_pack_mappings.len(),
            expected_pack_mappings.len(),
            "the C# project must declare exactly one pack property and native path per contract RID"
        );
        assert_eq!(
            project_pack_mappings.into_iter().collect::<BTreeSet<_>>(),
            expected_pack_mappings,
            "the C# project pack properties and runtimes/<rid>/native paths must match release/platforms.json"
        );

        for property in ["RuntimeIdentifier", "RuntimeIdentifiers"] {
            let accepted = msbuild_accepted_rids(property);
            assert_eq!(
                accepted.len(),
                expected_rids.len(),
                "buildTransitive {property} validation contains a duplicate or extra RID"
            );
            assert_eq!(
                accepted
                    .into_iter()
                    .map(str::to_owned)
                    .collect::<BTreeSet<_>>(),
                expected_rids,
                "buildTransitive {property} validation must accept exactly the contract RIDs"
            );
        }

        let supported = bridge_platform_supported_rids();
        assert_eq!(
            supported.len(),
            expected_rids.len(),
            "BridgePlatform contains a duplicate or extra supported RID"
        );
        assert_eq!(
            supported
                .into_iter()
                .map(str::to_owned)
                .collect::<BTreeSet<_>>(),
            expected_rids,
            "BridgePlatform must support exactly the contract RIDs"
        );

        let display_declaration = between(
            CSHARP_BRIDGE_PLATFORM,
            "internal const string SupportedRuntimeIdentifiers =",
            ";",
        );
        let displayed_rids = quoted_strings(display_declaration)
            .concat()
            .split(',')
            .map(str::trim)
            .map(str::to_owned)
            .collect::<BTreeSet<_>>();
        assert_eq!(
            displayed_rids, expected_rids,
            "BridgePlatform's supported-RID diagnostic must match the contract"
        );

        let native_name_declaration = between(CSHARP_NATIVE_API, "var fileName =", ";");
        let native_names = quoted_strings(native_name_declaration)
            .into_iter()
            .map(str::to_owned)
            .collect::<BTreeSet<_>>();
        assert_eq!(
            native_names, expected_native_assets,
            "the runtime loader's native asset names must match the contract package paths"
        );
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
        assert!(p.artifacts.csharp.is_none());
    }
}
