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
    #[arg(long, help = "path/to/baml_src", default_value = ".")]
    pub from: PathBuf,

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
        let (db, from, baml_files) = load_project_from_reporting(&self.from, &reporter)?;
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
        let bytecode = baml_compiler2_emit::generate_project_bytecode(&db, &compile_options)
            .map_err(|e| anyhow!("Compilation failed: {e:?}"))?;

        let engine = Arc::new(
            BexEngine::new(
                bytecode,
                Arc::new(sys_native::SysOps::native()),
                None,
                Vec::new(),
            )
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
        let total = selected.len();
        let mut passed = 0usize;
        let mut failed = 0usize;

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
                    run_legacy_test(&run_ctx, t, &mut passed, &mut failed);
                }
                TestKind::Testset { full_path } => {
                    let Some(reg_value) = registry_value.as_ref() else {
                        eprintln!(
                            "FAIL {}::{} - testset registry unavailable",
                            t.function_name, t.test_name
                        );
                        failed += 1;
                        continue;
                    };
                    run_testset_test(&run_ctx, reg_value, full_path, t, &mut passed, &mut failed);
                }
            }
        }

        let summary = format!("{passed} passed, {failed} failed, {total} total");
        if failed > 0 {
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
            eprintln!("  => {e:?}");
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
                vec![registry.clone(), BexExternalValue::String(name.clone())],
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
        BexExternalValue::Instance { class_name, fields } => {
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
        BexExternalValue::Instance { class_name, fields } => {
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
            BexExternalValue::String(full_path.to_string()),
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
                *failed += 1;
            }
        }
        Err(e) => {
            eprintln!("FAIL {}::{}", t.function_name, t.test_name);
            eprintln!("  => {e:?}");
            *failed += 1;
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
