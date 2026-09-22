//! Phase 1 snapshot tests: explicit type arguments at call sites.
//!
//! Each test is a labeled semantic matrix. Together they preserve the original
//! sixteen scenarios while sharing compiler setup and snapshot review surface.

use super::support::{make_db, render_hir, render_tir};
use crate::engine::TestDbExt;

#[test]
fn type_binding_rendering_matrix() {
    // type_binding_renders_both_right_hand_side_kinds
    let mut db = make_db();
    let file = db.file(
        "test.baml",
        r#"
class Wrapper<T> { value T }

function caller(t: reflect.Type) -> bool {
    type R = unreflect(t)
    type S = Wrapper<string>
    true
}
"#,
    );

    let tir = render_tir(&db, file);
    assert!(
        tir.contains("type R = unreflect(t) : reflect.Type"),
        "typed rendering lost the runtime operand:\n{tir}"
    );
    assert!(
        tir.contains("type S = Wrapper<string> : type"),
        "typed rendering lost the static type:\n{tir}"
    );
    let hir = render_hir(&db, file);
    assert!(
        hir.contains("type R = unreflect(t)"),
        "HIR rendering lost the runtime operand:\n{hir}"
    );
    assert!(
        hir.contains("type S = user.Wrapper<string>"),
        "HIR rendering lost the static type:\n{hir}"
    );
}

#[test]
fn direct_explicit_type_argument_matrix() {
    let mut db = make_db();
    let file = db.file(
        "test.baml",
        r#"
function identity<T>(x: T) -> T { x }
function no_generics(x: int) -> int { x }
function pair<A, B>(a: A, b: B) -> string { "ok" }

// explicit_type_arg_binds_directly: T is fixed to int.
function explicit_type_arg_binds_directly() -> int {
    identity<int>(42)
}

// bare_inference_picks_literal_type: T remains the literal type 42. Keeping
// the literal return contract makes this observably distinct from the case
// above instead of producing the old byte-identical widened snapshots.
function bare_inference_picks_literal_type() -> 42 {
    identity(42)
}

// wrong_type_arg_arity_nongeneric
function wrong_type_arg_arity_nongeneric() -> int {
    no_generics<int>(42)
}

// wrong_type_arg_arity_too_many
function wrong_type_arg_arity_too_many() -> int {
    identity<int, string>(42)
}

// explicit_two_type_args
function explicit_two_type_args() -> string {
    pair<int, string>(1, "hello")
}
"#,
    );
    insta::assert_snapshot!(render_tir(&db, file));
}

#[test]
fn single_parameter_instantiation_value_matrix() {
    let mut db = make_db();
    let file = db.file(
        "test.baml",
        r#"
function identity<T>(x: T) -> T { x }

// generic_apply_value_is_specialized
function generic_apply_value_is_specialized() -> string {
    let f = identity<int>
    "ok"
}

// generic_apply_value_rejects_wrong_arg
function generic_apply_value_rejects_wrong_arg() -> int {
    let f = identity<int>
    f("string")
}

// generic_apply_value_accepts_right_arg
function generic_apply_value_accepts_right_arg() -> int {
    let f = identity<int>
    f(1)
}

// generic_apply_value_arity_mismatch
function generic_apply_value_arity_mismatch() -> string {
    let f = identity<int, string>
    "ok"
}
"#,
    );
    insta::assert_snapshot!(render_tir(&db, file));
}

#[test]
fn multiple_parameter_instantiation_value_matrix() {
    let mut db = make_db();
    let file = db.file(
        "test.baml",
        r#"
function pair<A, B>(a: A, b: B) -> string { "ok" }

// generic_apply_two_type_args_specialized
function generic_apply_two_type_args_specialized() -> string {
    let f = pair<int, string>
    "done"
}

// generic_apply_two_type_args_rejects_wrong_arg
function generic_apply_two_type_args_rejects_wrong_arg() -> string {
    let f = pair<int, string>
    f(1, 2)
}
"#,
    );
    insta::assert_snapshot!(render_tir(&db, file));
}

#[test]
fn parenthesized_instantiation_recovery() {
    let mut db = make_db();
    let file = db.file(
        "test.baml",
        r#"
function identity<T>(x: T) -> T { x }

// generic_apply_through_parenthesized_receiver: AST lowering owns the primary
// diagnostic; TIR must recover a concrete instantiation without a cascade.
function generic_apply_through_parenthesized_receiver() -> string {
    let f = (identity)<int>
    "ok"
}
"#,
    );
    insta::assert_snapshot!(render_tir(&db, file));
}

#[test]
fn ambient_type_variable_instantiation_matrix() {
    let mut db = make_db();
    let file = db.file(
        "test.baml",
        r#"
function identity<T>(x: T) -> T { x }

// instantiation_value_call_keeps_ambient_typevar_rigid
function instantiation_value_call_keeps_ambient_typevar_rigid<T>(y: T) -> int {
    let f = identity<T>
    f(1)
}

// instantiation_value_call_preserves_valid_inference: matching rigid argument.
function instantiation_value_call_preserves_valid_inference<T>(y: T) -> T {
    let f = identity<T>
    f(y)
}

// The original companion also pinned the diagnostic for storing a bare,
// unrealized generic function before a later call.
function bare_generic_reference_requires_specialization() -> int {
    let g = identity
    g(5)
}
"#,
    );
    insta::assert_snapshot!(render_tir(&db, file));
}

#[test]
fn generic_lambda_instantiation_recovery() {
    let mut db = make_db();
    let file = db.file(
        "test.baml",
        r#"
// paren_generic_lambda_instantiation
function paren_generic_lambda_instantiation() -> int {
    let f = (<T>(x: T) -> T { x })<int>
    f("string")
}
"#,
    );
    insta::assert_snapshot!(render_tir(&db, file));
}
