#![allow(clippy::print_stdout, clippy::print_stderr)]

use std::{path::PathBuf, sync::Arc};

use anyhow::{Context, Result, anyhow};
use baml_db::{baml_compiler_diagnostics::Severity, baml_compiler2_emit};
use baml_project::ProjectDatabase;
use bex_engine::{
    BexCallArg, BexEngine, BexExternalValue, CancellationToken, FunctionCallContextBuilder,
    test_arg_to_external,
};
use clap::Args;
use sys_native::{CallId, SysOpsExt};

use crate::{
    project_load::load_project_from_reporting, reporter::Reporter, test_filter::TestFilter,
};

#[derive(Args, Clone, Debug)]
pub struct TestArgs {
    #[arg(long, help = "Project search starting point", value_name = "PATH")]
    pub from: Option<PathBuf>,

    /// Only list selected tests
    #[arg(long, default_value_t = false)]
    list: bool,

    #[arg(long, short = 'i')]
    /// Specific functions or tests to include. If none provided, runs all tests.
    ///
    /// Examples:
    ///
    /// -i "FunctionName::TestName" will match the specific test
    ///
    /// -i "FunctionName::" will run all tests in the function
    ///
    /// -i "::TestName" will run the test in any function
    ///
    /// -i "Get*::*Bar" will match with wildcards
    pub include: Vec<String>,

    #[arg(long, short = 'x')]
    /// Specific functions or tests to exclude. Takes precedence over --include.
    ///
    /// Uses the same syntax as --include.
    pub exclude: Vec<String>,
}

/// Source of a discovered test — determines how it's executed.
enum TestKind {
    /// Old-style `test "name" { functions [Foo] args {…} }` attached to an LLM
    /// function. Executed by calling the function directly with test args.
    Legacy { file_path: PathBuf },
    /// New-style test inside a `testset "name" { test "name" { … } }` block.
    /// Executed via `testing.TestRegistry.run_test(registry, full_path)`.
    Testset {
        /// Slash-separated path as the testing runtime knows it, e.g.
        /// `"tictactoe/x wins row"` or `"outer/inner/foo"`.
        full_path: String,
    },
}

struct DiscoveredTest {
    /// First segment shown to the filter as `FunctionName` — for legacy tests
    /// this is the user function under test; for testset tests it's the
    /// top-level testset name.
    function_name: String,
    /// Second segment shown to the filter as `TestName` — for legacy tests
    /// this is the test-block name; for testset tests it's the remainder of
    /// the slash-separated path after the first segment.
    test_name: String,
    kind: TestKind,
}

impl DiscoveredTest {
    fn display_location(&self) -> String {
        match &self.kind {
            TestKind::Legacy { file_path } => file_path.display().to_string(),
            TestKind::Testset { .. } => "<testset>".to_string(),
        }
    }
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
        let (db, from, baml_files) = load_project_from_reporting(self.from.as_deref(), &reporter)?;
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
        reporter.spin("Checking", format!("{} file(s)", baml_files.len()));
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
        let mut discovered: Vec<DiscoveredTest> = discover_legacy_tests(&db, project);

        // ── 4. Compile + engine + runtime (needed for testset discovery) ──
        reporter.spin("Compiling", format!("{} file(s)", baml_files.len()));
        let compile_options = baml_compiler2_emit::CompileOptions {
            emit_test_cases: true,
        };
        let bytecode = baml_compiler2_emit::generate_project_bytecode_with_prefix(
            &db,
            &compile_options,
            baml_compiler2_emit::OptLevel::Two,
            baml_builtins2_prebuilt::stdlib_prefix(),
        )
        .map_err(|e| anyhow!("Compilation failed: {e:?}"))?;

        let engine = Arc::new(
            BexEngine::new(bytecode, Arc::new(sys_native::SysOps::native()), Vec::new())
                .map_err(|e| anyhow!("Failed to create engine: {e:?}"))?,
        );
        let rt = tokio::runtime::Runtime::new().context("Failed to create tokio runtime")?;
        let cancel = CancellationToken::new();

        // ── 5. Discover testset tests via the engine ───────────────────────
        reporter.spin("Discovering", "tests");
        let (registry_value, testset_discovered) =
            match discover_testset_tests(&engine, &rt, cancel.clone()) {
                Ok(x) => x,
                Err(e) => {
                    reporter.warning(format_args!("testset discovery failed: {e}"));
                    (None, Vec::new())
                }
            };
        discovered.extend(testset_discovered);

        // ── 6. Filter ──────────────────────────────────────────────────────
        let filter = TestFilter::new(
            self.include.iter().map(|s| s.as_str()),
            self.exclude.iter().map(|s| s.as_str()),
        );
        let has_filters = !self.include.is_empty() || !self.exclude.is_empty();
        let selected: Vec<DiscoveredTest> = discovered
            .into_iter()
            .filter(|t| filter.includes(&t.function_name, &t.test_name))
            .collect();

        if selected.is_empty() {
            reporter.finish("Finished", "no tests selected");
            return Ok(crate::ExitCode::NoTestsRun);
        }

        if self.list {
            reporter.status("Selected", format!("{} test(s)", selected.len()));
            // Indented list under the cargo-style status line. These
            // are content (the actual list), not status updates, so
            // they go to stdout as plain prints — the reporter only
            // owns the prefixed status lines above/below.
            #[allow(clippy::print_stdout)]
            for t in &selected {
                println!(
                    "  {}::{}  ({})",
                    t.function_name,
                    t.test_name,
                    t.display_location()
                );
            }
            return Ok(crate::ExitCode::Success);
        }

        // ── 7. Execute ─────────────────────────────────────────────────────
        let aggregate_new_style = !has_filters
            && selected
                .iter()
                .any(|t| matches!(&t.kind, TestKind::Testset { .. }));
        let mut total = if aggregate_new_style {
            selected
                .iter()
                .filter(|t| matches!(&t.kind, TestKind::Legacy { .. }))
                .count()
        } else {
            selected.len()
        };
        let mut passed = 0usize;
        let mut failed = 0usize;
        let mut command_failed = false;

        let run_ctx = RunCtx {
            engine: &engine,
            rt: &rt,
            cancel: &cancel,
        };

        // Test execution writes per-test PASS/FAIL lines to stdout
        // through the run_legacy_test / run_testset_test helpers.
        // Clear the spinner so those lines don't fight with the ticks.
        reporter.abandon();

        for t in &selected {
            match &t.kind {
                TestKind::Legacy { .. } => {
                    let failed_before = failed;
                    run_legacy_test(&run_ctx, t, &mut passed, &mut failed);
                    command_failed |= failed > failed_before;
                }
                TestKind::Testset { full_path } => {
                    if aggregate_new_style {
                        continue;
                    }
                    let Some(reg_value) = registry_value.as_ref() else {
                        eprintln!(
                            "FAIL {}::{} - testset registry unavailable",
                            t.function_name, t.test_name
                        );
                        failed += 1;
                        command_failed = true;
                        continue;
                    };
                    let failed_before = failed;
                    run_testset_test(&run_ctx, reg_value, full_path, t, &mut passed, &mut failed);
                    command_failed |= failed > failed_before;
                }
            }
        }

        if aggregate_new_style {
            let Some(reg_value) = registry_value.as_ref() else {
                eprintln!("FAIL testing::* - testset registry unavailable");
                failed += 1;
                total += 1;
                let summary = format!("{passed} passed, {failed} failed, {total} total");
                crate::reporter::print_error(format_args!("test failures — {summary}"));
                return Ok(crate::ExitCode::TestFailure);
            };
            command_failed |=
                run_testset_registry(&run_ctx, reg_value, &mut passed, &mut failed, &mut total);
        }

        let summary = format!("{passed} passed, {failed} failed, {total} total");
        if command_failed {
            // `reporter.finish` styles success — print as an error so
            // the bold-red `Error:` carries the visual weight of "tests
            // failed" instead of dressing it up as a clean finish.
            crate::reporter::print_error(format_args!("test failures — {summary}"));
            Ok(crate::ExitCode::TestFailure)
        } else {
            reporter.finish("Finished", summary);
            Ok(crate::ExitCode::Success)
        }
    }
}

// ---------------------------------------------------------------------------
// Legacy test execution (unchanged from the original implementation)
// ---------------------------------------------------------------------------

/// Execute one legacy (`function + test block`) test case.
///
/// Parameters:
/// - `ctx`: Shared runtime context containing engine/runtime/cancellation state.
/// - `t`: Discovered test metadata (`function_name::test_name`) to execute.
/// - `passed`: Counter incremented when the test passes.
/// - `failed`: Counter incremented when the test fails or cannot execute.
///
/// Returns:
/// - `()`; results are emitted to stdout/stderr and reflected in counters.
///
/// Errors/Panics:
/// - Does not return errors; execution/argument failures are reported as `FAIL`.
/// - Does not panic under normal operation.
fn run_legacy_test(ctx: &RunCtx, t: &DiscoveredTest, passed: &mut usize, failed: &mut usize) {
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
// Legacy test discovery (unchanged, renamed from discover_tests)
// ---------------------------------------------------------------------------

#[allow(unused_variables)]
fn discover_legacy_tests(
    db: &ProjectDatabase,
    project: baml_workspace::Project,
) -> Vec<DiscoveredTest> {
    use baml_db::baml_compiler2_hir;

    let mut tests = Vec::new();

    for source_file in db.get_source_files() {
        let item_tree = baml_compiler2_hir::file_item_tree(db, source_file);
        let file_path = source_file.path(db);

        for test in item_tree.tests.values() {
            for func_ref in &test.function_refs {
                tests.push(DiscoveredTest {
                    function_name: func_ref.to_string(),
                    test_name: test.name.to_string(),
                    kind: TestKind::Legacy {
                        file_path: file_path.clone(),
                    },
                });
            }
        }
    }

    tests
}

// ---------------------------------------------------------------------------
// Testset discovery via BexEngine::collect_tests
//
// Mirrors what the LSP does when the playground asks for a project's test
// tree (bex_project::bex_lsp::multi_project::mod.rs:609). The sequence:
//
//   1. engine.collect_tests("user", …)
//        → `Handle` to a live `testing.TestRegistry`, or `Null` if the
//          project has no `$init_test` (i.e. no tests at all).
//   2. Repeatedly `serialize` + `expand_set` each encountered `lazyTestSet`
//        until no lazies remain. The CLI has no interactive UI so we
//        auto-expand eagerly; this executes any generator bodies (for-loops
//        inside testsets etc.) once at discovery time.
//   3. Final `serialize` → walk the tree and collect every leaf `type:"test"`
//        node's `name` (already slash-prefixed by the runtime's
//        register_test logic, see baml_std/testing/registry.baml:27).
//
// Execution then calls `TestRegistry.run_test(registry, full_path)` which
// navigates through the expansions map and runs the test body.
// ---------------------------------------------------------------------------

fn discover_testset_tests(
    engine: &Arc<BexEngine>,
    rt: &tokio::runtime::Runtime,
    cancel: CancellationToken,
) -> Result<(Option<BexExternalValue>, Vec<DiscoveredTest>)> {
    let registry = rt
        .block_on(engine.collect_tests("user", CallId::next(), cancel.clone()))
        .map_err(|e| anyhow!("collect_tests failed: {e:?}"))?;

    match &registry {
        BexExternalValue::Null => return Ok((None, Vec::new())),
        BexExternalValue::Handle(_) => {}
        other => {
            return Err(anyhow!(
                "unexpected collect_tests result type: {}",
                other.type_name()
            ));
        }
    }

    // Keep expanding lazy testsets until the tree is fully realized.
    // Each expansion may surface new lazies nested inside, so loop.
    loop {
        let serialized = serialize_registry(engine, rt, &cancel, &registry)?;
        let lazies = collect_lazy_names(&serialized);
        if lazies.is_empty() {
            break;
        }
        for name in lazies {
            let ctx = FunctionCallContextBuilder::new(CallId::next())
                .with_cancel_token(cancel.clone())
                .build();
            rt.block_on(engine.call_function(
                "testing.TestRegistry.expand_set",
                vec![
                    registry.clone(),
                    BexExternalValue::String(name.as_str().into()),
                ],
                ctx,
                true,
            ))
            .map_err(|e| anyhow!("expand_set({name:?}) failed: {e:?}"))?;
        }
    }

    let final_tree = serialize_registry(engine, rt, &cancel, &registry)?;
    let mut tests = Vec::new();
    flatten_tree(&final_tree, &mut tests);
    Ok((Some(registry), tests))
}

fn serialize_registry(
    engine: &Arc<BexEngine>,
    rt: &tokio::runtime::Runtime,
    cancel: &CancellationToken,
    registry: &BexExternalValue,
) -> Result<BexExternalValue> {
    let ctx = FunctionCallContextBuilder::new(CallId::next())
        .with_cancel_token(cancel.clone())
        .build();
    rt.block_on(engine.call_function(
        "testing.TestRegistry.serialize",
        vec![registry.clone()],
        ctx,
        true,
    ))
    .map_err(|e| anyhow!("TestRegistry.serialize failed: {e:?}"))
}

// ---------------------------------------------------------------------------
// Tree walkers over the `SerializedTestDef[]` shape produced by
// `testing.TestRegistry.serialize`. See baml_std/testing/registry.baml:99.
// ---------------------------------------------------------------------------

fn flatten_tree(value: &BexExternalValue, out: &mut Vec<DiscoveredTest>) {
    match unwrap_union(value) {
        BexExternalValue::Array { items, .. } => {
            for item in items {
                flatten_tree(item, out);
            }
        }
        BexExternalValue::Instance {
            class_name, fields, ..
        } => {
            if class_matches(class_name, "SerializedTest") {
                let kind = fields.get("type").and_then(as_string).unwrap_or_default();
                if kind == "test" {
                    if let Some(name) = fields.get("name").and_then(as_string) {
                        let (func, test) = split_top(name);
                        out.push(DiscoveredTest {
                            function_name: func,
                            test_name: test,
                            kind: TestKind::Testset {
                                full_path: name.to_string(),
                            },
                        });
                    }
                }
                // `lazyTestSet` entries are expected to have been expanded
                // before flattening; they contribute no tests of their own.
            } else if class_matches(class_name, "SerializedTestSet") {
                if let Some(items) = fields.get("items") {
                    flatten_tree(items, out);
                }
            }
        }
        _ => {}
    }
}

fn collect_lazy_names(value: &BexExternalValue) -> Vec<String> {
    let mut out = Vec::new();
    collect_lazy_names_inner(value, &mut out);
    out
}

fn collect_lazy_names_inner(value: &BexExternalValue, out: &mut Vec<String>) {
    match unwrap_union(value) {
        BexExternalValue::Array { items, .. } => {
            for item in items {
                collect_lazy_names_inner(item, out);
            }
        }
        BexExternalValue::Instance {
            class_name, fields, ..
        } => {
            if class_matches(class_name, "SerializedTest") {
                let kind = fields.get("type").and_then(as_string).unwrap_or_default();
                if kind == "lazyTestSet" {
                    if let Some(name) = fields.get("name").and_then(as_string) {
                        out.push(name.to_string());
                    }
                }
            } else if class_matches(class_name, "SerializedTestSet") {
                if let Some(items) = fields.get("items") {
                    collect_lazy_names_inner(items, out);
                }
            }
        }
        _ => {}
    }
}

// ---------------------------------------------------------------------------
// Testset test execution
// ---------------------------------------------------------------------------

/// Execute one test discovered from the testset runtime registry.
///
/// Parameters:
/// - `ctx`: Shared runtime context containing engine/runtime/cancellation state.
/// - `registry`: Live `testing.TestRegistry` handle/value returned by discovery.
/// - `full_path`: Slash-delimited runtime path for the test (e.g. `suite/case`).
/// - `t`: Human-facing discovered test metadata used for CLI labels.
/// - `passed`: Counter incremented when the test passes.
/// - `failed`: Counter incremented when the test fails or errors.
///
/// Returns:
/// - `()`; prints pass/fail lines and updates counters.
///
/// Errors/Panics:
/// - Does not return errors; runtime failures are rendered as `FAIL`.
/// - Does not panic under normal operation.
fn run_testset_test(
    ctx: &RunCtx,
    registry: &BexExternalValue,
    full_path: &str,
    t: &DiscoveredTest,
    passed: &mut usize,
    failed: &mut usize,
) {
    let call_ctx = FunctionCallContextBuilder::new(CallId::next())
        .with_cancel_token(ctx.cancel.clone())
        .build();
    match ctx.rt.block_on(ctx.engine.call_function(
        "testing.TestRegistry.run_test",
        vec![
            registry.clone(),
            BexExternalValue::String(full_path.to_string().into()),
        ],
        call_ctx,
        true,
    )) {
        Ok(report) => {
            let outcome = extract_outcome(&report).unwrap_or_else(|| "unknown".to_string());
            if outcome == "pass" {
                println!("PASS {}::{}", t.function_name, t.test_name);
                *passed += 1;
            } else {
                println!(
                    "FAIL {}::{} [outcome={outcome}]",
                    t.function_name, t.test_name
                );
                print_failure_messages(&report);
                *failed += 1;
            }
        }
        Err(e) => {
            eprintln!("FAIL {}::{}", t.function_name, t.test_name);
            eprintln!("  => {e}");
            *failed += 1;
        }
    }
}

fn run_testset_registry(
    ctx: &RunCtx,
    registry: &BexExternalValue,
    passed: &mut usize,
    failed: &mut usize,
    total: &mut usize,
) -> bool {
    let call_ctx = FunctionCallContextBuilder::new(CallId::next())
        .with_cancel_token(ctx.cancel.clone())
        .build();
    match ctx.rt.block_on(ctx.engine.call_function(
        "testing.TestRegistry.run_all",
        vec![registry.clone()],
        call_ctx,
        true,
    )) {
        Ok(report) => {
            let outcome = extract_outcome(&report).unwrap_or_else(|| "unknown".to_string());
            let aggregate_failed = outcome != "pass";
            if let Some((report_passed, report_failed, report_total)) =
                extract_leaf_summary(&report).or_else(|| {
                    extract_testset_summary(&report)
                        .map(|(_, passed, failed, total)| (passed, failed, total))
                })
            {
                *passed += report_passed;
                *failed += report_failed;
                *total += report_total;
                if !aggregate_failed {
                    println!("PASS testing::*");
                } else {
                    println!("FAIL testing::* [outcome={outcome}]");
                    print_failed_names(&report);
                    print_failure_messages(&report);
                    // A testset runner can fail the aggregate without marking
                    // individual children failed. Count that runner-level
                    // verdict as one displayed failure so the summary does not
                    // say "0 failed" when the aggregate itself failed.
                    if report_failed == 0 {
                        *failed += 1;
                        *total += 1;
                    }
                }
                aggregate_failed
            } else {
                *total += 1;
                if !aggregate_failed {
                    println!("PASS testing::*");
                    *passed += 1;
                } else {
                    println!("FAIL testing::* [outcome={outcome}]");
                    print_failed_names(&report);
                    print_failure_messages(&report);
                    *failed += 1;
                }
                aggregate_failed
            }
        }
        Err(e) => {
            eprintln!("FAIL testing::*");
            eprintln!("  => {e:?}");
            *failed += 1;
            *total += 1;
            true
        }
    }
}

/// Pull `TestReport.outcome` out of the returned value.
///
/// The runtime's `run_test` (baml_std/testing/registry.baml:172) returns a
/// `TestReport { outcome, runs }` where `outcome` is `"pass" | "fail" |
/// "error"`. Union-literal types may arrive wrapped; unwrap defensively.
fn extract_outcome(value: &BexExternalValue) -> Option<String> {
    let v = unwrap_union(value);
    if let BexExternalValue::Instance { fields, .. } = v {
        if let Some(outcome) = fields.get("outcome") {
            return as_string(unwrap_union(outcome)).map(str::to_string);
        }
    }
    None
}

fn extract_testset_summary(value: &BexExternalValue) -> Option<(String, usize, usize, usize)> {
    let v = unwrap_union(value);
    if let BexExternalValue::Instance {
        class_name, fields, ..
    } = v
    {
        if !class_matches(class_name, "TestSetReport") {
            return None;
        }
        let outcome = fields
            .get("outcome")
            .and_then(|v| as_string(unwrap_union(v)))?
            .to_string();
        let passed = fields.get("passed").and_then(as_usize)?;
        let failed = fields.get("failed").and_then(as_usize)?;
        let total = fields.get("total").and_then(as_usize)?;
        Some((outcome, passed, failed, total))
    } else {
        None
    }
}

fn extract_leaf_summary(value: &BexExternalValue) -> Option<(usize, usize, usize)> {
    let v = unwrap_union(value);
    if let BexExternalValue::Instance {
        class_name, fields, ..
    } = v
    {
        if class_matches(class_name, "TestReport") {
            let outcome = fields
                .get("outcome")
                .and_then(|v| as_string(unwrap_union(v)))?;
            return Some(if outcome == "pass" {
                (1, 0, 1)
            } else {
                (0, 1, 1)
            });
        }

        if class_matches(class_name, "TestSetReport") {
            if let Some(results) = fields.get("results") {
                if let BexExternalValue::Array { items, .. } = unwrap_union(results) {
                    if !items.is_empty() {
                        let mut passed = 0usize;
                        let mut failed = 0usize;
                        let mut total = 0usize;
                        for item in items {
                            let (child_passed, child_failed, child_total) =
                                extract_leaf_summary(item)?;
                            passed += child_passed;
                            failed += child_failed;
                            total += child_total;
                        }
                        return Some((passed, failed, total));
                    }
                }
            }
            return extract_testset_summary(value)
                .map(|(_, passed, failed, total)| (passed, failed, total));
        }
    }
    None
}

fn print_failed_names(value: &BexExternalValue) {
    for name in extract_failed_names(value) {
        println!("  failed: {name}");
    }
}

fn extract_failed_names(value: &BexExternalValue) -> Vec<String> {
    let v = unwrap_union(value);
    if let BexExternalValue::Instance { fields, .. } = v {
        if let Some(names) = fields.get("failed_names") {
            if let BexExternalValue::Array { items, .. } = unwrap_union(names) {
                return items
                    .iter()
                    .filter_map(|item| as_string(unwrap_union(item)).map(str::to_string))
                    .collect();
            }
        }
    }
    Vec::new()
}

fn print_failure_messages(value: &BexExternalValue) {
    for message in extract_failure_messages(value) {
        eprintln!("  => {message}");
    }
}

fn extract_failure_messages(value: &BexExternalValue) -> Vec<String> {
    let mut messages = Vec::new();
    collect_failure_messages(value, &mut messages);
    messages
}

fn collect_failure_messages(value: &BexExternalValue, messages: &mut Vec<String>) {
    let v = unwrap_union(value);
    match v {
        BexExternalValue::Array { items, .. } => {
            for item in items {
                collect_failure_messages(item, messages);
            }
        }
        BexExternalValue::Instance {
            class_name, fields, ..
        } => {
            if class_matches(class_name, "RunReport")
                && fields
                    .get("outcome")
                    .and_then(|v| as_string(unwrap_union(v)))
                    .is_some_and(|outcome| outcome != "pass")
            {
                if let Some(message) = fields
                    .get("message")
                    .and_then(|v| as_string(unwrap_union(v)))
                {
                    messages.push(message.to_string());
                }
            }
            if let Some(runs) = fields.get("runs") {
                collect_failure_messages(runs, messages);
            }
            if let Some(results) = fields.get("results") {
                collect_failure_messages(results, messages);
            }
        }
        _ => {}
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Strip a leading `Union { value, … }` wrapper. Union-typed fields (e.g.
/// `SerializedTestDef = SerializedTest | SerializedTestSet`) come back
/// wrapped when deep-copied across the FFI boundary.
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

/// Match the last segment of a namespaced class name, ignoring any leading
/// `testing.` / `user.` / similar prefix. `testing.SerializedTest` and
/// `SerializedTest` both match `"SerializedTest"`.
fn class_matches(class_name: &str, leaf: &str) -> bool {
    class_name == leaf
        || class_name
            .rsplit_once('.')
            .is_some_and(|(_, last)| last == leaf)
}

/// Split a testset's slash-separated path into (first-segment, rest). For
/// `"tictactoe/x wins row"` → ("tictactoe", "x wins row"). For a path with
/// no slash (shouldn't happen for testset tests but guard anyway) → ("",
/// whole-thing).
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

    fn test_report(outcome: &str) -> BexExternalValue {
        instance(
            "testing.TestReport",
            vec![("outcome", string(outcome)), ("runs", array(Vec::new()))],
        )
    }

    fn run_report(outcome: &str, message: Option<&str>) -> BexExternalValue {
        instance(
            "testing.RunReport",
            vec![
                ("outcome", string(outcome)),
                ("duration_ms", int(0)),
                (
                    "message",
                    message
                        .map(string)
                        .unwrap_or_else(|| BexExternalValue::Null),
                ),
            ],
        )
    }

    fn test_report_with_runs(outcome: &str, runs: Vec<BexExternalValue>) -> BexExternalValue {
        instance(
            "testing.TestReport",
            vec![("outcome", string(outcome)), ("runs", array(runs))],
        )
    }

    fn testset_report(
        outcome: &str,
        passed: i64,
        failed: i64,
        total: i64,
        failed_names: Vec<BexExternalValue>,
        results: Vec<BexExternalValue>,
    ) -> BexExternalValue {
        instance(
            "testing.TestSetReport",
            vec![
                ("outcome", string(outcome)),
                ("passed", int(passed)),
                ("failed", int(failed)),
                ("total", int(total)),
                ("failed_names", array(failed_names)),
                ("results", array(results)),
            ],
        )
    }

    #[test]
    fn extract_leaf_summary_counts_test_reports() {
        assert_eq!(extract_leaf_summary(&test_report("pass")), Some((1, 0, 1)));
        assert_eq!(extract_leaf_summary(&test_report("fail")), Some((0, 1, 1)));
        assert_eq!(extract_leaf_summary(&test_report("error")), Some((0, 1, 1)));
    }

    #[test]
    fn extract_leaf_summary_recurses_over_testset_results() {
        let nested = testset_report(
            "fail",
            99,
            99,
            99,
            Vec::new(),
            vec![test_report("fail"), test_report("error")],
        );
        let root = testset_report(
            "fail",
            99,
            99,
            99,
            Vec::new(),
            vec![test_report("pass"), nested],
        );

        assert_eq!(extract_leaf_summary(&root), Some((1, 2, 3)));
    }

    #[test]
    fn extract_leaf_summary_falls_back_to_testset_totals_without_results() {
        let report = testset_report("fail", 2, 1, 3, Vec::new(), Vec::new());

        assert_eq!(extract_leaf_summary(&report), Some((2, 1, 3)));
    }

    #[test]
    fn extract_failed_names_reads_string_names_and_unwraps_unions() {
        let wrapped = BexExternalValue::union(
            string("suite/three"),
            [RuntimeTy::string(), RuntimeTy::int()],
            RuntimeTy::string(),
        );
        let report = testset_report(
            "fail",
            1,
            2,
            3,
            vec![string("suite/two"), int(42), wrapped],
            Vec::new(),
        );

        assert_eq!(
            extract_failed_names(&report),
            vec!["suite/two".to_string(), "suite/three".to_string()]
        );
    }

    #[test]
    fn extract_failed_names_returns_empty_for_missing_or_malformed_names() {
        assert!(extract_failed_names(&test_report("fail")).is_empty());

        let report = instance(
            "testing.TestSetReport",
            vec![("failed_names", string("suite/two"))],
        );
        assert!(extract_failed_names(&report).is_empty());
    }

    #[test]
    fn extract_failure_messages_recurses_into_failed_child_runs() {
        let report = testset_report(
            "fail",
            0,
            1,
            1,
            Vec::new(),
            vec![test_report_with_runs(
                "fail",
                vec![
                    run_report("pass", Some("ignored")),
                    run_report("fail", Some("assertion failed: expected true")),
                    run_report("error", None),
                ],
            )],
        );

        assert_eq!(
            extract_failure_messages(&report),
            vec!["assertion failed: expected true".to_string()]
        );
    }
}
