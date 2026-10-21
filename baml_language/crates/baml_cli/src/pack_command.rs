// `baml pack` — compile one or more BAML functions into a single
// self-contained executable.
//
// Target resolution mirrors `baml run`'s shape minus two things:
//   - `-e` is not packageable (no persistent target to bake in).
//   - `[scripts]` are not packageable — scripts are a dev-time dispatch
//     mechanism, not an entry-point concept.
//
// Targets are picked via repeatable `-f/--function` flags. With multiple
// `-f` the packed binary becomes a multi-subcommand CLI (`./cli foo …`,
// `./cli bar …`); with a single `-f` it's the same shape with one
// subcommand. A bare positional `<TARGET>` runs a single function as the
// binary's only entry point (no subcommand layer).
//
// Standalone single-file sources are loaded via `--file <PATH>`, which
// is mutually exclusive with `--from <DIR>`. Without either, the project
// at the current directory is loaded.
//
// The output is the host binary (baml-pack-host) with a `PackEnvelope`
// (borsh-serialized) appended in an OS-native section. At runtime the
// host extracts the envelope, initializes the engine, and invokes the
// baked-in target with an auto-CLI parser driven by its signature.

#![allow(clippy::print_stdout, clippy::print_stderr)]

use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::Arc,
};

use anyhow::{Context, Result, anyhow};
use baml_db::{ProjectDatabase, baml_compiler_diagnostics::Severity};
use baml_exec::{OutputFormat, PACK_SECTION_NAME, PackEnvelope, validate_help_param};
use bex_engine::BexEngine;
use bex_vm_types::types::Program;
use clap::Args;
use sys_native::SysOpsExt;

use crate::{
    commands::release_version,
    project_load::{resolve_standalone_file, validate_file_project_flags, workspace_db},
    reporter::Reporter,
};

/// Package one or more BAML targets as a standalone executable.
///
/// A positional target produces a single-entry executable. One or more
/// `--function` flags produce an executable whose generated CLI has one
/// subcommand per function. Function parameters are derived from BAML types.
#[derive(Args, Clone, Debug)]
#[command(after_long_help = "\
Examples:
  Package one function:
    baml pack main

  Choose the executable path:
    baml pack main --output ./my-tool

  Package multiple functions as subcommands:
    baml pack --function Extract --function Classify --output ./baml-tools

  Package a function from a standalone file:
    baml pack --file script.baml main")]
pub struct PackArgs {
    #[command(flatten)]
    pub telemetry: crate::embed_telemetry::EmbedTelemetryArgs,
    #[command(flatten)]
    pub compiler: crate::commands::CompilerArgs,

    /// Function to package as the executable's only entry point.
    ///
    /// Mutually exclusive with `--function`.
    #[arg(value_name = "FUNCTION")]
    pub target: Option<String>,

    /// Add a function as a generated executable subcommand. Repeatable.
    ///
    /// Even one `--function` creates a subcommand. Use a positional `<TARGET>`
    /// to produce an executable with no subcommand layer.
    #[arg(
        short = 'f',
        long = "function",
        value_name = "NAME",
        help_heading = "Target options"
    )]
    pub functions: Vec<String>,

    /// Load one standalone source file instead of discovering a project.
    ///
    /// Mutually exclusive with `--project`.
    #[arg(long, value_name = "PATH", help_heading = "Project options")]
    pub file: Option<PathBuf>,

    /// Path for the packaged executable.
    ///
    /// Defaults to `[package].name`, the project directory name, or the source
    /// file stem, depending on the project mode.
    #[arg(short, long, help_heading = "Build options")]
    pub output: Option<PathBuf>,

    /// Compile the program's functions to Rust where possible.
    ///
    /// Every function the compiler admits runs as Rust inside the VM; the rest
    /// run as bytecode, unchanged. Builds a host binary with `cargo` from the
    /// BAML language workspace named by `--rust-workspace`.
    #[arg(long, help_heading = "Build options")]
    pub rust: bool,

    /// The `baml_language` workspace checkout the generated host is built from.
    #[arg(
        long,
        value_name = "DIR",
        requires = "rust",
        help_heading = "Build options"
    )]
    pub rust_workspace: Option<PathBuf>,

    /// With `--rust`, the directory the generated crate is built in.
    /// Defaults to `target/baml-rust` under the current directory.
    #[arg(
        long,
        value_name = "DIR",
        requires = "rust",
        help_heading = "Build options"
    )]
    pub rust_build_dir: Option<PathBuf>,

    /// With `--rust`, how hard cargo optimizes the generated crate.
    ///
    /// `release` is the shipping build (fat LTO, one codegen unit, symbols
    /// stripped). `fast` trades that for build time (opt-level 1, no LTO, line
    /// tables kept) and is for correctness runs only: its output is not what
    /// a release pack ships and its speed says nothing about one.
    #[arg(
        long,
        value_enum,
        value_name = "PROFILE",
        default_value_t = RustProfile::Release,
        requires = "rust",
        help_heading = "Build options"
    )]
    pub rust_profile: RustProfile,

    /// Target triple for the packaged executable.
    ///
    /// Defaults to the host platform. Cross-compilation downloads the matching
    /// pack host from the BAML release artifacts.
    #[arg(long = "target", value_name = "TRIPLE", help_heading = "Build options")]
    pub target_triple: Option<String>,

    #[arg(
        long,
        value_enum,
        value_name = "FORMAT",
        help = "Format returned values [default: json] [possible values: debug, json]",
        hide_default_value = true,
        hide_possible_values = true,
        default_value_t = OutputFormat::Json,
        help_heading = "Runtime options"
    )]
    pub output_format: OutputFormat,

    /// Deprecated alias for `--project`.
    #[arg(long, value_name = "PATH", hide = true)]
    pub from: Option<PathBuf>,

    #[arg(short = 'e', long = "expression", value_name = "EXPR", hide = true)]
    pub expression: Option<String>,
}

/// The cargo profile a `--rust` build compiles the generated crate under.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, clap::ValueEnum)]
pub enum RustProfile {
    #[default]
    Release,
    Fast,
}

impl RustProfile {
    /// The `[profile.release]` settings of the generated crate's manifest —
    /// the build always runs as `cargo build --release`, and the profile
    /// decides what that means.
    fn cargo_profile_settings(self) -> toml::Table {
        let mut settings = toml::Table::new();
        let mut set = |key: &str, value: toml::Value| {
            settings.insert(key.to_owned(), value);
        };
        match self {
            Self::Release => {
                set("lto", "fat".into());
                set("codegen-units", 1.into());
                set("opt-level", 3.into());
                set("strip", "symbols".into());
            }
            Self::Fast => {
                set("lto", false.into());
                set("codegen-units", 16.into());
                set("opt-level", 1.into());
                set("strip", "none".into());
                set("debug", "line-tables-only".into());
            }
        }
        set("panic", "unwind".into());
        settings
    }
}

/// One resolved entry point baked into a packed binary.
#[derive(Debug, Clone)]
struct ResolvedPackTarget {
    qualified_name: String,
    /// Display name (qualified, `user.` prefix stripped).
    display_name: String,
    /// CLI subcommand name (last `.`-segment of `display_name`).
    subcommand_name: String,
}

impl PackArgs {
    pub fn run(&self) -> Result<crate::ExitCode> {
        let reporter = Reporter::new();
        self.run_with_reporter(&reporter)
    }

    fn run_with_reporter(&self, reporter: &Reporter) -> Result<crate::ExitCode> {
        self.validate_flags()?;
        let manifest = self.artifact_manifest()?;
        let mut telemetry = btel_settings::artifact::ArtifactTelemetry::from_manifest(
            &manifest,
            btel_settings::artifact::ArtifactKind::Pack,
        )?;
        let default_environment = manifest
            .get("pack")
            .and_then(|table| table.get("telemetry_environment"));
        let default_environment = default_environment
            .map(|value| {
                value
                    .as_str()
                    .context("pack.telemetry_environment must be a string.")
            })
            .transpose()?;
        let provisioning = self
            .telemetry
            .prepare(&mut telemetry, default_environment, "pack")?;

        let (db, root, program, needs_format_hint) = self.load_and_compile(reporter)?;
        // Mirror `baml run`'s format advisory: if any source file
        // round-trips through `baml fmt` differently, surface a
        // non-fatal warning so users learn to keep packaged
        // projects formatted. Pack is a release-shaped operation,
        // so a clean tree matters at least as much as for run.
        if needs_format_hint {
            reporter.warning(crate::run_command::FORMAT_HINT);
        }

        // Signature info for target resolution / reserved `help` check.
        let engine = BexEngine::new(
            program.clone(),
            Arc::new(sys_native::SysOps::native()),
            vec![],
        )
        .map_err(|e| anyhow!("failed to initialize engine for resolution: {e:?}"))?;

        let (mode, targets) = self.resolve_targets(&engine)?;
        for t in &targets {
            validate_help_param(&engine, &t.qualified_name)?;
        }

        let mut envelope = PackEnvelope {
            program,
            mode: mode.clone(),
            targets: targets
                .iter()
                .map(|t| baml_exec::TargetEntry {
                    qualified_name: t.qualified_name.clone(),
                    display_name: t.display_name.clone(),
                    subcommand_name: t.subcommand_name.clone(),
                })
                .collect(),
            output_format: self.output_format,
            telemetry,
        };
        if let Some(provisioning) = provisioning {
            reporter.spin("Provisioning", "embedded telemetry");
            envelope.telemetry.embedded =
                Some(Box::new(provisioning.mint(envelope.telemetry_digest()?)?));
        }
        let (serialized, fingerprint) = baml_artifact::encode_with_digest(
            baml_artifact::ArtifactKind::PackedProgram,
            &envelope,
        )
        .map_err(|e| anyhow!("failed to serialize pack envelope: {e}"))?;

        let compiled = if self.rust {
            reporter.spin("Compiling", "BAML to Rust");
            let callables = envelope.program.rendered_callables();
            let entries: Option<Vec<usize>> = targets
                .iter()
                .map(|target| {
                    callables
                        .get(&target.qualified_name)
                        .map(|callable| callable.object.raw())
                })
                .collect();
            let module = baml_db::rust_backend::compile_program(
                &db,
                root,
                &envelope.program,
                crate::bytecode_cache::CLI_OPT_LEVEL,
                "program.bexa",
                fingerprint,
                entries.as_deref(),
            )
            .map_err(|error| anyhow!(error))?;
            Some(module)
        } else {
            None
        };
        drop(db);
        let label = label_for(&targets);
        reporter.spin("Packaging", &label);
        let target_triple = self.resolved_target_triple()?;
        let host_bytes = if compiled.is_some() {
            Vec::new()
        } else {
            read_host_binary(target_triple, reporter)?
        };

        let basename = self.resolve_output_basename()?;
        let output_path = self
            .output
            .clone()
            .unwrap_or_else(|| default_output_path(&basename, target_triple));

        if let Some(module) = compiled {
            self.build_rust(&module, &serialized, &output_path, target_triple, reporter)?;
        } else {
            let mut output_file = std::fs::File::create(&output_path)
                .with_context(|| format!("failed to create {}", output_path.display()))?;
            write_executable(&host_bytes, &serialized, &mut output_file, target_triple)?;
        }

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&output_path, std::fs::Permissions::from_mode(0o755))
                .with_context(|| {
                    format!("failed to set permissions on {}", output_path.display())
                })?;
        }

        // Cargo's `Finished` is `<artifact-or-profile> [metadata]
        // in <elapsed>`. Mirror that: the artifact location is the
        // primary fact, with target name + triple in brackets so
        // the user can see what was packed and for which platform
        // without parsing an ambiguous arrow.
        reporter.finish(
            "Finished",
            format!("{} [{label}, {}]", output_path.display(), target_triple),
        );
        Ok(crate::ExitCode::Success)
    }

    /// Validate flag combinations the clap-side `Args` derive can't catch.
    fn validate_flags(&self) -> Result<()> {
        if self.expression.is_some() {
            anyhow::bail!(
                "expression mode (`-e` / `--expression`) is not packageable; \
                 pass a positional `<TARGET>` or `-f <NAME>` instead."
            );
        }
        // `--file` and `--project` both name a source location. Reject the
        // combination up front instead of silently preferring one. Same rule
        // as `baml run`.
        validate_file_project_flags(self.file.as_deref(), self.from.as_deref())?;
        if let Some(target) = self.target.as_deref() {
            if looks_like_path(target) {
                anyhow::bail!(
                    "positional `<TARGET>` is a function name, not a file path. \
                     For a single-file source, use `--file {target}` and pass the \
                     function via `-f <NAME>`. For example:\n\
                     \n    `baml pack --file {target} -f <NAME>`\n",
                );
            }
            if !self.functions.is_empty() {
                anyhow::bail!(
                    "positional `<TARGET>` and `-f/--function` are mutually exclusive — \
                     use one or the other (positional packs a single-entry binary; \
                     `-f` produces a subcommand binary)."
                );
            }
        }
        if self.target.is_none() && self.functions.is_empty() {
            anyhow::bail!(
                "no target specified. Pass a positional `<TARGET>` to pack one \
                 function as the binary's only entry point, or one or more \
                 `-f <NAME>` flags to pack a subcommand binary."
            );
        }
        Ok(())
    }

    fn resolved_target_triple(&self) -> Result<&str> {
        match self.target_triple.as_deref() {
            Some(target) => validate_release_target_triple(target),
            None => release_host_target_triple(),
        }
    }

    fn load_and_compile(
        &self,
        reporter: &Reporter,
    ) -> Result<(ProjectDatabase, baml_db::SourceRoot, Program, bool)> {
        if let Some(file) = self.file.as_deref() {
            // Standalone `--file` mode has no project root, so there is no
            // cache seam — always a cold compile, same as `baml run --file`.
            let (db, package, needs_format_hint) = self.load_standalone(file)?;
            check_diagnostics(&db, "cannot pack: compilation errors found", reporter)?;
            let program = crate::bytecode_cache::compile_program(&db, package, None)
                .map_err(|e| anyhow!("compilation failed: {e:?}"))?;
            return Ok((db, package, program, needs_format_hint));
        }
        self.load_and_compile_project(reporter)
    }

    /// The generated crate embeds the program its compiled bodies replace.
    fn build_rust(
        &self,
        module: &baml_db::rust_backend::RustModule,
        serialized: &[u8],
        output_path: &Path,
        target_triple: &str,
        reporter: &Reporter,
    ) -> Result<()> {
        let Some(workspace) = self.rust_workspace.as_deref() else {
            anyhow::bail!(
                "`--rust` needs `--rust-workspace <DIR>`: the `baml_language` checkout \
                 whose `crates/bex_vm` the compiled bodies link against"
            );
        };
        let workspace = workspace
            .canonicalize()
            .with_context(|| format!("failed to resolve {}", workspace.display()))?;
        for crate_dir in [
            "crates/bex_vm",
            "crates/bex_vm_types",
            "crates/baml_pack_host",
        ] {
            if !workspace.join(crate_dir).join("Cargo.toml").is_file() {
                anyhow::bail!(
                    "`--rust-workspace` {} does not contain {crate_dir}",
                    workspace.display()
                );
            }
        }
        if target_triple != release_host_target_triple()? {
            anyhow::bail!("`--rust` builds for this machine only; drop `--target`");
        }

        let build_dir = self
            .rust_build_dir
            .clone()
            .unwrap_or_else(|| PathBuf::from("target").join("baml-rust"));
        let src_dir = build_dir.join("src");
        std::fs::create_dir_all(&src_dir)
            .with_context(|| format!("failed to create {}", src_dir.display()))?;
        std::fs::write(src_dir.join("program.bexa"), serialized)
            .context("failed to write the packed program")?;
        std::fs::write(src_dir.join("lib.rs"), &module.source)
            .context("failed to write the generated module")?;
        std::fs::write(
            src_dir.join("main.rs"),
            "fn main() -> std::process::ExitCode {\n    \
             baml_pack_host::run_embedded(\n        \
             baml_rust_program::PROGRAM_ARTIFACT,\n        \
             Some(&baml_rust_program::COMPILED_BODIES),\n    )\n}\n",
        )
        .context("failed to write the host shim")?;
        std::fs::write(
            build_dir.join("Cargo.toml"),
            rust_manifest(&workspace, self.rust_profile),
        )
        .context("failed to write the generated manifest")?;

        reporter.spin(
            "Building",
            format!("{} compiled functions with cargo", module.functions.len()),
        );
        // The host must accept the envelope this CLI encodes, and the
        // fingerprint exists to refuse a host built from other sources. So the
        // workspace must be at the commit this CLI was built from; the host
        // then derives the same fingerprint from its own checkout.
        if baml_artifact::ENFORCE_BUILD_FINGERPRINT {
            let fingerprint = baml_artifact::BUILD_FINGERPRINT;
            let head = std::process::Command::new("git")
                .arg("-C")
                .arg(&workspace)
                .args(["rev-parse", "HEAD"])
                .output()
                .ok()
                .filter(|output| output.status.success())
                .map(|output| String::from_utf8_lossy(&output.stdout).trim().to_owned());
            if head.as_deref() != Some(fingerprint) {
                anyhow::bail!(
                    "`--rust-workspace` {} is at {}, but this CLI was built from {fingerprint}; \
                     check out that commit so the host and this CLI agree on the program format",
                    workspace.display(),
                    head.as_deref().unwrap_or("an unknown commit"),
                );
            }
        }
        // Build against the workspace's own dependency versions, the ones its
        // tests ran with, rather than resolving fresh ones.
        let lockfile = workspace.join("Cargo.lock");
        if lockfile.is_file() {
            std::fs::copy(&lockfile, build_dir.join("Cargo.lock"))
                .context("failed to copy the workspace's Cargo.lock")?;
        }
        // An explicit target dir so the built binary's location is known
        // whatever the environment says.
        let target_dir = build_dir.join("target");
        let status = reporter.suspend(|| {
            let mut cargo = std::process::Command::new("cargo");
            cargo
                .arg("build")
                .arg("--release")
                .arg("--manifest-path")
                .arg(build_dir.join("Cargo.toml"))
                .arg("--target-dir")
                .arg(&target_dir)
                // From the workspace, so rustup selects the toolchain its
                // `rust-toolchain.toml` pins (the one the runtime was tested
                // with), not the machine's default.
                .current_dir(&workspace);
            // A BAML function with thousands of locals becomes one Rust
            // function as large, and rustc's default stack can overflow
            // generating its debug info; give it room unless the caller chose.
            if baml_env::os_var("RUST_MIN_STACK").is_none() {
                cargo.env("RUST_MIN_STACK", (256usize << 20).to_string());
            }
            cargo
                .status()
                .context("failed to run cargo; is it on PATH?")
        })?;
        if !status.success() {
            anyhow::bail!("cargo build of the generated host failed ({status})");
        }
        let built = target_dir
            .join("release")
            .join(format!("baml_rust_program{}", std::env::consts::EXE_SUFFIX));
        std::fs::copy(&built, output_path).with_context(|| {
            format!(
                "failed to copy {} to {}",
                built.display(),
                output_path.display()
            )
        })?;
        Ok(())
    }

    fn artifact_manifest(&self) -> Result<toml::Value> {
        if self.file.is_some() {
            return Ok(toml::Value::Table(toml::Table::new()));
        }
        let Some(root) = crate::project_load::find_project_root_from(self.from.as_deref())? else {
            return Ok(toml::Value::Table(toml::Table::new()));
        };
        let path = root.join("baml.toml");
        match std::fs::read_to_string(&path) {
            Ok(content) => {
                Ok(toml::from_str(&content)
                    .with_context(|| format!("Invalid {}", path.display()))?)
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                Ok(toml::Value::Table(toml::Table::new()))
            }
            Err(error) => Err(error).with_context(|| format!("failed to read {}", path.display())),
        }
    }

    /// Project-mode load + compile through the bytecode cache — the same warm
    /// flow as `baml run` (`run_command::load_and_compile`): whole-program hit
    /// when nothing changed, served packages plus a fresh user package on an
    /// edit. Pack shares run/check's exact cache key space, so a pack
    /// right after a run (or a
    /// re-pack) serves the identical `Program`. The packaged bytecode is
    /// target-independent (the `--target` triple only selects the host binary
    /// bytes), and emit determinism guarantees a reused image is byte-identical
    /// to a fresh compile, so serving from cache never changes the artifact.
    ///
    /// Compiled bodies and bytecode must come from the same lowering.
    fn load_and_compile_project(
        &self,
        reporter: &Reporter,
    ) -> Result<(ProjectDatabase, baml_db::SourceRoot, Program, bool)> {
        let mut session = crate::project_session::ProjectSession::open(
            self.from.as_deref(),
            if self.rust {
                crate::project_session::CacheUse::Off
            } else {
                crate::project_session::CacheUse::ReadWrite
            },
        )?;
        if session.is_empty() {
            anyhow::bail!("no `.baml` files found in {}", session.root().display());
        }
        // Mirror `baml run`'s per-file format check: probe each source through
        // the formatter and emit a single advisory if any file would change.
        let needs_format_hint = session.needs_format_hint();

        if let Some(program) = session.try_cached_program() {
            let package = session.package;
            return Ok((session.db, package, program, needs_format_hint));
        }

        // Seed the stdlib typed interface and install the served check rows —
        // the same warm-database setup run/check/test use.
        let warmth = session.warm_prep();
        let (served, stdlib_interface_hit) = (warmth.served, warmth.stdlib_interface_hit);
        let db = &session.db;
        let package = session.package;
        let cache = &session.cache;

        // Keep `baml pack` quiet during compilation. Its visible progress is
        // packaging/downloading/output-oriented; compile/count status belongs
        // to `check` and `generate`.
        //
        // With a cache, gate diagnostics through the incremental collector: it
        // serves the check rows of an unchanged project and checks every file
        // otherwise, returning the fresh per-file blobs to persist. Without a
        // cache, run the honest full check (no blobs to store).
        let fresh_diagnostics = if let Some(ctx) = cache {
            let incremental = ctx.collect_diagnostics_incremental(db, package, served.as_ref());
            bail_on_error_diagnostics(
                db,
                &incremental.merged,
                "cannot pack: compilation errors found",
                reporter,
            )?;
            Some(incremental.fresh_by_file)
        } else {
            check_diagnostics(db, "cannot pack: compilation errors found", reporter)?;
            None
        };

        let program = crate::bytecode_cache::compile_program(db, package, cache.as_ref())
            .map_err(|e| anyhow!("compilation failed: {e:?}"))?;
        if let Some(ctx) = cache {
            let fresh = fresh_diagnostics
                .as_ref()
                .expect("a cache is present, so fresh diagnostics were computed");
            ctx.verify_and_store(
                &session,
                &program,
                fresh,
                served.as_ref(),
                stdlib_interface_hit,
            )?;
        }
        Ok((session.db, package, program, needs_format_hint))
    }

    fn load_standalone(
        &self,
        file_path: &Path,
    ) -> Result<(ProjectDatabase, baml_db::SourceRoot, bool)> {
        let canonical = resolve_standalone_file(file_path)?;
        let content = std::fs::read_to_string(&canonical)
            .with_context(|| format!("failed to read {}", canonical.display()))?;
        let needs_format_hint = crate::run_command::source_needs_format_hint(&content);
        let parent = canonical.parent().unwrap_or_else(|| Path::new("."));
        let (mut db, workspace) = workspace_db(parent);
        db.add_or_update_file_in(workspace, &canonical, &content);
        Ok((db, workspace, needs_format_hint))
    }

    /// Resolve into `(mode, targets)`. Positional `<TARGET>` →
    /// [`PackMode::Single`] with one entry; one-or-more `-f` →
    /// [`PackMode::Subcommand`] (a single `-f` still gets the subcommand
    /// layer per the spec).
    fn resolve_targets(
        &self,
        engine: &BexEngine,
    ) -> Result<(baml_exec::PackMode, Vec<ResolvedPackTarget>)> {
        if let Some(target) = self.target.as_deref() {
            let resolved = resolve_one(engine, target)?;
            return Ok((baml_exec::PackMode::Single, vec![resolved]));
        }

        // Subcommand mode. Resolve each `-f` and reject duplicate subcommand names.
        let mut resolved: Vec<ResolvedPackTarget> = Vec::with_capacity(self.functions.len());
        for func in &self.functions {
            resolved.push(resolve_one(engine, func)?);
        }
        let mut seen: HashMap<&str, &str> = HashMap::new();
        for r in &resolved {
            if let Some(prev) = seen.insert(r.subcommand_name.as_str(), r.display_name.as_str()) {
                anyhow::bail!(
                    "two targets share subcommand name `{}` (`{}` and `{}`). \
                     Subcommand names come from the last `.`-segment of the function name; \
                     rename one of them so the binary's subcommands stay unambiguous.",
                    r.subcommand_name,
                    prev,
                    r.display_name,
                );
            }
        }
        Ok((baml_exec::PackMode::Subcommand, resolved))
    }

    /// Pick the default output basename.
    ///
    /// - `--file <PATH>` single-file mode: file stem (e.g. `foo.baml` →
    ///   `foo`). `baml.toml` isn't consulted — single-file packs are
    ///   intentionally hermetic.
    /// - Project mode: `[package].name` from `<from>/baml.toml` when a
    ///   manifest is present, else the project directory name for a
    ///   manifest-less `baml_src/` project (see
    ///   [`crate::project_load::resolve_project_name`]).
    fn resolve_output_basename(&self) -> Result<String> {
        if let Some(file) = self.file.as_deref() {
            // Keep `--file` hermetic: derive the name from the file path
            // alone, never from `--from`/project markers. A path with no
            // usable file-name component (no stem, e.g. `..`, or a non-UTF-8
            // name) can't be named automatically — bail rather than leak the
            // cwd's project context into a single-file pack.
            return file
                .file_stem()
                .and_then(|s| s.to_str())
                .map(str::to_string)
                .ok_or_else(|| {
                    anyhow::anyhow!(
                        "cannot derive an output name from `{}`; pass `-o <PATH>` to name the output.",
                        file.display()
                    )
                });
        }
        crate::project_load::resolve_project_name(self.from.as_deref())
    }
}

/// Resolve a single function-name string against the engine; returns
/// canonical qualified/display/subcommand-name triple.
fn resolve_one(engine: &BexEngine, func: &str) -> Result<ResolvedPackTarget> {
    if !engine.function_exists(func) {
        let suggestions = function_suggestions(engine, func);
        if suggestions.is_empty() {
            anyhow::bail!(
                "function `{func}` not found. Use `baml run --list` to see \
                 available targets."
            );
        }
        anyhow::bail!(
            "function `{func}` not found. Did you mean one of:\n{}",
            suggestions
                .iter()
                .map(|s| format!("  - {s}"))
                .collect::<Vec<_>>()
                .join("\n")
        );
    }
    let qualified_name = canonicalize_function_name(engine, func);
    let display_name = qualified_name
        .strip_prefix("user.")
        .unwrap_or(&qualified_name)
        .to_string();
    let subcommand_name = display_name
        .rsplit('.')
        .next()
        .unwrap_or(&display_name)
        .to_string();
    Ok(ResolvedPackTarget {
        qualified_name,
        display_name,
        subcommand_name,
    })
}

/// Label for status output: `display_name` for single targets,
/// `a,b,c` for subcommand packs.
fn label_for(targets: &[ResolvedPackTarget]) -> String {
    match targets {
        [single] => single.display_name.clone(),
        _ => targets
            .iter()
            .map(|t| t.subcommand_name.as_str())
            .collect::<Vec<_>>()
            .join(","),
    }
}

/// Collect diagnostics; render errors to stderr and bail with `ctx`.
fn check_diagnostics(db: &ProjectDatabase, ctx: &str, reporter: &Reporter) -> Result<()> {
    let diagnostics = baml_db::collect_diagnostics(db);
    bail_on_error_diagnostics(db, &diagnostics, ctx, reporter)
}

/// Render any `Error`-severity entries in an already-collected diagnostics
/// list and bail with `ctx`; no-op when the list is error-free.
///
/// When `reporter` has an active spinner, abandon it before printing so
/// the multi-line diagnostic block lands cleanly instead of getting
/// interleaved with the tick character.
fn bail_on_error_diagnostics(
    db: &ProjectDatabase,
    diagnostics: &[baml_db::baml_compiler_diagnostics::Diagnostic],
    ctx: &str,
    reporter: &Reporter,
) -> Result<()> {
    let errors: Vec<_> = diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .collect();
    if errors.is_empty() {
        return Ok(());
    }
    let rendered = crate::check_command::render_project_diagnostics(
        db,
        &errors.iter().copied().cloned().collect::<Vec<_>>(),
    );
    reporter.abandon();
    eprintln!("{rendered}");
    anyhow::bail!("{ctx}");
}

/// Return the qualified name the engine prefers when both `foo` and
/// `user.foo` resolve to the same function.
/// Suggest user functions whose name is similar to `query`. Ranked by
/// substring containment first, then jaro-winkler similarity. Returns up
/// to 5 display names, sorted.
fn function_suggestions(engine: &BexEngine, query: &str) -> Vec<String> {
    let mut hits: Vec<String> = engine
        .user_functions()
        .into_iter()
        .map(|f| f.display_name)
        .filter(|name| {
            name.contains(query)
                || query.contains(name.as_str())
                || strsim::jaro_winkler(name, query) > 0.7
        })
        .collect();
    hits.sort();
    hits.dedup();
    hits.truncate(5);
    hits
}

fn canonicalize_function_name(engine: &BexEngine, name: &str) -> String {
    engine
        .find_user_function(name)
        .map(|info| info.qualified_name)
        .unwrap_or_else(|| name.to_string())
}

fn rust_manifest(workspace: &Path, profile: RustProfile) -> String {
    let table = |entries: &[(&str, toml::Value)]| -> toml::Value {
        toml::Value::Table(
            entries
                .iter()
                .map(|(key, value)| ((*key).to_owned(), value.clone()))
                .collect(),
        )
    };
    let dependency = |name: &str| {
        let path = workspace.join("crates").join(name);
        (
            name.to_owned(),
            table(&[("path", path.to_string_lossy().into_owned().into())]),
        )
    };
    let name = "baml_rust_program";
    let manifest = table(&[
        (
            "package",
            table(&[
                ("name", name.into()),
                ("version", "0.0.0".into()),
                ("edition", "2024".into()),
                ("publish", false.into()),
            ]),
        ),
        // Its own workspace, so it is no member of the one it builds from.
        ("workspace", table(&[])),
        ("lib", table(&[("path", "src/lib.rs".into())])),
        (
            "bin",
            toml::Value::Array(vec![table(&[
                ("name", name.into()),
                ("path", "src/main.rs".into()),
            ])]),
        ),
        (
            "dependencies",
            toml::Value::Table(
                ["bex_vm", "bex_vm_types", "baml_pack_host"]
                    .into_iter()
                    .map(dependency)
                    .collect(),
            ),
        ),
        (
            "profile",
            table(&[(
                "release",
                toml::Value::Table(profile.cargo_profile_settings()),
            )]),
        ),
    ]);
    toml::to_string(&manifest).expect("a TOML table serializes")
}

fn read_host_binary(target_triple: &str, reporter: &Reporter) -> Result<Vec<u8>> {
    let exe = std::env::current_exe().context("failed to locate current executable")?;
    let dir = exe
        .parent()
        .ok_or_else(|| anyhow!("cannot determine directory of current executable"))?;
    let host_name = host_binary_name(target_triple);
    let host_path = dir.join(&host_name);
    let is_native = target_triple == release_host_target_triple()?;
    if is_native && host_path.exists() {
        return std::fs::read(&host_path)
            .with_context(|| format!("failed to read {}", host_path.display()));
    }

    // A workspace-built host sits next to the CLI but we're skipping it
    // because the requested `--target` isn't this machine's platform, so we
    // download a release host for that target instead. Surface it: a dev who
    // expected their local host embedded would otherwise silently get
    // released bytes — exactly the kind of skip that hides host-side bugs
    // from local testing. (No local host => nothing skipped; stay quiet, the
    // "Downloading" line below already explains the fetch.)
    if !is_native && host_path.exists() {
        reporter.warning(format_args!(
            "ignoring local `{}` (built for this machine) — packing for `{target_triple}` \
             downloads a matching release host instead.",
            host_path.display()
        ));
    }

    // Cargo emits `Downloading <crate>` when it has to fetch a missing
    // dependency from crates.io; same shape here so users see *why*
    // pack just paused. Routed through `reporter.spin` so the line
    // persists to scrollback above the still-ticking spinner.
    reporter.spin("Downloading", &host_name);
    download_host_binary_from_release(target_triple, &host_name)
}

fn host_binary_name(target_triple: &str) -> String {
    if target_triple.ends_with("windows-msvc") {
        "baml-pack-host.exe".to_string()
    } else {
        "baml-pack-host".to_string()
    }
}

fn download_host_binary_from_release(target: &str, host_name: &str) -> Result<Vec<u8>> {
    let fetcher = baml_release::Fetcher::from_toolchain_manifest(baml_release::ReleaseSpec {
        version: release_version().to_string(),
        target: target.to_string(),
    })
    .map_err(|err| anyhow!("{err}"))?;
    fetcher
        .fetch_binary(host_name)
        .map_err(|err| anyhow!("{err}"))
}

fn release_host_target_triple() -> Result<&'static str> {
    baml_release::release_host_target_triple()
}

fn validate_release_target_triple(target: &str) -> Result<&str> {
    baml_release::validate_release_target_triple(target)
        .map_err(|err| anyhow!("unsupported pack target `{target}`. {err}"))
}

/// Heuristic: does this positional `<TARGET>` look like a filesystem
/// path rather than a function name? Triggers when the user typed
/// something like `baml_src/main.baml` and we want to redirect them to
/// `--file`. Function names can't contain `/` or `\`, and the `.baml`
/// suffix is the strong signal — namespaced functions like `llm.Foo`
/// use `.` but never end in `.baml`.
fn looks_like_path(target: &str) -> bool {
    target.contains('/') || target.contains('\\') || target.ends_with(".baml")
}

fn default_output_path(default_basename: &str, target_triple: &str) -> PathBuf {
    let mut path = PathBuf::from(default_basename);
    if target_triple.ends_with("windows-msvc")
        && path.extension().and_then(|ext| ext.to_str()) != Some("exe")
    {
        path.set_extension("exe");
    }
    path
}

fn write_executable(
    host_bytes: &[u8],
    data: &[u8],
    writer: &mut std::fs::File,
    target_triple: &str,
) -> Result<()> {
    if target_triple.contains("linux") {
        libsui::Elf::new(host_bytes)
            .append(PACK_SECTION_NAME, data, writer)
            .context("failed to write ELF binary")?;
    } else if target_triple.contains("windows") {
        libsui::PortableExecutable::from(host_bytes)
            .context("failed to parse PE binary")?
            .write_resource(PACK_SECTION_NAME, data.to_vec())
            .context("failed to write PE resource")?
            .build(writer)
            .context("failed to build PE binary")?;
    } else if target_triple.contains("apple-darwin") {
        libsui::Macho::from(host_bytes.to_vec())
            .context("failed to parse Mach-O binary")?
            .write_section(PACK_SECTION_NAME, data.to_vec())
            .context("failed to write Mach-O section")?
            .build_and_sign(writer)
            .context("failed to build Mach-O binary")?;
    } else {
        anyhow::bail!("unsupported pack target `{target_triple}`");
    }
    Ok(())
}
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    fn engine_from_source(source: &str) -> BexEngine {
        let snapshot = baml_tests::engine::compile_source(source);
        BexEngine::new(snapshot, Arc::new(sys_native::SysOps::native()), Vec::new())
            .expect("BexEngine::new should succeed")
    }

    /// Build an engine from a multi-file project so we can exercise
    /// namespaced functions (`ns_<name>/foo.baml` → `<name>.foo`). Single
    /// `engine_from_source` can't express folder-based namespaces.
    fn engine_from_files(files: &[(&str, &str)]) -> BexEngine {
        let snapshot = baml_tests::stdlib_prefix::compile_multi_file(files);
        BexEngine::new(snapshot, Arc::new(sys_native::SysOps::native()), Vec::new())
            .expect("BexEngine::new should succeed")
    }

    fn pack_args() -> PackArgs {
        PackArgs {
            telemetry: Default::default(),
            compiler: crate::commands::CompilerArgs::default(),
            target: None,
            functions: Vec::new(),
            file: None,
            output: None,
            rust: false,
            rust_workspace: None,
            rust_build_dir: None,
            rust_profile: RustProfile::Release,
            target_triple: None,
            output_format: OutputFormat::Json,
            from: None,
            expression: None,
        }
    }

    // ── Target resolution ─────────────────────────────────────────────

    /// Positional `<TARGET>` → single-target pack with the function
    /// name as both display and subcommand.
    #[test]
    fn test_pack_positional_single_target() {
        let engine = engine_from_source("function main() -> int { 42 }");
        let mut args = pack_args();
        args.target = Some("main".to_string());
        let (mode, targets) = args.resolve_targets(&engine).unwrap();
        assert!(matches!(mode, baml_exec::PackMode::Single));
        assert_eq!(targets.len(), 1);
        assert_eq!(targets[0].qualified_name, "user.main");
        assert_eq!(targets[0].display_name, "main");
        assert_eq!(targets[0].subcommand_name, "main");
    }

    /// `--file` + `--from` rejected (both name a source).
    #[test]
    fn test_pack_rejects_file_plus_explicit_from() {
        let mut args = pack_args();
        args.functions = vec!["main".into()];
        args.file = Some(PathBuf::from("a.baml"));
        args.from = Some(PathBuf::from("./project"));
        let err = args.validate_flags().unwrap_err();
        let msg = format!("{err}");
        assert!(msg.contains("mutually exclusive"), "got: {msg}");
        assert!(msg.contains("--file"), "got: {msg}");
    }

    /// `-e` is rejected before any other validation runs.
    #[test]
    fn test_pack_rejects_expression_mode() {
        let mut args = pack_args();
        args.expression = Some("2 + 2".to_string());
        let err = args.run().unwrap_err();
        let msg = format!("{err}");
        assert!(
            msg.contains("not packageable"),
            "error should mention non-packageable; got: {msg}"
        );
        assert!(
            msg.contains("-e") || msg.contains("--expression"),
            "error should name the rejected flag; got: {msg}"
        );
    }

    /// No positional and no `-f` → user gets a clean "no target" error.
    #[test]
    fn test_pack_no_target_errors() {
        let args = pack_args();
        let err = args.validate_flags().unwrap_err();
        let msg = format!("{err}");
        assert!(msg.contains("no target"), "got: {msg}");
        assert!(msg.contains("-f"), "got: {msg}");
    }

    /// Positional + `-f` is rejected.
    #[test]
    fn test_pack_positional_plus_function_errors() {
        let mut args = pack_args();
        args.target = Some("main".to_string());
        args.functions = vec!["Summarize".to_string()];
        let err = args.validate_flags().unwrap_err();
        assert!(format!("{err}").contains("mutually exclusive"));
    }

    /// Path-shaped positional → redirect to `--file`, not the generic
    /// "mutually exclusive" message. Covers the common confusion of
    /// `baml pack baml_src/main.baml -f main`.
    #[test]
    fn test_pack_path_positional_suggests_file_flag() {
        let mut args = pack_args();
        args.target = Some("baml_src/main.baml".to_string());
        args.functions = vec!["main".to_string()];
        let err = args.validate_flags().unwrap_err();
        let msg = format!("{err}");
        assert!(msg.contains("--file"), "expected --file hint; got: {msg}");
        assert!(
            msg.contains("baml_src/main.baml"),
            "expected the path echoed back; got: {msg}",
        );
    }

    /// Same redirect when the path-shaped positional appears alone (no `-f`).
    /// The hint should win over the generic "no target specified" path.
    #[test]
    fn test_pack_bare_path_positional_suggests_file_flag() {
        let mut args = pack_args();
        args.target = Some("./hello.baml".to_string());
        let err = args.validate_flags().unwrap_err();
        let msg = format!("{err}");
        assert!(msg.contains("--file"), "got: {msg}");
    }

    #[test]
    fn test_looks_like_path_signal() {
        assert!(looks_like_path("baml_src/main.baml"));
        assert!(looks_like_path("hello.baml"));
        assert!(looks_like_path("./x"));
        assert!(looks_like_path(r"C:\foo"));
        // Function names don't look like paths.
        assert!(!looks_like_path("main"));
        assert!(!looks_like_path("llm.Summarize"));
        assert!(!looks_like_path("user.llm.Summarize"));
    }

    /// Single `-f` → subcommand mode with one entry (subcommand layer
    /// retained, per the spec).
    #[test]
    fn test_pack_single_function_uses_subcommand_mode() {
        let engine = engine_from_source(
            r#"
                function main() -> int { 1 }
                function Summarize(text: string) -> string { text }
            "#,
        );
        let mut args = pack_args();
        args.functions = vec!["Summarize".to_string()];
        let (mode, targets) = args.resolve_targets(&engine).unwrap();
        assert!(matches!(mode, baml_exec::PackMode::Subcommand));
        assert_eq!(targets.len(), 1);
        assert_eq!(targets[0].subcommand_name, "Summarize");
    }

    /// Multiple `-f` → subcommand mode, one entry per `-f`.
    #[test]
    fn test_pack_multi_function_subcommand_mode() {
        let engine = engine_from_source(
            r#"
                function Summarize(text: string) -> string { text }
                function Categorize(text: string) -> string { text }
            "#,
        );
        let mut args = pack_args();
        args.functions = vec!["Summarize".to_string(), "Categorize".to_string()];
        let (mode, targets) = args.resolve_targets(&engine).unwrap();
        assert!(matches!(mode, baml_exec::PackMode::Subcommand));
        assert_eq!(targets.len(), 2);
        assert_eq!(targets[0].subcommand_name, "Summarize");
        assert_eq!(targets[1].subcommand_name, "Categorize");
    }

    /// Subcommand name canonicalizes to the last `.`-segment of the
    /// display name regardless of how the user spelled the `-f` flag.
    #[test]
    fn test_pack_function_flag_strips_user_prefix() {
        let engine = engine_from_source("function Summarize(text: string) -> string { text }");
        let mut args = pack_args();
        args.functions = vec!["user.Summarize".to_string()];
        let (_mode, targets) = args.resolve_targets(&engine).unwrap();
        assert_eq!(targets[0].display_name, "Summarize");
        assert_eq!(targets[0].subcommand_name, "Summarize");
    }

    /// Unknown function → error.
    #[test]
    fn test_pack_function_unknown_errors() {
        let engine = engine_from_source("function main() -> int { 1 }");
        let mut args = pack_args();
        args.functions = vec!["DoesNotExist".to_string()];
        let err = args.resolve_targets(&engine).unwrap_err();
        assert!(format!("{err}").contains("not found"));
    }

    // ── Namespaced targets (folder-based, `ns_<name>/`) ──────────────

    /// Positional `<TARGET>` resolves a namespaced function by display
    /// name. The packed binary's `argv[1]` (identifier / subcommand
    /// name) is the *last* `.`-segment — `llm.Summarize` → `Summarize`.
    #[test]
    fn test_pack_namespaced_positional() {
        let engine = engine_from_files(&[(
            "ns_llm/summarize.baml",
            "function Summarize(text: string) -> string { text }",
        )]);
        let mut args = pack_args();
        args.target = Some("llm.Summarize".to_string());
        let (mode, targets) = args.resolve_targets(&engine).unwrap();
        assert!(matches!(mode, baml_exec::PackMode::Single));
        assert_eq!(targets.len(), 1);
        assert_eq!(targets[0].qualified_name, "user.llm.Summarize");
        assert_eq!(targets[0].display_name, "llm.Summarize");
        assert_eq!(targets[0].subcommand_name, "Summarize");
    }

    /// `-f` with namespaced names produces subcommand-mode targets keyed
    /// on each function's last `.`-segment.
    #[test]
    fn test_pack_namespaced_multi_function() {
        let engine = engine_from_files(&[
            (
                "ns_llm/summarize.baml",
                "function Summarize(text: string) -> string { text }",
            ),
            (
                "ns_util/greet.baml",
                "function Greet(name: string) -> string { name }",
            ),
        ]);
        let mut args = pack_args();
        args.functions = vec!["llm.Summarize".into(), "util.Greet".into()];
        let (mode, targets) = args.resolve_targets(&engine).unwrap();
        assert!(matches!(mode, baml_exec::PackMode::Subcommand));
        assert_eq!(targets.len(), 2);
        assert_eq!(targets[0].display_name, "llm.Summarize");
        assert_eq!(targets[0].subcommand_name, "Summarize");
        assert_eq!(targets[1].display_name, "util.Greet");
        assert_eq!(targets[1].subcommand_name, "Greet");
    }

    /// Two namespaces exporting the same leaf name → subcommand-name
    /// collision is rejected with a clear message that names both
    /// fully-qualified targets so the user can rename one.
    #[test]
    fn test_pack_namespaced_subcommand_name_collision_errors() {
        let engine = engine_from_files(&[
            ("ns_llm/foo.baml", "function Foo() -> int { 1 }"),
            ("ns_util/foo.baml", "function Foo() -> int { 2 }"),
        ]);
        let mut args = pack_args();
        args.functions = vec!["llm.Foo".into(), "util.Foo".into()];
        let err = args.resolve_targets(&engine).unwrap_err();
        let msg = format!("{err}");
        assert!(msg.contains("share subcommand name `Foo`"), "got: {msg}");
        assert!(msg.contains("llm.Foo"), "got: {msg}");
        assert!(msg.contains("util.Foo"), "got: {msg}");
    }

    /// Bare-name lookup also resolves namespaced targets via the
    /// shared resolver's suffix scan — `Summarize` finds `llm.Summarize`
    /// when it's the unique match.
    #[test]
    fn test_pack_namespaced_resolves_via_bare_name() {
        let engine = engine_from_files(&[(
            "ns_llm/summarize.baml",
            "function Summarize(text: string) -> string { text }",
        )]);
        let mut args = pack_args();
        args.functions = vec!["Summarize".into()];
        let (_, targets) = args.resolve_targets(&engine).unwrap();
        assert_eq!(targets[0].qualified_name, "user.llm.Summarize");
        assert_eq!(targets[0].subcommand_name, "Summarize");
    }

    /// Two `-f`s with the same trailing segment → error.
    #[test]
    fn test_pack_duplicate_subcommand_names_error() {
        // Without namespaces in `compile_source` we can only exercise the
        // duplicate path by repeating the same function. Engine accepts
        // duplicates in the input list — `resolve_targets` is what rejects.
        let engine = engine_from_source("function Foo() -> int { 1 }");
        let mut args = pack_args();
        args.functions = vec!["Foo".to_string(), "Foo".to_string()];
        let err = args.resolve_targets(&engine).unwrap_err();
        let msg = format!("{err}");
        assert!(msg.contains("share subcommand name"), "got: {msg}");
    }

    // ── Reserved `help` parameter — BEP-027 §"Auto-CLI conventions" ───

    /// A target whose signature declares `help` is rejected at pack time.
    #[test]
    fn test_validate_help_param_rejects_reserved_name() {
        let engine = engine_from_source(r#"function Entry(help: string) -> string { help }"#);
        let err = validate_help_param(&engine, "user.Entry").unwrap_err();
        let msg = format!("{err}");
        assert!(msg.contains("`help`"), "got: {msg}");
        assert!(msg.to_lowercase().contains("rename"), "got: {msg}");
    }

    /// A target without a `help` parameter passes the check.
    #[test]
    fn test_validate_help_param_allows_unrelated_names() {
        let engine =
            engine_from_source(r#"function Entry(text: string, verbose: bool) -> string { text }"#);
        validate_help_param(&engine, "user.Entry").unwrap();
    }

    /// Parameterless `main()` has no params at all → passes trivially.
    #[test]
    fn test_validate_help_param_parameterless_ok() {
        let engine = engine_from_source("function main() -> int { 1 }");
        validate_help_param(&engine, "user.main").unwrap();
    }

    // ── Default flag values — BEP-027 §"What `baml pack` changes" ─────

    /// Per BEP: "Default output format is `json`. `baml run` defaults to
    /// `debug` because its primary reader is a human. Packaged binaries
    /// default to `json` because they are production tools."
    #[test]
    fn test_pack_default_output_format_is_json() {
        use clap::Parser;
        #[derive(Parser)]
        struct Wrapper {
            #[command(flatten)]
            args: PackArgs,
        }
        let parsed = Wrapper::try_parse_from(["baml-pack"]).unwrap();
        assert!(matches!(parsed.args.output_format, OutputFormat::Json));
    }

    #[test]
    fn test_pack_target_triple_flag_parses() {
        use clap::Parser;
        #[derive(Parser)]
        struct Wrapper {
            #[command(flatten)]
            args: PackArgs,
        }
        let parsed =
            Wrapper::try_parse_from(["baml-pack", "--target", "x86_64-pc-windows-msvc"]).unwrap();
        assert_eq!(
            parsed.args.target_triple.as_deref(),
            Some("x86_64-pc-windows-msvc")
        );
    }

    /// `-f` / `--function` are repeatable through the clap derive — same
    /// regression coverage as `RunArgs`.
    #[test]
    fn test_pack_dash_f_is_repeatable() {
        use clap::Parser;
        #[derive(Parser)]
        struct Wrapper {
            #[command(flatten)]
            args: PackArgs,
        }
        let parsed =
            Wrapper::try_parse_from(["baml-pack", "-f", "a", "-f", "b", "--function", "c"])
                .unwrap();
        assert_eq!(parsed.args.functions, vec!["a", "b", "c"]);
        assert!(parsed.args.target.is_none());
    }

    /// `--file` binds to the PathBuf field on `PackArgs`.
    #[test]
    fn test_pack_file_flag_parses() {
        use clap::Parser;
        #[derive(Parser)]
        struct Wrapper {
            #[command(flatten)]
            args: PackArgs,
        }
        let parsed =
            Wrapper::try_parse_from(["baml-pack", "-f", "foo", "--file", "x.baml"]).unwrap();
        assert_eq!(parsed.args.functions, vec!["foo"]);
        assert_eq!(parsed.args.file.as_deref(), Some(Path::new("x.baml")));
    }

    // ── Output basename resolution ────────────────────────────────────

    /// Project mode (`baml.toml` with `[package].name`) → use that name
    /// directly. No fallback to function name; manifest validation up
    /// front guarantees the field is there.
    #[test]
    fn test_pack_resolve_basename_uses_package_name() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(
            tmp.path().join("baml.toml"),
            "[package]\nname = \"my-app\"\n",
        )
        .unwrap();
        let mut args = pack_args();
        args.from = Some(tmp.path().to_path_buf());
        assert_eq!(args.resolve_output_basename().unwrap(), "my-app");
    }

    /// `--file foo.baml` → file stem.
    #[test]
    fn test_pack_file_mode_defaults_to_file_stem() {
        let mut args = pack_args();
        args.file = Some(PathBuf::from("scripts/foo.baml"));
        assert_eq!(args.resolve_output_basename().unwrap(), "foo");
    }

    /// `--file` ignores adjacent `baml.toml` — single-file packs are
    /// hermetic by intent.
    #[test]
    fn test_pack_file_mode_ignores_adjacent_baml_toml() {
        let tmp = tempfile::tempdir().unwrap();
        let baml_file = tmp.path().join("hello.baml");
        std::fs::write(&baml_file, "function describe() -> int { 1 }").unwrap();
        std::fs::write(
            tmp.path().join("baml.toml"),
            "[package]\nname = \"unrelated\"\n",
        )
        .unwrap();

        let mut args = pack_args();
        args.file = Some(baml_file);
        // `--from` is omitted, and file mode short-circuits project lookup.
        // Stem wins.
        assert_eq!(args.resolve_output_basename().unwrap(), "hello");
    }

    /// A `--file` path with no usable file-name component (here `..`, which
    /// has no `file_stem`) must error rather than fall back to the project
    /// name — `--file` stays hermetic. Even with a valid `baml.toml` in
    /// `--from`, the result is an error, not the package name.
    #[test]
    fn test_pack_file_mode_nameless_errors_instead_of_consulting_project() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(
            tmp.path().join("baml.toml"),
            "[package]\nname = \"should-not-be-used\"\n",
        )
        .unwrap();

        let mut args = pack_args();
        args.from = Some(tmp.path().to_path_buf());
        args.file = Some(PathBuf::from("..")); // no file_stem

        let err = args.resolve_output_basename().unwrap_err();
        let msg = format!("{err}");
        assert!(msg.contains("-o"), "expected a `-o` hint, got: {msg}");
        assert!(
            !msg.contains("should-not-be-used"),
            "must not consult the project manifest in --file mode, got: {msg}"
        );
    }

    #[test]
    fn test_validate_release_target_triple_accepts_supported_targets() {
        for target in baml_release::SUPPORTED_RELEASE_TARGETS {
            assert_eq!(validate_release_target_triple(target).unwrap(), *target);
        }
    }

    #[test]
    fn test_validate_release_target_triple_rejects_unknown_target() {
        let err = validate_release_target_triple("wasm32-unknown-unknown").unwrap_err();
        let msg = format!("{err}");
        assert!(msg.contains("unsupported pack target"), "got: {msg}");
        assert!(msg.contains("x86_64-unknown-linux-gnu"), "got: {msg}");
    }

    #[test]
    fn test_rust_manifest_release_profile() {
        let manifest = rust_manifest(Path::new("/w\"s"), RustProfile::Release);
        let parsed: toml::Value = toml::from_str(&manifest).expect("valid TOML");
        assert_eq!(
            parsed["package"]["name"].as_str(),
            Some("baml_rust_program")
        );
        assert_eq!(parsed["bin"][0]["path"].as_str(), Some("src/main.rs"));
        assert!(
            parsed["workspace"]
                .as_table()
                .is_some_and(toml::Table::is_empty)
        );
        let path = Path::new("/w\"s").join("crates").join("bex_vm");
        assert_eq!(
            parsed["dependencies"]["bex_vm"]["path"].as_str(),
            path.to_str(),
            "a path is a TOML string, escaped as TOML"
        );
        let release = &parsed["profile"]["release"];
        assert_eq!(release["lto"].as_str(), Some("fat"));
        assert_eq!(release["codegen-units"].as_integer(), Some(1));
        assert_eq!(release["opt-level"].as_integer(), Some(3));
        assert_eq!(release["strip"].as_str(), Some("symbols"));
    }

    #[test]
    fn test_rust_manifest_fast_profile() {
        let manifest = rust_manifest(Path::new("/ws"), RustProfile::Fast);
        let parsed: toml::Value = toml::from_str(&manifest).expect("valid TOML");
        let release = &parsed["profile"]["release"];
        assert_eq!(release["lto"].as_bool(), Some(false));
        assert_eq!(release["opt-level"].as_integer(), Some(1));
        assert_eq!(release["debug"].as_str(), Some("line-tables-only"));
        assert_eq!(release["panic"].as_str(), Some("unwind"));
    }

    #[test]
    fn test_rust_profile_flag_parses_and_requires_rust() {
        use clap::Parser;
        #[derive(Parser)]
        struct Wrapper {
            #[command(flatten)]
            args: PackArgs,
        }
        let parsed = Wrapper::try_parse_from(["baml-pack", "--rust"]).unwrap();
        assert_eq!(parsed.args.rust_profile, RustProfile::Release);
        let parsed =
            Wrapper::try_parse_from(["baml-pack", "--rust", "--rust-profile", "fast"]).unwrap();
        assert_eq!(parsed.args.rust_profile, RustProfile::Fast);
        assert!(Wrapper::try_parse_from(["baml-pack", "--rust-profile", "fast"]).is_err());
    }

    #[test]
    fn test_host_binary_name_follows_target_triple() {
        assert_eq!(
            host_binary_name("x86_64-unknown-linux-gnu"),
            "baml-pack-host"
        );
        assert_eq!(
            host_binary_name("x86_64-pc-windows-msvc"),
            "baml-pack-host.exe"
        );
    }

    #[test]
    fn test_default_output_path_adds_exe_for_windows_target() {
        assert_eq!(
            default_output_path("main", "x86_64-pc-windows-msvc"),
            PathBuf::from("main.exe")
        );
        assert_eq!(
            default_output_path("main.exe", "x86_64-pc-windows-msvc"),
            PathBuf::from("main.exe")
        );
        assert_eq!(
            default_output_path("main", "x86_64-unknown-linux-gnu"),
            PathBuf::from("main")
        );
    }

    // ── Envelope roundtrip ────────────────────────────────────────────

    /// The versioned `PackedProgram` artifact envelope is the wire contract
    /// between `baml pack` and the pack host. A regression here breaks every
    /// packaged binary, so it gets its own roundtrip test.
    #[test]
    fn test_pack_envelope_roundtrip() {
        let snapshot = baml_tests::engine::compile_source("function main() -> int { 1 }");
        let envelope = PackEnvelope {
            program: snapshot,
            mode: baml_exec::PackMode::Single,
            targets: vec![baml_exec::TargetEntry {
                qualified_name: "user.main".to_string(),
                display_name: "main".to_string(),
                subcommand_name: "main".to_string(),
            }],
            output_format: OutputFormat::Json,
            telemetry: Default::default(),
        };
        let bytes =
            baml_artifact::encode(baml_artifact::ArtifactKind::PackedProgram, &envelope).unwrap();
        let decoded: PackEnvelope =
            baml_artifact::decode(baml_artifact::ArtifactKind::PackedProgram, &bytes).unwrap();
        assert!(matches!(decoded.mode, baml_exec::PackMode::Single));
        assert_eq!(decoded.targets.len(), 1);
        assert_eq!(decoded.targets[0].qualified_name, "user.main");
        assert_eq!(decoded.targets[0].subcommand_name, "main");
        assert!(matches!(decoded.output_format, OutputFormat::Json));
    }

    // ── canonicalize_function_name ────────────────────────────────────

    /// Bare name resolves to whatever qualified form the engine stores.
    /// The engine uses the `user.` prefix for user functions, so lookup
    /// by either form should produce the same canonical qualified name.
    #[test]
    fn test_canonicalize_function_name_resolves_bare_and_qualified() {
        let engine = engine_from_source("function Greet(x: string) -> string { x }");
        let canonical_bare = canonicalize_function_name(&engine, "Greet");
        let canonical_qualified = canonicalize_function_name(&engine, "user.Greet");
        assert_eq!(
            canonical_bare, canonical_qualified,
            "both spellings must canonicalize to the same name",
        );
    }

    /// Unknown names pass through unchanged; callers surface the error
    /// elsewhere (`function_exists` check in `resolve_target`).
    #[test]
    fn test_canonicalize_function_name_unknown_passes_through() {
        let engine = engine_from_source("function main() -> int { 1 }");
        let name = canonicalize_function_name(&engine, "DoesNotExist");
        assert_eq!(name, "DoesNotExist");
    }

    // ── project / load_standalone error paths ─────────────────────────

    /// An empty directory has no `.baml` files → project loading errors
    /// before ever reaching compilation.
    #[test]
    fn test_load_project_empty_dir_errors() {
        let tmp = tempfile::tempdir().unwrap();
        // Write a valid manifest so the project root is recognized, but
        // no `.baml` files in `baml_src/`.
        std::fs::write(
            tmp.path().join("baml.toml"),
            "[package]\nname = \"empty\"\n",
        )
        .unwrap();
        std::fs::create_dir(tmp.path().join("baml_src")).unwrap();

        let mut args = pack_args();
        args.from = Some(tmp.path().to_path_buf());
        let reporter = Reporter::new();
        let err = args.load_and_compile_project(&reporter).unwrap_err();
        assert!(
            format!("{err}").contains("no `.baml` files"),
            "expected no-files error; got: {err}",
        );
    }

    /// A nonexistent `.baml` file surfaces the filesystem error rather
    /// than silently returning an empty project.
    #[test]
    fn test_load_standalone_missing_file_errors() {
        let args = pack_args();
        let err = args
            .load_standalone(Path::new("/nonexistent/ghost/path.baml"))
            .unwrap_err();
        let msg = format!("{err:?}"); // use debug to capture the full context chain
        assert!(
            msg.contains("file not found") || msg.contains("nonexistent"),
            "expected file-not-found error; got: {msg}",
        );
    }

    // ── Did-you-mean suggestions for unknown `--function` ────────────

    /// A typo close to an existing function name yields a suggestion.
    #[test]
    fn test_pack_resolve_function_flag_unknown_with_suggestion() {
        let engine = engine_from_source(
            r#"
                function Summarize(text: string) -> string { text }
                function Categorize(text: string) -> string { text }
            "#,
        );
        let mut args = pack_args();
        args.functions = vec!["Summarise".to_string()]; // British spelling
        let err = args.resolve_targets(&engine).unwrap_err();
        let msg = format!("{err}");
        assert!(
            msg.contains("Did you mean"),
            "expected suggestion; got: {msg}"
        );
        assert!(
            msg.contains("Summarize"),
            "expected Summarize in list; got: {msg}"
        );
    }

    /// A truly unrelated name falls back to the no-suggestions message
    /// (which still points at `baml run --list`).
    #[test]
    fn test_pack_resolve_function_flag_unknown_with_no_suggestions() {
        let engine = engine_from_source("function Greet() -> int { 1 }");
        let mut args = pack_args();
        args.functions = vec!["totally_unrelated_xyz".to_string()];
        let err = args.resolve_targets(&engine).unwrap_err();
        let msg = format!("{err}");
        assert!(msg.contains("not found"), "got: {msg}");
        assert!(
            msg.contains("--list"),
            "expected --list pointer; got: {msg}"
        );
    }

    /// `function_suggestions` returns substring-containing candidates
    /// first, then jaro-winkler matches, deduplicated.
    #[test]
    fn test_pack_function_suggestions_matches_substrings_and_similar() {
        let engine = engine_from_source(
            r#"
                function Summarize(text: string) -> string { text }
                function ParseSummary() -> string { "x" }
                function Categorize(text: string) -> string { text }
            "#,
        );
        let hits = function_suggestions(&engine, "Summary");
        assert!(
            hits.iter().any(|n| n == "Summarize"),
            "expected Summarize (similar); got: {hits:?}"
        );
        assert!(
            hits.iter().any(|n| n == "ParseSummary"),
            "expected ParseSummary (contains query); got: {hits:?}"
        );
    }
}
