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
    boom
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
    baml.panics.DivisionByZero
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

// ============================================================================
// §N+3 — Regression (B-613): a panic escaping to the host must bypass the
// outer function's declared `throws` contract.
// ============================================================================

/// Assert that an uncaught throw surfaced from `main` is a clean panic
/// `Instance` of `expected_class` (not wrapped in a union and not a leaked
/// engine error).
fn assert_clean_panic(output: &baml_tests::engine::TestOutput, expected_class: &str) {
    let Err(bex_engine::EngineError::UnhandledThrow { value, .. }) = &output.result else {
        panic!("expected UnhandledThrow, got: {:?}", output.result);
    };
    let BexExternalValue::Instance { class_name, .. } = value.as_ref() else {
        panic!("expected panic Instance, got: {value:?}");
    };
    assert_eq!(class_name, expected_class);
}

/// The canonical B-613 repro: a `StackOverflow` panic unwinds out of a function
/// whose `throws` clause is a 2+-member union. Before the fix the engine
/// re-typed the escaping panic against the declared union (via
/// `find_matching_member`, which never matches a `baml.panics.*` value) and
/// leaked an internal `EngineError::TypeMismatch` naming `QualifiedTypeName` /
/// `TyAttr`. It must instead surface the clean `baml.panics.StackOverflow`.
#[tokio::test]
async fn union_throws_panic_escapes_as_clean_panic() {
    let output = baml_test!(
        r#"
function boom(n: int) -> int throws baml.errors.ParseError | baml.errors.InvalidArgument {
  boom(n + 1)
}

function main() -> int {
  boom(0)
}
"#
    );
    assert_clean_panic(&output, "baml.panics.StackOverflow");
}

/// Parity: a panic escaping to the host surfaces identically regardless of the
/// outer function's `throws` shape — no-throws, single-member, 2-member union,
/// and 3-member union all yield the same clean `baml.panics.DivisionByZero`.
/// Uses a `DivisionByZero` panic (deterministic and cheap) rather than
/// recursion. Before the fix only the 2+-member union shapes leaked; this
/// documents that the asymmetry is gone.
#[tokio::test]
async fn panic_escapes_all_throws_shapes_identically() {
    const NO_THROWS: &str = r#"
function boom() -> int {
  1 / 0
}
function main() -> int { boom() }
"#;
    const SINGLE: &str = r#"
function boom() -> int throws baml.errors.ParseError {
  1 / 0
}
function main() -> int { boom() }
"#;
    const UNION2: &str = r#"
function boom() -> int throws baml.errors.ParseError | baml.errors.InvalidArgument {
  1 / 0
}
function main() -> int { boom() }
"#;
    const UNION3: &str = r#"
function boom() -> int throws baml.errors.ParseError | baml.errors.InvalidArgument | baml.errors.Io {
  1 / 0
}
function main() -> int { boom() }
"#;

    for source in [NO_THROWS, SINGLE, UNION2, UNION3] {
        let output = baml_test!(source);
        assert_clean_panic(&output, "baml.panics.DivisionByZero");
    }
}

/// A `baml.panics.Exit { code }` escaping a 2-member union `throws` must still
/// funnel through the clean process-exit path (`extract_exit_code`) rather than
/// tripping the union re-typing — the panic bypass routes it through
/// `vm_value_to_owned`, so Exit is recognized and surfaces as
/// `EngineError::Exit { code }`.
#[tokio::test]
async fn exit_panic_escapes_union_throws_as_clean_exit() {
    let output = baml_test!(
        r#"
function boom() -> int throws baml.errors.ParseError | baml.errors.InvalidArgument {
  baml.sys.exit(3)
}

function main() -> int {
  boom()
}
"#
    );
    let Err(bex_engine::EngineError::Exit { code }) = output.result else {
        panic!("expected Exit, got: {:?}", output.result);
    };
    assert_eq!(code, 3);
}

/// Guard against over-bypassing: a *genuine* in-contract error (a real
/// `baml.errors.ParseError` thrown and left uncaught) escaping a 2-member union
/// `throws` must still get its proper union-metadata wrapping — the panic
/// bypass only fires for `baml.panics.*` values, not for declared throws.
#[tokio::test]
async fn genuine_in_contract_throw_still_wrapped_through_union() {
    let output = baml_test!(
        r#"
function boom() -> int throws baml.errors.ParseError | baml.errors.InvalidArgument {
  throw baml.errors.ParseError { message: "bad json" }
}

function main() -> int {
  boom()
}
"#
    );
    let Err(bex_engine::EngineError::UnhandledThrow { value, .. }) = &output.result else {
        panic!("expected UnhandledThrow, got: {:?}", output.result);
    };
    // Genuine in-contract throws are wrapped with union metadata for the wire;
    // panics are not. Confirm the wrapping is preserved.
    let BexExternalValue::Union { value: inner, .. } = value.as_ref() else {
        panic!("expected union-wrapped in-contract throw, got: {value:?}");
    };
    let BexExternalValue::Instance { class_name, .. } = inner.as_ref() else {
        panic!("expected Instance inside union, got: {inner:?}");
    };
    assert_eq!(class_name, "baml.errors.ParseError");
}
