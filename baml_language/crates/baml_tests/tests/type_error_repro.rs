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

// ============================================================================
// §N — Integer literals outside the i63 `int` range (B-266)
// ============================================================================

/// `int` is a 63-bit signed integer, so a bare literal of magnitude 2^62
/// (a valid i64, but `INT_MAX + 1`) is rejected at compile time rather than
/// panicking the VM at engine load. The diagnostic points at `bigint`.
#[tokio::test]
#[should_panic(expected = "[E0139]")]
async fn int_literal_above_max_is_rejected_at_compile_time() {
    let _ = baml_test!(
        r#"
        function main() -> int {
            4611686018427387904
        }
    "#
    );
}

/// A genuinely-too-large negated literal (magnitude past `INT_MIN`) is also a
/// compile error — the negated value is range-checked, not just the token.
#[tokio::test]
#[should_panic(expected = "[E0139]")]
async fn negated_int_literal_below_min_is_rejected_at_compile_time() {
    let _ = baml_test!(
        r#"
        function main() -> int {
            -5000000000000000000
        }
    "#
    );
}

/// The negative-literal fold only applies to a `-` directly on an integer
/// literal token. A *parenthesized* `-(2^62)` lowers its operand through a
/// child node, so the `+2^62` is rejected just like a bare one (matching the
/// Rust/Java/C# rule, where parentheses break the negative-literal form).
#[tokio::test]
#[should_panic(expected = "[E0139]")]
async fn parenthesized_oversized_literal_is_rejected() {
    let _ = baml_test!(
        r#"
        function main() -> int {
            -(4611686018427387904)
        }
    "#
    );
}

/// `int.min_value()` (`-2^62`) must remain writable as a negated literal even
/// though `+2^62` is not a legal `int` literal: the leading `-` forms a single
/// negative literal. (Mirrors the i64::MIN literal rule in Rust/Java/C#.)
#[tokio::test]
async fn negated_int_min_literal_is_valid() {
    let output = baml_test!(
        r#"
        function main() -> int {
            -4611686018427387904
        }
    "#
    );
    assert!(
        matches!(
            output.result,
            Ok(BexExternalValue::Int(-4611686018427387904))
        ),
        "expected INT_MIN, got: {:?}",
        output.result
    );
}

// ============================================================================
// §N+1 — Non-string map keys are rejected at compile time (B-533)
// ============================================================================

/// Runtime maps are string-keyed, so `map<int, V>` must fail during type
/// checking instead of compiling and crashing when `.set()` coerces the key.
#[tokio::test]
#[should_panic(expected = "map keys must be `string`; got `int`")]
async fn map_with_int_key_is_rejected_at_compile_time() {
    let _ = baml_test!(
        r#"
        function main(nums: int[]) -> int {
            let counts: map<int, int> = {};
            for (let n in nums) {
                let _ = counts.set(n, (counts.get(n) ?? 0) + 1);
            }
            0
        }
    "#
    );
}

#[tokio::test]
async fn map_with_string_alias_key_is_accepted() {
    let output = baml_test!(
        r#"
        type Key = string

        function main() -> int {
            let counts: map<Key, int> = {};
            let _ = counts.set("x", 1);
            counts.get("x") ?? 0
        }
    "#
    );
    assert!(
        matches!(output.result, Ok(BexExternalValue::Int(1))),
        "expected aliased string key map to work, got: {:?}",
        output.result
    );
}

#[tokio::test]
async fn map_with_repeated_string_alias_union_key_is_accepted() {
    let output = baml_test!(
        r#"
        type Key = string

        function main() -> int {
            let counts: map<Key | Key, int> = {};
            let _ = counts.set("x", 1);
            counts.get("x") ?? 0
        }
    "#
    );
    assert!(
        matches!(output.result, Ok(BexExternalValue::Int(1))),
        "expected repeated aliased string key union map to work, got: {:?}",
        output.result
    );
}

#[tokio::test]
async fn generic_map_key_annotation_is_deferred() {
    let output = baml_test!(
        r#"
        function passthrough<K, V>(items: map<K, V>) -> map<K, V> {
            items
        }

        function main() -> int {
            0
        }
    "#
    );
    assert!(
        matches!(output.result, Ok(BexExternalValue::Int(0))),
        "expected generic map key annotation to compile, got: {:?}",
        output.result
    );
}

#[tokio::test]
#[should_panic(expected = "map keys must be `string`; got `IntKey`")]
async fn map_with_int_alias_key_is_rejected_at_compile_time() {
    let _ = baml_test!(
        r#"
        type IntKey = int

        function main() -> int {
            let counts: map<IntKey, int> = {};
            0
        }
    "#
    );
}
