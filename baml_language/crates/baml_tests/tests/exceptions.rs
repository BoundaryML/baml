//! Exception handling tests: catch/throw/panic semantics.
//!
//! Progression from simple to complex, covering all catch arm pattern types:
//!   - Literal value patterns: "string" =>, 42 =>
//!   - Typed bindings: string =>, MyClass =>
//!   - Bare type sugar: baml.panics.DivisionByZero =>
//!   - Wildcard: _ =>
//!   - User-defined error classes
//!   - Multi-arm dispatch with mixed pattern types
//!   - Panics vs user throws
//!   - Nested, rethrow, sequential

use baml_tests::baml_test;
use bex_engine::BexExternalValue;

// ============================================================================
// §N — Stack trace tests
// ============================================================================

#[tokio::test]
async fn exception_stack_trace_through_closures() {
    let output = baml_test!(
        r#"
function inner() -> int {
  throw "from_closure"
}

function outer() -> int {
  let f = inner
  f()
}

function main() -> int {
  outer()
}
"#
    );

    let err = output.result.unwrap_err();
    insta::assert_snapshot!(err, @r#"
    Traceback (most recent call last):
      File "test.baml", line 12, in user.main
      File "test.baml", line 8, in user.outer
      File "test.baml", line 3, in user.inner
    uncaught throw: String("from_closure")
    "#);
}

#[tokio::test]
async fn exception_stack_trace_on_panic() {
    let output = baml_test!(
        r#"
function divider(x: int) -> int {
  x / 0
}

function caller() -> int {
  divider(42)
}

function main() -> int {
  caller()
}
"#
    );

    let err = output.result.unwrap_err();
    insta::assert_snapshot!(err, @r#"
    Traceback (most recent call last):
      File "test.baml", line 11, in user.main
      File "test.baml", line 7, in user.caller
      File "test.baml", line 3, in user.divider
    uncaught throw: Instance { class_name: "baml.panics.DivisionByZero", fields: {"dividend": Int(42)} }
    "#);
}

// ============================================================================
// §N+1 — catch (e, stack_trace) binding
// ============================================================================

#[tokio::test]
async fn catch_with_stack_trace_binding() {
    let output = baml_test!(
        r#"
function inner() -> string {
  throw "boom"
}

function main() -> string {
  inner() catch (e, st) {
    _ => { st.to_string() }
  }
}
"#
    );

    let BexExternalValue::String(st) = output.result.unwrap() else {
        panic!("expected String variant");
    };
    insta::assert_snapshot!(st, @r#"
    Traceback (most recent call last):
      File "test.baml", line 7, in user.main
      File "test.baml", line 3, in user.inner
    "#);
}

#[tokio::test]
async fn catch_stack_trace_on_panic() {
    let output = baml_test!(
        r#"
function divider() -> int {
  42 / 0
}

function main() -> int | string {
  divider() catch (e, st) {
    baml.panics.DivisionByZero => { st.to_string() }
  }
}
"#
    );

    let result = output.result.unwrap();
    let st = match result {
        BexExternalValue::String(s) => s,
        BexExternalValue::Union { value, .. } => match *value {
            BexExternalValue::String(s) => s,
            other => panic!("expected String inside Union, got: {other:?}"),
        },
        other => panic!("expected String or Union, got: {other:?}"),
    };
    insta::assert_snapshot!(st, @r#"
    Traceback (most recent call last):
      File "test.baml", line 7, in user.main
      File "test.baml", line 3, in user.divider
    "#);
}
