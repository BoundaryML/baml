//! `baml __emit-rust`: ahead-of-time Rust emission for the supported subset.
//!
//! Hidden developer command. It lowers one entry function and its direct
//! callees through the MIR-to-Rust backend and writes a standalone Cargo
//! project that links `bex_lang`, the runtime crate shared with the VM. The
//! generated binary contains no bytecode interpreter.

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

/// Emit a standalone Rust project for one BAML function (developer preview).
#[derive(Args, Debug)]
#[command(after_long_help = "\
Examples:
  Emit `user.main` into ./native and build it:
    baml __emit-rust --function main --out ./native \\
        --runtime-path <baml checkout>/baml_language/crates/bex_lang
    cargo run --release --manifest-path ./native/Cargo.toml -- --n 27

  Show which functions of the project the native backend accepts:
    baml __emit-rust --report")]
pub struct EmitRustArgs {
    #[command(flatten)]
    pub compiler: crate::commands::CompilerArgs,

    /// Deprecated alias for `--project`.
    #[arg(long, value_name = "PATH", hide = true)]
    pub from: Option<PathBuf>,

    /// Entry function: a bare name (`main`) or link name (`user.main`).
    #[arg(long, value_name = "NAME", required_unless_present = "report")]
    pub function: Option<String>,

    /// Directory that receives the generated Cargo project.
    #[arg(long, value_name = "DIR", required_unless_present = "report")]
    pub out: Option<PathBuf>,

    /// Path to the `bex_lang` crate the generated project links against.
    #[arg(long, value_name = "DIR", required_unless_present = "report")]
    pub runtime_path: Option<PathBuf>,

    /// Crate name of the generated project.
    #[arg(long, value_name = "NAME", default_value = "baml_native")]
    pub crate_name: String,

    /// Print the per-function admission report for the whole workspace
    /// instead of emitting a project.
    #[arg(long)]
    pub report: bool,
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
            let mut accepted = 0usize;
            for loc in &functions {
                let link_name = MirFunctionId::Declared(*loc).link_name(db);
                match native::admit(db, *loc) {
                    Ok(()) => {
                        accepted += 1;
                        println!("native      {link_name}");
                    }
                    Err(native::Rejection::Unsupported(reason)) => {
                        println!("unsupported {link_name}: {reason}");
                    }
                    Err(native::Rejection::Invalid(reason)) => {
                        println!("INVALID     {link_name}: {reason}");
                    }
                }
            }
            println!("{accepted} of {} function(s) admitted", functions.len());
            return Ok(crate::ExitCode::Success);
        }

        let (Some(wanted), Some(out), Some(runtime_path)) =
            (&self.function, &self.out, &self.runtime_path)
        else {
            bail!("--function, --out and --runtime-path are required unless --report is given");
        };

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

        reporter.spin("Emitting", MirFunctionId::Declared(entry).link_name(db));
        let module = match native::compile(db, entry) {
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
        println!(
            "build: cargo build --release --manifest-path {}",
            out.join("Cargo.toml").display()
        );
        Ok(crate::ExitCode::Success)
    }
}
