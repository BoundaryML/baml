#![allow(clippy::print_stdout, clippy::print_stderr)]

use std::{
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
};

use anyhow::{Context, Result, anyhow};
use baml_exec::test_report::{
    self, TestSelection, consume_flat_report, parse_flat_report, pluralize,
};
use bex_engine::{
    BexEngine, BexExternalValue, CancellationToken, FunctionCallContext,
    FunctionCallContextBuilder, logger::TraceLogger,
};
use clap::{Args, FromArgMatches};
use sys_native::CallId;

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
  --project, --agent-skill-check, --from, --features, or --help. With no default profile,
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

    /// Emit a duration-free machine report instead of the human test report.
    #[arg(long, hide = true)]
    corpus_report: bool,

    /// Internal corpus gate entry: one exact canonical ID, including literal '*'.
    #[arg(long, hide = true, requires = "corpus_report")]
    corpus_exact: Option<String>,

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
        default_value_t = TestLogLevel::Info,
        ignore_case = true,
        value_name = "LEVEL",
        help = "Set the BAML log level; overrides BAML_LOG [default: info] [possible values: off, error, warn, info, debug, trace]",
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

    /// Fail any single test that runs longer than this.
    ///
    /// Accepts durations such as `500ms`, `30s` or `15m`; `none` disables the
    /// per-test deadline.
    #[arg(
        long = "test-timeout",
        value_name = "DURATION",
        default_value = "5m",
        help_heading = "Execution options"
    )]
    test_timeout: crate::optional_duration::OptionalDuration,

    /// Run at most this many test bodies at once.
    ///
    /// Defaults to twice the available CPU parallelism.
    #[arg(
        long = "max-concurrency",
        value_name = "N",
        value_parser = clap::value_parser!(u32).range(1..),
        help_heading = "Execution options"
    )]
    max_concurrency: Option<u32>,

    #[command(flatten)]
    shutdown: crate::shutdown::ShutdownArgs,
}

#[derive(Debug, Default)]
struct TestInvocation {
    corpus_exact: Option<String>,
    list: bool,
    selection: TestSelection,
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
        let selection = &self.selection;
        TestFilter::includes_patterns(id, &selection.profile_include, &selection.profile_exclude)
            && TestFilter::includes_patterns(id, &selection.cli_include, &selection.cli_exclude)
    }

    fn has_filters(&self) -> bool {
        self.selection.has_filters()
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
    shutdown_timeout: crate::optional_duration::OptionalDuration,
    test_timeout: crate::optional_duration::OptionalDuration,
    max_concurrency: Option<u32>,
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

fn finish_engine(
    ctx: &RunCtx<'_>,
    reporter: &Reporter,
    status: bex_engine::ProcessStatus,
) -> usize {
    crate::shutdown::shutdown_engine(ctx.rt, ctx.engine, reporter, status, ctx.shutdown_timeout);
    if let Some(error) = ctx.engine.initial_cloud_authorization_error() {
        crate::reporter::print_error(format_args!("{error}"));
        return ctx
            .unhandled_spawn_failures
            .load(Ordering::SeqCst)
            .saturating_add(1);
    }
    ctx.unhandled_spawn_failures.load(Ordering::SeqCst)
}

impl TestArgs {
    pub fn run(&self) -> Result<crate::ExitCode> {
        let reporter = Reporter::new();
        // ── 1. Load project ────────────────────────────────────────────────
        let mut session = crate::project_session::ProjectSession::open(
            self.from.as_deref(),
            crate::project_session::CacheUse::ReadWrite,
        )?;
        let invocation =
            self.resolve_invocation(session.resolved.manifest.as_deref(), session.root())?;
        crate::output::init(invocation.output)?;
        if session.is_empty() {
            reporter.abandon();
            crate::reporter::print_error(format_args!(
                "no .baml files found in {}",
                session.root().display()
            ));
            return Ok(crate::ExitCode::NoTestsRun);
        }

        // Warm `--list` fast path. The fully-expanded test leaf names are a pure
        // function of the compiled
        // Program, cached under its key. On a hit we render + select directly
        // and skip engine boot, `$init`/`$init_test`, and in-VM testset
        // expansion entirely — the whole `--list` discovery floor. Gated off
        // under DEV_BAML_BUILD_CACHE_VERIFY=always (the oracle must run honest
        // discovery); any miss/corruption falls through to the
        // honest path below.
        if invocation.list {
            if let Some(exit) = self.try_cached_list(&reporter, session.cache.as_ref(), &invocation)
            {
                return Ok(exit);
            }
        }

        let cached_program = session.try_cached_program();

        let cached_engine = cached_program.and_then(|program| {
            // Bytecode-cache hit: the Program carries the in-VM test registry,
            // so the database (typecheck, HIR discovery, emit) is skipped.
            match crate::runtime_telemetry::create_engine(
                program,
                Vec::new(),
                session.root(),
                crate::runtime_telemetry::session_sources(&session),
            ) {
                Ok(engine) => Some(Arc::new(engine)),
                Err(_) => None,
            }
        });

        let engine = if let Some(hit) = cached_engine {
            hit
        } else {
            let warmth = session.warm_prep();
            let (served, stdlib_interface_hit) = (warmth.served, warmth.stdlib_interface_hit);
            let db = &session.db;
            let package = session.package;
            let cache = &session.cache;
            // ── 2. Diagnostics ─────────────────────────────────────────────
            // Keep `baml test` quiet during the compile phase. `baml check`
            // and `baml generate` own the compile/count progress lines. With a
            // cache, gate through the incremental collector (serve the check
            // rows of an unchanged project, check every file otherwise, and
            // carry the fresh per-file blobs into the manifest); without one,
            // run the honest full check. The merged set is byte-identical.
            let (diagnostics, fresh_diagnostics) = if let Some(ctx) = cache {
                let incremental = ctx.collect_diagnostics_incremental(db, package, served.as_ref());
                (incremental.merged, Some(incremental.fresh_by_file))
            } else {
                (baml_db::ProjectDiagnostics::collect(db), None)
            };
            // A refused world renders its errors as run/pack do; `test`
            // reports the failure through its exit code.
            let world = match baml_db::CheckedWorld::check(&diagnostics, package) {
                Ok(world) => world,
                Err(refused) => {
                    let rendered =
                        crate::check_command::render_project_diagnostics(db, refused.errors());
                    reporter.abandon();
                    eprintln!("{rendered}");
                    return Ok(crate::ExitCode::Other);
                }
            };

            // 3. Compile + engine + runtime
            let program = crate::bytecode_cache::compile_program(&world, cache.as_ref())
                .map_err(|e| anyhow!("compilation failed: {e}"))?;
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

            Arc::new(
                crate::runtime_telemetry::create_engine(
                    program,
                    Vec::new(),
                    session.root(),
                    crate::runtime_telemetry::session_sources(&session),
                )
                .context("failed to create engine")?,
            )
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
            shutdown_timeout: self.shutdown.shutdown_timeout,
            test_timeout: self.test_timeout,
            max_concurrency: self.max_concurrency,
        };

        // ── 5. Resolve the testset registry handle ─────────────────────────
        // `collect_tests` runs `$init_test`, returning a live `testing.TestRegistry`
        // handle (or `Null` when the project declares no tests). All testset
        // discovery/filtering/execution then happens inside that package via
        // `run_filtered` / `list_filtered`.
        reporter.spin("Discovering", "tests");
        let discovery_started = std::time::Instant::now();
        let registry = match rt.block_on(engine.collect_tests(CallId::next(), cancel.clone())) {
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
                return Ok(
                    if finish_engine(&run_ctx, &reporter, bex_engine::ProcessStatus::Error) != 0 {
                        crate::ExitCode::TestFailure
                    } else {
                        crate::ExitCode::Other
                    },
                );
            }
        };

        crate::reporter::print_verbose(format_args!(
            "discovered tests in {:.2?} (test registry: {})",
            discovery_started.elapsed(),
            if registry.is_some() {
                "present"
            } else {
                "none"
            },
        ));

        // 5. List mode
        if invocation.list {
            let testset_names = match &registry {
                Some(reg) => match list_selected_testset_names(&run_ctx, reg, &invocation) {
                    Ok(names) => names,
                    Err(e) => {
                        reporter.abandon();
                        crate::reporter::print_error(format_args!("failed to list tests: {e}"));
                        return Ok(
                            if finish_engine(&run_ctx, &reporter, bex_engine::ProcessStatus::Error)
                                != 0
                            {
                                crate::ExitCode::TestFailure
                            } else {
                                crate::ExitCode::Other
                            },
                        );
                    }
                },
                None => Vec::new(),
            };

            if finish_engine(&run_ctx, &reporter, bex_engine::ProcessStatus::Success) != 0 {
                return Ok(crate::ExitCode::TestFailure);
            }

            // Write-through the discovery cache (+ DEV_BAML_BUILD_CACHE_VERIFY=always oracle) so a
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
                    testset_leaf_names: testset_names.clone(),
                };
                ctx.verify_test_discovery(&disco)?;
                ctx.store_test_discovery(&disco);
            }

            return Ok(render_test_list(&reporter, &testset_names));
        }

        // 6. Execute
        // Test execution writes per-test PASS/FAIL lines to stdout; clear the
        // spinner so those lines don't fight with the ticks.
        reporter.abandon();

        let mut passed = 0usize;
        let mut failed = 0usize;
        let mut tolerated = 0usize;
        let mut total = 0usize;
        let mut command_failed = false;

        // Tests run inside the stdlib: a single `run_filtered` call
        // expands, filters, runs (concurrently via spawn/await), and aggregates
        // (honoring testset runners), returning a tolerated-aware flat report.
        if let Some(reg) = &registry {
            match run_filtered_report(&run_ctx, reg, &invocation) {
                Ok(value) => match parse_flat_report(&value) {
                    Some(flat) => {
                        if self.corpus_report {
                            println!(
                                "BAML_CORPUS_REPORT\t{}",
                                serde_json::json!({
                                    "outcome":flat.outcome, "passed":flat.passed_names,
                                    "failed":flat.failed_names, "tolerated":flat.tolerated_names,
                                    "messages":flat.messages, "total":flat.total,
                                })
                            );
                            passed += flat.passed;
                            failed += flat.failed;
                            tolerated += flat.tolerated;
                            total += flat.total;
                            command_failed |= !flat.passed();
                        } else {
                            consume_flat_report(
                                &flat,
                                &mut passed,
                                &mut failed,
                                &mut tolerated,
                                &mut total,
                                &mut command_failed,
                            )
                        }
                    }
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

        let status = if failed == 0 {
            bex_engine::ProcessStatus::Success
        } else {
            bex_engine::ProcessStatus::Error
        };
        let unhandled_spawn_failure_count = finish_engine(&run_ctx, &reporter, status);
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
        let profile_args = resolve_profile(
            self.no_profile,
            self.profile.as_deref(),
            manifest_text,
            project_root,
        )?;

        let mut output = crate::output::OutputArgs::default();
        if let Some(profile) = &profile_args {
            profile.output.apply_to(&mut output);
        }
        self.cli_output.apply_to(&mut output);

        let invocation = TestInvocation {
            corpus_exact: self.corpus_exact.clone(),
            // Boolean flags compose naturally. Future value-taking scalar test
            // options should use clap value-source tracking so an explicit CLI
            // value overrides the profile value.
            list: self.list || profile_args.as_ref().is_some_and(|p| p.test.list),
            selection: TestSelection {
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
            },
            output,
            logs: self
                .cli_log
                .or_else(|| profile_args.as_ref().and_then(|p| p.logs))
                .unwrap_or(self.log),
        };
        let selection = &invocation.selection;
        validate_selectors(
            selection
                .profile_include
                .iter()
                .chain(&selection.profile_exclude)
                .chain(&selection.cli_include)
                .chain(&selection.cli_exclude),
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
                    | "--agent-skill-check"
                    | "--from"
                    | "--features"
                    | "--help"
                    | "-h"
            ) || token.starts_with("--profile=")
                || token.starts_with("--project=")
                || token.starts_with("--agent-skill-check=")
                || token.starts_with("--from=")
                || token.starts_with("--features=");
            if bootstrap {
                anyhow::bail!(
                    "invalid argument `{token}` in test profile `{name}`: profile args cannot contain --profile, --no-profile, --project, --agent-skill-check, --from, --features, or --help"
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
    /// cache is absent/disabled, under `DEV_BAML_BUILD_CACHE_VERIFY=always`, or on a discovery
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
        let testset_names: Vec<String> = disco
            .testset_leaf_names
            .into_iter()
            .filter(|name| invocation.includes_id(name))
            .collect();
        Some(render_test_list(reporter, &testset_names))
    }
}

/// The profile a `baml test` invocation applies — `profile` when named, else
/// the manifest's default unless `no_profile` — parsed through the real test
/// CLI grammar. `None` when no profile applies or the profile has no args.
fn resolve_profile(
    no_profile: bool,
    profile: Option<&str>,
    manifest_text: Option<&str>,
    project_root: &std::path::Path,
) -> Result<Option<ParsedProfileArgs>> {
    let manifest = manifest_text
        .map(baml_db::manifest::parse)
        .transpose()
        .with_context(|| {
            format!(
                "failed to read test profiles from {}",
                project_root.join("baml.toml").display()
            )
        })?;

    let selected_name = if no_profile {
        None
    } else if let Some(name) = profile {
        Some(name)
    } else {
        manifest.as_ref().and_then(|m| m.test.default.as_deref())
    };

    let Some(name) = selected_name else {
        return Ok(None);
    };
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
    TestArgs::parse_profile_args(name, &profile.args).with_context(|| {
        format!(
            "{}: invalid [test.profiles.{name}].args",
            project_root.join("baml.toml").display()
        )
    })
}

/// The selectors a bare `baml test` applies from the project's default
/// profile, as `(include, exclude)` — what `baml pack --tests` bakes into a
/// test binary so it selects the same tests.
pub(crate) fn default_profile_selectors(
    manifest_text: Option<&str>,
    project_root: &std::path::Path,
) -> Result<(Vec<String>, Vec<String>)> {
    let profile = resolve_profile(false, None, manifest_text, project_root)?;
    Ok(profile
        .map(|profile| (profile.test.include, profile.test.exclude))
        .unwrap_or_default())
}

/// Render the selected `--list` output to stdout and return the exit code.
/// Shared by the honest path and the warm discovery-cache path so both produce
/// byte-identical stdout. Status/`Selected` lines go to stderr (via the
/// reporter); only the list itself is stdout content.
fn render_test_list(reporter: &Reporter, testset_names: &[String]) -> crate::ExitCode {
    if testset_names.is_empty() {
        reporter.finish("Finished", "no tests selected");
        return crate::ExitCode::NoTestsRun;
    }
    reporter.status("Selected", format!("{} test(s)", testset_names.len()));
    // Indented list under the cargo-style status line. These are content (the
    // actual list), not status updates, so they go to stdout as plain prints.
    //
    // Canonicalize the order at this single render point — both the fresh-compile
    // and bytecode-cache paths flow through here — so `--list` output is
    // byte-identical between them regardless of upstream map/enumeration order.
    for name in testset_names {
        println!("  {name}  (<testset>)");
    }
    crate::ExitCode::Success
}

// ---------------------------------------------------------------------------
// Testset entry points: a single BAML call does discovery + filtering +
// concurrent execution + aggregation, returning a flat report. The calls and
// the report live in `baml_exec::test_report`, shared with packed test
// binaries.
// ---------------------------------------------------------------------------

fn run_filtered_report(
    ctx: &RunCtx,
    registry: &BexExternalValue,
    invocation: &TestInvocation,
) -> Result<BexExternalValue> {
    let (call_ctx, logs) = ctx.call_context(CallId::next());
    // Cap concurrently RUNNING test bodies, by default at twice the core
    // count (see `default_max_concurrency`).
    let max_concurrency = ctx
        .max_concurrency
        .map_or_else(test_report::default_max_concurrency, i64::from);
    let test_timeout_ms = test_report::test_timeout_ms(ctx.test_timeout.0);
    ctx.block_on_with_logs(
        async {
            if let Some(name) = &invocation.corpus_exact {
                test_report::run_exact(ctx.engine, registry, &invocation.selection, name, call_ctx)
                    .await
            } else {
                test_report::run_filtered(
                    ctx.engine,
                    registry,
                    &invocation.selection,
                    max_concurrency,
                    test_timeout_ms,
                    call_ctx,
                )
                .await
            }
        },
        logs.as_ref(),
    )
}

fn list_selected_testset_names(
    ctx: &RunCtx,
    registry: &BexExternalValue,
    invocation: &TestInvocation,
) -> Result<Vec<String>> {
    let call_ctx = FunctionCallContextBuilder::new(CallId::next())
        .with_cancel_token(ctx.cancel.clone())
        .build();
    ctx.rt.block_on(test_report::list_filtered(
        ctx.engine,
        registry,
        &invocation.selection,
        call_ctx,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

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

        let error = TestArgs::parse_profile_args(
            "bad_skill_check",
            &["--agent-skill-check=off".to_string()],
        )
        .unwrap_err()
        .to_string();
        assert!(
            error.contains("cannot contain") && error.contains("--agent-skill-check"),
            "{error}"
        );
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
