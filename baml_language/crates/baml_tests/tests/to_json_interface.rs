//! Tests for the `baml.ToJson` interface: compile-error diagnostics.
//!
//! Compiler diagnostics cannot be asserted in corpus `test` blocks.

/// Compile errors raised in the user file, as `[CODE] message`.
fn compile_errors(source: &str) -> Vec<String> {
    use baml_compiler_diagnostics::Severity;
    use baml_tests::stdlib_prefix::{check_user_files, setup_test_db};
    let db = setup_test_db(source);
    check_user_files(&db)
        .into_iter()
        .filter(|d| matches!(d.severity, Severity::Error))
        .map(|d| format!("[{}] {}", d.code(), d.message_with_primary_label()))
        .collect()
}

#[test]
fn direct_to_json_method_is_banned() {
    let errors = compile_errors(
        r#"
        class Point {
            x int
            function to_json(self) -> baml.json.json throws never { 1 }
        }
    "#,
    );
    assert!(
        errors
            .iter()
            .any(|e| e.contains("to_json") && e.contains("baml.ToJson")),
        "expected a to_json ban error recommending baml.ToJson; got:\n  {}",
        errors.join("\n  ")
    );
}

#[test]
fn to_json_via_interface_is_allowed() {
    let errors = compile_errors(
        r#"
        class Point {
            x int
            implements baml.ToJson {
                function to_json(self) -> baml.json.json throws baml.json.SerializationError { 1 }
            }
        }
    "#,
    );
    assert!(
        !errors.iter().any(|e| e.contains("to_json")),
        "implementing baml.ToJson must not be flagged; got:\n  {}",
        errors.join("\n  ")
    );
}
