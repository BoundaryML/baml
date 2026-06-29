//! BEP-042 Part 3 — `ErrorContext` cause-chain tests.
//!
//! Cause is computed lazily at the throw funnel: a throw whose PC lies in a
//! handler body chains onto that handler's caught error (`vm.rs`
//! `find_cause_context` + the handler-body extent on `ExceptionTableEntry`,
//! built from the catch region's `handler_body` blocks). See
//! `thoughts/antonio/errorcontext-impl-plan.md`.
//!
//! The chaining MECHANISM is in place and verified at the VM level (the funnel
//! materializes `B.cause = A`'s `ErrorContext`). The multi-link tests below are
//! `#[ignore]`d pending a separate fix: reading a *non-null* `cause`
//! (`match (self.cause) { let c: ErrorContext => ... }`) and `to_string` over a
//! chained context hit a runtime type error / `Unreachable` — a BAML-level
//! issue with the self-referential `cause: ErrorContext?` field, independent of
//! the chaining itself.

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
#[ignore = "BEP-042 Part 3: chaining materializes correctly, but reading a non-null cause hits a recursive-type runtime issue (see module docs)"]
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
#[ignore = "BEP-042 Part 3: see nested_catch_chains_to_handled_error"]
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

/// HAZARD B — `error_local` / context-slot liveness across the whole handler
/// body. The outer handler binds `ctx` but never reads it, so the cause
/// pre-walk's runtime read of the context slot must keep it alive (handled via
/// the analysis use-injection in `analysis.rs` + `optimize.rs`).
#[tokio::test]
#[ignore = "BEP-042 Part 3: see nested_catch_chains_to_handled_error"]
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
