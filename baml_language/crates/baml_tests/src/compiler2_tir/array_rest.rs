//! Array rest-pattern binding tests (B-531).
//!
//! Semantics under test (the "Rust-parity subset"):
//!   - `..` may carry a binding: `..let r`, `.._`, and pure bind chains
//!     (`..let r: let s`), optionally ascribed with a list type
//!     (`..let r: int[]`). The binding is typed as the *slice* (`elem[]`)
//!     and binds a copy of the matched middle.
//!   - Everything else after `..` is rejected with a targeted diagnostic:
//!     bare type patterns (`..int`, `..int[]`), structural patterns
//!     (`..[let x]`, `..Box { .. }`, `..[]`), or-patterns, and bind chains
//!     ending in a structural link (`..let r: [let x]`).
//!   - Non-list ascriptions on a rest binding are a plain type mismatch
//!     against `elem[]` (`..let r: int` on `int[]`), and so are narrowing
//!     ascriptions (`..let r: int[]` on `(int|string)[]`) — the slice's
//!     runtime tag is always exactly `elem[]`, so narrowing never matches.
//!
//! The restriction could be expanded in the future; each rejected shape is
//! blocked for a different reason:
//!   - Nested array rests (`[a, ..[b, ..r], c]`) are SOUND today — the
//!     flattener rewrites them into the outer flat shape before the
//!     usefulness matrix runs. Rejected only to keep one spelling per
//!     pattern (`[..[]]` = `[]` is the confusing poster child). Cheapest
//!     to allow later.
//!   - Type patterns and class destructures after `..` are statically dead:
//!     runtime list type tests are invariant tag compares and the slice tag
//!     is always `elem[]`. Nothing to enable unless list tests ever become
//!     structural.
//!   - Refutable forms (or-patterns of shapes, e.g. `..([] | [_])`) need a
//!     rest-constraint column in the usefulness matrix first: TIR drops the
//!     rest sub-pattern's DPat (`lower_array_pat` keeps only its bindings)
//!     while MIR emits its runtime test, so allowing them today makes
//!     "exhaustive" matches that fall through at runtime.
//!
//! These tests are written BEFORE ungating (test-first): with the blanket
//! `RestSubPatternNotSupported` gate still in place, the positive tests and
//! the message assertions below fail. They pass once the gate is replaced
//! by the subset check.

use super::support::{make_db, render_tir};
use crate::engine::TestDbExt;

/// Stable prefix of the subset-rejection diagnostic. The full message is
/// "rest pattern `..` can only carry a binding; write `..let name` or
/// `..let name: T[]`".
const REST_BINDING_ONLY: &str = "rest pattern `..` can only carry a binding";

/// The old blanket-gate message. Must not appear anywhere once ungated.
const OLD_GATE: &str = "rest pattern `..` cannot carry a sub-pattern";

// ============================================================================
// Positive: binding shapes that must compile clean
// ============================================================================

#[test]
fn rest_binding_types_slice_as_element_list() {
    let mut db = make_db();
    let file = db.file(
        "test.baml",
        r#"
function f(xs: int[]) -> int {
    match (xs) {
        [let a, ..let r] => r[0],
        _ => 0
    }
}
"#,
    );
    let output = render_tir(&db, file);

    assert!(
        !output.contains(REST_BINDING_ONLY) && !output.contains(OLD_GATE),
        "rest binding should be allowed, got:\n{output}"
    );
    assert!(
        output.contains("r[0] : int"),
        "rest binding should be typed int[], so r[0] is int, got:\n{output}"
    );
}

#[test]
fn rest_binding_between_prefix_and_suffix() {
    let mut db = make_db();
    let file = db.file(
        "test.baml",
        r#"
function f(xs: int[]) -> int {
    match (xs) {
        [let a, ..let mid, let z] => mid.length() + a + z,
        _ => 0
    }
}
"#,
    );
    let output = render_tir(&db, file);

    assert!(
        !output.contains(REST_BINDING_ONLY) && !output.contains(OLD_GATE),
        "rest binding between prefix and suffix should be allowed, got:\n{output}"
    );
    assert!(
        !output.contains("type mismatch"),
        "prefix/suffix elements are int, mid is int[], got:\n{output}"
    );
}

#[test]
fn rest_wildcard_is_allowed() {
    let mut db = make_db();
    let file = db.file(
        "test.baml",
        r#"
function f(xs: int[]) -> int {
    match (xs) {
        [let a, .._] => a,
        _ => 0
    }
}
"#,
    );
    let output = render_tir(&db, file);

    assert!(
        !output.contains(REST_BINDING_ONLY) && !output.contains(OLD_GATE),
        "`.._` should behave like bare `..`, got:\n{output}"
    );
}

#[test]
fn rest_binding_list_ascription_is_valid() {
    let mut db = make_db();
    let file = db.file(
        "test.baml",
        r#"
function f() -> int {
    let [..let rest: int[]] = [1, 2]
    return rest[0]
}
"#,
    );
    let output = render_tir(&db, file);

    assert!(
        !output.contains(REST_BINDING_ONLY) && !output.contains(OLD_GATE),
        "list-ascribed rest binding should be allowed, got:\n{output}"
    );
    assert!(
        !output.contains("type mismatch"),
        "int[] ascription matches the slice type, got:\n{output}"
    );
}

/// A narrowing ascription can never match at runtime: the slice produced by
/// `Array.slice` carries the scrutinee's element tag (`(int|string)[]` here),
/// and runtime list type tests compare tags with invariant element positions.
/// Accepting `..let r: int[]` would compile a statically-dead arm, so it must
/// be rejected even though `int[]` is a subtype of the slice type.
#[test]
fn rest_binding_narrowing_ascription_is_rejected() {
    let mut db = make_db();
    let file = db.file(
        "test.baml",
        r#"
function f(xs: (int | string)[]) -> int {
    match (xs) {
        [..let r: int[]] => r.length(),
        _ => 0
    }
}
"#,
    );
    let output = render_tir(&db, file);

    assert!(
        output.contains("type mismatch"),
        "narrowing the rest slice is statically dead; must be diagnosed, got:\n{output}"
    );
}

#[test]
fn rest_binding_pure_bind_chain_is_valid() {
    let mut db = make_db();
    let file = db.file(
        "test.baml",
        r#"
function f(xs: int[]) -> int {
    match (xs) {
        [..let r: let s] => r[0] + s[0],
        _ => 0
    }
}
"#,
    );
    let output = render_tir(&db, file);

    assert!(
        !output.contains(REST_BINDING_ONLY) && !output.contains(OLD_GATE),
        "pure bind chains after `..` should be allowed, got:\n{output}"
    );
    assert!(
        !output.contains("type mismatch") && !output.contains("unresolved name"),
        "both chain links bind the slice (int[]), got:\n{output}"
    );
}

#[test]
fn rest_binding_union_element_type() {
    let mut db = make_db();
    let file = db.file(
        "test.baml",
        r#"
function f(xs: (int | string)[]) -> int {
    match (xs) {
        [..let r] => r.length()
    }
}
"#,
    );
    let output = render_tir(&db, file);

    assert!(
        !output.contains(REST_BINDING_ONLY) && !output.contains(OLD_GATE),
        "rest binding over a union element type should be allowed, got:\n{output}"
    );
    assert!(
        !output.contains("type mismatch") && !output.contains("non-exhaustive"),
        "rest-only pattern is irrefutable and well-typed, got:\n{output}"
    );
}

#[test]
fn rest_binding_generic_element_type() {
    let mut db = make_db();
    let file = db.file(
        "test.baml",
        r#"
function f<T>(xs: T[]) -> int {
    match (xs) {
        [let first, ..let r] => r.length(),
        _ => 0
    }
}
"#,
    );
    let output = render_tir(&db, file);

    assert!(
        !output.contains(REST_BINDING_ONLY) && !output.contains(OLD_GATE),
        "rest binding over a generic element type should be allowed, got:\n{output}"
    );
    assert!(
        !output.contains("type mismatch"),
        "rest binds T[], got:\n{output}"
    );
}

#[test]
fn rest_binding_evolving_list_scrutinee() {
    let mut db = make_db();
    let file = db.file(
        "test.baml",
        r#"
function f() -> int {
    let xs = [];
    xs.push(1);
    match (xs) {
        [..let r] => r.length()
    }
}
"#,
    );
    let output = render_tir(&db, file);

    assert!(
        !output.contains(REST_BINDING_ONLY) && !output.contains(OLD_GATE),
        "rest binding over an evolving list should be allowed, got:\n{output}"
    );
    assert!(
        !output.contains("type mismatch"),
        "rest binds the settled element list, got:\n{output}"
    );
}

// ============================================================================
// Or-patterns: rest bindings participate in the same-names rule
// ============================================================================

#[test]
fn or_pattern_rest_binding_same_names_across_branches() {
    let mut db = make_db();
    let file = db.file(
        "test.baml",
        r#"
class NumberBag {
    field int[]
}

function f(v: NumberBag | int[][]) -> int {
    match (v) {
        NumberBag { field } | [[..let field]: int[], .._] => field[0],
        _ => 0
    }
}
"#,
    );
    let output = render_tir(&db, file);

    assert!(
        !output.contains(REST_BINDING_ONLY) && !output.contains(OLD_GATE),
        "rest binding inside an or-branch should be allowed, got:\n{output}"
    );
    assert!(
        !output.contains("must bind the same names"),
        "both branches bind `field` (int[]), got:\n{output}"
    );
}

/// The same-names rule for or-patterns is a HIR diagnostic, so this test
/// goes through `collect_diagnostics` rather than the TIR renderer.
#[test]
fn or_pattern_rest_binding_missing_in_sibling_is_rejected() {
    let db = baml_db::testing::setup_test_db(
        r#"
function f(xs: int[]) -> int {
    match (xs) {
        [..let a] | [.._] => 1
    }
}

function main() -> int { 0 }
"#,
    );
    let diags = baml_db::collect_diagnostics(&db);

    assert!(
        diags
            .iter()
            .any(|d| d.message.contains("must bind the same names")),
        "one branch binds `a`, the other binds nothing, got: {:?}",
        diags.iter().map(|d| &d.message).collect::<Vec<_>>()
    );
}

// ============================================================================
// Refutability and exhaustiveness
// ============================================================================

#[test]
fn rest_only_binding_is_irrefutable_in_let() {
    let mut db = make_db();
    let file = db.file(
        "test.baml",
        r#"
function f(xs: int[]) -> int {
    let [..let r] = xs
    return r.length()
}
"#,
    );
    let output = render_tir(&db, file);

    assert!(
        !output.contains("refutable pattern in"),
        "`[..let r]` matches every list, got:\n{output}"
    );
    assert!(
        !output.contains(REST_BINDING_ONLY) && !output.contains(OLD_GATE),
        "rest-only binding should be allowed, got:\n{output}"
    );
}

#[test]
fn rest_binding_with_prefix_is_refutable_in_let() {
    let mut db = make_db();
    let file = db.file(
        "test.baml",
        r#"
function f(xs: int[]) -> int {
    let [let a, ..let r] = xs
    return a
}
"#,
    );
    let output = render_tir(&db, file);

    assert!(
        output.contains("refutable pattern in"),
        "prefix requires length >= 1, so the let is refutable, got:\n{output}"
    );
}

#[test]
fn match_with_rest_binding_arm_is_exhaustive() {
    let mut db = make_db();
    let file = db.file(
        "test.baml",
        r#"
function f(xs: int[]) -> int {
    match (xs) {
        [] => 0,
        [let x, ..let r] => x
    }
}
"#,
    );
    let output = render_tir(&db, file);

    assert!(
        !output.contains("non-exhaustive"),
        "[] plus [x, ..r] covers every length, got:\n{output}"
    );
    assert!(
        !output.contains(REST_BINDING_ONLY) && !output.contains(OLD_GATE),
        "rest binding should be allowed, got:\n{output}"
    );
}

// ============================================================================
// Rejected shapes: targeted diagnostic, no cascades
// ============================================================================

#[test]
fn rest_bare_type_pattern_is_rejected() {
    let mut db = make_db();
    let file = db.file(
        "test.baml",
        r#"
function f(xs: int[]) -> int {
    match (xs) {
        [let a, ..int] => a,
        _ => 0
    }
}
"#,
    );
    let output = render_tir(&db, file);

    assert!(
        output.contains(REST_BINDING_ONLY),
        "`..int` reads element-wise but would type-check against the slice; reject it, got:\n{output}"
    );
    assert!(
        !output.contains("type mismatch"),
        "the shape rejection must preempt the slice-type mismatch cascade, got:\n{output}"
    );
}

#[test]
fn rest_list_type_pattern_is_rejected() {
    let mut db = make_db();
    let file = db.file(
        "test.baml",
        r#"
function f(xs: int[]) -> int {
    match (xs) {
        [let a, ..int[]] => a,
        _ => 0
    }
}
"#,
    );
    let output = render_tir(&db, file);

    assert!(
        output.contains(REST_BINDING_ONLY),
        "slice-level type tests after `..` are rejected (would need deep runtime list checks), got:\n{output}"
    );
}

#[test]
fn rest_structural_array_is_rejected() {
    let mut db = make_db();
    let file = db.file(
        "test.baml",
        r#"
function f(xs: int[]) -> int {
    match (xs) {
        [..[let x]] => x,
        _ => 0
    }
}
"#,
    );
    let output = render_tir(&db, file);

    assert!(
        output.contains(REST_BINDING_ONLY),
        "structural destructure of the slice is rejected, got:\n{output}"
    );
}

#[test]
fn rest_class_destructure_is_rejected() {
    let mut db = make_db();
    let file = db.file(
        "test.baml",
        r#"
class Box {
    value int
}

function f(boxes: Box[]) -> int {
    match (boxes) {
        [..Box { value }] => value,
        _ => 0
    }
}
"#,
    );
    let output = render_tir(&db, file);

    assert!(
        output.contains(REST_BINDING_ONLY),
        "class destructure of the slice is rejected, got:\n{output}"
    );
}

#[test]
fn rest_or_pattern_is_rejected() {
    let mut db = make_db();
    let file = db.file(
        "test.baml",
        r#"
function f(xs: int[]) -> int {
    match (xs) {
        [..(let a | let b)] => 1,
        _ => 0
    }
}
"#,
    );
    let output = render_tir(&db, file);

    assert!(
        output.contains(REST_BINDING_ONLY),
        "or-patterns after `..` are rejected, got:\n{output}"
    );
}

#[test]
fn rest_bind_chain_with_structural_tail_is_rejected() {
    let mut db = make_db();
    let file = db.file(
        "test.baml",
        r#"
function f(xs: int[]) -> int {
    match (xs) {
        [..let r: [let x]] => x,
        _ => 0
    }
}
"#,
    );
    let output = render_tir(&db, file);

    assert!(
        output.contains(REST_BINDING_ONLY),
        "bind chains may not end in a structural link, got:\n{output}"
    );
}

#[test]
fn rest_binding_non_list_ascription_is_type_mismatch() {
    let mut db = make_db();
    let file = db.file(
        "test.baml",
        r#"
function f() -> int {
    let [..let rest: int] = [1, 2]
    return 0
}
"#,
    );
    let output = render_tir(&db, file);

    assert!(
        output.contains("type mismatch: expected int[], got int"),
        "the rest slice is int[]; ascribing int is a plain mismatch, got:\n{output}"
    );
    assert!(
        !output.contains(REST_BINDING_ONLY) && !output.contains(OLD_GATE),
        "binding-shaped rests pass the shape check even when the ascription is wrong, got:\n{output}"
    );
}

// ============================================================================
// Nested-rest torture: exactly one diagnostic, no crashes, no cascades
// ============================================================================

#[test]
fn nested_bare_rest_is_rejected_once() {
    let mut db = make_db();
    let file = db.file(
        "test.baml",
        r#"
function f(xs: int[]) -> int {
    match (xs) {
        [..[..]] => 1,
        _ => 0
    }
}
"#,
    );
    let output = render_tir(&db, file);

    assert_eq!(
        output.matches(REST_BINDING_ONLY).count(),
        1,
        "the flattener must not silently collapse `[..[..]]` to `[..]`; one diag, got:\n{output}"
    );
}

#[test]
fn nested_rest_binding_is_rejected_without_name_cascade() {
    let mut db = make_db();
    let file = db.file(
        "test.baml",
        r#"
function f(xs: int[]) -> int {
    match (xs) {
        [..[..let r]] => r.length(),
        _ => 0
    }
}
"#,
    );
    let output = render_tir(&db, file);

    assert!(
        output.contains(REST_BINDING_ONLY),
        "the flattener must not silently collapse `[..[..let r]]` to `[..let r]`, got:\n{output}"
    );
    assert!(
        !output.contains("unresolved name"),
        "rejected rest bindings must still resolve in the body (one diag, no cascade), got:\n{output}"
    );
}

#[test]
fn rest_empty_array_is_rejected() {
    let mut db = make_db();
    let file = db.file(
        "test.baml",
        r#"
function f(xs: int[]) -> int {
    match (xs) {
        [let a, ..[]] => a,
        _ => 0
    }
}
"#,
    );
    let output = render_tir(&db, file);

    assert!(
        output.contains(REST_BINDING_ONLY),
        "`..[]` (rest-must-be-empty) must not flatten to a fixed-length pattern, got:\n{output}"
    );
}

#[test]
fn rest_nested_array_with_inner_rest_is_rejected() {
    let mut db = make_db();
    let file = db.file(
        "test.baml",
        r#"
function f(xs: int[]) -> int {
    match (xs) {
        [let a, ..[let b, ..let r, let c], let d] => a + b + c + d,
        _ => 0
    }
}
"#,
    );
    let output = render_tir(&db, file);

    assert!(
        output.contains(REST_BINDING_ONLY),
        "the flattening showcase must be rejected, not rewritten, got:\n{output}"
    );
    assert!(
        !output.contains("unresolved name"),
        "bindings inside the rejected rest must not cascade, got:\n{output}"
    );
}

#[test]
fn deeply_nested_rest_reports_single_diagnostic() {
    let mut db = make_db();
    let file = db.file(
        "test.baml",
        r#"
function f(xs: int[]) -> int {
    match (xs) {
        [..[..[..[..[..]]]]] => 1,
        _ => 0
    }
}
"#,
    );
    let output = render_tir(&db, file);

    assert_eq!(
        output.matches(REST_BINDING_ONLY).count(),
        1,
        "reject at the outermost structural link only; no per-level spam, got:\n{output}"
    );
}

#[test]
fn rest_binding_on_non_list_scrutinee_has_no_name_cascade() {
    let mut db = make_db();
    let file = db.file(
        "test.baml",
        r#"
function f() -> int {
    let [..let r] = 42
    return r.length()
}
"#,
    );
    let output = render_tir(&db, file);

    assert!(
        !output.contains("unresolved name"),
        "r must still be bound (to some recovery list type) despite the scrutinee error, got:\n{output}"
    );
}
