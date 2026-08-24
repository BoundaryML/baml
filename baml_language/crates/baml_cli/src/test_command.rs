#![allow(clippy::print_stdout, clippy::print_stderr)]

use std::{
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
};

use anyhow::{Context, Result, anyhow};
use baml_db::{baml_compiler_diagnostics::Severity, baml_compiler2_emit};
use baml_project::ProjectDatabase;
use baml_type::RuntimeTy;
use bex_engine::{
    BexCallArg, BexEngine, BexExternalValue, CancellationToken, FunctionCallContext,
    FunctionCallContextBuilder, logger::TraceLogger, test_arg_to_external,
};
use clap::{Args, FromArgMatches};
use sys_native::{CallId, SysOpsExt};

use crate::{
    bytecode_cache::CacheContext,
    log_output::{LogLevel as TestLogLevel, LogOutput},
    reporter::Reporter,
    test_filter::TestFilter,
};

/// Run BAML tests.
///
/// With no filters, runs every test selected by the active profile, or every
/// project test when no profile is configured. Use `--list` to discover the
/// canonical IDs accepted by `--include` and `--exclude`.
#[derive(Args, Clone, Debug)]
#[command(after_long_help = r#"SELECTORS:
  Test IDs are case-sensitive and canonical: `root[.namespace]::testset::test`.
  Plain selectors match anywhere in the full ID. A selector containing `*` is
  an anchored full-ID glob, and `*` also matches `::`. Repeated includes are OR.
  Excludes always win. With no includes, every non-excluded test is selected.

PROFILES:
  Profile names are case-sensitive. A profile is preset `baml test` argv, parsed
  without shell expansion:

    [test]
    default = "regular"

    [test.profiles.regular]
    args = ["-x", "::integration::", "--color", "never"]

  Profile includes establish the initial candidates; direct CLI includes narrow
  them. Excludes accumulate and always win. Direct scalar options override
  profile scalar options. Profile args cannot contain --profile, --no-profile,
  --project, --directory, --from, --features, or --help. With no default profile,
  all tests are selected.

Examples:
  List available tests:
    baml test --list

  Run tests in the payments namespace:
    baml test -i "root.payments::*"

  Run integration tests except slow tests:
    baml test -i "*::integration::*" -x "slow""#)]
pub struct TestArgs {
    #[command(flatten)]
    pub compiler: crate::commands::CompilerArgs,

    #[arg(long, value_name = "PATH", hide = true)]
    pub from: Option<PathBuf>,

    /// Apply a named test profile from `baml.toml`.
    ///
    /// Profile arguments establish the initial test set; command-line filters
    /// further narrow it.
    #[arg(
        long,
        value_name = "NAME",
        conflicts_with = "no_profile",
        help_heading = "Profile options"
    )]
    profile: Option<String>,

    /// Do not apply the default profile configured in `baml.toml`.
    #[arg(
        long,
        default_value_t = false,
        conflicts_with = "profile",
        help_heading = "Profile options"
    )]
    no_profile: bool,

    /// List selected test IDs instead of running them.
    ///
    /// Each canonical ID is valid as an `--include` or `--exclude` selector.
    #[arg(long, default_value_t = false, help_heading = "Selection options")]
    list: bool,

    #[arg(long, short = 'i', help_heading = "Selection options")]
    /// Include tests matching a canonical-ID selector. Repeatable.
    pub include: Vec<String>,

    #[arg(long, short = 'x', help_heading = "Selection options")]
    /// Exclude tests matching a selector. Exclusions take precedence.
    pub exclude: Vec<String>,

    /// Explicit global output options, injected by the top-level parser so
    /// direct scalar values can override the selected profile's values.
    #[arg(skip)]
    pub(crate) cli_output: TestOutputOverrides,

    #[arg(
        long = "log",
        env = "BAML_LOG",
        value_enum,
        default_value_t = TestLogLevel::Off,
        ignore_case = true,
        value_name = "LEVEL",
        help = "Set the BAML log level; overrides BAML_LOG [default: off] [possible values: off, error, warn, info, debug, trace]",
        hide_default_value = true,
        hide_env = true,
        hide_possible_values = true,
        help_heading = "Test output options"
    )]
    pub log: TestLogLevel,

    /// Explicit command-line log level, injected by the top-level parser so a
    /// direct scalar value can override the selected profile's value.
    #[arg(skip)]
    pub(crate) cli_log: Option<TestLogLevel>,
}

#[derive(Debug, Default)]
struct TestInvocation {
    list: bool,
    profile_include: Vec<String>,
    profile_exclude: Vec<String>,
    cli_include: Vec<String>,
    cli_exclude: Vec<String>,
    output: crate::output::OutputArgs,
    logs: TestLogLevel,
}

#[derive(Debug)]
struct ParsedProfileArgs {
    test: TestArgs,
    output: TestOutputOverrides,
    logs: Option<TestLogLevel>,
}

#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct TestOutputOverrides {
    pub(crate) preset: Option<crate::output::OutputPreset>,
    pub(crate) color: Option<crate::output::ColorChoice>,
    pub(crate) no_progress: Option<bool>,
    pub(crate) hyperlinks: Option<crate::output::HyperlinkChoice>,
    pub(crate) diagnostic_format: Option<crate::output::DiagnosticFormatChoice>,
}

impl TestOutputOverrides {
    pub(crate) fn from_cli_matches(
        matches: &clap::ArgMatches,
        output: crate::output::OutputArgs,
    ) -> Self {
        let explicitly_set = |id| {
            matches
                .value_source(id)
                .is_some_and(|source| source != clap::parser::ValueSource::DefaultValue)
        };
        Self {
            preset: explicitly_set("preset").then_some(output.preset),
            color: explicitly_set("color").then_some(output.color).flatten(),
            no_progress: explicitly_set("no_progress").then_some(output.no_progress),
            hyperlinks: explicitly_set("hyperlinks")
                .then_some(output.hyperlinks)
                .flatten(),
            diagnostic_format: explicitly_set("diagnostic_format")
                .then_some(output.diagnostic_format)
                .flatten(),
        }
    }

    fn from_profile_matches(matches: &clap::ArgMatches, output: crate::output::OutputArgs) -> Self {
        let supplied =
            |id| matches.value_source(id) == Some(clap::parser::ValueSource::CommandLine);
        Self {
            preset: supplied("preset").then_some(output.preset),
            color: supplied("color").then_some(output.color).flatten(),
            no_progress: supplied("no_progress").then_some(output.no_progress),
            hyperlinks: supplied("hyperlinks")
                .then_some(output.hyperlinks)
                .flatten(),
            diagnostic_format: supplied("diagnostic_format")
                .then_some(output.diagnostic_format)
                .flatten(),
        }
    }

    fn apply_to(self, output: &mut crate::output::OutputArgs) {
        if let Some(preset) = self.preset {
            output.preset = preset;
        }
        if let Some(color) = self.color {
            output.color = Some(color);
        }
        if let Some(no_progress) = self.no_progress {
            output.no_progress = no_progress;
        }
        if let Some(hyperlinks) = self.hyperlinks {
            output.hyperlinks = Some(hyperlinks);
        }
        if let Some(diagnostic_format) = self.diagnostic_format {
            output.diagnostic_format = Some(diagnostic_format);
        }
    }
}

impl TestInvocation {
    fn includes_id(&self, id: &str) -> bool {
        TestFilter::includes_patterns(id, &self.profile_include, &self.profile_exclude)
            && TestFilter::includes_patterns(id, &self.cli_include, &self.cli_exclude)
    }

    fn has_filters(&self) -> bool {
        !self.profile_include.is_empty()
            || !self.profile_exclude.is_empty()
            || !self.cli_include.is_empty()
            || !self.cli_exclude.is_empty()
    }
}

/// A legacy `test "name" { functions [Foo] args {…} }` attached to an LLM
/// function, discovered from HIR. Executed by calling the function directly
/// with the test args. New-style `testset`/`test` blocks are discovered and run
/// entirely inside the `testing` stdlib package (see `run_filtered`), so they
/// have no Rust-side representation.
struct LegacyTest {
    /// Engine lookup name (the compiler's `user`-package spelling).
    function_name: String,
    test_name: String,
    /// Public, stable selector/report id.
    canonical_id: String,
    file_path: PathBuf,
}

fn canonical_legacy_id(function_name: &str, test_name: &str) -> String {
    let function = function_name.strip_prefix("user.").unwrap_or(function_name);
    format!("root.{function}::{test_name}")
}

fn qualify_function_from_source(function_name: &str, source_file: &std::path::Path) -> String {
    if function_name.contains('.') {
        return function_name.to_string();
    }
    let namespace = source_file
        .parent()
        .into_iter()
        .flat_map(std::path::Path::components)
        .filter_map(|component| {
            let std::path::Component::Normal(component) = component else {
                return None;
            };
            component.to_str()?.strip_prefix("ns_").map(str::to_string)
        })
        .collect::<Vec<_>>()
        .join(".");
    if namespace.is_empty() {
        function_name.to_string()
    } else {
        format!("{namespace}.{function_name}")
    }
}

fn validate_selectors<'a>(selectors: impl Iterator<Item = &'a String>) -> Result<()> {
    for selector in selectors {
        let is_canonical_root =
            selector == "root" || selector.starts_with("root.") || selector.starts_with("root::");
        // Canonical ids may contain a literal `/` inside a user-provided test
        // name. Only diagnose the recognizable legacy shape: an unrooted
        // two-column selector whose test tail used `/` for further nesting.
        if selector.contains('/')
            && selector.contains("::")
            && !selector.contains('*')
            && !is_canonical_root
        {
            anyhow::bail!(
                "test selector `{selector}` uses the old `/` hierarchy separator; use `::` instead (for example `{}`)",
                selector.replace('/', "::")
            );
        }
    }
    Ok(())
}

/// Bundle of the engine + tokio runtime + cancellation token shared by every
/// test invocation. Kept as a struct rather than separate arguments so the
/// helper functions don't trip `clippy::too_many_arguments`.
struct RunCtx<'a> {
    engine: &'a Arc<BexEngine>,
    rt: &'a tokio::runtime::Runtime,
    cancel: &'a CancellationToken,
    unhandled_spawn_failures: &'a AtomicUsize,
    logs: TestLogLevel,
}

impl RunCtx<'_> {
    fn call_context(&self, call_id: CallId) -> (FunctionCallContext, Option<TraceLogger>) {
        let builder =
            FunctionCallContextBuilder::new(call_id).with_cancel_token(self.cancel.clone());
        LogOutput::new(self.logs, "test").call_context(builder)
    }

    fn block_on_with_logs<T>(
        &self,
        future: impl std::future::Future<Output = T>,
        producer: Option<&TraceLogger>,
    ) -> T {
        LogOutput::new(self.logs, "test").block_on(self.rt, future, producer)
    }
}

fn finish_engine(ctx: &RunCtx<'_>, reporter: &Reporter) -> usize {
    crate::shutdown::shutdown_engine(ctx.rt, ctx.engine, reporter);
    ctx.unhandled_spawn_failures.load(Ordering::SeqCst)
}

impl TestArgs {
    pub fn run(&self) -> Result<crate::ExitCode> {
        let reporter = Reporter::new();
        // ── 1. Load project ────────────────────────────────────────────────
        let mut session = crate::project_session::ProjectSession::open(
            self.from.as_deref(),
            crate::project_session::CacheUse::ReadWriteTests,
        )?;
        let invocation =
            self.resolve_invocation(session.resolved.manifest.as_deref(), session.root())?;
        crate::output::init(invocation.output);
        if session.is_empty() {
            reporter.abandon();
            crate::reporter::print_error(format_args!(
                "no .baml files found in {}",
                session.root().display()
            ));
            return Ok(crate::ExitCode::NoTestsRun);
        }

        // Warm `--list` fast path. The flattened test list (legacy tests +
        // fully-expanded testset leaf names) is a pure function of the compiled
        // Program, cached under its key. On a hit we render + select directly
        // and skip engine boot, `$init`/`$init_test`, and in-VM testset
        // expansion entirely — the whole `--list` discovery floor. Gated off
        // under BAML_CACHE_VERIFY (the oracle must run honest discovery) and
        // BAML_NO_DISCOVERY_CACHE; any miss/corruption falls through to the
        // honest path below.
        if invocation.list {
            if let Some(exit) = self.try_cached_list(&reporter, session.cache.as_ref(), &invocation)
            {
                return Ok(exit);
            }
        }

        let cached_program = session.try_cached_program();

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
            let warmth = session.warm_prep();
            let (reuse_plan, stdlib_interface_hit) =
                (warmth.reuse_plan, warmth.stdlib_interface_hit);
            let db = &session.db;
            let cache = &session.cache;
            let project = db
                .get_project()
                .ok_or_else(|| anyhow!("no project context"))?;

            // ── 2. Diagnostics ─────────────────────────────────────────────
            // Keep `baml test` quiet during the compile phase. `baml check`
            // and `baml generate` own the compile/count progress lines. With a
            // cache, gate through the incremental collector (narrow to the reuse
            // plan's dirty files, serve clean files from their cached blobs, and
            // carry the fresh per-file blobs into the manifest); without one,
            // run the honest full check. The merged set is byte-identical.
            let (diagnostics, fresh_diagnostics) = if let Some(ctx) = cache {
                let incremental = ctx.collect_diagnostics_incremental(db, reuse_plan.as_ref());
                (incremental.merged, Some(incremental.fresh_by_file))
            } else {
                (baml_project::collect_diagnostics(db), None)
            };
            let errors: Vec<_> = diagnostics
                .iter()
                .filter(|d| d.severity == Severity::Error)
                .cloned()
                .collect();
            if !errors.is_empty() {
                // Render the full diagnostic block so test errors look like
                // run/pack errors instead of a bullet list of messages. Sources
                // and paths cover every user file plus builtins — an error in one
                // file may carry related spans elsewhere.
                let rendered = crate::check_command::render_project_diagnostics(db, &errors);
                reporter.abandon();
                eprintln!("{rendered}");
                return Ok(crate::ExitCode::Other);
            }

            // ── 3. Discover legacy tests from HIR ──────────────────────────
            let legacy = discover_legacy_tests(db, project);

            // ── 4. Compile + engine + runtime ──────────────────────────────
            let compile_options = baml_compiler2_emit::CompileOptions {
                emit_test_cases: true,
            };
            let compiled = crate::bytecode_cache::compile_program_artifacts(
                db,
                &compile_options,
                cache.as_ref(),
                reuse_plan.as_ref(),
            )
            .map_err(|e| anyhow!("compilation failed: {e:?}"))?;
            if let Some(ctx) = cache {
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
            // Warm-run evidence: with the stdlib interface seeded this is 0 (the
            // seed served every stdlib package); a cold run reports up to 6.
            crate::bytecode_cache::cache_debug(format_args!(
                "stdlib interface: {} honest derivation(s) this process",
                baml_db::baml_compiler2_hir_ty::package_interface::stdlib_honest_derivations()
            ));
            // Warm-incremental evidence: with the diagnostics cache serving clean
            // files this counts only the dirty files' scopes.
            crate::bytecode_cache::cache_debug(format_args!(
                "body inferences: {} this process",
                baml_db::baml_compiler2_hir_ty::infer::body_inferences()
            ));

            let bytecode = compiled.program;
            let engine = Arc::new(
                BexEngine::new(bytecode, Arc::new(sys_native::SysOps::native()), Vec::new())
                    .map_err(|e| anyhow!("failed to create engine: {e:?}"))?,
            );
            (engine, legacy)
        };
        let unhandled_spawn_failures = Arc::new(AtomicUsize::new(0));
        let unhandled_spawn_failures_for_handler = Arc::clone(&unhandled_spawn_failures);
        engine.set_unhandled_spawn_error_handler(Some(Arc::new(move |report| {
            let cancelled = report.cancelled;
            let error = report.into_engine_error();
            if cancelled {
                eprintln!("WARN cancelled spawned task failed: {error}");
            } else {
                eprintln!("FAIL testing::unhandled_spawn_error");
                eprintln!("  => {error}");
                unhandled_spawn_failures_for_handler.fetch_add(1, Ordering::SeqCst);
            }
        })));
        let rt = tokio::runtime::Runtime::new().context("failed to create tokio runtime")?;
        let cancel = CancellationToken::new();
        let run_ctx = RunCtx {
            engine: &engine,
            rt: &rt,
            cancel: &cancel,
            unhandled_spawn_failures: &unhandled_spawn_failures,
            logs: invocation.logs,
        };

        // ── 5. Resolve the testset registry handle ─────────────────────────
        // `collect_tests` runs `$init_test`, returning a live `testing.TestRegistry`
        // handle (or `Null` when the project declares no tests). All testset
        // discovery/filtering/execution then happens inside that package via
        // `run_filtered` / `list_filtered`.
        reporter.spin("Discovering", "tests");
        let discovery_started = std::time::Instant::now();
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
                    return Ok(if finish_engine(&run_ctx, &reporter) != 0 {
                        crate::ExitCode::TestFailure
                    } else {
                        crate::ExitCode::Other
                    });
                }
            };

        crate::reporter::print_verbose(format_args!(
            "discovered tests in {:.2?} ({} legacy test(s); testset registry: {})",
            discovery_started.elapsed(),
            legacy.len(),
            if registry.is_some() {
                "present"
            } else {
                "none"
            },
        ));

        // ── 6. Filter legacy tests (testset filtering happens in BAML) ──────
        let legacy_selected: Vec<&LegacyTest> = legacy
            .iter()
            .filter(|t| invocation.includes_id(&t.canonical_id))
            .collect();

        // ── 7. List mode ───────────────────────────────────────────────────
        if invocation.list {
            let testset_names = match &registry {
                Some(reg) => match list_selected_testset_names(&run_ctx, reg, &invocation) {
                    Ok(names) => names,
                    Err(e) => {
                        reporter.abandon();
                        crate::reporter::print_error(format_args!("failed to list tests: {e}"));
                        return Ok(if finish_engine(&run_ctx, &reporter) != 0 {
                            crate::ExitCode::TestFailure
                        } else {
                            crate::ExitCode::Other
                        });
                    }
                },
                None => Vec::new(),
            };

            if finish_engine(&run_ctx, &reporter) != 0 {
                return Ok(crate::ExitCode::TestFailure);
            }

            // Write-through the discovery cache (+ BAML_CACHE_VERIFY oracle) so a
            // later `--list` skips engine boot entirely. The cached datum is the
            // UNFILTERED flattened list, so any -i/-x is served from one entry;
            // with no filters the display list above already IS the unfiltered
            // list, so no extra VM call. Written only from an error-free
            // discovery (never-save-on-error).
            // A filtered/profiled discovery may have deliberately pruned lazy,
            // expensive testsets. Do not defeat that guarantee by expanding the
            // excluded tree merely to populate an unfiltered cache entry. A
            // cache produced by an earlier unfiltered run can still serve this
            // invocation through the fast path above.
            if let Some(ctx) = &session.cache
                && !invocation.has_filters()
                && !testset_names
                    .iter()
                    .any(|name| name.ends_with("::(failed to expand)"))
            {
                let disco = crate::bytecode_cache::TestDiscovery {
                    legacy: legacy.iter().map(cached_legacy_test).collect(),
                    testset_leaf_names: testset_names.clone(),
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
            match run_filtered_report(&run_ctx, reg, &invocation) {
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
                        eprintln!("AGGREGATE FAIL - could not read test report");
                        failed += 1;
                        total += 1;
                        command_failed = true;
                    }
                },
                Err(e) => {
                    eprintln!("AGGREGATE FAIL");
                    eprintln!("  => {e}");
                    failed += 1;
                    total += 1;
                    command_failed = true;
                }
            }
        }

        let unhandled_spawn_failure_count = finish_engine(&run_ctx, &reporter);
        if unhandled_spawn_failure_count != 0 {
            failed += unhandled_spawn_failure_count;
            total += unhandled_spawn_failure_count;
            command_failed = true;
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

    fn resolve_invocation(
        &self,
        manifest_text: Option<&str>,
        project_root: &std::path::Path,
    ) -> Result<TestInvocation> {
        let manifest = manifest_text
            .map(crate::manifest::parse)
            .transpose()
            .with_context(|| {
                format!(
                    "failed to read test profiles from {}",
                    project_root.join("baml.toml").display()
                )
            })?;

        let selected_name = if self.no_profile {
            None
        } else if let Some(name) = &self.profile {
            Some(name.as_str())
        } else {
            manifest.as_ref().and_then(|m| m.test.default.as_deref())
        };

        let profile_args = if let Some(name) = selected_name {
            let manifest = manifest.as_ref().ok_or_else(|| {
                anyhow!(
                    "test profile `{name}` was requested, but {} does not exist",
                    project_root.join("baml.toml").display()
                )
            })?;
            let profile = manifest.test.profiles.get(name).ok_or_else(|| {
                let available = if manifest.test.profiles.is_empty() {
                    "none configured".to_string()
                } else {
                    manifest
                        .test
                        .profiles
                        .keys()
                        .map(String::as_str)
                        .collect::<Vec<_>>()
                        .join(", ")
                };
                anyhow!(
                    "test profile `{name}` is not defined in {} (available: {available})",
                    project_root.join("baml.toml").display()
                )
            })?;
            Self::parse_profile_args(name, &profile.args).with_context(|| {
                format!(
                    "{}: invalid [test.profiles.{name}].args",
                    project_root.join("baml.toml").display()
                )
            })?
        } else {
            None
        };

        let mut output = crate::output::OutputArgs::default();
        if let Some(profile) = &profile_args {
            profile.output.apply_to(&mut output);
        }
        self.cli_output.apply_to(&mut output);

        let invocation = TestInvocation {
            // Boolean flags compose naturally. Future value-taking scalar test
            // options should use clap value-source tracking so an explicit CLI
            // value overrides the profile value.
            list: self.list || profile_args.as_ref().is_some_and(|p| p.test.list),
            profile_include: profile_args
                .as_ref()
                .map(|p| p.test.include.clone())
                .unwrap_or_default(),
            profile_exclude: profile_args
                .as_ref()
                .map(|p| p.test.exclude.clone())
                .unwrap_or_default(),
            cli_include: self.include.clone(),
            cli_exclude: self.exclude.clone(),
            output,
            logs: self
                .cli_log
                .or_else(|| profile_args.as_ref().and_then(|p| p.logs))
                .unwrap_or(self.log),
        };
        validate_selectors(
            invocation
                .profile_include
                .iter()
                .chain(&invocation.profile_exclude)
                .chain(&invocation.cli_include)
                .chain(&invocation.cli_exclude),
        )?;
        Ok(invocation)
    }

    fn parse_profile_args(name: &str, tokens: &[String]) -> Result<Option<ParsedProfileArgs>> {
        for token in tokens {
            let bootstrap = matches!(
                token.as_str(),
                "--profile"
                    | "--no-profile"
                    | "--project"
                    | "--directory"
                    | "--from"
                    | "--features"
                    | "--help"
                    | "-h"
            ) || token.starts_with("--profile=")
                || token.starts_with("--project=")
                || token.starts_with("--directory=")
                || token.starts_with("--from=")
                || token.starts_with("--features=");
            if bootstrap {
                anyhow::bail!(
                    "invalid argument `{token}` in test profile `{name}`: profile args cannot contain --profile, --no-profile, --project, --directory, --from, --features, or --help"
                );
            }
        }
        if tokens.is_empty() {
            return Ok(None);
        }
        // Parse with the real top-level command grammar so options shown by
        // `baml test --help` are validated the same way in a profile.
        let command = crate::commands::RuntimeCli::command();
        let matches = command
            .try_get_matches_from(
                ["baml", "test"]
                    .into_iter()
                    .chain(tokens.iter().map(String::as_str)),
            )
            .map_err(|e| anyhow!("invalid args in test profile `{name}`: {e}"))?;
        let logs_is_explicit = matches
            .subcommand_matches("test")
            .and_then(|matches| matches.value_source("log"))
            == Some(clap::parser::ValueSource::CommandLine);
        let parsed = crate::commands::RuntimeCli::from_arg_matches(&matches)
            .map_err(|e| anyhow!("invalid args in test profile `{name}`: {e}"))?;
        let output = TestOutputOverrides::from_profile_matches(&matches, parsed.output);
        let crate::commands::Commands::Test(test) = parsed.command else {
            unreachable!("synthetic profile argv always selects the test command")
        };
        Ok(Some(ParsedProfileArgs {
            logs: logs_is_explicit.then_some(test.log),
            test,
            output,
        }))
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
        invocation: &TestInvocation,
    ) -> Option<crate::ExitCode> {
        if CacheContext::verify_enabled() {
            return None;
        }
        let disco = cache?.load_test_discovery()?;
        let legacy_selected: Vec<crate::bytecode_cache::CachedLegacyTest> = disco
            .legacy
            .into_iter()
            .filter(|t| invocation.includes_id(&t.canonical_id))
            .collect();
        let testset_names: Vec<String> = disco
            .testset_leaf_names
            .into_iter()
            .filter(|name| invocation.includes_id(name))
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
    //
    // Canonicalize the order at this single render point — both the fresh-compile
    // and bytecode-cache paths flow through here — so `--list` output is
    // byte-identical between them regardless of upstream map/enumeration order.
    // Sort by canonical id, with the path as a deterministic tiebreak.
    let mut legacy_sorted: Vec<&crate::bytecode_cache::CachedLegacyTest> =
        legacy_selected.iter().collect();
    legacy_sorted.sort_by(|a, b| {
        a.canonical_id
            .cmp(&b.canonical_id)
            .then_with(|| a.file_path.cmp(&b.file_path))
    });
    for t in legacy_sorted {
        println!("  {}  ({})", t.canonical_id, t.file_path);
    }
    for name in testset_names {
        println!("  {name}  (<testset>)");
    }
    crate::ExitCode::Success
}

/// Project a Rust-side [`LegacyTest`] into the cache/render payload shape (its
/// root-relative `file_path` rendered to the display string `--list` prints).
fn cached_legacy_test(t: &LegacyTest) -> crate::bytecode_cache::CachedLegacyTest {
    crate::bytecode_cache::CachedLegacyTest {
        function_name: t.function_name.clone(),
        test_name: t.test_name.clone(),
        canonical_id: t.canonical_id.clone(),
        file_path: t.file_path.display().to_string(),
    }
}

// ---------------------------------------------------------------------------
// Legacy test discovery + execution (unchanged behavior, narrowed types)
// ---------------------------------------------------------------------------

#[allow(unused_variables)]
// BUG: test discovery is duplicated with different semantics in
// `baml_project::symbols::list_tests_with_metadata`. This copy iterates files
// (sees duplicate-named tests) and qualifies function refs unconditionally
// with the *test's* namespace; the playground copy iterates resolved
// namespace items (keep-first winner) and qualifies only when the ref
// resolves in the same namespace. Neither handles a cross-namespace ref by
// the *resolved function's* namespace, which is the correct rule. Unify on
// one `baml_surface`-side derivation once that rule is ratified.
fn discover_legacy_tests(
    db: &ProjectDatabase,
    project: baml_workspace::Project,
) -> Vec<LegacyTest> {
    use baml_db::baml_compiler2_ppir::item_data::{file_tests, test_data};

    let mut tests = Vec::new();
    let root = project.root(db);

    for source_file in db.get_source_files() {
        // Root-relative for display, matching how emit records source paths —
        // keeps `--list` output identical between compiled and
        // bytecode-cache-served runs.
        let file_path = source_file.path(db);
        let file_path = file_path.strip_prefix(&root).unwrap_or(&file_path);
        let namespace = baml_db::baml_compiler2_hir::file_package::file_package(db, source_file)
            .namespace_path
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join(".");

        for test_loc in file_tests(db, source_file) {
            let test = test_data(db, *test_loc);
            for func_ref in &test.function_refs {
                let function_name = if namespace.is_empty() {
                    func_ref.to_string()
                } else {
                    format!("{namespace}.{func_ref}")
                };
                tests.push(LegacyTest {
                    function_name: function_name.clone(),
                    test_name: test.name.to_string(),
                    canonical_id: canonical_legacy_id(&function_name, test.name.as_ref()),
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
/// Each test's `file_path` comes from [`bex_vm_types::TestCase::source_file`] —
/// the test-defining file recorded at emit (`baml_compiler2_emit` Pass 8 via
/// `relative_source_path`) in the same project-root-relative form
/// [`discover_legacy_tests`] derives — so `--list` output is byte-identical
/// between a fresh compile and a bytecode-cache hit.
fn legacy_tests_from_program(program: &bex_vm_types::Program) -> Vec<LegacyTest> {
    let mut tests = Vec::new();
    for tc in &program.test_cases {
        // `source_file` is empty only for a blob predating the field, which the
        // cache format version already gates out, so the `<unknown>` fallback is
        // unreachable in practice.
        let file_path = if tc.source_file.is_empty() {
            PathBuf::from("<unknown>")
        } else {
            PathBuf::from(&tc.source_file)
        };
        for func in &tc.function_names {
            let function_name = qualify_function_from_source(func, &file_path);
            tests.push(LegacyTest {
                function_name: function_name.clone(),
                test_name: tc.name.clone(),
                canonical_id: canonical_legacy_id(&function_name, &tc.name),
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
                "FAIL {} - test case not found in compiled program",
                t.canonical_id
            );
            *failed += 1;
            return;
        }
    };

    let ordered_args = match build_ordered_args(ctx.engine, &t.function_name, test_case) {
        Ok(args) => args,
        Err(e) => {
            eprintln!("FAIL {} - {e}", t.canonical_id);
            *failed += 1;
            return;
        }
    };

    let (call_ctx, logs) = ctx.call_context(CallId::next());
    let result = ctx.block_on_with_logs(
        ctx.engine
            .call_function_bound_args(&t.function_name, ordered_args, call_ctx, true),
        logs.as_ref(),
    );
    match result {
        Ok(result) => {
            println!("PASS {}", t.canonical_id);
            println!("  => {result:?}");
            *passed += 1;
        }
        Err(e) => {
            eprintln!("FAIL {}", t.canonical_id);
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
// `testing.TestRegistry.run_filtered(registry, profile filters, CLI filters)` and
// `testing.TestRegistry.list_filtered(...)` are defined in
// baml_std/testing/registry.baml. `self` is passed as the first positional
// argument (the live registry handle).
// ---------------------------------------------------------------------------

fn run_filtered_report(
    ctx: &RunCtx,
    registry: &BexExternalValue,
    invocation: &TestInvocation,
) -> Result<BexExternalValue> {
    let (call_ctx, logs) = ctx.call_context(CallId::next());
    // Cap concurrently RUNNING test bodies at twice the core count. The
    // runner admits a leaf only when a slot is free, so a five-thousand-test
    // corpus holds ~2N live VM threads instead of five thousand — which keeps
    // per-thread GC costs bounded and keeps wall-clock timing assertions
    // meaningful under full-corpus load.
    let max_concurrency =
        i64::try_from(std::thread::available_parallelism().map_or(8, std::num::NonZero::get) * 2)
            .unwrap_or(16);
    let result = ctx.block_on_with_logs(
        ctx.engine.call_function(
            "testing.TestRegistry.run_filtered",
            vec![
                registry.clone(),
                string_array(&invocation.profile_include),
                string_array(&invocation.profile_exclude),
                string_array(&invocation.cli_include),
                string_array(&invocation.cli_exclude),
                BexExternalValue::Int(max_concurrency),
            ],
            call_ctx,
            true,
        ),
        logs.as_ref(),
    );
    result.map_err(|e| anyhow!("run_filtered failed: {e}"))
}

fn list_selected_testset_names(
    ctx: &RunCtx,
    registry: &BexExternalValue,
    invocation: &TestInvocation,
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
                string_array(&invocation.profile_include),
                string_array(&invocation.profile_exclude),
                string_array(&invocation.cli_include),
                string_array(&invocation.cli_exclude),
            ],
            call_ctx,
            true,
        ))
        .map_err(|e| anyhow!("list_filtered failed: {e}"))?;
    Ok(string_array_values(&value))
}

/// Print a flat report and fold its counts into the running totals.
///
/// Print the aggregate testset result and fold its counts into the running
/// totals. Both filtered and unfiltered runs go through the same aggregate path
/// (filtered selections still run under their testset runners), so a failing
/// leaf whose runner still passes the suite is reported as a *tolerated* failure
/// — kept out of the hard `failed` count and not failing the command. Every
/// leaf line uses the same canonical ID emitted by `baml test --list`; aggregate
/// runner verdicts are labeled as aggregates rather than presented as test IDs.
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
    // to a vacuous `pass` over zero tests. Printing a green aggregate pass
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

    // Duration suffixes come from the parallel `*_ms` arrays; a missing or
    // negative entry (fallback-correlated identities, expansion sentinels)
    // prints the bare line the output always had.
    let with_ms = |ms: &[i64], i: usize| -> String {
        match ms.get(i) {
            Some(ms) if *ms >= 0 => format!(" ({ms}ms)"),
            _ => String::new(),
        }
    };
    for (i, name) in flat.passed_names.iter().enumerate() {
        println!("PASS {name}{}", with_ms(&flat.passed_ms, i));
    }
    for (i, name) in flat.tolerated_names.iter().enumerate() {
        println!("TOLERATED {name}{}", with_ms(&flat.tolerated_ms, i));
    }
    for (i, name) in flat.failed_names.iter().enumerate() {
        println!("FAIL {name}{}", with_ms(&flat.failed_ms, i));
    }

    if flat.outcome == "pass" {
        if flat.tolerated > 0 {
            println!(
                "AGGREGATE PASS [outcome=pass; {} tolerated {}]",
                flat.tolerated,
                pluralize(flat.tolerated, "failure", "failures")
            );
        }
    } else {
        println!("AGGREGATE FAIL [outcome={}]", flat.outcome);
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
    passed_names: Vec<String>,
    failed_names: Vec<String>,
    tolerated_names: Vec<String>,
    /// Per-leaf durations parallel to the matching name arrays; may be
    /// shorter (or empty) when identities came from the registry's fallback
    /// correlation, and `-1` marks a leaf with no measured run — print the
    /// duration only when a non-negative entry exists at the name's index.
    passed_ms: Vec<i64>,
    failed_ms: Vec<i64>,
    tolerated_ms: Vec<i64>,
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
    let passed_names = fields
        .get("passed_names")
        .map(string_array_values)
        .unwrap_or_default();
    let failed_names = fields
        .get("failed_names")
        .map(string_array_values)
        .unwrap_or_default();
    let tolerated_names = fields
        .get("tolerated_names")
        .map(string_array_values)
        .unwrap_or_default();
    let passed_ms = fields
        .get("passed_ms")
        .map(int_array_values)
        .unwrap_or_default();
    let failed_ms = fields
        .get("failed_ms")
        .map(int_array_values)
        .unwrap_or_default();
    let tolerated_ms = fields
        .get("tolerated_ms")
        .map(int_array_values)
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
        passed_names,
        failed_names,
        tolerated_names,
        passed_ms,
        failed_ms,
        tolerated_ms,
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

fn int_array_values(value: &BexExternalValue) -> Vec<i64> {
    match unwrap_union(value) {
        BexExternalValue::Array { items, .. } => items
            .iter()
            .filter_map(|item| match unwrap_union(item) {
                BexExternalValue::Int(i) => Some(*i),
                _ => None,
            })
            .collect(),
        _ => Vec::new(),
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
                ("passed_names", array(Vec::new())),
                ("failed_names", array(failed_names)),
                ("tolerated_names", array(Vec::new())),
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
                ("passed_names", array(vec![string("root::passes")])),
                ("failed_names", array(Vec::new())),
                ("tolerated_names", array(Vec::new())),
                ("messages", array(Vec::new())),
            ],
        );

        let parsed = parse_flat_report(&value).expect("should parse");
        assert_eq!(parsed.outcome, "pass");
        assert_eq!(parsed.total, 1);
        assert_eq!(parsed.passed_names, ["root::passes"]);
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
        // green aggregate pass line contradicting the non-zero exit (B-628).
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

    #[test]
    fn profile_args_use_real_test_cli_grammar_and_reject_bootstrap_flags() {
        let tokens = vec![
            "-i".to_string(),
            "root::integration::*".to_string(),
            "-x".to_string(),
            "*::flaky::*".to_string(),
            "--log".to_string(),
            "info".to_string(),
        ];
        let parsed = TestArgs::parse_profile_args("ci", &tokens)
            .unwrap()
            .unwrap();
        assert_eq!(parsed.test.include, vec!["root::integration::*"]);
        assert_eq!(parsed.test.exclude, vec!["*::flaky::*"]);
        assert_eq!(parsed.logs, Some(TestLogLevel::Info));

        let globals = TestArgs::parse_profile_args(
            "globals",
            &[
                "--color".to_string(),
                "never".to_string(),
                "--no-progress".to_string(),
            ],
        )
        .unwrap()
        .unwrap();
        assert_eq!(
            globals.output.color,
            Some(crate::output::ColorChoice::Never)
        );
        assert_eq!(globals.output.no_progress, Some(true));

        let error = TestArgs::parse_profile_args("bad_features", &["--features=beta".to_string()])
            .unwrap_err()
            .to_string();
        assert!(
            error.contains("cannot contain") && error.contains("--features"),
            "{error}"
        );

        let error = TestArgs::parse_profile_args("bad", &["--profile=other".to_string()])
            .unwrap_err()
            .to_string();
        assert!(error.contains("cannot contain --profile"), "{error}");
    }

    #[test]
    fn explicit_cli_scalar_overrides_profile_scalar() {
        let cli = crate::commands::RuntimeCli::parse_from_smart(vec![
            "baml".into(),
            "test".into(),
            "--profile".into(),
            "ci".into(),
            "--color".into(),
            "always".into(),
            "--log".into(),
            "debug".into(),
        ]);
        let crate::commands::Commands::Test(args) = cli.command else {
            panic!("expected test command")
        };
        let invocation = args
            .resolve_invocation(
                Some(
                    r#"
[test.profiles.ci]
args = ["--color", "never", "--log", "warn"]
"#,
                ),
                std::path::Path::new("/project"),
            )
            .unwrap();
        assert_eq!(
            invocation.output.color,
            Some(crate::output::ColorChoice::Always)
        );
        assert_eq!(invocation.logs, TestLogLevel::Debug);
    }
}
