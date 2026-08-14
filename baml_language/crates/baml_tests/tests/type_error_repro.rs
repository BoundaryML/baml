//! Regression tests for array indexing with null / wrong-typed subscripts.
//!
//! These isolate components of the tic-tac-toe bug, where a list indexed by a
//! null (or otherwise non-int) value used to slip past the checker and abort
//! the VM with the confusing `type error: expected int, got any`. The subscript
//! is now validated at compile time, so these are rejected with a clear
//! diagnostic before they ever run.
//!
//! Compile-error tests using `#[should_panic]` stay in Rust because the BAML
//! corpus only runs code that successfully compiles.

use baml_tests::baml_test;

// ============================================================================
// §1 — A null array index is rejected at compile time (plain `[]`)
// ============================================================================

/// Null indices are rejected at compile time with a mismatched-types diagnostic
/// (`found int | null`). This test verifies the rejection using `#[should_panic]`.
#[tokio::test]
#[should_panic(expected = "mismatched types: expected `int`, found `int | null`")]
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
// §N — Integer literals outside the i63 `int` range (B-266)
// ============================================================================

/// `int` is a 63-bit signed integer, so a bare literal of magnitude 2^62
/// (a valid i64, but `INT_MAX + 1`) is rejected at compile time rather than
/// panicking the VM at engine load. The diagnostic points at `bigint`.
#[tokio::test]
#[should_panic(expected = "[E0150]")]
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
#[should_panic(expected = "[E0150]")]
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
#[should_panic(expected = "[E0150]")]
async fn parenthesized_oversized_literal_is_rejected() {
    let _ = baml_test!(
        r#"
        function main() -> int {
            -(4611686018427387904)
        }
    "#
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
#[should_panic(expected = "map keys must be `string`; got `K`")]
async fn generic_map_key_annotation_is_rejected() {
    // No bound can prove a type variable string-denoting, so `map<K, V>` could be
    // instantiated at a non-string key — rejected at the declaration (E0067).
    let _ = baml_test!(
        r#"
        function passthrough<K, V>(items: map<K, V>) -> map<K, V> {
            items
        }

        function main() -> int {
            0
        }
    "#
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
