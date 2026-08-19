//! The one table mapping a bridge target to the command that installs its
//! runtime, and the only place that knows how a canonical BAML version is
//! spelled in each package registry.
//!
//! A bridge runtime must match the CLI's `baml_version::CANONICAL_VERSION`
//! exactly (`bridge_cffi`'s `validate_generated_metadata` enforces it at host
//! startup), but until now nothing printed "run exactly this".
//!
//! # This never runs anything
//!
//! `bridge install` prints; it does not execute a package manager and does not
//! edit `pyproject.toml`, `package.json`, `build.gradle.kts`, or `*.csproj`.
//! Beyond being the stated contract, the edit is unknowable (uv vs poetry vs
//! pdm vs pipenv vs pip; pnpm vs yarn vs bun vs npm, and *which*
//! `package.json` in a monorepo) — and more fundamentally it is never
//! sufficient, because it must be followed by a lock-resolving step (`uv
//! sync`, `pnpm install`, `go mod tidy`). A manifest edited without its
//! lockfile fails at import time and in CI, which is strictly worse than
//! printing a line.
//!
//! Instead of a blind dump, the ecosystem is detected read-only from lockfiles
//! already present, one recommendation is printed with the evidence that chose
//! it, and the alternates follow. With no evidence, the recommendation is the
//! modern tool (`uv`, `pnpm`), not a lowest common denominator.
//!
//! # There is no `update`
//!
//! Because the version is pinned exactly, install and upgrade emit the same
//! string in every pinned ecosystem (`uv add 'baml_bridge==0.15.0'` does
//! both), so one command covers it and its header says so.

use std::path::{Path, PathBuf};

use baml_codegen_types::OutputType;

/// How a canonical BAML version is spelled in a given registry.
///
/// Ported from `scripts/baml-language-version` (`to_pypi_version`,
/// `registry_versions_for`); a contract test asserts the two still agree, so
/// a release-time rename cannot silently desync.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Registry {
    PyPi,
    Npm,
    CratesIo,
    NuGet,
    Maven,
    SwiftPm,
    Go,
}

impl Registry {
    /// The registry key used by `scripts/baml-language-version`.
    pub(crate) const fn plan_key(self) -> &'static str {
        match self {
            Self::PyPi => "pypi",
            Self::Npm => "npm",
            Self::CratesIo => "crates_io",
            Self::NuGet => "nuget",
            Self::Maven => "maven",
            Self::SwiftPm => "swiftpm",
            Self::Go => "go",
        }
    }

    /// Translate a canonical version into this registry's spelling.
    ///
    /// Every registry but PyPI and Go takes the canonical SemVer verbatim.
    pub(crate) fn version(self, canonical: &str) -> String {
        match self {
            Self::PyPi => to_pypi_version(canonical),
            Self::Go => format!("v{canonical}"),
            _ => canonical.to_string(),
        }
    }
}

/// PEP 440 cannot express `-nightly.20260812.a`, so a nightly canonical
/// version encodes as `MAJOR.MINOR.PATCH.dev<YYYYMMDD><NN>`, where `NN` is
/// the same-day letter's zero-based index. A release version passes through.
fn to_pypi_version(canonical: &str) -> String {
    let Some((base, suffix)) = canonical.split_once("-nightly.") else {
        return canonical.to_string();
    };
    let Some((date, letter)) = suffix.split_once('.') else {
        return canonical.to_string();
    };
    let Some(index) = letter
        .chars()
        .next()
        .filter(|_| letter.len() == 1)
        .and_then(|letter| letter.is_ascii_lowercase().then(|| letter as u8 - b'a'))
    else {
        return canonical.to_string();
    };
    format!("{base}.dev{date}{index:02}")
}

/// A package the generated code imports but the runtime deliberately does not
/// depend on.
///
/// `baml_bridge` cannot depend on `pydantic`, because depending on it would
/// mean picking v1 or v2 on the user's behalf. But generated Python emits
/// `import pydantic` and `class X(pydantic.BaseModel)`, so installing the
/// bridge alone leaves a project that fails at import. The requirement
/// therefore has to travel in the printed install command.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Peer {
    pub(crate) requirement: &'static str,
}

/// One way to install, and the file whose presence votes for it.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Candidate {
    pub(crate) tool: &'static str,
    /// Lockfiles or manifests that select this tool. Empty means "no
    /// evidence can pick this; it is only ever an alternate".
    pub(crate) evidence: &'static [&'static str],
    /// `{package}`, `{version}`, and `{peers}` are substituted. `{peers}`
    /// expands to a leading space plus each quoted peer requirement, or to
    /// nothing when the target has no peers.
    pub(crate) template: &'static str,
}

/// What a target's runtime needs, if anything.
#[derive(Clone, Copy, Debug)]
pub(crate) enum Install {
    Published {
        registry: Registry,
        package: &'static str,
        peers: &'static [Peer],
        /// Ranked best-first. The first entry is also the no-evidence default.
        candidates: &'static [Candidate],
    },
    /// Nothing to install, with the reason.
    NotApplicable(&'static str),
}

const PYTHON_PEERS: &[Peer] = &[Peer {
    requirement: "pydantic>=2",
}];

const PYTHON_CANDIDATES: &[Candidate] = &[
    Candidate {
        tool: "uv",
        evidence: &["uv.lock"],
        template: "uv add '{package}=={version}'{peers}",
    },
    Candidate {
        tool: "poetry",
        evidence: &["poetry.lock"],
        template: "poetry add '{package}=={version}'{peers}",
    },
    Candidate {
        tool: "pdm",
        evidence: &["pdm.lock"],
        template: "pdm add '{package}=={version}'{peers}",
    },
    Candidate {
        tool: "pipenv",
        evidence: &["Pipfile"],
        template: "pipenv install '{package}=={version}'{peers}",
    },
    Candidate {
        tool: "pip",
        evidence: &[],
        template: "pip install '{package}=={version}'{peers}",
    },
];

const NODE_CANDIDATES: &[Candidate] = &[
    Candidate {
        tool: "pnpm",
        evidence: &["pnpm-lock.yaml"],
        template: "pnpm add {package}@{version}",
    },
    Candidate {
        tool: "yarn",
        evidence: &["yarn.lock"],
        template: "yarn add {package}@{version}",
    },
    Candidate {
        tool: "bun",
        evidence: &["bun.lock", "bun.lockb"],
        template: "bun add {package}@{version}",
    },
    Candidate {
        tool: "npm",
        evidence: &["package-lock.json"],
        template: "npm install {package}@{version}",
    },
];

const GO_CANDIDATES: &[Candidate] = &[Candidate {
    tool: "go",
    evidence: &["go.mod"],
    template: "go get {package}@{version}",
}];

const JAVA_CANDIDATES: &[Candidate] = &[
    Candidate {
        tool: "gradle (kotlin dsl)",
        evidence: &["build.gradle.kts"],
        template: "// build.gradle.kts\ndependencies {\n    implementation(\"{package}:{version}\")\n}",
    },
    Candidate {
        tool: "gradle (groovy)",
        evidence: &["build.gradle"],
        template: "// build.gradle\ndependencies {\n    implementation '{package}:{version}'\n}",
    },
    Candidate {
        tool: "maven",
        evidence: &["pom.xml"],
        template: "<!-- pom.xml -->\n<dependency>\n  <groupId>com.boundaryml</groupId>\n  <artifactId>baml-bridge</artifactId>\n  <version>{version}</version>\n</dependency>",
    },
];

const CSHARP_CANDIDATES: &[Candidate] = &[
    Candidate {
        tool: "central package management",
        evidence: &["Directory.Packages.props"],
        template: "<!-- Directory.Packages.props -->\n<PackageVersion Include=\"{package}\" Version=\"{version}\" />",
    },
    Candidate {
        tool: "dotnet",
        evidence: &[],
        template: "dotnet add package {package} --version {version}",
    },
];

const SWIFT_CANDIDATES: &[Candidate] = &[Candidate {
    tool: "swiftpm",
    evidence: &["Package.swift"],
    template: "// Package.swift\n.package(url: \"https://github.com/BoundaryML/{package}\", exact: \"{version}\")\n// then add the `BamlBridge` product to your target's dependencies",
}];

/// The runtime each target needs.
pub(crate) const fn install_for(output_type: OutputType) -> Install {
    match output_type {
        OutputType::PythonPydantic => Install::Published {
            registry: Registry::PyPi,
            package: "baml_bridge",
            peers: PYTHON_PEERS,
            candidates: PYTHON_CANDIDATES,
        },
        OutputType::TypescriptNode => Install::Published {
            registry: Registry::Npm,
            package: "@boundaryml/baml-bridge",
            peers: &[],
            candidates: NODE_CANDIDATES,
        },
        OutputType::TypescriptWeb => Install::Published {
            registry: Registry::Npm,
            package: "@boundaryml/baml-bridge-web",
            peers: &[],
            candidates: NODE_CANDIDATES,
        },
        OutputType::Go => Install::Published {
            registry: Registry::Go,
            package: "github.com/boundaryml/baml-go",
            peers: &[],
            candidates: GO_CANDIDATES,
        },
        OutputType::Java => Install::Published {
            registry: Registry::Maven,
            package: "com.boundaryml:baml-bridge",
            peers: &[],
            candidates: JAVA_CANDIDATES,
        },
        OutputType::CSharp => Install::Published {
            registry: Registry::NuGet,
            package: "baml-bridge",
            peers: &[],
            candidates: CSHARP_CANDIDATES,
        },
        OutputType::Swift => Install::Published {
            registry: Registry::SwiftPm,
            package: "baml-swift",
            peers: &[],
            candidates: SWIFT_CANDIDATES,
        },
        OutputType::Rust => Install::NotApplicable(
            "the `baml_bridge` crate is not published yet; the generated crate already pins the \
             matching version, so depend on the generated `baml_sdk` by path",
        ),
        OutputType::Cpp => Install::NotApplicable(
            "nothing to install: the generated tree vendors its headers and dlopens the runtime",
        ),
    }
}

/// A concrete install line, plus why this tool was chosen.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct Command {
    pub(crate) tool: String,
    pub(crate) command: String,
    /// The lockfile that selected this tool, when one did.
    pub(crate) evidence: Option<PathBuf>,
}

/// The recommendation and its alternates for one target.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct Plan {
    pub(crate) package: String,
    pub(crate) version: String,
    pub(crate) recommended: Command,
    pub(crate) alternates: Vec<Command>,
}

/// Build the install plan for `output_type`, detecting the host ecosystem
/// read-only by looking for lockfiles in `host_directory` and its ancestors.
///
/// Returns `Err` with the explanation for targets that install nothing.
pub(crate) fn plan(
    output_type: OutputType,
    canonical_version: &str,
    host_directory: &Path,
    project_root: &Path,
) -> Result<Plan, &'static str> {
    let (registry, package, peers, candidates) = match install_for(output_type) {
        Install::Published {
            registry,
            package,
            peers,
            candidates,
        } => (registry, package, peers, candidates),
        Install::NotApplicable(reason) => return Err(reason),
    };

    let version = registry.version(canonical_version);
    let render = |candidate: &Candidate| {
        let evidence = find_evidence(candidate, host_directory, project_root);
        Command {
            tool: candidate.tool.to_string(),
            command: fill(candidate.template, package, &version, peers),
            evidence,
        }
    };

    let rendered: Vec<Command> = candidates.iter().map(render).collect();
    // Rank by evidence first, then by the table's own order, which puts the
    // modern tool first so a project with no lockfile still gets a good default.
    let recommended_index = rendered
        .iter()
        .position(|command| command.evidence.is_some())
        .unwrap_or(0);
    let mut alternates = rendered.clone();
    let recommended = alternates.remove(recommended_index);

    Ok(Plan {
        package: package.to_string(),
        version,
        recommended,
        alternates,
    })
}

/// Look for a candidate's evidence in `directory` up to `stop_at`, inclusive.
///
/// Bounded on purpose: an unbounded walk reaches `/`, so a stray `~/uv.lock`
/// would pick the package manager for every project on the machine.
fn find_evidence(candidate: &Candidate, directory: &Path, stop_at: &Path) -> Option<PathBuf> {
    for ancestor in directory.ancestors() {
        for name in candidate.evidence {
            let path = ancestor.join(name);
            if path.is_file() {
                return Some(path);
            }
        }
        if ancestor == stop_at {
            break;
        }
    }
    None
}

fn fill(template: &str, package: &str, version: &str, peers: &[Peer]) -> String {
    let peers = peers
        .iter()
        .map(|peer| format!(" '{}'", peer.requirement))
        .collect::<String>();
    template
        .replace("{package}", package)
        .replace("{version}", version)
        .replace("{peers}", &peers)
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;

    #[test]
    fn release_versions_pass_through_every_registry_but_go() {
        assert_eq!(Registry::PyPi.version("0.15.0"), "0.15.0");
        assert_eq!(Registry::Npm.version("0.15.0"), "0.15.0");
        assert_eq!(Registry::Maven.version("0.15.0"), "0.15.0");
        assert_eq!(Registry::Go.version("0.15.0"), "v0.15.0");
    }

    #[test]
    fn nightly_versions_use_the_pep_440_dev_encoding() {
        assert_eq!(
            Registry::PyPi.version("0.15.0-nightly.20260812.a"),
            "0.15.0.dev2026081200"
        );
        assert_eq!(
            Registry::PyPi.version("0.15.0-nightly.20260812.c"),
            "0.15.0.dev2026081202"
        );
        // Non-PyPI registries carry the canonical spelling verbatim.
        assert_eq!(
            Registry::Npm.version("0.15.0-nightly.20260812.a"),
            "0.15.0-nightly.20260812.a"
        );
        assert_eq!(
            Registry::Go.version("0.15.0-nightly.20260812.a"),
            "v0.15.0-nightly.20260812.a"
        );
    }

    /// Generated Python imports pydantic, which the runtime cannot depend on,
    /// so the printed command has to carry it.
    #[test]
    fn the_python_command_installs_the_pydantic_peer() {
        let dir = tempfile::tempdir().unwrap();

        let plan = plan(OutputType::PythonPydantic, "0.15.0", dir.path(), dir.path()).unwrap();

        assert_eq!(
            plan.recommended.command,
            "uv add 'baml_bridge==0.15.0' 'pydantic>=2'"
        );
        for alternate in &plan.alternates {
            assert!(
                alternate.command.contains("pydantic>=2"),
                "`{}` drops the peer",
                alternate.command
            );
        }
    }

    #[test]
    fn with_no_evidence_the_modern_tool_is_recommended() {
        let dir = tempfile::tempdir().unwrap();

        assert_eq!(
            plan(OutputType::PythonPydantic, "0.15.0", dir.path(), dir.path())
                .unwrap()
                .recommended
                .tool,
            "uv"
        );
        assert_eq!(
            plan(OutputType::TypescriptNode, "0.15.0", dir.path(), dir.path())
                .unwrap()
                .recommended
                .tool,
            "pnpm"
        );
    }

    #[test]
    fn a_lockfile_selects_its_tool_and_is_reported_as_the_evidence() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("poetry.lock"), "").unwrap();

        let plan = plan(OutputType::PythonPydantic, "0.15.0", dir.path(), dir.path()).unwrap();

        assert_eq!(plan.recommended.tool, "poetry");
        assert_eq!(
            plan.recommended.evidence.as_deref(),
            Some(dir.path().join("poetry.lock").as_path())
        );
        // The unchosen tools remain available.
        assert!(plan.alternates.iter().any(|command| command.tool == "uv"));
    }

    /// The host manifest usually sits above the generated bridge.
    #[test]
    fn evidence_is_found_in_ancestor_directories() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("pnpm-lock.yaml"), "").unwrap();
        let nested = dir.path().join("app").join("baml_sdk");
        fs::create_dir_all(&nested).unwrap();

        assert_eq!(
            plan(OutputType::TypescriptNode, "0.15.0", &nested, dir.path())
                .unwrap()
                .recommended
                .tool,
            "pnpm"
        );
    }

    #[test]
    fn go_pins_the_v_prefixed_module_version() {
        let dir = tempfile::tempdir().unwrap();

        let plan = plan(OutputType::Go, "0.15.0", dir.path(), dir.path()).unwrap();

        assert_eq!(
            plan.recommended.command,
            "go get github.com/boundaryml/baml-go@v0.15.0"
        );
    }

    #[test]
    fn targets_that_install_nothing_explain_why() {
        let dir = tempfile::tempdir().unwrap();

        assert!(
            plan(OutputType::Rust, "0.15.0", dir.path(), dir.path())
                .unwrap_err()
                .contains("not published yet")
        );
        assert!(
            plan(OutputType::Cpp, "0.15.0", dir.path(), dir.path())
                .unwrap_err()
                .contains("vendors its headers")
        );
    }

    /// Every published target must render a command with no placeholder left
    /// behind and no peer silently dropped.
    #[test]
    fn every_published_target_renders_completely() {
        let dir = tempfile::tempdir().unwrap();

        for &output_type in OutputType::all() {
            let Ok(plan) = plan(output_type, "0.15.0", dir.path(), dir.path()) else {
                continue;
            };
            for command in std::iter::once(&plan.recommended).chain(&plan.alternates) {
                // Not "contains no brace": Gradle and Maven blocks have their
                // own. Only unsubstituted placeholders are a bug.
                for placeholder in ["{package}", "{version}", "{peers}"] {
                    assert!(
                        !command.command.contains(placeholder),
                        "{output_type:?} left {placeholder} unsubstituted: {}",
                        command.command
                    );
                }
                assert!(
                    command.command.contains(&plan.version),
                    "{output_type:?} does not pin the version: {}",
                    command.command
                );
            }
        }
    }

    /// The Rust table and `scripts/baml-language-version` must never drift:
    /// a release-time rename in one has to fail here rather than silently
    /// print an uninstallable command.
    #[test]
    #[allow(clippy::print_stderr)]
    fn registry_version_spellings_match_the_release_script() {
        let script = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../../scripts/baml-language-version")
            .canonicalize();
        let Ok(script) = script else {
            eprintln!("skipping: scripts/baml-language-version not found");
            return;
        };

        // Load the script as a module and ask it directly, rather than
        // shelling out to `compute --channel canary`. That subcommand only
        // ever yields the *current* version — today a release, whose PyPI and
        // npm spellings are both identity — so it would leave the nightly
        // `.devN` encoding, the one place the translations actually diverge,
        // untested.
        const PROBE: &str = "\
import importlib.machinery, importlib.util, json, sys
loader = importlib.machinery.SourceFileLoader('blv', sys.argv[1])
spec = importlib.util.spec_from_loader('blv', loader)
module = importlib.util.module_from_spec(spec)
sys.modules['blv'] = module
loader.exec_module(module)
print(json.dumps({v: module.registry_versions_for(v) for v in sys.argv[2:]}))
";
        // Release, first nightly of a day, a later letter, and the last one.
        let versions = [
            "0.15.0",
            "0.15.0-nightly.20260812.a",
            "0.15.0-nightly.20260812.c",
            "0.16.0-nightly.20261231.z",
        ];

        let output = std::process::Command::new("python3")
            .arg("-c")
            .arg(PROBE)
            .arg(&script)
            .args(versions)
            .output();
        let Ok(output) = output else {
            eprintln!("skipping: python3 is unavailable");
            return;
        };
        assert!(
            output.status.success(),
            "release script probe failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );

        let expected: std::collections::HashMap<String, std::collections::HashMap<String, String>> =
            serde_json::from_slice(&output.stdout).expect("probe emits registry_versions_for JSON");

        for version in versions {
            let expected = &expected[version];
            for registry in [
                Registry::PyPi,
                Registry::Npm,
                Registry::CratesIo,
                Registry::NuGet,
                Registry::Maven,
                Registry::SwiftPm,
                Registry::Go,
            ] {
                let key = registry.plan_key();
                assert_eq!(
                    registry.version(version),
                    expected[key],
                    "`{key}` spelling of `{version}` drifted from scripts/baml-language-version"
                );
            }
        }
    }
}
