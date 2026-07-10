#![allow(clippy::print_stdout, clippy::print_stderr)]

use std::{future::Future, io::Write as _, path::PathBuf, sync::Arc, time::Duration};

use anyhow::{Context, Result, anyhow};
use baml_db::{baml_compiler_diagnostics::Severity, baml_compiler2_emit};
use baml_project::ProjectDatabase;
use baml_type::RuntimeTy;
use bex_engine::{
    BexCallArg, BexEngine, BexExternalValue, CancellationToken, CaptureDefaults,
    FunctionCallContext, FunctionCallContextBuilder, test_arg_to_external,
    value_capture::{TraceCaptureConfig, TraceCaptureProducer},
};
use bex_events::value::{ValueCodec, ValueRef, ValueWriteOutcome};
use bridge_ctypes::baml_core::cffi::{
    BamlOutboundValue, baml_outbound_value::Value as OutboundValue,
};
use clap::{Args, ValueEnum};
use prost::Message;
use sys_native::{CallId, SysOpsExt};

use crate::{project_load::load_project_for_build, reporter::Reporter, test_filter::TestFilter};

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

    /// Print BAML `log.*` events to stdout at or above this level.
    ///
    /// Logs are off by default. Because logs use stdout, callers can retain
    /// the raw stream with `baml test --logs INFO > baml-test.log`.
    #[arg(
        long,
        value_enum,
        default_value_t = TestLogLevel::Off,
        ignore_case = true,
        value_name = "LEVEL"
    )]
    pub logs: TestLogLevel,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, ValueEnum)]
pub enum TestLogLevel {
    #[default]
    Off,
    Error,
    Warn,
    Info,
    Debug,
}

impl TestLogLevel {
    fn allows(self, event_level: Option<&str>) -> bool {
        let threshold = match self {
            Self::Off => return false,
            Self::Error => 1,
            Self::Warn => 2,
            Self::Info => 3,
            Self::Debug => 4,
        };
        let event = match event_level.unwrap_or("info").to_ascii_lowercase().as_str() {
            "error" => 1,
            "warn" | "warning" => 2,
            "info" => 3,
            "debug" => 4,
            _ => 3,
        };
        event <= threshold
    }
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
    logs: TestLogLevel,
}

impl RunCtx<'_> {
    fn call_context(&self, call_id: CallId) -> (FunctionCallContext, Option<TraceCaptureProducer>) {
        let builder =
            FunctionCallContextBuilder::new(call_id).with_cancel_token(self.cancel.clone());
        if self.logs == TestLogLevel::Off {
            return (builder.build(), None);
        }

        // Test logs are drained after each selected invocation. Keep value
        // capture disabled and reserve a generous log-only queue so ordinary
        // suites do not silently lose events while their test bodies run.
        let producer =
            TraceCaptureProducer::new(TraceCaptureConfig::enabled_with_budgets(0, 100_000));
        let context = builder
            .with_capture_defaults(CaptureDefaults {
                values_enabled: false,
                logs_enabled: true,
            })
            .with_value_capture(producer.clone())
            .build();
        (context, Some(producer))
    }

    fn print_logs(&self, producer: Option<&TraceCaptureProducer>) {
        let Some(producer) = producer else {
            return;
        };
        let mut next_id = 0usize;
        let report = producer.drain_to_value_recorder_report(|draft, body| {
            next_id += 1;
            if let Some(log) = &draft.log
                && self.logs.allows(log.level.as_deref())
            {
                let level = log.level.as_deref().unwrap_or("info").to_ascii_uppercase();
                println!("[{level}] {}", render_log_body(&body));
            }
            Ok(ValueWriteOutcome {
                value_ref: ValueRef::available(
                    format!("test_log_{next_id}"),
                    ValueCodec::BamlOutboundValue,
                    body.len(),
                    body.len(),
                ),
            })
        });
        for failure in report.failures {
            eprintln!("WARN test log capture failed: {}", failure.diagnostic);
        }
        // A redirected stdout is not necessarily line-buffered. Explicitly
        // flush each drained batch so `tail -f` observes test logs live.
        let _ = std::io::stdout().flush();
    }

    fn block_on_with_logs<T>(
        &self,
        future: impl Future<Output = T>,
        producer: Option<&TraceCaptureProducer>,
    ) -> T {
        let Some(producer) = producer else {
            return self.rt.block_on(future);
        };
        self.rt.block_on(async {
            tokio::pin!(future);
            let mut interval = tokio::time::interval(Duration::from_millis(50));
            interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            loop {
                tokio::select! {
                    result = &mut future => {
                        self.print_logs(Some(producer));
                        break result;
                    }
                    _ = interval.tick() => self.print_logs(Some(producer)),
                }
            }
        })
    }
}

fn render_log_body(body: &[u8]) -> String {
    let Ok(value) = BamlOutboundValue::decode(body) else {
        return format!("<undecodable BAML log: {} bytes>", body.len());
    };
    match value.value.as_ref() {
        None | Some(OutboundValue::NullValue(_)) => "null".to_string(),
        Some(OutboundValue::StringValue(value)) => value.clone(),
        Some(OutboundValue::IntValue(value)) => value.to_string(),
        Some(OutboundValue::FloatValue(value)) => value.to_string(),
        Some(OutboundValue::BoolValue(value)) => value.to_string(),
        Some(OutboundValue::BigintValue(value)) => value.clone(),
        Some(_) => format!("{value:?}"),
    }
}

impl TestArgs {
    pub fn run(&self) -> Result<crate::ExitCode> {
        let reporter = Reporter::new();
        // ── 1. Load project ────────────────────────────────────────────────
        let (db, from, baml_files) =
            load_project_for_build(self.from.as_deref(), &reporter, false)?;
        if baml_files.is_empty() {
            reporter.abandon();
            crate::reporter::print_error(format_args!(
                "no .baml files found in {}",
                from.display()
            ));
            return Ok(crate::ExitCode::NoTestsRun);
        }
        let project = db
            .get_project()
            .ok_or_else(|| anyhow!("No project context"))?;

        // ── 2. Diagnostics ─────────────────────────────────────────────────
        // Keep `baml test` quiet during the compile phase. `baml check` and
        // `baml generate` own the compile/count progress lines.
        let source_files = db.get_source_files();
        let diagnostics = baml_project::collect_diagnostics(&db, project, &source_files);
        let errors: Vec<_> = diagnostics
            .iter()
            .filter(|d| d.severity == Severity::Error)
            .collect();
        if !errors.is_empty() {
            // Render the full ariadne block so test errors look like
            // run/pack errors instead of the previous "bullet list of
            // messages" shape.
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

        // ── 3. Discover legacy tests from HIR ──────────────────────────────
        let legacy = discover_legacy_tests(&db, project);

        // ── 4. Compile + engine + runtime ──────────────────────────────────
        let compile_options = baml_compiler2_emit::CompileOptions {
            emit_test_cases: true,
        };
        let bytecode = db
            .generate_bytecode_unchecked(&compile_options)
            .map_err(|e| anyhow!("Compilation failed: {e:?}"))?;

        let engine = Arc::new(
            BexEngine::new(bytecode, Arc::new(sys_native::SysOps::native()), Vec::new())
                .map_err(|e| anyhow!("Failed to create engine: {e:?}"))?,
        );
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
            logs: self.logs,
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

            if legacy_selected.is_empty() && testset_names.is_empty() {
                reporter.finish("Finished", "no tests selected");
                return Ok(crate::ExitCode::NoTestsRun);
            }

            reporter.status(
                "Selected",
                format!("{} test(s)", legacy_selected.len() + testset_names.len()),
            );
            // Indented list under the cargo-style status line. These are
            // content (the actual list), not status updates, so they go to
            // stdout as plain prints.
            for t in &legacy_selected {
                println!(
                    "  {}::{}  ({})",
                    t.function_name,
                    t.test_name,
                    t.file_path.display()
                );
            }
            for name in &testset_names {
                let (func, test) = split_top(name);
                println!("  {func}::{test}  (<testset>)");
            }
            return Ok(crate::ExitCode::Success);
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

    for source_file in db.get_source_files() {
        let item_tree = baml_compiler2_hir::file_item_tree(db, source_file);
        let file_path = source_file.path(db);

        for test in item_tree.tests.values() {
            for func_ref in &test.function_refs {
                tests.push(LegacyTest {
                    function_name: func_ref.to_string(),
                    test_name: test.name.to_string(),
                    file_path: file_path.clone(),
                });
            }
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

    let (call_ctx, logs) = ctx.call_context(CallId::next());
    let result = ctx.block_on_with_logs(
        ctx.engine
            .call_function_bound_args(&t.function_name, ordered_args, call_ctx, true),
        logs.as_ref(),
    );
    match result {
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
    let (call_ctx, logs) = ctx.call_context(CallId::next());
    let result = ctx.block_on_with_logs(
        ctx.engine.call_function(
            "testing.TestRegistry.run_filtered",
            vec![
                registry.clone(),
                string_array(include),
                string_array(exclude),
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
    fn test_log_level_filters_at_or_above_threshold() {
        assert!(!TestLogLevel::Off.allows(Some("error")));
        assert!(TestLogLevel::Info.allows(Some("error")));
        assert!(TestLogLevel::Info.allows(Some("warn")));
        assert!(TestLogLevel::Info.allows(Some("info")));
        assert!(!TestLogLevel::Info.allows(Some("debug")));
        assert!(TestLogLevel::Debug.allows(Some("debug")));
    }

    #[test]
    fn consume_fail_with_zero_hard_failed_synthesizes_one() {
        // Runner fails the aggregate without marking any child failed.
        let parsed =
            parse_flat_report(&flat_report("fail", 1, 0, 0, 1, Vec::new(), Vec::new())).unwrap();
        assert_eq!(consume(&parsed), (1, 1, 0, 2, true));
    }
}
