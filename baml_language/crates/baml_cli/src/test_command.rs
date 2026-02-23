#![allow(clippy::print_stdout, clippy::print_stderr)]

use std::{collections::BTreeMap, path::PathBuf, sync::Arc};

use anyhow::{Context, Result, anyhow};
use baml_db::{
    baml_compiler_diagnostics::Severity,
    baml_compiler_emit,
    baml_compiler_hir::{ItemId, file_item_tree, project_items},
};
use baml_project::ProjectDatabase;
use baml_workspace::discover_baml_files;
use bex_engine::{BexEngine, BexExternalValue, FunctionCallContextBuilder, test_arg_to_external};
use clap::Args;
use sys_native::{CallId, SysOpsExt};

use crate::test_filter::TestFilter;

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

/// A discovered test: a (function_name, test_name) pair with its source location.
struct DiscoveredTest {
    function_name: String,
    test_name: String,
    file_path: PathBuf,
}

impl TestArgs {
    pub fn run(&self) -> Result<crate::ExitCode> {
        let from = std::fs::canonicalize(&self.from)
            .with_context(|| format!("Could not resolve baml_src path: {}", self.from.display()))?;

        // Set up the compiler database and load all .baml files.
        let mut db = ProjectDatabase::new();
        let project = db.set_project_root(&from);
        let baml_files = discover_baml_files(&from);
        if baml_files.is_empty() {
            eprintln!("No .baml files found in {}", from.display());
            return Ok(crate::ExitCode::NoTestsRun);
        }

        for file_path in &baml_files {
            let content = std::fs::read_to_string(file_path)
                .with_context(|| format!("Failed to read {}", file_path.display()))?;
            db.add_or_update_file(file_path, &content);
        }

        // Discover all (function, test) pairs from the HIR.
        let discovered = discover_tests(&db, project);

        // Apply include/exclude filters.
        let filter = TestFilter::new(
            self.include.iter().map(|s| s.as_str()),
            self.exclude.iter().map(|s| s.as_str()),
        );

        let selected: BTreeMap<(String, String), PathBuf> = discovered
            .into_iter()
            .filter(|t| filter.includes(&t.function_name, &t.test_name))
            .map(|t| ((t.function_name, t.test_name), t.file_path))
            .collect();

        if selected.is_empty() {
            println!("No tests selected.");
            return Ok(crate::ExitCode::NoTestsRun);
        }

        if self.list {
            println!("Selected tests ({}):\n", selected.len());
            for ((func, test), path) in &selected {
                println!("  {func}::{test}  ({path})", path = path.display());
            }
            return Ok(crate::ExitCode::Success);
        }

        // Check for diagnostic errors before compiling.
        let source_files = db.get_source_files();
        let diagnostics = baml_project::collect_diagnostics(&db, project, &source_files);
        let errors: Vec<_> = diagnostics
            .iter()
            .filter(|d| d.severity == Severity::Error)
            .collect();
        if !errors.is_empty() {
            eprintln!("Compilation errors found ({}):", errors.len());
            for diag in &errors {
                eprintln!("  error: {}", diag.message);
            }
            return Ok(crate::ExitCode::Other);
        }

        // Compile to bytecode (with test cases included).
        let compile_options = baml_compiler_emit::CompileOptions {
            emit_test_cases: true,
        };
        let bytecode = baml_compiler_emit::generate_project_bytecode(&db, &compile_options)
            .map_err(|e| anyhow!("Compilation failed: {e:?}"))?;

        // Create the engine with native (tokio-based) sys ops.
        let engine = BexEngine::new(bytecode, Arc::new(sys_native::SysOps::native()), None)
            .map_err(|e| anyhow!("Failed to create engine: {e:?}"))?;

        // Create a tokio runtime for async execution.
        let rt = tokio::runtime::Runtime::new().context("Failed to create tokio runtime")?;

        // Run tests sequentially.
        let total = selected.len();
        let mut passed = 0usize;
        let mut failed = 0usize;

        for (func_name, test_name) in selected.keys() {
            // Look up the compiled test case from the engine.
            let test_case = match engine.test_case(func_name, test_name) {
                Some(tc) => tc,
                None => {
                    eprintln!(
                        "FAIL {func_name}::{test_name} - test case not found in compiled program"
                    );
                    failed += 1;
                    continue;
                }
            };

            // Convert TestArgValue -> BexExternalValue and order by function params.
            let ordered_args = match build_ordered_args(&engine, func_name, test_case) {
                Ok(args) => args,
                Err(e) => {
                    eprintln!("FAIL {func_name}::{test_name} - {e}");
                    failed += 1;
                    continue;
                }
            };

            // Execute the function.
            match rt.block_on(engine.call_function(
                func_name,
                ordered_args,
                FunctionCallContextBuilder::new(CallId::next()).build(),
            )) {
                Ok(result) => {
                    println!("PASS {func_name}::{test_name}");
                    println!("  => {result:?}");
                    passed += 1;
                }
                Err(e) => {
                    eprintln!("FAIL {func_name}::{test_name}");
                    eprintln!("  => {e:?}");
                    failed += 1;
                }
            }
        }

        println!("\nResults: {passed} passed, {failed} failed, {total} total");

        if failed > 0 {
            Ok(crate::ExitCode::TestFailure)
        } else {
            Ok(crate::ExitCode::Success)
        }
    }
}

/// Build ordered args Vec from a test case, matching the function's parameter order.
fn build_ordered_args(
    engine: &BexEngine,
    function_name: &str,
    test_case: &bex_vm_types::TestCase,
) -> Result<Vec<BexExternalValue>> {
    let params = engine
        .function_params(function_name)
        .map_err(|e| anyhow!("failed to get params for {function_name}: {e:?}"))?;

    let ordered: Vec<BexExternalValue> = params
        .into_iter()
        .map(|(name, _ty)| {
            test_case
                .args
                .get(name)
                .map(test_arg_to_external)
                .ok_or_else(|| anyhow!("missing argument '{name}' for function {function_name}"))
        })
        .collect::<Result<_>>()?;

    Ok(ordered)
}

/// Walk the HIR to discover all (function_name, test_name) pairs.
///
/// Each test block references one or more functions via `function_refs`.
/// We expand each test into one entry per referenced function, matching
/// the old engine's `walk_function_test_pairs` behavior.
fn discover_tests(db: &ProjectDatabase, project: baml_workspace::Project) -> Vec<DiscoveredTest> {
    let items = project_items(db, project);
    let mut tests = Vec::new();

    for item in items.items(db) {
        if let ItemId::Test(test_loc) = item {
            let file = test_loc.file(db);
            let item_tree = file_item_tree(db, file);
            let test = &item_tree[test_loc.id(db)];
            let file_path = file.path(db);

            for func_ref in &test.function_refs {
                tests.push(DiscoveredTest {
                    function_name: func_ref.to_string(),
                    test_name: test.name.to_string(),
                    file_path: file_path.clone(),
                });
            }
        }
    }

    tests
}
