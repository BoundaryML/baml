//! `baml __emit-rust`: ahead-of-time Rust emission for the supported subset.
//!
//! Hidden developer command. It lowers the requested functions (or, with
//! `--all`, every admitted function of the workspace) and their transitive
//! callees through the MIR-to-Rust backend and writes a standalone Cargo
//! project that links `bex_aot`, the native runtime, which shares the
//! language's semantics with the VM through `bex_lang`. The generated binary
//! contains no bytecode interpreter.

// `println!` is this command's output: the admission report and the emitted
// function list. The workspace ban on `print*!` targets stray debug prints.
#![allow(clippy::print_stdout)]

use std::path::PathBuf;

use anyhow::{Context, Result, bail};
use baml_db::{
    baml_compiler_diagnostics::Severity,
    baml_compiler2_hir::item_data::{file_functions, function_data},
    baml_compiler2_mir::MirFunctionId,
    baml_compiler2_rust as native,
};
use clap::Args;

use crate::reporter::{Reporter, print_error};

/// Emit a standalone Rust project for BAML functions (developer preview).
#[derive(Args, Debug)]
#[command(after_long_help = "\
Examples:
  Emit `user.main` into ./native and build it:
    baml __emit-rust --function main --out ./native \\
        --runtime-path <baml checkout>/baml_language/crates/bex_aot
    cargo run --release --manifest-path ./native/Cargo.toml -- --n 27

  Emit `prepare` and `run` into one library (the shim is built around the
  first function named):
    baml __emit-rust --function prepare --function run --out ./native \\
        --runtime-path <baml checkout>/baml_language/crates/bex_aot

  Emit every function the backend admits, printing the admission report:
    baml __emit-rust --all --out ./native --runtime-path ...

  Show which functions of the project the native backend accepts:
    baml __emit-rust --report")]
pub struct EmitRustArgs {
    #[command(flatten)]
    pub compiler: crate::commands::CompilerArgs,

    /// Deprecated alias for `--project`.
    #[arg(long, value_name = "PATH", hide = true)]
    pub from: Option<PathBuf>,

    /// A function to compile: a bare name (`main`) or link name
    /// (`user.main`). Repeatable; the first one is the host shim's entry.
    #[arg(
        long,
        value_name = "NAME",
        action = clap::ArgAction::Append,
        required_unless_present_any = ["report", "all"]
    )]
    pub function: Vec<String>,

    /// Compile every function of the workspace the backend admits, printing
    /// the admission report; rejected functions are skipped.
    #[arg(long, conflicts_with = "function")]
    pub all: bool,

    /// Directory that receives the generated Cargo project.
    #[arg(long, value_name = "DIR", required_unless_present = "report")]
    pub out: Option<PathBuf>,

    /// Path to the `bex_aot` crate the generated project links against.
    #[arg(long, value_name = "DIR", required_unless_present = "report")]
    pub runtime_path: Option<PathBuf>,

    /// Crate name of the generated project.
    #[arg(long, value_name = "NAME", default_value = "baml_native", value_parser = crate_name)]
    pub crate_name: String,

    /// Print the per-function admission report for the whole workspace
    /// instead of emitting a project.
    #[arg(long)]
    pub report: bool,
}

/// A Cargo package name the generated manifest and binary can carry: an
/// ASCII letter, then letters, digits, `-` or `_`.
fn crate_name(name: &str) -> std::result::Result<String, String> {
    let mut chars = name.chars();
    let valid = chars.next().is_some_and(|c| c.is_ascii_alphabetic())
        && chars.all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_'));
    if valid {
        Ok(name.to_owned())
    } else {
        Err(format!(
            "`{name}` is not a crate name (a letter, then letters, digits, `-` or `_`)"
        ))
    }
}

impl EmitRustArgs {
    pub fn run(&self) -> Result<crate::ExitCode> {
        let reporter = Reporter::new();
        let session = crate::project_session::ProjectSession::open(
            self.from.as_deref(),
            crate::project_session::CacheUse::ReadOnly,
        )?;
        if session.is_empty() {
            reporter.abandon();
            print_error(format_args!(
                "no .baml files found in {}",
                session.root().display()
            ));
            return Ok(crate::ExitCode::Other);
        }
        let db = &session.db;

        reporter.spin("Checking", format!("{} file(s)", session.file_count()));
        let diagnostics = baml_db::collect_diagnostics(db);
        let errors = diagnostics
            .iter()
            .filter(|d| d.severity == Severity::Error)
            .count();
        if errors > 0 {
            reporter.abandon();
            print_error(format_args!(
                "{errors} compiler error(s); run `baml check` for details"
            ));
            return Ok(crate::ExitCode::Other);
        }

        let functions: Vec<_> = db
            .workspace_files()
            .into_iter()
            .flat_map(|file| file_functions(db, file).iter().copied())
            .collect();

        if self.report {
            reporter.finish("Checked", format!("{} function(s)", functions.len()));
            let admitted = print_report(db, &functions);
            println!("{admitted} of {} function(s) admitted", functions.len());
            return Ok(crate::ExitCode::Success);
        }

        let (Some(out), Some(runtime_path)) = (&self.out, &self.runtime_path) else {
            bail!("--out and --runtime-path are required unless --report is given");
        };

        let roots: Vec<_> = if self.all {
            reporter.finish("Checked", format!("{} function(s)", functions.len()));
            let mut roots: Vec<_> = functions
                .iter()
                .copied()
                .filter(|loc| native::admit_closure(db, *loc).is_ok())
                .collect();
            print_report(db, &functions);
            if roots.is_empty() {
                print_error(format_args!(
                    "the native backend admits no function of this workspace"
                ));
                return Ok(crate::ExitCode::Other);
            }
            // The shim is built around the first root: prefer one it can
            // call from the command line.
            if let Some(callable) = roots.iter().position(|loc| {
                native::admit(db, *loc).is_ok_and(|admitted| admitted.shim_callable())
            }) {
                roots.swap(0, callable);
            }
            roots
        } else {
            let mut roots = Vec::with_capacity(self.function.len());
            for wanted in &self.function {
                let mut matches = functions.iter().copied().filter(|loc| {
                    let link_name = MirFunctionId::Declared(*loc).link_name(db);
                    link_name == *wanted || function_data(db, *loc).name.as_str() == wanted.as_str()
                });
                let Some(entry) = matches.next() else {
                    reporter.abandon();
                    print_error(format_args!(
                        "function `{wanted}` not found in the workspace"
                    ));
                    return Ok(crate::ExitCode::InvalidArgs);
                };
                if matches.next().is_some() {
                    reporter.abandon();
                    print_error(format_args!(
                        "function name `{wanted}` is ambiguous; use its link name (for example `user.{wanted}`)"
                    ));
                    return Ok(crate::ExitCode::InvalidArgs);
                }
                if !roots.contains(&entry) {
                    roots.push(entry);
                }
            }
            roots
        };

        reporter.spin(
            "Emitting",
            roots
                .iter()
                .map(|loc| MirFunctionId::Declared(*loc).link_name(db))
                .collect::<Vec<_>>()
                .join(", "),
        );
        let module = match native::compile_many(db, &roots) {
            Ok(module) => module,
            Err(native::Rejection::Unsupported(reason)) => {
                reporter.abandon();
                print_error(format_args!(
                    "the native backend does not support this function yet: {reason}"
                ));
                return Ok(crate::ExitCode::Other);
            }
            Err(native::Rejection::Invalid(reason)) => {
                reporter.abandon();
                bail!("internal compiler error in the native backend: {reason}");
            }
        };

        let entry = &module.functions[module.entry];
        if entry.shim_callable()
            && let Err(error) = baml_exec::validate_help_param_names(
                &entry.link_name,
                entry.params.iter().map(|(name, _)| name.as_str()),
            )
        {
            reporter.abandon();
            print_error(format_args!("{error}"));
            return Ok(crate::ExitCode::InvalidArgs);
        }

        let runtime_path = runtime_path
            .canonicalize()
            .with_context(|| format!("resolving --runtime-path {}", runtime_path.display()))?;
        std::fs::create_dir_all(out).with_context(|| format!("creating {}", out.display()))?;
        let options = native::ProjectOptions {
            crate_name: &self.crate_name,
            runtime_path: &runtime_path,
            release_profile: true,
        };
        native::write_project(&module, out, &options)
            .with_context(|| format!("writing the project into {}", out.display()))?;

        reporter.finish(
            "Emitted",
            format!(
                "{} function(s) into {}",
                module.functions.len(),
                out.display()
            ),
        );
        for function in &module.functions {
            println!("native {}", function.link_name);
        }
        let entry = &module.functions[module.entry];
        if !entry.shim_callable() {
            println!(
                "shim: `{}` takes non-scalar arguments; the project is usable as a library only",
                entry.link_name
            );
        }
        println!(
            "build: cargo build --release --manifest-path {}",
            out.join("Cargo.toml").display()
        );
        Ok(crate::ExitCode::Success)
    }
}

/// Print one admission line per function and return how many are admitted.
/// A function is admitted only together with everything it calls, so the
/// count is what `--all` compiles; a rejection names the callee it comes
/// from. An admitted function whose parameters the host shim cannot parse
/// from the command line is marked `(library only)`.
fn print_report(
    db: &baml_db::ProjectDatabase,
    functions: &[baml_db::baml_compiler2_hir::loc::FunctionLoc<'_>],
) -> usize {
    let mut accepted = 0usize;
    for loc in functions {
        let link_name = MirFunctionId::Declared(*loc).link_name(db);
        match native::admit_closure(db, *loc) {
            Ok(admitted) => {
                accepted += 1;
                if admitted.shim_callable() {
                    println!("native      {link_name}");
                } else {
                    println!("native (library only) {link_name}");
                }
            }
            Err(native::Rejection::Unsupported(reason)) => {
                println!("unsupported {reason}");
            }
            Err(native::Rejection::Invalid(reason)) => {
                println!("INVALID     {reason}");
            }
        }
    }
    accepted
}
