//! Interpreter evaluation tests.
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
// Use the CoreRuntime pattern: select implementation based on feature flag
#[cfg(feature = "thir-interpreter")]
use baml_runtime::async_interpreter_runtime::BamlAsyncInterpreterRuntime as CoreRuntime;
#[cfg(not(feature = "thir-interpreter"))]
use baml_runtime::async_vm_runtime::BamlAsyncVmRuntime as CoreRuntime;
use baml_types::BamlValue;

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

/// Test result including value and watch notifications
#[derive(Debug)]
struct TestResult {
    value: BamlValue,
    watch_notifications: Vec<String>,
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
            // Remove "//" prefix, keeping all whitespace after it
            let content = trimmed.strip_prefix("//").unwrap_or(trimmed);
            // If there's a leading space after //, remove just one space
            let content = content.strip_prefix(" ").unwrap_or(content);
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

/// Evaluate an expression using the selected runtime
async fn eval_expr(source: &str, expr_to_eval: &str) -> Result<TestResult> {
    use std::{
        collections::HashMap,
        sync::{Arc, Mutex},
    };

    use baml_runtime::BamlSrcReader;
    use baml_types::BamlValue;

    // Collect watch notifications
    let notifications = Arc::new(Mutex::new(Vec::new()));
    let notifications_clone = notifications.clone();

    // Create watch handler using the helper function
    let watch_handler = baml_compiler::watch::shared_handler(
        move |notification: baml_compiler::watch::WatchNotification| {
            let mut notifs = notifications_clone.lock().unwrap();

            // Only handle Value notifications (not Block, StreamStart, etc.)
            if let baml_compiler::watch::WatchBamlValue::Value(value_with_meta) = notification.value
            {
                if let Some(var_name) = notification.variable_name {
                    // Convert BamlValueWithMeta to BamlValue and serialize as JSON
                    let baml_value = value_with_meta.value();
                    let value_str = serde_json::to_string(&baml_value)
                        .unwrap_or_else(|_| format!("{:?}", baml_value));
                    let formatted = format!("[watch] {} = {}", var_name, value_str);
                    notifs.push(formatted);
                }
            }
        },
    );

    // The expression should be a function call like "Add()" or "WithWatch()"
    // Extract the function name from the expression (everything before the opening paren)
    let function_name = expr_to_eval
        .trim()
        .split('(')
        .next()
        .ok_or_else(|| anyhow::anyhow!("Invalid expression: {}", expr_to_eval))?
        .trim()
        .to_string();

    // Create runtime with source - need to pass as HashMap
    let mut files = HashMap::new();
    files.insert("test.baml", source);

    let env_vars: HashMap<String, String> = std::env::vars().collect();

    let core_runtime = CoreRuntime::from_file_content(".", &files, env_vars.clone())?;

    // Create context manager - BamlSrcReader is Option<Box<dyn Fn...>>, so pass None
    let ctx = baml_runtime::RuntimeContextManager::new(None);

    // Call the function directly
    let (result, _call_id) = core_runtime
        .call_function(
            function_name,
            &baml_types::BamlMap::new(),
            &ctx,
            None, // TypeBuilder
            None, // ClientRegistry
            None, // collectors
            env_vars,
            None, // tags
            baml_runtime::TripWire::new(None),
            Some(watch_handler),
        )
        .await;

    let function_result = result?;

    // Extract the BamlValue from FunctionResult
    // result_with_constraints() returns &Option<Result<ResponseBamlValue>>
    let response_value = function_result
        .result_with_constraints()
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("No result returned"))?
        .as_ref()
        .map_err(|e| anyhow::anyhow!("Failed to parse result: {}", e))?;

    // ResponseBamlValue is a wrapper around BamlValueWithMeta, access the inner value with .0
    // Convert BamlValueWithMeta to BamlValue by stripping metadata
    let value: BamlValue = (&response_value.0).into();

    // Extract watch notifications
    let watch_notifications = notifications.lock().unwrap().clone();

    Ok(TestResult {
        value,
        watch_notifications,
    })
}

/// Run a single interpreter test
async fn run_test(test: &InterpreterTest) -> Result<()> {
    let result = eval_expr(&test.source, &test.expr).await?;

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

    // Add the result value - format as JSON-like output to match expected format
    // Use serde_json to serialize the BamlValue
    let value_str = serde_json::to_string_pretty(&result.value)
        .unwrap_or_else(|_| format!("{:#?}", result.value));
    actual_lines.push(value_str);

    let actual = actual_lines.join("\n");
    let expected = test.expected.trim();

    if actual != expected {
        // Check if UPDATE_EXPECT environment variable is set to a truthy value
        let should_update = std::env::var("UPDATE_EXPECT")
            .ok()
            .map(|v| {
                let v = v.to_lowercase();
                // Consider truthy: "1", "true", "yes"
                // Consider falsy: "0", "false", "no", or empty string
                v == "1" || v == "true" || v == "yes"
            })
            .unwrap_or(false);

        if should_update {
            update_test_expectation(test, &actual)?;
            println!("Updated expectation for test: {}", test.file_path.display());
            Ok(())
        } else {
            anyhow::bail!(
                "Test failed: {}\nExpression: {}\nExpected:\n{}\n\nActual:\n{}\n\nRun with UPDATE_EXPECT=1 to update the expectation.",
                test.file_path.display(),
                test.expr,
                expected,
                actual
            );
        }
    } else {
        Ok(())
    }
}

/// Update the test file with new expected output
fn update_test_expectation(test: &InterpreterTest, new_output: &str) -> Result<()> {
    // Read the original file
    let original_content = std::fs::read_to_string(&test.file_path)
        .with_context(|| format!("Failed to read test file: {}", test.file_path.display()))?;

    // Build the new content by replacing the expected output section
    let mut new_content = String::new();
    let mut lines = original_content.lines();
    let mut in_test_output = false;
    let mut found_test = false;

    while let Some(line) = lines.next() {
        let trimmed = line.trim();

        // Check if this is the start of our test expression
        if trimmed.starts_with("//>") {
            let expr = trimmed.trim_start_matches("//>").trim();
            if expr == test.expr {
                found_test = true;
                new_content.push_str(line);
                new_content.push('\n');

                // Skip the old expected output lines
                in_test_output = true;
                continue;
            }
        }

        if in_test_output {
            // Skip old expected output (lines starting with "//")
            if trimmed.starts_with("//") || trimmed.is_empty() {
                continue;
            } else {
                // We've reached the end of the expected output section
                in_test_output = false;

                // Insert the new expected output
                new_content.push_str("//\n");
                for output_line in new_output.lines() {
                    new_content.push_str("// ");
                    new_content.push_str(output_line);
                    new_content.push('\n');
                }

                // Add the current line (which is not part of the expected output)
                new_content.push_str(line);
                new_content.push('\n');
            }
        } else {
            new_content.push_str(line);
            new_content.push('\n');
        }
    }

    // If we're still in test output at the end of file, insert the new output
    if in_test_output {
        new_content.push_str("//\n");
        for output_line in new_output.lines() {
            new_content.push_str("// ");
            new_content.push_str(output_line);
            new_content.push('\n');
        }
    }

    if !found_test {
        anyhow::bail!("Could not find test expression '{}' in file", test.expr);
    }

    // Write the updated content back to the file
    std::fs::write(&test.file_path, new_content).with_context(|| {
        format!(
            "Failed to write updated test file: {}",
            test.file_path.display()
        )
    })?;

    Ok(())
}

/// Main test function that discovers and runs all interpreter tests
#[tokio::test]
async fn executor_tests() -> Result<()> {
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
        run_test(test).await.with_context(|| {
            format!(
                "Test failed in {} for expression: {}",
                test.file_path.display(),
                test.expr
            )
        })?;
    }

    Ok(())
}
