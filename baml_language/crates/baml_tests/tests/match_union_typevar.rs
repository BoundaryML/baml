//! Regression tests for the union-`TypeVar` match-introspection bug.
//!
//! A generic function whose scrutinee is a union carrying a `TypeVar` beside
//! concrete members — `T | string | null` — must compile: the `let v: T` arm
//! is *not* unreachable just because `let s: string` and `null` arms precede
//! it. The concrete arms must NOT over-claim the open `T` union member.
//!
//! See `union_targets_for_pattern` in `baml_compiler2_tir/src/builder.rs`.

use baml_compiler_diagnostics::{DiagnosticId, Severity};
use baml_project::{collect_diagnostics, testing::setup_test_db};

/// Collect all error-severity diagnostic ids for a source program.
fn error_ids(source: &str) -> Vec<DiagnosticId> {
    let db = setup_test_db(source);
    let project = db.get_project().expect("project must be set");
    let files = db.get_source_files();
    collect_diagnostics(&db, project, &files)
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

/// A bare `let v: T` arm claims ONLY the union's open `T` member — never a
/// concrete sibling like `string`/`null`. So placing `let v: T` first does
/// NOT shadow the concrete arms: when `T` is instantiated to e.g. `int`, that
/// arm cannot match a `string` or `null` value, so those arms stay reachable.
/// This is the pattern-side of the directional overlap (B-633): the mirror of
/// the member-side rule, and the fix for the symmetric false "unreachable arm"
/// (E0063) — reporting these concrete arms unreachable was the bug.
#[test]
fn typevar_arm_first_does_not_shadow_concrete_arms() {
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
        "expected concrete arms after a bare `let v: T` to stay reachable (T claims only its own member), got errors: {errors:?}"
    );
}

/// A `TypeVar` union beside a concrete *class* sibling — the real streaming
/// `TStream | StreamNoYield` shape — must also compile, including the
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
