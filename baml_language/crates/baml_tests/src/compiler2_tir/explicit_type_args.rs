//! Phase 1 snapshot tests: explicit type arguments at call sites.
//!
//! Verifies that `<T1, T2, ...>` written at call sites are:
//! 1. Preserved through CST→AST lowering into `Expr::Call::type_args`.
//! 2. Resolved and bound directly, bypassing forward/reverse inference.
//! 3. Diagnosed as `WrongTypeArgArity` when the count is wrong.
//!
//! Note: the TIR renderer prints type-parameter *names* at call sites (e.g. `identity<T>(...)`),
//! not the resolved arguments. Divergence from inference is therefore observed via the
//! resulting expression *type*, not the call-site syntax.

use super::support::{make_db, render_tir};

/// Explicit type args bind T directly, distinct from what inference would produce.
///
/// In a let-binding with no type annotation, `identity(42)` would infer `T = 42`
/// (the literal type), then widen to `int` (`42 -> int`). With explicit `<int>`,
/// T is bound to `int` directly, so the call's type is plain `int` (no widening arrow).
#[test]
fn explicit_type_arg_binds_directly() {
    let mut db = make_db();
    let file = db.add_file(
        "test.baml",
        r#"
function identity<T>(x: T) -> T { x }
function caller() -> string {
    let x = identity<int>(42);
    "ok"
}
"#,
    );
    insta::assert_snapshot!(render_tir(&db, file), @r#"
    function user.identity<T>(x: T) -> T throws never {
      { : T
        x : T
      }
    }
    function user.caller() -> string throws never {
      { : "ok"
        let x = identity<T>(42) : int
        "ok" : "ok"
      }
    }
    "#);
}

/// Companion to `explicit_type_arg_binds_directly`: same call site without `<int>`.
/// Shows inference picking the literal type `42` and widening (`42 -> int`),
/// distinguishable from the explicit-arg snapshot above (`: int`).
#[test]
fn bare_inference_picks_literal_type() {
    let mut db = make_db();
    let file = db.add_file(
        "test.baml",
        r#"
function identity<T>(x: T) -> T { x }
function caller() -> string {
    let x = identity(42);
    "ok"
}
"#,
    );
    insta::assert_snapshot!(render_tir(&db, file), @r#"
    function user.identity<T>(x: T) -> T throws never {
      { : T
        x : T
      }
    }
    function user.caller() -> string throws never {
      { : "ok"
        let x = identity<T>(42) : 42 -> int
        "ok" : "ok"
      }
    }
    "#);
}

/// Arity mismatch: function declares zero generic params but caller provides one.
/// Should produce a `WrongTypeArgArity` diagnostic at the call-site span.
#[test]
fn wrong_type_arg_arity_nongeneric() {
    let mut db = make_db();
    let file = db.add_file(
        "test.baml",
        r#"
function no_generics(x: int) -> int { x }
function caller() -> int {
    no_generics<int>(42)
}
"#,
    );
    insta::assert_snapshot!(render_tir(&db, file), @r"
    function user.no_generics(x: int) -> int throws never {
      { : int
        x : int
      }
    }
    function user.caller() -> int throws never {
      { : int
        no_generics(42) : int
      }
      !! 70..94: function `no_generics` expects 0 type argument(s), got 1
    }
    ");
}

/// Arity mismatch: function declares one generic param but caller provides two.
/// On arity failure, explicit-arg resolution returns `None` and the call falls
/// back to ordinary inference — `T = 42` (literal) — which then unions with the
/// caller's declared `-> int` return type, producing `int | 42` at the body.
/// The `WrongTypeArgArity` diagnostic is the primary correctness signal here.
#[test]
fn wrong_type_arg_arity_too_many() {
    let mut db = make_db();
    let file = db.add_file(
        "test.baml",
        r#"
function identity<T>(x: T) -> T { x }
function caller() -> int {
    identity<int, string>(42)
}
"#,
    );
    insta::assert_snapshot!(render_tir(&db, file), @r"
    function user.identity<T>(x: T) -> T throws never {
      { : T
        x : T
      }
    }
    function user.caller() -> int throws never {
      { : int | 42
        identity<T>(42) : int | 42
      }
      !! 66..95: function `identity` expects 1 type argument(s), got 2
    }
    ");
}

/// Multiple type params: explicit binding of two type vars resolves cleanly.
#[test]
fn explicit_two_type_args() {
    let mut db = make_db();
    let file = db.add_file(
        "test.baml",
        r#"
function pair<A, B>(a: A, b: B) -> string { "ok" }
function caller() -> string {
    pair<int, string>(1, "hello")
}
"#,
    );
    insta::assert_snapshot!(render_tir(&db, file), @r#"
    function user.pair<A, B>(a: A, b: B) -> string throws never {
      { : "ok"
        "ok" : "ok"
      }
    }
    function user.caller() -> string throws never {
      { : string
        pair<A, B>(1, "hello") : string
      }
    }
    "#);
}
