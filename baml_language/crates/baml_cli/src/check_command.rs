use std::path::PathBuf;

use anyhow::Result;
use baml_db::baml_compiler_diagnostics::{Diagnostic, Severity, render};
use baml_project::ProjectDatabase;
use clap::Args;

use crate::{
    bytecode_cache::{CacheContext, prepare_reuse_plan},
    project_load::{build_db_from_sources, resolve_project_sources},
    reporter::Reporter,
};

#[derive(Args, Debug)]
pub struct CheckArgs {
    /// Project search starting point. Defaults to the current directory.
    #[arg(long, value_name = "PATH")]
    pub from: Option<PathBuf>,
}

impl CheckArgs {
    pub fn run(&self) -> Result<crate::ExitCode> {
        let reporter = Reporter::new();
        let resolved = resolve_project_sources(self.from.as_deref())?;
        if resolved.files.is_empty() {
            reporter.abandon();
            crate::reporter::print_error(format_args!(
                "no .baml files found in {}",
                resolved.root.display()
            ));
            return Ok(crate::ExitCode::Other);
        }
        let file_count = resolved.files.len();
        let mut db = build_db_from_sources(&resolved, |_| {});
        let cache = CacheContext::open(&resolved, false);

        // Check is a read-only cache consumer. It can use the immutable stdlib
        // interface and the last successful compile's reuse seeds, but cannot
        // advance the manifest without emitting matching Program/unit entries.
        if !CacheContext::verify_enabled()
            && let Some(interface) = cache.as_ref().and_then(CacheContext::load_stdlib_interface)
        {
            db.set_seeded_stdlib_interface(interface);
        }
        let reuse_plan = cache.as_ref().and_then(|cache| cache.plan_reuse(&db));
        let reuse_plan = prepare_reuse_plan(&mut db, reuse_plan);

        reporter.spin("Checking", format!("{file_count} file(s)"));
        let diagnostics = cache.as_ref().map_or_else(
            || baml_project::collect_diagnostics(&db),
            |cache| cache.collect_diagnostics_for_check(&db, reuse_plan.as_ref()),
        );
        if let Some(cache) = &cache {
            cache.verify_diagnostics(&db)?;
        }
        if !diagnostics.is_empty() {
            let rendered = render_project_diagnostics(&db, &diagnostics);
            reporter.suspend(|| {
                #[allow(clippy::print_stderr)]
                {
                    eprintln!("{rendered}");
                }
            });
        }

        let error_count = diagnostics
            .iter()
            .filter(|diag| diag.severity == Severity::Error)
            .count();
        if error_count > 0 {
            reporter.abandon();
            return Ok(crate::ExitCode::Other);
        }

        reporter.finish("Finished", format!("checked {file_count} file(s)"));
        Ok(crate::ExitCode::Success)
    }
}

pub(crate) fn render_project_diagnostics(
    db: &ProjectDatabase,
    diagnostics: &[Diagnostic],
) -> String {
    let mut sources = std::collections::HashMap::new();
    let mut file_paths = std::collections::HashMap::new();
    for source_file in db.get_source_files() {
        let file_id = source_file.file_id(db);
        sources.insert(file_id, source_file.text(db).to_string());
        file_paths.insert(file_id, source_file.path(db));
    }
    for source_file in baml_db::baml_compiler2_hir::compiler2_all_files(db) {
        let file_id = source_file.file_id(db);
        sources
            .entry(file_id)
            .or_insert_with(|| source_file.text(db).to_string());
        file_paths
            .entry(file_id)
            .or_insert_with(|| source_file.path(db));
    }
    render::render_diagnostics(
        diagnostics,
        &sources,
        &file_paths,
        &render::RenderConfig::cli_auto(),
    )
}
