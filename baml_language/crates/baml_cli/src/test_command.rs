#![allow(clippy::print_stdout, clippy::print_stderr)]

use std::{path::PathBuf, sync::Arc};

use anyhow::{Context, Result, anyhow};
use baml_db::{baml_compiler_diagnostics::Severity, baml_compiler2_emit};
use baml_project::ProjectDatabase;
use baml_type::RuntimeTy;
use bex_engine::{
    BexCallArg, BexEngine, BexExternalValue, CancellationToken, FunctionCallContextBuilder,
    test_arg_to_external,
};
use clap::Args;
use sys_native::{CallId, SysOpsExt};

use crate::{
    bytecode_cache::CacheContext,
    project_load::{build_db_from_sources, resolve_project_sources},
    reporter::Reporter,
    test_filter::TestFilter,
};

#[derive(Args, Clone, Debug)]
pub struct TestArgs {
    #[arg(long, help = "Project search starting point", value_name = "PATH")]
    pub from: Option<PathBuf>,

    /// List the selected tests instead of running them. The
    /// "Testset::TestName" shown on each line is a valid -i / -x selector.
    #[arg(long, default_value_t = false)]
    list: bool,

    #[arg(long, short = 'i')]
    /// Tests (or whole testsets) to include. If none provided, runs all tests.
    ///
    /// A selector is "Testset::TestName": the part before `::` is the enclosing
    /// testset, the part after is the test. Either side may be empty or use `*`
    /// wildcards. (For a legacy function-attached `test`, the part before `::`
    /// is the function name instead.)
    ///
    /// Examples:
    ///
    /// -i "MySet::my test"  one test inside testset "MySet"
    ///
    /// -i "MySet::"  every test in testset "MySet"
    ///
    /// -i "::my test"  a test in any testset — also the form for a top-level
    /// `test` with no enclosing testset (its testset name is empty)
    ///
    /// -i "Get*::*Bar"  wildcards on either side
    pub include: Vec<String>,

    #[arg(long, short = 'x')]
    /// Tests (or whole testsets) to exclude. Takes precedence over --include.
    ///
    /// Uses the same "Testset::TestName" syntax as --include.
    pub exclude: Vec<String>,
}

/// A legacy `test "name" { functions [Foo] args {…} }` attached to an LLM
/// function, discovered from HIR. Executed by calling the function directly
/// with the test args. New-style `testset`/`test` blocks are discovered and run
/// entirely inside the `testing` stdlib package (see `run_filtered`), so they
/// have no Rust-side representation.
struct LegacyTest {
    function_name: String,
    test_name: String,
    file_path: PathBuf,
}

/// Bundle of the engine + tokio runtime + cancellation token shared by every
/// test invocation. Kept as a struct rather than separate arguments so the
/// helper functions don't trip `clippy::too_many_arguments`.
struct RunCtx<'a> {
    engine: &'a Arc<BexEngine>,
    rt: &'a tokio::runtime::Runtime,
    cancel: &'a CancellationToken,
}

impl TestArgs {
    pub fn run(&self) -> Result<crate::ExitCode> {
        let reporter = Reporter::new();
        // ── 1. Load project ────────────────────────────────────────────────
        let resolved = resolve_project_sources(self.from.as_deref())?;
        if resolved.files.is_empty() {
            reporter.abandon();
            crate::reporter::print_error(format_args!(
                "no .baml files found in {}",
                resolved.root.display()
            ));
            return Ok(crate::ExitCode::NoTestsRun);
        }

        let cache = CacheContext::open(&resolved, /* emit_test_cases */ true);

        // Warm `--list` fast path. The flattened test list (legacy tests +
        // fully-expanded testset leaf names) is a pure function of the compiled
        // Program, cached under its key. On a hit we render + select directly
        // and skip engine boot, `$init`/`$init_test`, and in-VM testset
        // expansion entirely — the whole `--list` discovery floor. Gated off
        // under BAML_CACHE_VERIFY (the oracle must run honest discovery) and
        // BAML_NO_DISCOVERY_CACHE; any miss/corruption falls through to the
        // honest path below.
        if self.list {
            if let Some(exit) = self.try_cached_list(&reporter, cache.as_ref()) {
                return Ok(exit);
            }
        }

        let cached_program = if CacheContext::verify_enabled() {
            None
        } else {
            cache.as_ref().and_then(CacheContext::load)
        };

        let cached_engine = cached_program.and_then(|program| {
            // Bytecode-cache hit: the Program carries everything the test run
            // needs — compiled test cases for the legacy runner, testset code
            // for the in-VM registry — so the database (typecheck, HIR
            // discovery, emit) is skipped entirely.
            let legacy = legacy_tests_from_program(&program);
            match BexEngine::new(program, Arc::new(sys_native::SysOps::native()), Vec::new()) {
                Ok(engine) => Some((Arc::new(engine), legacy)),
                Err(error) => {
                    crate::bytecode_cache::cache_debug(format_args!(
                        "cached program rejected by VM; recompiling: {error:?}"
                    ));
                    None
                }
            }
        });

        let (engine, legacy) = if let Some(hit) = cached_engine {
            hit
        } else {
            let mut db = build_db_from_sources(&resolved, |_| {});
            let project = db
                .get_project()
                .ok_or_else(|| anyhow!("No project context"))?;

            // Seed the stdlib typed interface (B-694) before the first typecheck
            // query, so a fresh process skips re-deriving stdlib types. Gated off
            // under verify so the oracle exercises the honest path.
            let stdlib_interface_hit = !CacheContext::verify_enabled()
                && cache
                    .as_ref()
                    .and_then(|ctx| ctx.load_stdlib_interface())
                    .map(|by_package| db.set_seeded_stdlib_interface(by_package))
                    .is_some();

            // Per-file reuse: decide from the manifest which files' compiled
            // bytecode can be spliced from the previous program.
            let reuse_plan = cache.as_ref().and_then(|ctx| ctx.plan_reuse(&db));
            let reuse_plan = crate::bytecode_cache::prepare_reuse_plan(&mut db, reuse_plan);

            // ── 2. Diagnostics ─────────────────────────────────────────────
            // Keep `baml test` quiet during the compile phase. `baml check`
            // and `baml generate` own the compile/count progress lines. With a
            // cache, gate through the incremental collector (narrow to the reuse
            // plan's dirty files, serve clean files from their cached blobs, and
            // carry the fresh per-file blobs into the manifest); without one,
            // run the honest full check. The merged set is byte-identical.
            let source_files = db.get_source_files();
            let (diagnostics, fresh_diagnostics) = if let Some(ctx) = &cache {
                let incremental = ctx.collect_diagnostics_incremental(&db, reuse_plan.as_ref());
                (incremental.merged, Some(incremental.fresh_by_file))
            } else {
                (baml_project::collect_diagnostics(&db), None)
            };
            let errors: Vec<_> = diagnostics
                .iter()
                .filter(|d| d.severity == Severity::Error)
                .collect();
            if !errors.is_empty() {
                // Render the full ariadne block so test errors look like
                // run/pack errors instead of the previous "bullet list of
                // messages" shape. Sources/paths cover ALL files — an error
                // in one file may carry related spans elsewhere.
                let mut sources = std::collections::HashMap::new();
                let mut file_paths = std::collections::HashMap::new();
                for sf in &source_files {
                    let file_id = sf.file_id(&db);
                    sources.insert(file_id, sf.text(&db).to_string());
                    file_paths.insert(file_id, sf.path(&db));
                }
                let rendered = baml_db::baml_compiler_diagnostics::render::render_diagnostics(
                    &errors.iter().copied().cloned().collect::<Vec<_>>(),
                    &sources,
                    &file_paths,
                    &baml_db::baml_compiler_diagnostics::render::RenderConfig::cli_auto(),
                );
                reporter.abandon();
                eprintln!("{rendered}");
                return Ok(crate::ExitCode::Other);
            }

            // ── 3. Discover legacy tests from HIR ──────────────────────────
            let legacy = discover_legacy_tests(&db, project);

            // ── 4. Compile + engine + runtime ──────────────────────────────
            let compile_options = baml_compiler2_emit::CompileOptions {
                emit_test_cases: true,
            };
            let compiled = crate::bytecode_cache::compile_program_artifacts(
                &db,
                &compile_options,
                cache.as_ref(),
                reuse_plan.as_ref(),
            )
            .map_err(|e| anyhow!("Compilation failed: {e:?}"))?;
            if let Some(ctx) = &cache {
                ctx.verify_against(&compiled.program)?;
                ctx.verify_stdlib_interface(&db)?;
                ctx.verify_diagnostics(&db)?;
                ctx.verify_stdlib_diagnostics(&db)?;
                ctx.verify_callable_throws_fragments(&db)?;
                let fresh = fresh_diagnostics
                    .as_ref()
                    .expect("a cache is present, so fresh diagnostics were computed");
                if let Err(e) = ctx.store_artifacts_with_manifest(
                    &db,
                    &compiled.program,
                    compiled.units.as_deref(),
                    fresh,
                    reuse_plan.as_ref(),
                ) {
                    crate::bytecode_cache::cache_debug(format_args!("store failed: {e}"));
                }
                // Materialize the stdlib interface blob on a miss (idempotent on
                // a hit, so only write when the seed was absent).
                if !stdlib_interface_hit {
                    ctx.store_stdlib_interface(&db);
                }
                // Materialize the per-toolchain builtin-diagnostics blob on a
                // miss (self-gating on blob presence, so a warm hit is a no-op).
                ctx.store_stdlib_diagnostics(&db);
                // Sampled field verification (rustc-style 1-in-32): after the
                // compile result exists, ~1 warm run in 32 re-derives one served
                // clean file on a fresh, un-seeded database and hard-errors on
                // any drift. Bounded latency, loud on silent staleness.
                ctx.maybe_sampled_verify(reuse_plan.as_ref(), || {
                    build_db_from_sources(&resolved, |_| {})
                })?;
            }
            // Warm-run evidence: with the stdlib interface seeded this is 0 (the
            // seed served every stdlib package); a cold run reports up to 6.
            crate::bytecode_cache::cache_debug(format_args!(
                "stdlib interface: {} honest derivation(s) this process",
                baml_db::baml_compiler2_tir::package_interface::stdlib_honest_derivations()
            ));
            // Warm-incremental evidence: with the diagnostics cache serving clean
            // files this counts only the dirty files' scopes.
            crate::bytecode_cache::cache_debug(format_args!(
                "scope inferences: {} this process",
                baml_db::baml_compiler2_tir::inference::scope_inferences()
            ));

            let bytecode = compiled.program;
            let engine = Arc::new(
                BexEngine::new(bytecode, Arc::new(sys_native::SysOps::native()), Vec::new())
                    .map_err(|e| anyhow!("Failed to create engine: {e:?}"))?,
            );
            (engine, legacy)
        };
        let rt = tokio::runtime::Runtime::new().context("Failed to create tokio runtime")?;
        let cancel = CancellationToken::new();

        // ── 5. Resolve the testset registry handle ─────────────────────────
        // `collect_tests` runs `$init_test`, returning a live `testing.TestRegistry`
        // handle (or `Null` when the project declares no tests). All testset
        // discovery/filtering/execution then happens inside that package via
        // `run_filtered` / `list_filtered`.
        reporter.spin("Discovering", "tests");
        let registry =
            match rt.block_on(engine.collect_tests("user", CallId::next(), cancel.clone())) {
                Ok(BexExternalValue::Null) => None,
                Ok(handle @ BexExternalValue::Handle(_)) => Some(handle),
                Ok(other) => {
                    reporter.warning(format_args!(
                        "unexpected collect_tests result: {}",
                        other.type_name()
                    ));
                    None
                }
                Err(e) => {
                    // A failure resolving the registry (vs. a project with no
                    // tests, which returns Null) is a real error — don't silently
                    // continue as if there were no testset tests.
                    reporter.abandon();
                    crate::reporter::print_error(format_args!("testset discovery failed: {e}"));
                    return Ok(crate::ExitCode::Other);
                }
            };

        // ── 6. Filter legacy tests (testset filtering happens in BAML) ──────
        let filter = TestFilter::new(
            self.include.iter().map(|s| s.as_str()),
            self.exclude.iter().map(|s| s.as_str()),
        );
        let legacy_selected: Vec<&LegacyTest> = legacy
            .iter()
            .filter(|t| filter.includes(&t.function_name, &t.test_name))
            .collect();

        let run_ctx = RunCtx {
            engine: &engine,
            rt: &rt,
            cancel: &cancel,
        };

        // ── 7. List mode ───────────────────────────────────────────────────
        if self.list {
            let testset_names = match &registry {
                Some(reg) => {
                    match list_selected_testset_names(&run_ctx, reg, &self.include, &self.exclude) {
                        Ok(names) => names,
                        Err(e) => {
                            reporter.abandon();
                            crate::reporter::print_error(format_args!("failed to list tests: {e}"));
                            return Ok(crate::ExitCode::Other);
                        }
                    }
                }
                None => Vec::new(),
            };

            // Write-through the discovery cache (+ BAML_CACHE_VERIFY oracle) so a
            // later `--list` skips engine boot entirely. The cached datum is the
            // UNFILTERED flattened list, so any -i/-x is served from one entry;
            // with no filters the display list above already IS the unfiltered
            // list, so no extra VM call. Written only from an error-free
            // discovery (never-save-on-error).
            if let Some(ctx) = &cache {
                let all_leaf_names = if self.include.is_empty() && self.exclude.is_empty() {
                    testset_names.clone()
                } else {
                    match &registry {
                        Some(reg) => match list_selected_testset_names(&run_ctx, reg, &[], &[]) {
                            Ok(names) => names,
                            Err(e) => {
                                reporter.abandon();
                                crate::reporter::print_error(format_args!(
                                    "failed to list tests: {e}"
                                ));
                                return Ok(crate::ExitCode::Other);
                            }
                        },
                        None => Vec::new(),
                    }
                };
                let disco = crate::bytecode_cache::TestDiscovery {
                    legacy: legacy.iter().map(cached_legacy_test).collect(),
                    testset_leaf_names: all_leaf_names,
                };
                ctx.verify_test_discovery(&disco)?;
                ctx.store_test_discovery(&disco);
            }

            let legacy_lines: Vec<crate::bytecode_cache::CachedLegacyTest> = legacy_selected
                .iter()
                .copied()
                .map(cached_legacy_test)
                .collect();
            return Ok(render_test_list(&reporter, &legacy_lines, &testset_names));
        }

        // ── 8. Execute ─────────────────────────────────────────────────────
        // Test execution writes per-test PASS/FAIL lines to stdout; clear the
        // spinner so those lines don't fight with the ticks.
        reporter.abandon();

        let mut passed = 0usize;
        let mut failed = 0usize;
        let mut tolerated = 0usize;
        let mut total = 0usize;
        let mut command_failed = false;

        // Legacy tests run individually in Rust (they invoke an LLM function
        // directly with bound args).
        for t in &legacy_selected {
            let failed_before = failed;
            run_legacy_test(&run_ctx, t, &mut passed, &mut failed);
            total += 1;
            command_failed |= failed > failed_before;
        }

        // Testset tests run inside the stdlib: a single `run_filtered` call
        // expands, filters, runs (concurrently via spawn/await), and aggregates
        // (honoring testset runners), returning a tolerated-aware flat report.
        if let Some(reg) = &registry {
            match run_filtered_report(&run_ctx, reg, &self.include, &self.exclude) {
                Ok(value) => match parse_flat_report(&value) {
                    Some(flat) => consume_flat_report(
                        &flat,
                        &mut passed,
                        &mut failed,
                        &mut tolerated,
                        &mut total,
                        &mut command_failed,
                    ),
                    None => {
                        eprintln!("FAIL testing::* - could not read test report");
                        failed += 1;
                        total += 1;
                        command_failed = true;
                    }
                },
                Err(e) => {
                    eprintln!("FAIL testing::*");
                    eprintln!("  => {e}");
                    failed += 1;
                    total += 1;
                    command_failed = true;
                }
            }
        }

        if total == 0 {
            reporter.finish("Finished", "no tests selected");
            return Ok(crate::ExitCode::NoTestsRun);
        }

        let summary = if command_failed && tolerated > 0 {
            format!(
                "{passed} passed, {failed} failed, {tolerated} tolerated {}, {total} total",
                pluralize(tolerated, "failure", "failures")
            )
        } else if !command_failed && tolerated > 0 {
            format!(
                "aggregate passed — {passed} passed, {tolerated} tolerated {}, {total} total",
                pluralize(tolerated, "failure", "failures")
            )
        } else {
            format!("{passed} passed, {failed} failed, {total} total")
        };
        if command_failed {
            // `reporter.finish` styles success — print as an error so the
            // bold-red `Error:` carries the visual weight of "tests failed".
            crate::reporter::print_error(format_args!("test failures — {summary}"));
            Ok(crate::ExitCode::TestFailure)
        } else {
            reporter.finish("Finished", summary);
            Ok(crate::ExitCode::Success)
        }
    }

    /// Warm `--list` fast path: render the flattened test list straight from the
    /// discovery cache and skip engine boot entirely. The include/exclude filter
    /// is re-applied live in Rust via [`TestFilter`] — which mirrors the BAML
    /// `testing.leaf_selected` used on the honest path, so the selection (and
    /// hence stdout) is byte-identical to a cold run. Returns `None` when the
    /// cache is absent/disabled, under `BAML_CACHE_VERIFY`, or on a discovery
    /// miss/corruption — every case falls through to honest discovery.
    fn try_cached_list(
        &self,
        reporter: &Reporter,
        cache: Option<&CacheContext>,
    ) -> Option<crate::ExitCode> {
        if CacheContext::verify_enabled() {
            return None;
        }
        let disco = cache?.load_test_discovery()?;
        let filter = TestFilter::new(
            self.include.iter().map(|s| s.as_str()),
            self.exclude.iter().map(|s| s.as_str()),
        );
        let legacy_selected: Vec<crate::bytecode_cache::CachedLegacyTest> = disco
            .legacy
            .into_iter()
            .filter(|t| filter.includes(&t.function_name, &t.test_name))
            .collect();
        let testset_names: Vec<String> = disco
            .testset_leaf_names
            .into_iter()
            .filter(|name| {
                let (func, test) = split_top(name);
                filter.includes(&func, &test)
            })
            .collect();
        crate::bytecode_cache::cache_debug(format_args!(
            "served `test --list` from discovery cache ({} legacy + {} testset leaf(s) selected); \
             engine boot skipped",
            legacy_selected.len(),
            testset_names.len(),
        ));
        Some(render_test_list(reporter, &legacy_selected, &testset_names))
    }
}

/// Render the selected `--list` output to stdout and return the exit code.
/// Shared by the honest path and the warm discovery-cache path so both produce
/// byte-identical stdout. Status/`Selected` lines go to stderr (via the
/// reporter); only the list itself is stdout content.
fn render_test_list(
    reporter: &Reporter,
    legacy_selected: &[crate::bytecode_cache::CachedLegacyTest],
    testset_names: &[String],
) -> crate::ExitCode {
    if legacy_selected.is_empty() && testset_names.is_empty() {
        reporter.finish("Finished", "no tests selected");
        return crate::ExitCode::NoTestsRun;
    }
    reporter.status(
        "Selected",
        format!("{} test(s)", legacy_selected.len() + testset_names.len()),
    );
    // Indented list under the cargo-style status line. These are content (the
    // actual list), not status updates, so they go to stdout as plain prints.
    for t in legacy_selected {
        println!("  {}::{}  ({})", t.function_name, t.test_name, t.file_path);
    }
    for name in testset_names {
        let (func, test) = split_top(name);
        println!("  {func}::{test}  (<testset>)");
    }
    crate::ExitCode::Success
}

/// Project a Rust-side [`LegacyTest`] into the cache/render payload shape (its
/// root-relative `file_path` rendered to the display string `--list` prints).
fn cached_legacy_test(t: &LegacyTest) -> crate::bytecode_cache::CachedLegacyTest {
    crate::bytecode_cache::CachedLegacyTest {
        function_name: t.function_name.clone(),
        test_name: t.test_name.clone(),
        file_path: t.file_path.display().to_string(),
    }
}

// ---------------------------------------------------------------------------
// Legacy test discovery + execution (unchanged behavior, narrowed types)
// ---------------------------------------------------------------------------

#[allow(unused_variables)]
fn discover_legacy_tests(
    db: &ProjectDatabase,
    project: baml_workspace::Project,
) -> Vec<LegacyTest> {
    use baml_db::baml_compiler2_hir;

    let mut tests = Vec::new();
    let root = project.root(db);

    for source_file in db.get_source_files() {
        let item_tree = baml_compiler2_hir::file_item_tree(db, source_file);
        // Root-relative for display, matching how emit records source paths —
        // keeps `--list` output identical between compiled and
        // bytecode-cache-served runs.
        let file_path = source_file.path(db);
        let file_path = file_path.strip_prefix(&root).unwrap_or(&file_path);

        for test in item_tree.tests.values() {
            for func_ref in &test.function_refs {
                tests.push(LegacyTest {
                    function_name: func_ref.to_string(),
                    test_name: test.name.to_string(),
                    file_path: file_path.to_path_buf(),
                });
            }
        }
    }

    tests
}

/// Derive the legacy-test list from a compiled [`bex_vm_types::Program`]
/// (bytecode-cache hit path).
///
/// `Program::test_cases` holds the same (test, target functions) pairs that
/// HIR discovery yields — `run_legacy_test` already resolves each test against
/// the engine's copy by name — so no database is needed for execution.
///
/// KNOWN `--list` DIVERGENCE (interim): [`discover_legacy_tests`] (fresh run)
/// derives each test's `file_path` from the file where the `test` block was
/// *defined*. [`bex_vm_types::TestCase::source_file`] records that
/// test-defining file (set in `baml_compiler2_emit` Pass 8 via
/// `relative_source_path`, the same project-root-relative form
/// `discover_legacy_tests` uses), so `--list` output is identical between a
/// fresh compile and a bytecode-cache hit.
fn legacy_tests_from_program(program: &bex_vm_types::Program) -> Vec<LegacyTest> {
    let mut tests = Vec::new();
    for tc in &program.test_cases {
        // The test-defining file, recorded at emit time (Pass 8) via
        // `relative_source_path` — the same project-root-relative form
        // `discover_legacy_tests` uses, so `--list` output is identical
        // between a fresh compile and a bytecode-cache hit. Empty only for a
        // blob predating the source_file field (impossible: the format version gates it).
        let file_path = if tc.source_file.is_empty() {
            PathBuf::from("<unknown>")
        } else {
            PathBuf::from(&tc.source_file)
        };
        for func in &tc.function_names {
            tests.push(LegacyTest {
                function_name: func.clone(),
                test_name: tc.name.clone(),
                file_path: file_path.clone(),
            });
        }
    }
    tests
}

/// Execute one legacy (`function + test block`) test case.
///
/// Results are emitted to stdout/stderr and reflected in the counters; this
/// never returns an error or panics under normal operation.
fn run_legacy_test(ctx: &RunCtx, t: &LegacyTest, passed: &mut usize, failed: &mut usize) {
    let test_case = match ctx.engine.test_case(&t.function_name, &t.test_name) {
        Some(tc) => tc,
        None => {
            eprintln!(
                "FAIL {}::{} - test case not found in compiled program",
                t.function_name, t.test_name
            );
            *failed += 1;
            return;
        }
    };

    let ordered_args = match build_ordered_args(ctx.engine, &t.function_name, test_case) {
        Ok(args) => args,
        Err(e) => {
            eprintln!("FAIL {}::{} - {e}", t.function_name, t.test_name);
            *failed += 1;
            return;
        }
    };

    match ctx.rt.block_on(
        ctx.engine.call_function_bound_args(
            &t.function_name,
            ordered_args,
            FunctionCallContextBuilder::new(CallId::next())
                .with_cancel_token(ctx.cancel.clone())
                .build(),
            true,
        ),
    ) {
        Ok(result) => {
            println!("PASS {}::{}", t.function_name, t.test_name);
            println!("  => {result:?}");
            *passed += 1;
        }
        Err(e) => {
            eprintln!("FAIL {}::{}", t.function_name, t.test_name);
            eprintln!("  => {e}");
            *failed += 1;
        }
    }
}

fn build_ordered_args(
    engine: &BexEngine,
    function_name: &str,
    test_case: &bex_vm_types::TestCase,
) -> Result<Vec<BexCallArg>> {
    let params = engine
        .function_params(function_name)
        .map_err(|e| anyhow!("failed to get params for {function_name}: {e:?}"))?;

    let ordered: Vec<BexCallArg> = params
        .into_iter()
        .map(|(name, _ty, has_default)| {
            if let Some(value) = test_case.args.get(name) {
                Ok(BexCallArg::Provided(Box::new(test_arg_to_external(value))))
            } else if has_default {
                Ok(BexCallArg::OmittedDefault)
            } else {
                Err(anyhow!(
                    "missing argument '{name}' for function {function_name}"
                ))
            }
        })
        .collect::<Result<_>>()?;

    Ok(ordered)
}

// ---------------------------------------------------------------------------
// Testset entry points: a single BAML call does discovery + filtering +
// concurrent execution + aggregation, returning a flat report.
//
// `testing.TestRegistry.run_filtered(registry, include[], exclude[])` and
// `testing.TestRegistry.list_filtered(...)` are defined in
// baml_std/testing/registry.baml. `self` is passed as the first positional
// argument (the live registry handle).
// ---------------------------------------------------------------------------

fn run_filtered_report(
    ctx: &RunCtx,
    registry: &BexExternalValue,
    include: &[String],
    exclude: &[String],
) -> Result<BexExternalValue> {
    let call_ctx = FunctionCallContextBuilder::new(CallId::next())
        .with_cancel_token(ctx.cancel.clone())
        .build();
    ctx.rt
        .block_on(ctx.engine.call_function(
            "testing.TestRegistry.run_filtered",
            vec![
                registry.clone(),
                string_array(include),
                string_array(exclude),
            ],
            call_ctx,
            true,
        ))
        .map_err(|e| anyhow!("run_filtered failed: {e}"))
}

fn list_selected_testset_names(
    ctx: &RunCtx,
    registry: &BexExternalValue,
    include: &[String],
    exclude: &[String],
) -> Result<Vec<String>> {
    let call_ctx = FunctionCallContextBuilder::new(CallId::next())
        .with_cancel_token(ctx.cancel.clone())
        .build();
    let value = ctx
        .rt
        .block_on(ctx.engine.call_function(
            "testing.TestRegistry.list_filtered",
            vec![
                registry.clone(),
                string_array(include),
                string_array(exclude),
            ],
            call_ctx,
            true,
        ))
        .map_err(|e| anyhow!("list_filtered failed: {e}"))?;
    Ok(string_array_values(&value))
}

/// Print a flat report and fold its counts into the running totals.
///
/// Filtered runs print one `PASS/FAIL func::test` line per selected leaf
/// (parent testset runners are bypassed). Unfiltered runs print a single
/// aggregate `PASS/FAIL testing::*` line plus failed child names and failure
/// messages, and the exit verdict follows the aggregate `outcome` (so a runner
/// like `PassRate` can pass the suite even with a failing leaf).
/// Print the aggregate testset result and fold its counts into the running
/// totals. Both filtered and unfiltered runs go through the same aggregate path
/// (filtered selections still run under their testset runners), so a failing
/// leaf whose runner still passes the suite is reported as a *tolerated* failure
/// — kept out of the hard `failed` count and not failing the command.
fn consume_flat_report(
    flat: &FlatReport,
    passed: &mut usize,
    failed: &mut usize,
    tolerated: &mut usize,
    total: &mut usize,
    command_failed: &mut bool,
) {
    *passed += flat.passed;
    *failed += flat.failed;
    *tolerated += flat.tolerated;
    *total += flat.total;

    // An empty selection (a filter that matched no testset tests) aggregates
    // to a vacuous `pass` over zero tests. Printing a green `PASS testing::*`
    // for that contradicts the `NoTestsRun` exit the caller then returns and
    // reads as success to anything parsing stdout, so skip the aggregate line
    // entirely and let the caller's "no tests selected" guard speak.
    //
    // Guard *only* the vacuous-pass case: a zero-test report with a non-pass
    // outcome must still fall through to the FAIL branch so it prints, sets
    // `command_failed`, and synthesizes a displayed failure — a real failure
    // is never silently dropped just because it carried no leaves.
    if flat.total == 0 && flat.outcome == "pass" {
        return;
    }

    if flat.outcome == "pass" {
        if flat.tolerated > 0 {
            println!(
                "PASS testing::* [outcome=pass; {} tolerated {}]",
                flat.tolerated,
                pluralize(flat.tolerated, "failure", "failures")
            );
            // The aggregate-pass path's failed_names hold the tolerated names
            // (a passing suite contributes none to the top failed_names, so this
            // is usually empty — printed for symmetry with the FAIL path).
            for name in &flat.failed_names {
                println!("  tolerated: {name}");
            }
        } else {
            println!("PASS testing::*");
        }
    } else {
        println!("FAIL testing::* [outcome={}]", flat.outcome);
        for name in &flat.failed_names {
            println!("  failed: {name}");
        }
        for message in &flat.messages {
            eprintln!("  => {message}");
        }
        // A testset runner can fail the aggregate without marking any child
        // failed. Count that verdict as one displayed failure so the summary
        // does not read "0 failed" when the aggregate itself failed.
        if flat.failed == 0 {
            *failed += 1;
            *total += 1;
        }
        *command_failed = true;
    }
}

fn pluralize(n: usize, singular: &str, plural: &str) -> String {
    if n == 1 {
        singular.to_string()
    } else {
        plural.to_string()
    }
}

// ---------------------------------------------------------------------------
// Flat report parsing
//
// `FlatTestReport` (baml_std/testing/types.baml) is intentionally shallow:
// primitive fields only, already aggregated (including the tolerated-failure
// split), so the driver reads it directly instead of walking a nested,
// union-wrapped `TestReport | TestSetReport` tree.
// ---------------------------------------------------------------------------

struct FlatReport {
    outcome: String,
    passed: usize,
    failed: usize,
    tolerated: usize,
    total: usize,
    failed_names: Vec<String>,
    messages: Vec<String>,
}

fn parse_flat_report(value: &BexExternalValue) -> Option<FlatReport> {
    let BexExternalValue::Instance { fields, .. } = unwrap_union(value) else {
        return None;
    };
    let outcome = fields
        .get("outcome")
        .and_then(|v| as_string(unwrap_union(v)))?
        .to_string();
    // Counts are always present in a well-formed FlatTestReport; treat a missing
    // one as an FFI contract break (parse fails -> reported, not silently zeroed).
    let passed = fields.get("passed").and_then(as_usize)?;
    let failed = fields.get("failed").and_then(as_usize)?;
    let tolerated = fields.get("tolerated").and_then(as_usize)?;
    let total = fields.get("total").and_then(as_usize)?;
    let failed_names = fields
        .get("failed_names")
        .map(string_array_values)
        .unwrap_or_default();
    let messages = fields
        .get("messages")
        .map(string_array_values)
        .unwrap_or_default();
    Some(FlatReport {
        outcome,
        passed,
        failed,
        tolerated,
        total,
        failed_names,
        messages,
    })
}

// ---------------------------------------------------------------------------
// FFI value helpers
// ---------------------------------------------------------------------------

fn string_array(items: &[String]) -> BexExternalValue {
    BexExternalValue::Array {
        element_type: RuntimeTy::string(),
        items: items
            .iter()
            .map(|s| BexExternalValue::String(s.as_str().into()))
            .collect(),
    }
}

fn string_array_values(value: &BexExternalValue) -> Vec<String> {
    match unwrap_union(value) {
        BexExternalValue::Array { items, .. } => items
            .iter()
            .filter_map(|item| as_string(unwrap_union(item)).map(str::to_string))
            .collect(),
        _ => Vec::new(),
    }
}

/// Strip a leading `Union { value, … }` wrapper. Union-typed values (e.g. the
/// `Outcome = "pass" | "fail" | "error"` field) come back wrapped when
/// deep-copied across the FFI boundary.
fn unwrap_union(value: &BexExternalValue) -> &BexExternalValue {
    match value {
        BexExternalValue::Union { value, .. } => unwrap_union(value),
        other => other,
    }
}

fn as_string(value: &BexExternalValue) -> Option<&str> {
    match value {
        BexExternalValue::String(s) => Some(s.as_str()),
        BexExternalValue::Union { value, .. } => as_string(value),
        _ => None,
    }
}

fn as_usize(value: &BexExternalValue) -> Option<usize> {
    match unwrap_union(value) {
        BexExternalValue::Int(i) => usize::try_from(*i).ok(),
        _ => None,
    }
}

/// Split a testset's slash-separated path into (first-segment, rest). For
/// `"tictactoe/x wins row"` → ("tictactoe", "x wins row"). For a path with no
/// slash → ("", whole-thing).
fn split_top(full_path: &str) -> (String, String) {
    match full_path.split_once('/') {
        Some((head, tail)) => (head.to_string(), tail.to_string()),
        None => (String::new(), full_path.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use baml_type::RuntimeTy;

    use super::*;

    fn string(value: &str) -> BexExternalValue {
        BexExternalValue::String(value.into())
    }

    fn int(value: i64) -> BexExternalValue {
        BexExternalValue::Int(value)
    }

    fn array(items: Vec<BexExternalValue>) -> BexExternalValue {
        BexExternalValue::Array {
            element_type: RuntimeTy::unknown(),
            items,
        }
    }

    fn instance(class_name: &str, fields: Vec<(&str, BexExternalValue)>) -> BexExternalValue {
        BexExternalValue::Instance {
            class_name: class_name.to_string(),
            type_args: Vec::new(),
            fields: fields
                .into_iter()
                .map(|(name, value)| (name.to_string(), value))
                .collect(),
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn flat_report(
        outcome: &str,
        passed: i64,
        failed: i64,
        tolerated: i64,
        total: i64,
        failed_names: Vec<BexExternalValue>,
        messages: Vec<BexExternalValue>,
    ) -> BexExternalValue {
        instance(
            "testing.FlatTestReport",
            vec![
                ("outcome", string(outcome)),
                ("passed", int(passed)),
                ("failed", int(failed)),
                ("tolerated", int(tolerated)),
                ("total", int(total)),
                ("failed_names", array(failed_names)),
                ("messages", array(messages)),
            ],
        )
    }

    #[test]
    fn parses_flat_report_fields() {
        let value = flat_report(
            "fail",
            1,
            1,
            1,
            3,
            vec![string("hard/fails")],
            vec![string("assertion failed")],
        );

        let parsed = parse_flat_report(&value).expect("should parse");
        assert_eq!(parsed.outcome, "fail");
        assert_eq!(
            (parsed.passed, parsed.failed, parsed.tolerated, parsed.total),
            (1, 1, 1, 3)
        );
        assert_eq!(parsed.failed_names, vec!["hard/fails".to_string()]);
        assert_eq!(parsed.messages, vec!["assertion failed".to_string()]);
    }

    #[test]
    fn parse_flat_report_unwraps_union_wrapped_outcome() {
        // Union-typed fields (Outcome) arrive wrapped across the FFI boundary.
        let wrapped_outcome = BexExternalValue::union(
            string("pass"),
            [RuntimeTy::string(), RuntimeTy::int()],
            RuntimeTy::string(),
        );
        let value = instance(
            "testing.FlatTestReport",
            vec![
                ("outcome", wrapped_outcome),
                ("passed", int(1)),
                ("failed", int(0)),
                ("tolerated", int(0)),
                ("total", int(1)),
                ("failed_names", array(Vec::new())),
                ("messages", array(Vec::new())),
            ],
        );

        let parsed = parse_flat_report(&value).expect("should parse");
        assert_eq!(parsed.outcome, "pass");
        assert_eq!(parsed.total, 1);
    }

    fn consume(report: &FlatReport) -> (usize, usize, usize, usize, bool) {
        let (mut passed, mut failed, mut tolerated, mut total, mut command_failed) =
            (0, 0, 0, 0, false);
        consume_flat_report(
            report,
            &mut passed,
            &mut failed,
            &mut tolerated,
            &mut total,
            &mut command_failed,
        );
        (passed, failed, tolerated, total, command_failed)
    }

    #[test]
    fn consume_empty_report_folds_zero_counts_without_printing_pass() {
        // A no-match selector aggregates to a vacuous `pass` over zero tests.
        // Folding it must leave every counter at zero (so the caller's
        // `total == 0` guard fires "no tests selected") and must NOT print a
        // green `PASS testing::*` line contradicting the non-zero exit (B-628).
        let parsed =
            parse_flat_report(&flat_report("pass", 0, 0, 0, 0, Vec::new(), Vec::new())).unwrap();
        assert_eq!(consume(&parsed), (0, 0, 0, 0, false));
    }

    #[test]
    fn consume_empty_report_with_fail_outcome_still_propagates_failure() {
        // The vacuous-pass skip must NOT swallow a zero-test report that
        // reports a failure. A `total == 0 && outcome == "fail"` report has to
        // fall through to the FAIL branch: it sets `command_failed` and
        // synthesizes one displayed failure (so the summary isn't "0 failed"
        // while the aggregate failed). Regression guard for the narrowed guard.
        let parsed =
            parse_flat_report(&flat_report("fail", 0, 0, 0, 0, Vec::new(), Vec::new())).unwrap();
        assert_eq!(consume(&parsed), (0, 1, 0, 1, true));
    }

    #[test]
    fn consume_aggregate_pass_with_tolerated_failure() {
        // PassRate-style: aggregate passes; the failing leaf is tolerated, not hard.
        let parsed =
            parse_flat_report(&flat_report("pass", 2, 0, 1, 3, Vec::new(), Vec::new())).unwrap();
        assert_eq!(consume(&parsed), (2, 0, 1, 3, false));
    }

    #[test]
    fn consume_aggregate_fail_keeps_tolerated_out_of_failed() {
        // A hard sibling fails the command; tolerated stays separate from failed.
        let parsed = parse_flat_report(&flat_report(
            "fail",
            1,
            1,
            1,
            3,
            vec![string("hard/fails")],
            Vec::new(),
        ))
        .unwrap();
        assert_eq!(consume(&parsed), (1, 1, 1, 3, true));
    }

    #[test]
    fn consume_fail_with_zero_hard_failed_synthesizes_one() {
        // Runner fails the aggregate without marking any child failed.
        let parsed =
            parse_flat_report(&flat_report("fail", 1, 0, 0, 1, Vec::new(), Vec::new())).unwrap();
        assert_eq!(consume(&parsed), (1, 1, 0, 2, true));
    }
}
