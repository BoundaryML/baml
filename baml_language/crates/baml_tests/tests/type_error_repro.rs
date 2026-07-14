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

// ============================================================================
// §N - Control-flow joins subtype-reduce instead of accumulating unions (B-236)
// ============================================================================
//
// Port of TypeScript's flow-join simplification (typescript-go
// `getUnionOrEvolvingArrayType` + `removeSubtypes`): at an if/else or match
// join, an unestablished empty `[]` / `{}` branch is established by the other
// branch's concrete container type, and union members that are subtypes of
// other members are dropped.

/// The original B-236 repro: `if c { xs } else { [] }` must join `string[]`
/// with the empty `[]` to `string[]`, not the union `string[] | _[]`. The
/// union made `.join` unresolvable (and before that, mis-dispatched it to the
/// Map impl, aborting the VM with `expected map, got array`).
#[tokio::test]
async fn if_else_concrete_list_with_empty_else_branch_runs() {
    let output = baml_test!(
        r#"
        function f(xs: string[], m: int) -> string {
            let top = if (m > 0) { xs.slice(0, m) } else { [] };
            top.join(" ")
        }
        function main() -> string {
            f(["a", "b", "c"], 2)
        }
    "#
    );
    assert!(
        matches!(&output.result, Ok(BexExternalValue::String(s)) if s == "a b"),
        "expected \"a b\", got: {:?}",
        output.result
    );
}

/// The empty branch is not just typeable - it also runs: with `m = 0` the
/// else-branch's actual empty array flows through the `string[]`-typed binding
/// and `.join` returns `""`.
#[tokio::test]
async fn if_else_empty_else_branch_taken_at_runtime() {
    let output = baml_test!(
        r#"
        function f(xs: string[], m: int) -> string {
            let top = if (m > 0) { xs.slice(0, m) } else { [] };
            top.join(" ")
        }
        function main() -> string {
            f(["a", "b", "c"], 0)
        }
    "#
    );
    assert!(
        matches!(&output.result, Ok(BexExternalValue::String(s)) if s.is_empty()),
        "expected \"\", got: {:?}",
        output.result
    );
}

/// The map analogue: `if c { {"a": 1} } else { {} }` joins to
/// `map<string, int>`, so indexing resolves.
#[tokio::test]
async fn if_else_concrete_map_with_empty_else_branch_runs() {
    let output = baml_test!(
        r#"
        function main() -> int {
            let m = if (true) { {"a": 1} } else { {} };
            m["a"]
        }
    "#
    );
    assert!(
        matches!(output.result, Ok(BexExternalValue::Int(1))),
        "expected 1, got: {:?}",
        output.result
    );
}

/// Two empty branches stay *evolving*: the join must not prematurely commit
/// the container, so a later `push` still establishes the element type.
#[tokio::test]
async fn if_else_both_empty_branches_still_establishable() {
    let output = baml_test!(
        r#"
        function main() -> int {
            let x = if (true) { [] } else { [] };
            x.push(5);
            x[0]
        }
    "#
    );
    assert!(
        matches!(output.result, Ok(BexExternalValue::Int(5))),
        "expected 5, got: {:?}",
        output.result
    );
}

/// Two *committed* lists of different element types do NOT element-join to
/// `(int | string)[]` (that would let a write through the joined view corrupt
/// an aliased `int[]`). The join stays a union, on which mutation is rejected.
#[tokio::test]
#[should_panic(expected = "has no member `push`")]
async fn if_else_committed_lists_of_different_elements_stay_a_union() {
    let _ = baml_test!(
        r#"
        function main() -> int {
            let v = if (true) { [1] } else { ["a"] };
            v.push(2);
            0
        }
    "#
    );
}

/// Subtype reduction at the join: a literal branch joined with its base type
/// reduces to the base (`1 | int` -> `int`), so the result stays usable as a
/// plain `int` binding.
#[tokio::test]
async fn if_else_literal_and_base_reduce_to_base() {
    let output = baml_test!(
        r#"
        function f(n: int) -> int {
            let v = if (n > 0) { 1 } else { n };
            let w: int = v;
            w
        }
        function main() -> int {
            f(-3)
        }
    "#
    );
    assert!(
        matches!(output.result, Ok(BexExternalValue::Int(-3))),
        "expected -3, got: {:?}",
        output.result
    );
}
