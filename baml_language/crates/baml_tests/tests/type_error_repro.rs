//! Regression tests for array indexing with null / wrong-typed subscripts.
//!
//! These isolate components of the tic-tac-toe bug, where a list indexed by a
//! null (or otherwise non-int) value used to slip past the checker and abort
//! the VM with the confusing `type error: expected int, got any`. The subscript
//! is now validated at compile time, so these are rejected with a clear
//! diagnostic before they ever run.

use baml_tests::baml_test;
use bex_engine::BexExternalValue;

// ============================================================================
// §1 — A null array index is rejected at compile time (plain `[]`)
// ============================================================================

/// Previously this compiled and aborted the VM at runtime with the confusing
/// `got any`. It is now a compile-time type mismatch (`got int | null`), so
/// `baml_test!` fails compilation before execution.
#[tokio::test]
#[should_panic(expected = "type mismatch: expected int, got int | null")]
async fn array_index_with_null_is_rejected_at_compile_time() {
    let _ = baml_test!(
        r#"
        function main() -> string {
            let arr = ["a", "b", "c"];
            let idx: int? = null;
            arr[idx]
        }
    "#
    );
}

// ============================================================================
// §2 — The optional index `?.[]` is null-safe in the *index* too
// ============================================================================

/// `?.[]` is the null-safe index operator, so a null subscript short-circuits
/// the whole expression to null instead of aborting the VM (it used to crash
/// with `got any`). The base guard and the index guard are symmetric.
#[tokio::test]
async fn optional_index_with_null_index_returns_null() {
    let output = baml_test!(
        r#"
        function main() -> int? {
            let arr: int[]? = [10, 20, 30];
            let i: int? = null;
            arr?.[i]
        }
    "#
    );
    assert!(
        matches!(output.result, Ok(BexExternalValue::Null)),
        "expected null (not a crash), got: {:?}",
        output.result
    );
}

/// A valid (non-null) index through `?.[]` still returns the element.
#[tokio::test]
async fn optional_index_with_valid_index_returns_element() {
    let output = baml_test!(
        r#"
        function main() -> int? {
            let arr: int[]? = [10, 20, 30];
            let i: int? = 1;
            arr?.[i]
        }
    "#
    );
    assert!(
        matches!(output.result, Ok(BexExternalValue::Int(20))),
        "expected 20, got: {:?}",
        output.result
    );
}
