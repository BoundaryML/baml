//! Regression tests for the union-`TypeVar` match-introspection bug.
//!
//! A generic function whose scrutinee is a union carrying a `TypeVar` beside
//! concrete members — `T | string | null` — must compile: the `let v: T` arm
//! is *not* unreachable just because `let s: string` and `null` arms precede
//! it. The concrete arms must NOT over-claim the open `T` union member.
//!
//! See the union-claim rules in `baml_compiler2_hir_ty/src/infer/pat.rs`.

use baml_compiler_diagnostics::{DiagnosticId, Severity};
use baml_tests::stdlib_prefix::{check_user_files, setup_test_db};

/// Collect all error-severity diagnostic ids for a source program.
fn error_ids(source: &str) -> Vec<DiagnosticId> {
    let db = setup_test_db(source);
    check_user_files(&db)
        .into_iter()
        .filter(|d| matches!(d.severity, Severity::Error))
        .map(|d| d.id)
        .collect()
}

/// The full `tag_or_value` match body compiles with no errors: in particular
/// `let v: T` is reachable and `let s: string` is not reported unreachable.
#[test]
fn tag_or_value_union_typevar_match_compiles() {
    let src = r#"
        function tag_or_value<T>(x: T | string | null) -> T? {
          match (x) {
            let s: string => null,
            null => null,
            let v: T => v,
          }
        }
        function main() -> int { 0 }
    "#;
    let errors = error_ids(src);
    assert!(
        errors.is_empty(),
        "expected tag_or_value to compile cleanly, got errors: {errors:?}"
    );
}

/// The `let v: T` arm is load-bearing for exhaustiveness: dropping it must
/// reintroduce a non-exhaustive-match error (the open `T` member is no longer
/// covered). This proves the fix did not silently make the match exhaustive
/// without `let v: T`.
#[test]
fn tag_or_value_without_typevar_arm_is_non_exhaustive() {
    let src = r#"
        function tag_or_value<T>(x: T | string | null) -> T? {
          match (x) {
            let s: string => null,
            null => null,
          }
        }
        function main() -> int { 0 }
    "#;
    let errors = error_ids(src);
    assert!(
        errors.contains(&DiagnosticId::NonExhaustiveMatch),
        "expected NonExhaustiveMatch when the `let v: T` arm is dropped, got: {errors:?}"
    );
}

/// Adding the `let v: T` arm BEFORE the concrete arms leaves those concrete
/// arms reachable: a rigid arm covers only its *own* union member (it is a
/// possible-but-not-covering match against the concrete members), so with
/// `T = int` and `x = "hi"` the `T` arm's runtime test fails and the `string`
/// arm genuinely fires. Each arm covers exactly its member, so the match is
/// exhaustive and no arm is unreachable. (This used to assert the opposite —
/// the pre-overlap-oracle coverage treated `let v: T` as a catch-all.)
#[test]
fn typevar_arm_first_leaves_concrete_arms_reachable() {
    let src = r#"
        function tag_or_value<T>(x: T | string | null) -> T? {
          match (x) {
            let v: T => v,
            let s: string => null,
            null => null,
          }
        }
        function main() -> int { 0 }
    "#;
    let errors = error_ids(src);
    assert!(
        errors.is_empty(),
        "expected concrete arms after a bare `let v: T` to stay reachable, got: {errors:?}"
    );
}

/// A `TypeVar` union beside a concrete *class* sibling — the real streaming
/// `TStream | NoYield` shape — must also compile, including the
/// `let p: T | Concrete` binding form whose natural type is itself a union
/// carrying the `TypeVar` (this was the regression the naive fix introduced).
#[test]
fn typevar_union_with_class_sibling_compiles() {
    let src = r#"
        class Sentinel {}
        function pick<T>(x: T | Sentinel) -> T? {
          let y: T | Sentinel = x;
          match (y) {
            Sentinel {} => null,
            let v: T => v,
          }
        }
        function main() -> int { 0 }
    "#;
    let errors = error_ids(src);
    assert!(
        errors.is_empty(),
        "expected TypeVar|Class union match (incl. the `let y: T|Sentinel` binding) to compile, got: {errors:?}"
    );
}
