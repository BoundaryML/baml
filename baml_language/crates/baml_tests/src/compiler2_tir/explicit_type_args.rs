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

use super::support::{make_db, render_ppir, render_tir};
use crate::engine::TestDbExt;

#[test]
fn nested_runtime_type_binding_render_preserves_operand_name() {
    let mut db = make_db();
    let file = db.file(
        "test.baml",
        r#"
class Wrapper<T> { value T }

function caller(t: reflect.Type) -> bool {
    type T = Wrapper<unreflect(t)>
    true
}
"#,
    );

    let tir = render_tir(&db, file);
    assert!(
        tir.contains("type T = Wrapper<unreflect(t)> : type"),
        "typed rendering lost the nested operand:\n{tir}"
    );
    let ppir = render_ppir(&db, file);
    assert!(
        ppir.contains("type T = user.Wrapper<unreflect(t)>"),
        "HIR rendering lost the nested operand:\n{ppir}"
    );
}

/// Explicit type args bind T directly, distinct from what inference would produce.
///
/// In a let-binding with no type annotation, `identity(42)` would infer `T = 42`
/// (the literal type), then widen to `int` (`42 -> int`). With explicit `<int>`,
/// T is bound to `int` directly, so the call's type is plain `int` (no widening arrow).
#[test]
fn explicit_type_arg_binds_directly() {
    let mut db = make_db();
    let file = db.file(
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
        let x = identity(42) : int
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
    let file = db.file(
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
        let x = identity(42) : int
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
    let file = db.file(
        "test.baml",
        r#"
function no_generics(x: int) -> int { x }
function caller() -> int {
    no_generics<int>(42)
}
"#,
    );
    insta::assert_snapshot!(render_tir(&db, file), @"
    function user.no_generics(x: int) -> int throws never {
      { : int
        x : int
      }
    }
    function user.caller() -> int throws never {
      { : int
        no_generics(42) : int
      }
      !! 74..94: function `no_generics` expects 0 type argument(s), got 1
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
    let file = db.file(
        "test.baml",
        r#"
function identity<T>(x: T) -> T { x }
function caller() -> int {
    identity<int, string>(42)
}
"#,
    );
    insta::assert_snapshot!(render_tir(&db, file), @"
    function user.identity<T>(x: T) -> T throws never {
      { : T
        x : T
      }
    }
    function user.caller() -> int throws never {
      { : int
        identity(42) : int
      }
      !! 70..95: function `identity` expects 1 type argument(s), got 2
    }
    ");
}

/// Instantiation expression: `foo<int>` as a *value* (not called) binds T=int
/// and produces a concrete, specialized function type `(x: int) -> int`.
#[test]
fn generic_apply_value_is_specialized() {
    let mut db = make_db();
    let file = db.file(
        "test.baml",
        r#"
function identity<T>(x: T) -> T { x }
function caller() -> string {
    let f = identity<int>;
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
        let f = identity<...> : (x: int) -> int throws never
        "ok" : "ok"
      }
    }
    "#);
}

/// Regression for the silent-drop bug: `let f = identity<int>; f("s")` must be a
/// type error — the specialized value takes `int`, not `string`. Before
/// instantiation expressions existed, `identity<int>` collapsed to the fully
/// generic `identity` and `f("s")` was wrongly accepted by re-inferring T=string.
#[test]
fn generic_apply_value_rejects_wrong_arg() {
    let mut db = make_db();
    let file = db.file(
        "test.baml",
        r#"
function identity<T>(x: T) -> T { x }
function caller() -> int {
    let f = identity<int>;
    f("string")
}
"#,
    );
    insta::assert_snapshot!(render_tir(&db, file), @r#"
    function user.identity<T>(x: T) -> T throws never {
      { : T
        x : T
      }
    }
    function user.caller() -> int throws never {
      { : int
        let f = identity<...> : (x: int) -> int throws never
        f("string") : int
      }
      !! 99..107: type mismatch: expected int, got "string"
    }
    "#);
}

/// Companion: calling the specialized value with a matching `int` is accepted.
#[test]
fn generic_apply_value_accepts_right_arg() {
    let mut db = make_db();
    let file = db.file(
        "test.baml",
        r#"
function identity<T>(x: T) -> T { x }
function caller() -> int {
    let f = identity<int>;
    f(1)
}
"#,
    );
    insta::assert_snapshot!(render_tir(&db, file), @"
    function user.identity<T>(x: T) -> T throws never {
      { : T
        x : T
      }
    }
    function user.caller() -> int throws never {
      { : int
        let f = identity<...> : (x: int) -> int throws never
        f(1) : int
      }
    }
    ");
}

/// Arity mismatch on an instantiation expression: `identity<int, string>` (1
/// declared param, 2 provided) → `WrongTypeArgArity`.
#[test]
fn generic_apply_value_arity_mismatch() {
    let mut db = make_db();
    let file = db.file(
        "test.baml",
        r#"
function identity<T>(x: T) -> T { x }
function caller() -> string {
    let f = identity<int, string>;
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
        let f = identity<...> : (x: int) -> int throws never
        "ok" : "ok"
      }
      !! 81..102: function `identity` expects 1 type argument(s), got 2
    }
    "#);
}

/// Multiple bound type args as a *value*: `pair<int, string>` specializes BOTH
/// params, yielding the concrete `(a: int, b: string) -> string`.
#[test]
fn generic_apply_two_type_args_specialized() {
    let mut db = make_db();
    let file = db.file(
        "test.baml",
        r#"
function pair<A, B>(a: A, b: B) -> string { "ok" }
function caller() -> string {
    let f = pair<int, string>;
    "done"
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
      { : "done"
        let f = pair<...> : (a: int, b: string) -> string throws never
        "done" : "done"
      }
    }
    "#);
}

/// Multiple bound type args reject a mismatched argument: with `pair<int, string>`,
/// calling `f(1, 2)` is a type error on the second (`string`) parameter, proving
/// each param specialized independently (the first `int` arg is fine).
#[test]
fn generic_apply_two_type_args_rejects_wrong_arg() {
    let mut db = make_db();
    let file = db.file(
        "test.baml",
        r#"
function pair<A, B>(a: A, b: B) -> string { "ok" }
function caller() -> string {
    let f = pair<int, string>;
    f(1, 2)
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
        let f = pair<...> : (a: int, b: string) -> string throws never
        f(1, 2) : string
      }
      !! 122..123: type mismatch: expected string, got 2
    }
    "#);
}

/// A parenthesized base `(identity)<int>` is *rejected* — only a bare path
/// reference may be specialized into a value — but lowering recovers by still
/// specializing the inner path, so there is no cascading "must be specialized"
/// error and the value's type is the concrete `(x: int) -> int` shown below. The
/// rejection itself (`TypeArgsOnNonPathBase`) is an AST-lowering diagnostic that
/// `render_tir` does not surface; `parenthesized_generic_apply_is_rejected`
/// (interfaces.rs) covers it.
#[test]
fn generic_apply_through_parenthesized_receiver() {
    let mut db = make_db();
    let file = db.file(
        "test.baml",
        r#"
function identity<T>(x: T) -> T { x }
function caller() -> string {
    let f = (identity)<int>;
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
        let f = identity<...> : (x: int) -> int throws never
        "ok" : "ok"
      }
    }
    "#);
}

/// Multiple type params: explicit binding of two type vars resolves cleanly.
#[test]
fn explicit_two_type_args() {
    let mut db = make_db();
    let file = db.file(
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
        pair(1, "hello") : string
      }
    }
    "#);
}

/// A *param-dependent instantiation value* (`let f = foo<T>` inside a generic
/// body) is a function value whose type `(T) -> T` mentions the enclosing,
/// rigid `T`. Calling it with a mismatched concrete argument must be rejected —
/// `T` is fixed by `pd`'s caller, so `f(1)` is a type error rather than silently
/// re-inferring `T = int` and collapsing `foo<T>` to `foo<int>`.
#[test]
fn instantiation_value_call_keeps_ambient_typevar_rigid() {
    let mut db = make_db();
    let file = db.file(
        "test.baml",
        r#"
function identity<T>(x: T) -> T { x }
function pd<T>(y: T) -> int {
    let f = identity<T>;
    f(1)
}
"#,
    );
    insta::assert_snapshot!(render_tir(&db, file), @"
    function user.identity<T>(x: T) -> T throws never {
      { : T
        x : T
      }
    }
    function user.pd<T>(y: T) -> int throws never {
      { : T
        let f = identity<...> : (x: T) -> T throws never
        f<T>(1) : T
      }
      !! 67..104: type mismatch: expected int, got T
      !! 100..101: type mismatch: expected T, got 1
    }
    ");
}

/// Companion to the above: calling the rigid instantiation value with a
/// *matching* argument (`y : T` inside `fwd<T>`) is fine. By contrast, a bare
/// *uninstantiated* generic reference (`let g = identity`) is an unrealized
/// function value and is rejected — it must be specialized before use.
#[test]
fn instantiation_value_call_preserves_valid_inference() {
    let mut db = make_db();
    let file = db.file(
        "test.baml",
        r#"
function identity<T>(x: T) -> T { x }
function fwd<T>(y: T) -> T {
    let f = identity<T>;
    f(y)
}
function uses() -> int {
    let g = identity;
    g(5)
}
"#,
    );
    insta::assert_snapshot!(render_tir(&db, file), @"
    function user.identity<T>(x: T) -> T throws never {
      { : T
        x : T
      }
    }
    function user.fwd<T>(y: T) -> T throws never {
      { : T
        let f = identity<...> : (x: T) -> T throws never
        f<T>(y) : T
      }
    }
    function user.uses() -> int throws never {
      { : int
        let g = identity : (x: int) -> int throws never
        g(5) : int
      }
      !! 141..149: generic function `identity` needs concrete type arguments before it can be stored in `g`. Specialize it explicitly, for example `identity<int>`. Or write the concrete function type after the binding name: `let g: (int) -> int throws never = identity`. Calling `identity(...)` directly works only when that call's arguments or expected result determine every type argument
    }
    ");
}

/// A generic *lambda* (`<T>(x: T) -> T { x }`) is a function value parameterizing
/// itself, which is rejected with a hard error — even wrapped in parentheses with
/// explicit type args. The invalid lambda takes the error type, so the surrounding
/// `<int>` application and the call recover without cascading.
#[test]
fn paren_generic_lambda_instantiation() {
    let mut db = make_db();
    let file = db.file(
        "test.baml",
        r#"
function caller() -> int {
    let f = (<T>(x: T) -> T { x })<int>;
    f("string")
}
"#,
    );
    insta::assert_snapshot!(render_tir(&db, file), @r#"
    function user.caller() -> int throws never {
      { : !error
        let f = : (x: !error) -> !error throws never
          (x: T) -> T { ... } : (x: !error) -> !error throws never
            {
              x
            }
        f("string") : !error
      }
      !! 48..49: unresolved type: T
      !! 54..55: unresolved type: T
    }
    lambda user.caller {
    }
    "#);
}
