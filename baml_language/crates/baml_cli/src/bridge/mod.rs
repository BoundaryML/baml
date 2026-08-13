//! `baml bridge` — everything about the generated client libraries.
//!
//! The codebase, the concept docs, the `baml-bridges` agent skill, and every
//! runtime crate (`bridge_cffi`, `bridge_python`, `bridge_go`, …) call these
//! **bridges**. Until now the CLI never said the word: `baml generate` was a
//! bare top-level verb with nowhere to hang related operations.
//!
//! `baml generate` and `baml generate add` keep working as hidden aliases,
//! because they appear in existing scripts and CI jobs.

pub(crate) mod fingerprint;
pub(crate) mod install;
pub(crate) mod passive;
pub(crate) mod status;

use std::path::{Path, PathBuf};

use anyhow::{Result, anyhow};
use clap::{Args, Subcommand};

use crate::{ExitCode, reporter::Reporter};

#[derive(Args, Clone, Debug)]
pub(crate) struct BridgeArgs {
    #[command(subcommand)]
    pub command: BridgeCommand,
}

#[derive(Subcommand, Clone, Debug)]
pub(crate) enum BridgeCommand {
    #[command(about = "Generate the client library for each configured bridge")]
    Generate(BridgeGenerateArgs),

    #[command(about = "Add a bridge to baml.toml, then print how to install its runtime")]
    Add(crate::generate::AddGeneratorArgs),

    #[command(about = "Print (never run) the command to install each bridge runtime")]
    Install(BridgeInstallArgs),

    #[command(about = "List configured bridges, their output directories, and their freshness")]
    List(BridgeListArgs),
}

/// Generate the client library for each `[generator.<name>]` in `baml.toml`.
///
/// With `--check`, nothing is written: the command compares the recorded
/// inputs against the current ones and exits non-zero if any bridge is out of
/// date.
#[derive(Args, Clone, Debug)]
#[command(after_long_help = "\
Examples:
  Generate every configured bridge:
    baml bridge generate

  Fail if a bridge is out of date, writing nothing (for CI and pre-commit):
    baml bridge generate --check

  Override the output directory:
    baml bridge generate --output-dir ./generated")]
pub(crate) struct BridgeGenerateArgs {
    // Not a doc comment: doc comments render into user-facing help, and the
    // rationale below is for maintainers.
    //
    // `--check` deliberately compares *inputs*, never regenerated output.
    // That makes it immune to codegen nondeterminism (Go shells out to the
    // host `gofmt`) and lets it run without compiling at all.
    /// Exit non-zero if any bridge is out of date. Writes nothing.
    #[arg(long, help_heading = "Generation options")]
    pub check: bool,

    /// Output directory override (takes precedence over generator config)
    #[arg(
        long = "output-dir",
        alias = "output",
        short = 'o',
        value_name = "PATH",
        help_heading = "Generation options"
    )]
    pub output: Option<PathBuf>,
}

/// Print the command that installs each configured bridge's runtime.
///
/// This never runs a package manager and never edits a host manifest
/// (`pyproject.toml`, `package.json`, `build.gradle.kts`, `*.csproj`). The
/// correct edit depends on the project's tooling, and on its own it is never
/// enough — it has to be followed by a lock-resolving step. A manifest edited
/// without its lockfile fails at import time and in CI.
///
/// Because the runtime version is pinned exactly, the printed command both
/// installs and upgrades; there is no separate `update`.
#[derive(Args, Clone, Debug)]
#[command(after_long_help = "\
Examples:
  Print the install command for every configured bridge:
    baml bridge install

  Print it for a specific project:
    baml bridge install --project ./my-project")]
pub(crate) struct BridgeInstallArgs {}

/// List every configured bridge with its target, output directory, and
/// whether its generated code is up to date.
#[derive(Args, Clone, Debug)]
#[command(after_long_help = "\
Examples:
  List every configured bridge:
    baml bridge list

  List the bridges of a specific project:
    baml bridge list --project ./my-project")]
pub(crate) struct BridgeListArgs {}

impl BridgeArgs {
    pub(crate) fn run(&self, project: Option<&Path>) -> Result<ExitCode> {
        match &self.command {
            BridgeCommand::Generate(args) if args.check => check(project),
            BridgeCommand::Generate(args) => {
                crate::generate::run_generate(project, args.output.as_deref())
            }
            BridgeCommand::Add(args) => args.run(project),
            BridgeCommand::Install(_) => print_install(project),
            BridgeCommand::List(_) => list(project),
        }
    }
}

/// A configured bridge together with everything a read-only command needs.
struct Resolved {
    root: PathBuf,
    fingerprint: String,
    generators: Vec<crate::generate::GeneratorDef>,
    /// Sections present in `baml.toml` that failed validation. They are not
    /// silently dropped: a bridge you configured but mis-specified should not
    /// read as a bridge you never configured.
    invalid: usize,
}

/// Resolve the project and its configured bridges without compiling.
///
/// Full generator diagnostics (with spans) are rendered by `bridge generate`,
/// which fails on them. These read-only commands only count the bad sections,
/// so they can say a section exists and is broken without pretending to be a
/// compiler.
fn resolve(from: Option<&Path>) -> Result<Resolved> {
    let layout = crate::project_load::resolve_project_layout(from)?
        .ok_or_else(|| anyhow!("no BAML project found; run `baml init` first"))?;
    let fingerprint = fingerprint::compute(&layout.root, &layout.source_root)?;
    let (generators, diagnostics) = crate::generate::discover_generators(&layout.root);
    let invalid = invalid_section_count(&layout.root, generators.len(), &diagnostics);
    Ok(Resolved {
        root: layout.root,
        fingerprint,
        generators,
        invalid,
    })
}

/// How many `[generator.<name>]` sections exist but did not validate.
fn invalid_section_count(
    root: &Path,
    valid: usize,
    diagnostics: &[baml_db::baml_compiler_diagnostics::Diagnostic],
) -> usize {
    if diagnostics.is_empty() {
        return 0;
    }
    let declared = std::fs::read_to_string(root.join("baml.toml"))
        .ok()
        .and_then(|content| crate::manifest::parse(&content).ok())
        .map_or(0, |manifest| manifest.generator.len());
    declared.saturating_sub(valid)
}

/// Report sections that exist but did not validate, so a misconfigured bridge
/// is never invisible.
fn report_invalid(resolved: &Resolved) {
    if resolved.invalid > 0 {
        crate::reporter::print_warning(format_args!(
            "{} `[generator]` section(s) in baml.toml are invalid and were skipped; \
             run `baml bridge generate` to see why",
            resolved.invalid
        ));
    }
}

/// `bridge generate --check`: compare inputs, write nothing, never compile.
fn check(from: Option<&Path>) -> Result<ExitCode> {
    let resolved = resolve(from)?;
    if resolved.generators.is_empty() {
        report_invalid(&resolved);
        crate::reporter::print_error(if resolved.invalid > 0 {
            "no usable `[generator.<name>]` sections in baml.toml"
        } else {
            "no `[generator.<name>]` sections found in baml.toml"
        });
        return Ok(ExitCode::Other);
    }
    report_invalid(&resolved);

    let mut stale = false;
    for generator in &resolved.generators {
        let status = status::evaluate(
            &generator.output_dir,
            &resolved.fingerprint,
            baml_version::CANONICAL_VERSION,
            status::Depth::VerifyFiles,
        )?;
        match &status {
            status::Status::Fresh => {}
            // Unlike the passive warning, `--check` does report a bridge that
            // was never generated: a CI job asking "is the committed bridge
            // current?" must fail when there is no bridge at all.
            status::Status::NeverGenerated => {
                stale = true;
                crate::reporter::print_error(format_args!(
                    "bridge `{}` has never been generated; run `baml bridge generate`",
                    generator.name
                ));
            }
            status::Status::Stale(reasons) => {
                stale = true;
                crate::reporter::print_error(format_args!(
                    "{}",
                    status::warning(&generator.name, reasons)
                ));
            }
        }
    }

    if stale {
        return Ok(ExitCode::BridgeStale);
    }
    Reporter::new().finish(
        "Checked",
        format!("{} bridge(s) up to date", resolved.generators.len()),
    );
    Ok(ExitCode::Success)
}

/// `bridge list`: the configured bridges and their freshness.
fn list(from: Option<&Path>) -> Result<ExitCode> {
    let resolved = resolve(from)?;
    if resolved.generators.is_empty() {
        report_invalid(&resolved);
        crate::reporter::print_error(if resolved.invalid > 0 {
            "no usable `[generator.<name>]` sections in baml.toml"
        } else {
            "no `[generator.<name>]` sections found in baml.toml"
        });
        return Ok(ExitCode::Other);
    }
    report_invalid(&resolved);

    #[allow(clippy::print_stdout)]
    for generator in &resolved.generators {
        // Manifest depth: `list` is an overview, not an audit, so it does not
        // re-hash every generated file. `--check` does that.
        let status = status::evaluate(
            &generator.output_dir,
            &resolved.fingerprint,
            baml_version::CANONICAL_VERSION,
            status::Depth::Manifest,
        )?;
        let freshness = match status {
            status::Status::Fresh => "up to date".to_string(),
            status::Status::NeverGenerated => "not generated".to_string(),
            status::Status::Stale(reasons) => match reasons.first() {
                Some(status::Reason::ToolchainSkew { generated_by, .. }) => {
                    format!("out of date (built by BAML {generated_by})")
                }
                Some(status::Reason::ProvenanceMissing) => {
                    "out of date (built before freshness tracking)".to_string()
                }
                _ => "out of date".to_string(),
            },
        };
        println!(
            "{}\t{}\t{}\t{}",
            generator.name,
            generator.output_type,
            display_relative(&generator.output_dir, &resolved.root),
            freshness
        );
    }
    Ok(ExitCode::Success)
}

/// `bridge install`: print, never run.
fn print_install(from: Option<&Path>) -> Result<ExitCode> {
    let resolved = resolve(from)?;
    if resolved.generators.is_empty() {
        report_invalid(&resolved);
        crate::reporter::print_error(if resolved.invalid > 0 {
            "no usable `[generator.<name>]` sections in baml.toml"
        } else {
            "no `[generator.<name>]` sections found in baml.toml"
        });
        return Ok(ExitCode::Other);
    }
    report_invalid(&resolved);

    #[allow(clippy::print_stdout)]
    for (index, generator) in resolved.generators.iter().enumerate() {
        if index > 0 {
            println!();
        }
        print_install_for(generator, &resolved.root);
    }
    Ok(ExitCode::Success)
}

/// Print one bridge's install block. Shared with `bridge add`, which shows
/// the same thing immediately after writing `baml.toml`.
#[allow(clippy::print_stdout)]
pub(crate) fn print_install_for(generator: &crate::generate::GeneratorDef, root: &Path) {
    // Detect the host ecosystem from the directory that will contain the
    // generated bridge, since that is where the host manifest lives.
    let host = generator.output_dir.parent().unwrap_or(root);
    match install::plan(
        generator.output_type,
        baml_version::CANONICAL_VERSION,
        host,
        root,
    ) {
        Ok(plan) => {
            println!(
                "{} ({}) — install or upgrade to {}:",
                generator.name, generator.output_type, plan.version
            );
            println!();
            for line in plan.recommended.command.lines() {
                println!("  {line}");
            }
            if let Some(evidence) = &plan.recommended.evidence {
                println!(
                    "  # recommended: found {}",
                    display_relative(evidence, root)
                );
            } else {
                println!("  # recommended: no lockfile found, so this is the default");
            }
            if !plan.alternates.is_empty() {
                println!();
                println!("  # alternatives:");
                for alternate in &plan.alternates {
                    println!("  #   {}:", alternate.tool);
                    // Every line gets its own `#`. Gradle, Maven and SwiftPM
                    // are multi-line blocks, so prefixing only the first line
                    // would leave the rest looking like live commands.
                    for line in alternate.command.lines() {
                        println!("  #     {line}");
                    }
                }
            }
        }
        Err(reason) => {
            println!("{} ({}) — {reason}", generator.name, generator.output_type);
        }
    }
}

fn display_relative(path: &Path, root: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .display()
        .to_string()
}

#[cfg(test)]
mod tests {
    use std::fs;

    use baml_codegen_types::{
        GeneratedOutputFile, OutputOptions, OutputProvenance, VcsPolicy, write_generated_output,
    };

    use super::*;

    /// A project with one Python bridge configured.
    fn project() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        fs::write(
            dir.path().join("baml.toml"),
            "[package]\nname = \"test\"\n\n[generator.client1]\noutput_type = \"python/pydantic\"\n\
             output_dir = \".\"\nnaming_convention = \"preserve-case\"\n",
        )
        .unwrap();
        let source_root = dir.path().join("baml_src");
        fs::create_dir_all(&source_root).unwrap();
        fs::write(source_root.join("main.baml"), "class A {}").unwrap();
        dir
    }

    /// Write a bridge recorded as built from the project's current inputs.
    fn generate_bridge(dir: &Path) {
        let resolved = resolve(Some(dir)).unwrap();
        let generator = &resolved.generators[0];
        write_generated_output(
            &generator.output_dir,
            vec![GeneratedOutputFile::new("value.py", "generated")],
            &OutputOptions {
                provenance: OutputProvenance {
                    input_fingerprint: resolved.fingerprint.clone(),
                    toolchain_version: baml_version::CANONICAL_VERSION.to_string(),
                    generator_name: generator.name.clone(),
                },
                vcs: VcsPolicy::Ignore,
            },
        )
        .unwrap();
    }

    #[test]
    fn check_passes_for_a_freshly_generated_bridge() {
        let dir = project();
        generate_bridge(dir.path());

        assert!(matches!(
            check(Some(dir.path())).unwrap(),
            ExitCode::Success
        ));
    }

    #[test]
    fn check_fails_after_a_source_edit() {
        let dir = project();
        generate_bridge(dir.path());
        fs::write(dir.path().join("baml_src/main.baml"), "class A { x int }").unwrap();

        assert!(matches!(
            check(Some(dir.path())).unwrap(),
            ExitCode::BridgeStale
        ));
    }

    /// An `output_dir` override changes where a bridge lands, not what is in
    /// it, so it must not invalidate one.
    #[test]
    fn check_passes_after_a_comment_only_edit_is_regenerated() {
        let dir = project();
        generate_bridge(dir.path());
        // `baml.toml` is embedded verbatim into every bridge, so a comment
        // edit really does make it stale until regenerated.
        let manifest = dir.path().join("baml.toml");
        let content = fs::read_to_string(&manifest).unwrap();
        fs::write(&manifest, format!("# note\n{content}")).unwrap();
        assert!(matches!(
            check(Some(dir.path())).unwrap(),
            ExitCode::BridgeStale
        ));

        generate_bridge(dir.path());
        assert!(matches!(
            check(Some(dir.path())).unwrap(),
            ExitCode::Success
        ));
    }

    #[test]
    fn check_reports_a_bridge_that_was_never_generated() {
        let dir = project();

        assert!(matches!(
            check(Some(dir.path())).unwrap(),
            ExitCode::BridgeStale
        ));
    }

    #[test]
    fn check_catches_a_hand_edited_generated_file() {
        let dir = project();
        generate_bridge(dir.path());
        let generated = resolve(Some(dir.path())).unwrap().generators[0]
            .output_dir
            .join("value.py");
        fs::write(&generated, "tampered").unwrap();

        assert!(matches!(
            check(Some(dir.path())).unwrap(),
            ExitCode::BridgeStale
        ));
    }

    #[test]
    fn list_and_install_succeed_for_a_configured_project() {
        let dir = project();
        generate_bridge(dir.path());

        assert!(matches!(list(Some(dir.path())).unwrap(), ExitCode::Success));
        assert!(matches!(
            print_install(Some(dir.path())).unwrap(),
            ExitCode::Success
        ));
    }
}
