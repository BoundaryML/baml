//! BEP-042 Part 3 — `ErrorContext` cause-chain tests.
//!
//! Cause is computed lazily at the throw funnel: a throw whose PC lies in a
//! handler body chains onto that handler's caught error (`vm.rs`
//! `find_cause_context` + the handler-body extent on `ExceptionTableEntry`,
//! built from the catch region's `handler_body` blocks). See
//! `thoughts/antonio/errorcontext-impl-plan.md`.

use baml_tests::baml_test;
use bex_engine::BexExternalValue;

#[track_caller]
fn expect_string(v: BexExternalValue) -> String {
    match v {
        BexExternalValue::String(s) => s.to_string(),
        other => panic!("expected String, got {other:?}"),
    }
}

/// Shipped single-link behavior: a lone caught error has no cause, so
/// `root_cause()` is the error itself.
#[tokio::test]
async fn single_error_root_cause_is_self() {
    let output = baml_test!(
        r#"
function boom() -> string { throw "kaboom" }

function main() -> string {
  boom() catch (e, ctx) {
    _ => {
      match (ctx.root_cause().error) {
        let s: string => s
        _ => "unexpected non-string error"
      }
    }
  }
}
"#
    );
    assert_eq!(expect_string(output.result.unwrap()), "kaboom");
}

/// Nested `catch`: throwing a different error while handling one chains the
/// new error onto the error being handled (Python `__context__`-style).
#[tokio::test]
async fn nested_catch_chains_to_handled_error() {
    let output = baml_test!(
        r#"
function fail_a() -> string { throw "A" }
function fail_b() -> string { throw "B" }

function main() -> string {
  fail_a() catch (e, ctx) {
    _ => {
      // B is thrown while A is being handled -> B.cause == A.
      fail_b() catch (e2, ctx2) {
        _ => {
          match (ctx2.root_cause().error) {
            let s: string => s
            _ => "no cause found"
          }
        }
      }
    }
  }
}
"#
    );
    assert_eq!(expect_string(output.result.unwrap()), "A");
}

/// HAZARD A — handler-body PC coverage.
///
/// A throw inside a `catch` arm body must chain onto the error that arm is
/// handling, even when the arm contains out-of-line constructs (a nested
/// `defer` pad and a nested `catch`) that fragment the handler body across
/// basic blocks.
#[tokio::test]
async fn hazard_a_nested_construct_in_catch_arm_chains_to_outer_error() {
    let output = baml_test!(
        r#"
function fail_outer() -> string { throw "E" }
function fail_inner() -> string { throw "Y" }

function main() -> string {
  fail_outer() catch (e, ctx) {
    _ => {
      defer { }
      fail_inner() catch (e2, ctx2) {
        _ => {
          match (ctx2.root_cause().error) {
            let s: string => s
            _ => "MISSED: enclosing handled error not found in chain"
          }
        }
      }
    }
  }
}
"#
    );
    assert_eq!(expect_string(output.result.unwrap()), "E");
}

/// A *rethrow* must not graft a spurious cause link. When a `catch` has no
/// matching arm, the error re-raises (the no-match fall-through, which the VM
/// flags as a rethrow). The re-raise executes from the catch's own dispatch
/// block, which lies inside that catch's handler body — so without the rethrow
/// guard the cause pre-walk would chain the re-raised "B" onto the very error
/// the catch is examining (also "B"), a self-link. Because nothing was being
/// handled when "B" arose, its `cause` must stay empty.
#[tokio::test]
async fn no_match_rethrow_does_not_self_chain() {
    let output = baml_test!(
        r#"
function fail_b() -> string { throw "B" }

function main() -> string {
  // The inner catch binds `ictx`, so "B" is materialized into its context
  // slot. The arm does not match the string "B", so "B" re-raises from the
  // dispatch block — which must NOT chain "B" onto its own context.
  (
    fail_b() catch (inner, ictx) {
      baml.errors.InvalidArgument => { "unreachable" }
    }
  ) catch (e2, ctx2) {
    _ => {
      match (ctx2.cause) {
        null => "no cause"
        let c: baml.errors.ErrorContext => "MISCHAINED: rethrow self-linked"
      }
    }
  }
}
"#
    );
    assert_eq!(expect_string(output.result.unwrap()), "no cause");
}

/// Per-block handler-body coverage. A throw in code laid out in the PC *gap*
/// between a catch arm's fragmented blocks must not be attributed to that
/// catch. The first catch fully handles "A"; its arm is fragmented by a nested
/// `defer` whose pad is laid out out-of-line, so the later `throw "C"` sits
/// between the arm's main block and that pad. A single `[handler_pc, max_end)`
/// span would over-cover it and mis-chain "C" onto "A"; per-block ranges keep
/// it outside, so "C" has no cause.
#[tokio::test]
async fn throw_after_catch_in_layout_gap_does_not_chain() {
    let output = baml_test!(
        r#"
function fail_a() -> string { throw "A" }
function fail_c() -> string { throw "C" }

function main() -> string {
  let recovered = fail_a() catch (e, ctx) {
    _ => {
      defer { }
      "recovered"
    }
  }
  // "A" is fully handled; this is a brand-new failure, not "during handling
  // of A". Its context must have no cause.
  fail_c() catch (e2, ctx2) {
    _ => {
      match (ctx2.cause) {
        null => recovered
        let c: baml.errors.ErrorContext => "MISCHAINED: gap over-covered"
      }
    }
  }
}
"#
    );
    assert_eq!(expect_string(output.result.unwrap()), "recovered");
}

/// Sibling defers that throw while a scope unwinds form a cause chain. `scope`
/// throws "X"; three sibling defers each throw while unwinding. They run LIFO
/// (C, then B, then A), and each throw is "during handling of" the previous
/// in-flight error, so the surviving error "A" chains A -> B -> C -> X.
/// `root_cause` walks to the original failure "X".
#[tokio::test]
async fn sibling_defers_that_throw_chain_to_root_cause() {
    let output = baml_test!(
        r#"
function scope() -> string {
  defer { throw "A" }
  defer { throw "B" }
  defer { throw "C" }
  throw "X"
}

function main() -> string {
  scope() catch (e, ctx) {
    _ => {
      match (ctx.root_cause().error) {
        let s: string => s
        _ => "no root cause found"
      }
    }
  }
}
"#
    );
    assert_eq!(expect_string(output.result.unwrap()), "X");
}

/// HAZARD B — `error_local` / context-slot liveness across the whole handler
/// body. The outer handler binds `ctx` but never reads it, so the cause
/// pre-walk's runtime read of the context slot must keep it alive (handled via
/// the analysis use-injection in `analysis.rs` + `optimize.rs`).
#[tokio::test]
async fn hazard_b_unused_binding_slot_liveness_preserves_cause() {
    let output = baml_test!(
        r#"
function fail_outer() -> string { throw "E" }
function fail_inner() -> string { throw "Y" }

function main() -> string {
  fail_outer() catch (e, ctx) {
    _ => {
      let a = 1 + 2
      let b = a * a
      let c = b + a
      fail_inner() catch (e2, ctx2) {
        _ => {
          match (ctx2.root_cause().error) {
            let s: string => s
            _ => "GARBAGE: outer context slot was recolored before the inner throw"
          }
        }
      }
    }
  }
}
"#
    );
    assert_eq!(expect_string(output.result.unwrap()), "E");
}
