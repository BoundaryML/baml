//! Tests for the `baml.FromJson` interface and the `baml.json.to<T>` driver
//! (the deserialize counterpart of `baml.ToJson` / `baml.json.from`).
//!
//! These tests focus on compile-error / diagnostics assertions, verifying
//! compiler errors rather than execution behavior.

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
fn from_json_interface_compiles() {
    // The no-`self` `from_json(j) -> Self` interface method + an in-body impl
    // must typecheck cleanly.
    let errors = compile_errors(
        r#"
        class Temp {
            celsius float
            implements baml.FromJson {
                function from_json(j: baml.json.json) -> Self throws baml.json.DecodeError {
                    Temp { celsius: baml.json.to<float>(baml.json.field(j, "c")) }
                }
            }
        }
        function main() -> int { 1 }
        "#,
    );
    assert!(
        errors.is_empty(),
        "implementing baml.FromJson should compile clean; got:\n  {}",
        errors.join("\n  ")
    );
}

#[test]
fn empty_implementor_is_rejected() {
    // `from_json` is a required method (no default body), so an empty
    // `implements baml.FromJson {}` is a missing-required-method error. Non-
    // implementors get structural decode from `baml.json.to` instead.
    let errors = compile_errors(
        r#"
        class Point {
            x int
            y int
            implements baml.FromJson {}
        }
        function main() -> int { 1 }
        "#,
    );
    assert!(
        errors.iter().any(|e| e.contains("from_json")),
        "empty implements baml.FromJson should require from_json; got:\n  {}",
        errors.join("\n  ")
    );
}

#[test]
fn static_from_json_charges_only_json_decode_error() {
    // The sugar charges exactly `DecodeError` (not the unaccounted-callee
    // `unknown`): a wrapper declaring only that throws compiles clean, and a
    // structural decode needs no `ParseError`.
    let errors = compile_errors(
        r#"
        class User { name string  age int }
        function decode(j: baml.json.json) -> User throws baml.json.DecodeError {
            User.from_json(j)
        }
        function main() -> int { 1 }
        "#,
    );
    assert!(
        errors.is_empty(),
        "Type.from_json should charge only DecodeError; got:\n  {}",
        errors.join("\n  ")
    );
}
