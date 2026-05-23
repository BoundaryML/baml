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

/// TS-style instantiation expression in a let-binding.
///
/// `let cb = identity<string>;` produces a value whose type is the
/// non-generic substituted function signature. Inferring the lambda body
/// shows the bound `T = string`.
#[test]
fn instantiation_expression_binds_type_arg_into_value() {
    let mut db = make_db();
    let file = db.add_file(
        "test.baml",
        r#"
function identity<T>(x: T) -> T { x }
function caller() -> string {
    let cb = identity<string>;
    "ok"
}
"#,
    );
    insta::assert_snapshot!(render_tir(&db, file));
}

/// Multi-arg instantiation expression: `pair<int, string>` substitutes both
/// type variables into the resulting non-generic signature.
#[test]
fn instantiation_expression_with_multiple_type_args() {
    let mut db = make_db();
    let file = db.add_file(
        "test.baml",
        r#"
function pair<A, B>(a: A, b: B) -> string { "ok" }
function caller() -> string {
    let cb = pair<int, string>;
    "ok"
}
"#,
    );
    insta::assert_snapshot!(render_tir(&db, file));
}

/// Wrong arity on an instantiation expression: `identity<int, string>`
/// when `identity` takes one type parameter reports `WrongTypeArgArity`.
#[test]
fn instantiation_expression_wrong_arity_errors() {
    let mut db = make_db();
    let file = db.add_file(
        "test.baml",
        r#"
function identity<T>(x: T) -> T { x }
function caller() -> int {
    let cb = identity<int, string>;
    1
}
"#,
    );
    insta::assert_snapshot!(render_tir(&db, file));
}

/// Regression: passing a literal-returning lambda to a generic function
/// parameter `y: () -> T` must let T be inferred from the lambda body
/// rather than strict-checking the literal against the still-unbound `T`.
///
/// Previously this produced `type mismatch: expected T, got "hi"` because
/// the lambda's body was checked against `T` in a `let` context that
/// provided no upstream binding.  The fix routes an unbound-TypeVar
/// expected return through synthesis mode (`None`), letting the outer
/// call's argument bindings unify T.
#[test]
fn lambda_arg_to_generic_function_infers_typevar_from_body() {
    let mut db = make_db();
    let file = db.add_file(
        "test.baml",
        r#"
function f<T>(param2: bool, y: () -> T) -> T {
  return y()
}

function bar() -> void {
  let y = f(true, () => { return "hi"; });
}
"#,
    );
    let out = render_tir(&db, file);
    assert!(
        !out.contains("type mismatch"),
        "expected no diagnostic; got:\n{out}",
    );
}

/// Composite TypeVar in expected return: `y: () -> Box<T>`.  The
/// expected return contains `T` but isn't a bare `Ty::TypeVar`.  Body
/// inference must still happen in synthesis mode (so `infer_bindings`
/// sees the concrete `Box<int>` and binds `T = int`), and the surface
/// return must use the synthesized type rather than the unresolved
/// composite — otherwise the outer call's bindings never see the
/// concrete return and `T` stays unbound.
#[test]
fn lambda_returning_composite_typevar_infers_concrete() {
    // Lambda body is an expression (no `return` statement) so the body's
    // inferred type is the expression's type. With a `return` statement
    // the block diverges and has type `Never` — a separate, deeper
    // limitation in BAML's lambda inference that's orthogonal to this
    // surface_ret_ty fix.
    let mut db = make_db();
    let file = db.add_file(
        "test.baml",
        r#"
class Box<T> {
  value T
}
function f<T>(y: () -> Box<T>) -> Box<T> {
  return y()
}
function bar() -> int {
  let b = f(() => { Box<int> { value: 1 } });
  b.value
}
"#,
    );
    let out = render_tir(&db, file);
    assert!(
        !out.contains("type mismatch"),
        "expected no diagnostic; got:\n{out}",
    );
    // `b` must be typed `Box<int>` (not `Box<T>`) and `b.value` `int`
    // — the let binding's rendering proves the outer call's argument
    // inference saw the concrete lambda return and bound `T = int`.
    assert!(
        out.contains("let b = ") && out.contains(": user.Box<int>"),
        "expected `b: Box<int>` after lambda-driven inference; got:\n{out}",
    );
    assert!(
        out.contains("b.value : int"),
        "expected `b.value : int`; got:\n{out}",
    );
}

/// Property access after an instantiation expression: `f<int>.method`.
/// BAML's parser accepts it (`.` is in our follow set) and the
/// FIELD_ACCESS_EXPR parent of EXPR_WITH_TYPE_ARGS short-circuits the
/// `Expr::Instantiation` wrap.  This locks in the current behavior; a
/// follow-up could choose to error like TS (TS1477) but for now we accept.
#[test]
fn instantiation_followed_by_member_access() {
    let mut db = make_db();
    let file = db.add_file(
        "test.baml",
        r#"
class Holder<T> {
    value T
    function get(self) -> T { self.value }
}
function caller() -> int {
    let inst = Holder<int> { value: 5 };
    inst.get()
}
"#,
    );
    insta::assert_snapshot!(render_tir(&db, file));
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
