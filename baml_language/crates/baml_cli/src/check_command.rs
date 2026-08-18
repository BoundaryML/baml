use std::path::PathBuf;

use anyhow::Result;
use baml_db::baml_compiler_diagnostics::{Diagnostic, Severity, render};
use baml_project::ProjectDatabase;
use clap::Args;

use crate::reporter::Reporter;

/// Check BAML source files for compiler errors.
///
/// Discovers the nearest BAML project from the search path, checks every
/// source file in that project, and prints compiler errors and warnings.
#[derive(Args, Debug)]
#[command(after_long_help = "\
Environment:
  BAML does not load .env files. Export variables in the invoking process or use an environment-loading tool.

Examples:
  Check the nearest project:
    baml check

  Check a specific project:
    baml check --project ./my-project")]
pub struct CheckArgs {
    #[command(flatten)]
    pub compiler: crate::commands::CompilerArgs,

    /// Deprecated alias for `--project`.
    #[arg(long, value_name = "PATH", hide = true)]
    pub from: Option<PathBuf>,
}

impl CheckArgs {
    pub fn run(&self) -> Result<crate::ExitCode> {
        let reporter = Reporter::new();
        let mut session = crate::project_session::ProjectSession::open(
            self.from.as_deref(),
            crate::project_session::CacheUse::ReadWrite,
        )?;
        if session.is_empty() {
            reporter.abandon();
            crate::reporter::print_error(format_args!(
                "no .baml files found in {}",
                session.root().display()
            ));
            return Ok(crate::ExitCode::Other);
        }
        let file_count = session.file_count();

        // The manifest can only advance alongside emitted Program/unit entries,
        // so a clean check *seeds* the cache below by running the same
        // emit-and-store path as run/test (gated to checks that actually have
        // something to advance). That turns a check-only workflow warm instead
        // of leaving it on the full-compile path forever.
        let warmth = session.warm_prep();
        let (reuse_plan, stdlib_interface_hit) = (warmth.reuse_plan, warmth.stdlib_interface_hit);
        let (db, cache) = (&session.db, &session.cache);

        reporter.spin("Checking", format!("{file_count} file(s)"));
        // With a cache, collect through the incremental collector so the fresh
        // per-file blobs are available for seeding below; its merged set is
        // identical to the read-only collector's.
        let (diagnostics, fresh_diagnostics) = match cache {
            Some(ctx) => {
                let incremental = ctx.collect_diagnostics_incremental(db, reuse_plan.as_ref());
                (incremental.merged, Some(incremental.fresh_by_file))
            }
            None => (baml_project::collect_diagnostics(db), None),
        };
        if let Some(cache) = cache {
            cache.verify_diagnostics(db)?;
            cache.verify_stdlib_diagnostics(db)?;
            // Sampled field verification (rustc-style 1-in-32): `baml check`
            // serves clean files' cached diagnostics and seeds their throws, so
            // it is a warm serving path like run/test. After the served result
            // exists, ~1 run in 32 re-derives one served clean file on a fresh,
            // un-seeded database and hard-errors on any drift.
            cache.maybe_sampled_verify(reuse_plan.as_ref(), || session.honest_db())?;
            // Materialize the per-toolchain builtin-diagnostics blob on a miss.
            // Unlike the manifest, this blob is a build constant (keyed only by
            // the compiler fingerprint), so a read-only `check` may safely write
            // it — letting a check-only workflow drop the builtin re-inference
            // tail on its second run. Self-gates on blob presence, and the honest
            // re-derivation is Salsa-memoized against the check just performed.
            cache.store_stdlib_diagnostics(db);
        }
        // Warm-incremental evidence (mirrors run/test): with the diagnostics
        // cache serving clean user files and the stdlib blob serving builtins,
        // this counts only the dirty files' scopes; a cold check walks every one.
        crate::bytecode_cache::cache_debug(format_args!(
            "body inferences: {} this process",
            baml_db::baml_compiler2_hir_ty::infer::body_inferences()
        ));
        if !diagnostics.is_empty() {
            let rendered = render_project_diagnostics(db, &diagnostics);
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

        // Seed the bytecode cache from this clean check — the same
        // emit-and-store path run/test use — but only when it advances the
        // manifest: no reuse plan (missing/stale manifest) or a plan with
        // dirty files. A fully-current manifest (zero dirty files) has
        // nothing to store, and skipping keeps the no-op check emit-free.
        // Seeding is an optimization: a failed emit on a diagnostics-clean
        // project is logged, never surfaced as a check failure.
        if let Some(ctx) = cache {
            let should_seed = reuse_plan
                .as_ref()
                .is_none_or(|plan| !plan.dirty_files.is_empty());
            if should_seed {
                match crate::bytecode_cache::compile_program_artifacts(
                    db,
                    &baml_db::baml_compiler2_emit::CompileOptions {
                        emit_test_cases: false,
                    },
                    cache.as_ref(),
                    reuse_plan.as_ref(),
                ) {
                    Ok(compiled) => {
                        let fresh = fresh_diagnostics
                            .as_ref()
                            .expect("a cache is present, so fresh diagnostics were computed");
                        ctx.verify_and_store(
                            db,
                            &compiled,
                            fresh,
                            reuse_plan.as_ref(),
                            stdlib_interface_hit,
                            || session.honest_db(),
                        )?;
                    }
                    Err(err) => {
                        crate::bytecode_cache::cache_debug(format_args!(
                            "check: cache seeding skipped (emit failed): {err:?}"
                        ));
                    }
                }
            }
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
    let mut source_files = std::collections::HashMap::new();
    for source_file in db.get_source_files() {
        let file_id = source_file.file_id(db);
        sources.insert(file_id, source_file.text(db).to_string());
        file_paths.insert(file_id, source_file.path(db));
        source_files.insert(file_id, source_file);
    }
    for source_file in baml_db::baml_compiler2_hir::compiler2_all_files(db) {
        let file_id = source_file.file_id(db);
        sources
            .entry(file_id)
            .or_insert_with(|| source_file.text(db).to_string());
        file_paths
            .entry(file_id)
            .or_insert_with(|| source_file.path(db));
        source_files.entry(file_id).or_insert(source_file);
    }
    let config = crate::output::policy().diagnostic_render_config();
    let mut highlights = baml_db::baml_compiler_diagnostics::SourceHighlights::new();
    if config.color && config.format == render::DiagnosticFormat::Human {
        let file_ids = diagnostics
            .iter()
            .flat_map(|diagnostic| {
                diagnostic
                    .annotations
                    .iter()
                    .map(|annotation| annotation.span.file_id)
                    .chain(
                        diagnostic
                            .related_info
                            .iter()
                            .map(|related| related.span.file_id),
                    )
            })
            .collect::<std::collections::HashSet<_>>();
        let highlighter = crate::paint::Highlighter::new(db);
        for file_id in file_ids {
            if let Some(source_file) = source_files.get(&file_id) {
                highlights.insert(file_id, highlighter.spans(*source_file));
            }
        }
    }
    let message_highlighter = crate::paint::MessageHighlighter::default();
    render::render_diagnostics_with_highlighters(
        diagnostics,
        &sources,
        &file_paths,
        &highlights,
        Some(&message_highlighter),
        &config,
    )
}
