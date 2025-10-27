//! Executor evaluation tests.
//!
//! These tests evaluate BAML expressions and compare the results to expected outputs.
//! Test files are in the `executor_tests/` directory and use the format:
//!
//! ```baml
//! function Foo() -> int {
//!   1 + 2
//! }
//!
//! //> Foo()
//! //
//! // 3
//! ```
//!
//! The first comment starting with `>` contains the expression to evaluate,
//! and subsequent comments contain the expected pretty-printed result.

use std::path::PathBuf;

use anyhow::{Context, Result};
use baml_types::BamlValue;

// Use the CoreRuntime pattern: select implementation based on feature flag
#[cfg(feature = "thir-interpreter")]
type TestRuntime = ThirInterpreterRuntime;
#[cfg(not(feature = "thir-interpreter"))]
type TestRuntime = VmRuntime;

/// Test case parsed from a .baml file
#[derive(Debug)]
struct InterpreterTest {
    /// The BAML source code
    source: String,
    /// Expression to evaluate (from comment starting with `>`)
    expr: String,
    /// Expected output (from subsequent comments)
    expected: String,
    /// File path for error reporting
    file_path: PathBuf,
}

/// Parse a test file into test cases
fn parse_test_file(path: &PathBuf) -> Result<Vec<InterpreterTest>> {
    let source = std::fs::read_to_string(path)
        .with_context(|| format!("Failed to read test file: {}", path.display()))?;

    let mut tests = Vec::new();
    let mut current_expr: Option<String> = None;
    let mut current_expected = Vec::new();
    let mut baml_source = String::new();
    let mut in_comment_block = false;

    for line in source.lines() {
        let trimmed = line.trim();

        if trimmed.starts_with("//>") {
            // Start of a new test expression
            if let Some(expr) = current_expr.take() {
                // Save previous test
                tests.push(InterpreterTest {
                    source: baml_source.clone(),
                    expr,
                    expected: current_expected.join("\n"),
                    file_path: path.clone(),
                });
                current_expected.clear();
            }

            current_expr = Some(trimmed.trim_start_matches("//>").trim().to_string());
            in_comment_block = true;
        } else if in_comment_block && trimmed.starts_with("//") {
            // Continuation of expected output
            let content = trimmed.trim_start_matches("//").trim_start();
            current_expected.push(content.to_string());
        } else if in_comment_block && trimmed.is_empty() {
            // Empty line might be part of expected output
            current_expected.push(String::new());
        } else if !trimmed.starts_with("//") {
            // Not a comment, back to source code
            in_comment_block = false;
            baml_source.push_str(line);
            baml_source.push('\n');
        }
    }

    // Don't forget the last test
    if let Some(expr) = current_expr {
        tests.push(InterpreterTest {
            source: baml_source.clone(),
            expr,
            expected: current_expected.join("\n"),
            file_path: path.clone(),
        });
    }

    Ok(tests)
}

/// Test result including value and watch notifications
#[derive(Debug)]
struct TestResult {
    value: BamlValue,
    watch_notifications: Vec<String>,
}

/// Run interpreter tests using THIR interpreter
#[cfg(feature = "thir-interpreter")]
struct ThirInterpreterRuntime;

#[cfg(feature = "thir-interpreter")]
impl ThirInterpreterRuntime {
    fn eval_expr(source: &str, expr: &str) -> Result<TestResult> {
        use std::sync::{Arc, Mutex};

        use baml_compiler::{
            thir::{interpret::interpret_thir, typecheck::typecheck},
            watch::{SharedWatchHandler, WatchNotification},
        };
        use internal_baml_ast::parse_standalone_expression;
        use internal_baml_diagnostics::{Diagnostics, SourceFile};

        // Parse and compile the BAML source
        // Convert to owned string to satisfy 'static lifetime requirement
        let source_owned = source.to_string();
        let source_static: &'static str = Box::leak(source_owned.into_boxed_str());
        let ast = baml_compiler::test::ast(source_static)?;
        let hir = baml_compiler::test::hir(&ast)?;
        let (thir, _diagnostics) = typecheck(&hir)?;

        // Parse the expression to evaluate
        let mut diagnostics = Diagnostics::new(SourceFile::new_static(expr));
        let hir_expr = parse_standalone_expression(expr, &mut diagnostics)?;
        let thir_expr = baml_compiler::thir::typecheck::typecheck_expression(
            &hir_expr,
            &baml_compiler::thir::typecheck::TypeContext::new(&hir),
            &mut diagnostics,
        );

        // Collect watch notifications
        let notifications = Arc::new(Mutex::new(Vec::new()));
        let notifications_clone = notifications.clone();

        let watch_handler = SharedWatchHandler::new(move |notification: WatchNotification| {
            let mut notifs = notifications_clone.lock().unwrap();
            // Format the notification
            let formatted = format!(
                "[watch] {} = {}",
                notification.variable_name,
                format!("{:#?}", notification.value).trim()
            );
            notifs.push(formatted);
        });

        // Interpret the expression (no LLM calls in tests)
        let result = tokio::runtime::Runtime::new()?.block_on(async {
            interpret_thir(
                &thir,
                &thir_expr,
                |_name, _args, _ctx| {
                    Box::pin(async {
                        anyhow::bail!("LLM calls not supported in interpreter tests")
                    })
                },
                Some(watch_handler),
                None, // No function name
            )
            .await
        })?;

        // Extract collected notifications
        let watch_notifications = notifications.lock().unwrap().clone();

        // Convert to BamlValue
        let value = baml_types::baml_value_with_meta_to_baml_value(result);

        Ok(TestResult {
            value,
            watch_notifications,
        })
    }
}

/// Run interpreter tests using VM
#[cfg(not(feature = "thir-interpreter"))]
struct VmRuntime;

#[cfg(not(feature = "thir-interpreter"))]
impl VmRuntime {
    fn eval_expr(source: &str, expr: &str) -> Result<TestResult> {
        use baml_vm::{BamlVmProgram, EvalStack};

        // Compile to VM
        // Convert to owned string to satisfy 'static lifetime requirement
        let source_owned = source.to_string();
        let source_static: &'static str = Box::leak(source_owned.into_boxed_str());
        let ast = baml_compiler::test::ast(source_static)?;
        let program = baml_compiler::compile(&ast)?;

        // For now, VM tests would need additional implementation
        // to evaluate arbitrary expressions
        anyhow::bail!("VM expression evaluation not yet implemented for tests")
    }
}

/// Run a single interpreter test
fn run_test(test: &InterpreterTest) -> Result<()> {
    let result = TestRuntime::eval_expr(&test.source, &test.expr)?;

    // Build actual output: value + watch notifications
    let mut actual_lines = Vec::new();

    // Add watch notifications first
    for notification in &result.watch_notifications {
        actual_lines.push(notification.clone());
    }

    // Add separator if there are watch notifications
    if !result.watch_notifications.is_empty() {
        actual_lines.push(String::new());
    }

    // Add the result value
    actual_lines.push(format!("{:#?}", result.value).trim().to_string());

    let actual = actual_lines.join("\n");
    let expected = test.expected.trim();

    if actual != expected {
        anyhow::bail!(
            "Test failed: {}\nExpression: {}\nExpected:\n{}\n\nActual:\n{}",
            test.file_path.display(),
            test.expr,
            expected,
            actual
        );
    }

    Ok(())
}

/// Main test function that discovers and runs all interpreter tests
#[test]
fn executor_tests() -> Result<()> {
    let test_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/executor_tests");

    if !test_dir.exists() {
        // No tests yet, skip
        return Ok(());
    }

    let mut all_tests = Vec::new();

    // Discover all .baml test files
    for entry in std::fs::read_dir(&test_dir)? {
        let entry = entry?;
        let path = entry.path();

        if path.extension().and_then(|s| s.to_str()) == Some("baml") {
            let tests = parse_test_file(&path)?;
            all_tests.extend(tests);
        }
    }

    println!("Running {} interpreter tests", all_tests.len());

    // Run all tests
    for test in &all_tests {
        run_test(test).with_context(|| {
            format!(
                "Test failed in {} for expression: {}",
                test.file_path.display(),
                test.expr
            )
        })?;
    }

    Ok(())
}
