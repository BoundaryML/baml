//! Minimal reproduction tests for "type error: expected int, got any".
//!
//! These tests isolate the components of the tic-tac-toe bug:
//! - Array indexing with non-int values
//! - Null values used as array indices
//! - User-defined function argument type checking gaps
//! - The confusing "any" error message for null values

use baml_tests::baml_test;

// ============================================================================
// §1 — Array indexing with null produces "got any" instead of "got null"
// ============================================================================

/// Direct reproduction: index an array with a null value.
/// This should produce a type error, but the error message says "got any"
/// instead of "got null".
#[tokio::test]
async fn array_index_with_null_says_got_any() {
    let output = baml_test!(
        r#"
        function main() -> string {
            let arr = ["a", "b", "c"];
            let idx: int? = null;
            arr[idx]
        }
    "#
    );

    let err = output.result.unwrap_err();
    let msg = err.to_string();
    // This is the current (confusing) behavior: "got any" instead of "got null"
    assert!(
        msg.contains("type error"),
        "expected a type error, got: {msg}"
    );
}
