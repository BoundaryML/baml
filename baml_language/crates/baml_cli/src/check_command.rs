use std::path::PathBuf;

use anyhow::{Context, Result};
use baml_db::{
    baml_compiler2_emit,
    baml_compiler_diagnostics::{Severity, render},
};
use clap::Args;

use crate::{project_load::load_project_for_build, reporter::Reporter};

#[derive(Args, Debug)]
pub struct CheckArgs {
    /// Project search starting point. Defaults to the current directory.
    #[arg(long, value_name = "PATH")]
    pub from: Option<PathBuf>,
}

impl CheckArgs {
    pub fn run(&self) -> Result<crate::ExitCode> {
        let reporter = Reporter::new();
        let (db, from, baml_files) =
            load_project_for_build(self.from.as_deref(), &reporter, false)?;
        if baml_files.is_empty() {
            reporter.abandon();
            crate::reporter::print_error(format_args!(
                "no .baml files found in {}",
                from.display()
            ));
            return Ok(crate::ExitCode::Other);
        }

        reporter.spin("Checking", format!("{} file(s)", baml_files.len()));
        let result = db.check();
        if !result.diagnostics.is_empty() {
            let rendered = render::render_diagnostics(
                &result.diagnostics,
                &result.sources,
                &result.file_paths,
                &render::RenderConfig::cli_auto(),
            );
            reporter.suspend(|| {
                #[allow(clippy::print_stderr)]
                {
                    eprintln!("{rendered}");
                }
            });
        }

        let error_count = result
            .diagnostics
            .iter()
            .filter(|diag| diag.severity == Severity::Error)
            .count();
        if error_count > 0 {
            reporter.abandon();
            return Ok(crate::ExitCode::Other);
        }

        // The bytecode build runs under the same "Checking" spinner — a
        // separate "Compiling N file(s)" line would just repeat the count.
        // `generate_bytecode_unchecked` rather than `get_bytecode`: the
        // error gate just above already ran the diagnostics pass, and
        // `get_bytecode` would redundantly run it a second time for its own
        // gate.
        if let Err(err) = db
            .generate_bytecode_unchecked(&baml_compiler2_emit::CompileOptions {
                emit_test_cases: false,
            })
            .with_context(|| format!("failed to compile BAML project at {}", from.display()))
        {
            reporter.abandon();
            return Err(err);
        }

        reporter.finish("Finished", format!("checked {} file(s)", baml_files.len()));
        Ok(crate::ExitCode::Success)
    }
}
