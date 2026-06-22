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

use std::sync::Arc;

use baml_tests::{
    baml_test,
    engine::{OptLevel, compile_source_with_opt},
};
use bex_engine::{BexEngine, BexExternalValue, FunctionCallContextBuilder};
use bex_vm_types::{Instruction, Object, ObjectIndex};
use sys_native::SysOpsExt;

// ============================================================================
// §VM — bytecode-level panic regression tests
// ============================================================================

#[tokio::test]
async fn catch_stale_field_slot_invalid_field_access() {
    let mut program = compile_source_with_opt(
        r#"
        class Short {
            x int
        }

        function oob() -> int {
            let keep = Short { x: 0 };
            keep.x;
            0
        }

        function main() -> int {
            oob() catch (e) {
                let err: baml.panics.InvalidFieldAccess => err.field_index
            }
        }
        "#,
        OptLevel::One,
    );

    let class_idx = program
        .objects
        .iter()
        .position(|object| {
            matches!(
                object,
                Object::Class(class) if class.name.display_name().as_str() == "Short"
            )
        })
        .expect("Short class should exist");

    let oob_idx = program
        .function_index("user.oob")
        .expect("user.oob should exist");
    let Object::Function(func) = program
        .objects
        .get_mut(oob_idx)
        .expect("user.oob object should exist")
    else {
        panic!("user.oob should be a function");
    };

    func.bytecode.instructions = vec![
        Instruction::AllocInstance {
            class_obj: ObjectIndex::from_raw(class_idx),
            ntypeargs: 0,
        },
        Instruction::LoadField(1),
        Instruction::Return,
    ];
    func.bytecode.compact = None;

    let engine = Arc::new(
        BexEngine::new(program, Arc::new(sys_ops::SysOps::native()), Vec::new()).expect("engine"),
    );

    let result = engine
        .call_function_bound_args(
            "user.main",
            Vec::new(),
            FunctionCallContextBuilder::new(sys_types::CallId::next()).build(),
            true,
        )
        .await;

    assert_eq!(result, Ok(BexExternalValue::Int(1)));
}

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
    uncaught throw: Instance { class_name: "baml.panics.DivisionByZero", type_args: [], fields: {"dividend": Int(42)} }
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

// ============================================================================
// §N+2 — Regression: nested catch routing for cross-frame throws
// ============================================================================

/// Regression: a throw propagating out of a CALLED function must be caught by
/// the INNERMOST enclosing `catch`, not the outermost.
///
/// Inline `throw`s are lowered as a direct jump to the handler, but a throw
/// that escapes a callee — or a runtime panic — reaches the caller's exception
/// table. The VM now selects the innermost covering entry (largest `start_pc`)
/// rather than the first; previously it picked the outermost and mis-routed.
///
/// Here `boom()` throws inside `boom`; the inner `catch (e) { _ => 1 }` handles
/// it (→ 1). (Paired fix: a same-frame rethrow now syncs the dispatch `pc` to
/// the handler so the enclosing block aborts instead of falling through — see
/// the `baml_src/ns_exceptions` wildcard panic-rethrow tests.)
#[tokio::test]
async fn nested_catch_inner_catches_callee_throw() {
    let output = baml_test!(
        r#"
function boom() -> void { throw "x" }

function main() -> int {
  ({ boom(); 0 } catch (e) { _ => 1 }) catch (e2) { _ => 2 }
}
"#
    );
    assert_eq!(output.result.unwrap(), BexExternalValue::Int(1));
}
