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

/// A non-throwing `defer` on the unwind frame must NOT wipe the propagating
/// error's cause chain. The pad's transparent re-raise must preserve the cause
/// computed at the throw site, so `root_cause` still walks past the re-raised
/// error to the original error.
#[tokio::test]
async fn defer_on_unwind_preserves_cause_chain() {
    let output = baml_test!(
        r#"
function fail_low() -> string { throw "origin" }

function mid() -> string {
  defer { }
  fail_low() catch (e, ctx) {
    _ => throw "wrap"
  }
}

function main() -> string {
  mid() catch (e, ctx) {
    _ => {
      match (ctx.root_cause().error) {
        let s: string => s
        _ => "LOST: cause chain wiped by the defer re-raise"
      }
    }
  }
}
"#
    );
    assert_eq!(expect_string(output.result.unwrap()), "origin");
}

/// The rendered chain must preserve "During handling of..." sections across the
/// defer re-raise: the full error chain must survive in the string output.
#[tokio::test]
async fn defer_on_unwind_preserves_to_string_chain() {
    let output = baml_test!(
        r#"
function fail_low() -> string { throw "origin" }

function mid() -> string {
  defer { }
  fail_low() catch (e, ctx) {
    _ => throw "wrap"
  }
}

function main() -> string {
  mid() catch (e, ctx) {
    _ => ctx.to_string()
  }
}
"#
    );
    let rendered = expect_string(output.result.unwrap());
    assert!(
        rendered.contains("During handling of the above error, another error occurred"),
        "missing chain separator; the defer re-raise dropped the cause:\n{rendered}"
    );
    assert!(
        rendered.contains("origin"),
        "missing the original error frame:\n{rendered}"
    );
    assert!(
        rendered.contains("wrap"),
        "missing the superseding error frame:\n{rendered}"
    );
}

/// A non-throwing defer body must preserve the cause chain: the pad's
/// transparent re-raise must survive even when the defer body has effects
/// (mutating an outer local).
#[tokio::test]
async fn defer_with_body_preserves_cause_chain() {
    let output = baml_test!(
        r#"
function fail_low() -> string { throw "origin" }

function mid() -> string {
  let marker = "start"
  defer { marker = marker + "-cleanup" }
  fail_low() catch (e, ctx) {
    _ => throw "wrap"
  }
}

function main() -> string {
  mid() catch (e, ctx) {
    _ => {
      match (ctx.root_cause().error) {
        let s: string => s
        _ => "LOST: cause chain wiped by the defer re-raise"
      }
    }
  }
}
"#
    );
    assert_eq!(expect_string(output.result.unwrap()), "origin");
}

/// Multiple stacked non-throwing defers must each preserve the cause chain via
/// their transparent re-raises.
#[tokio::test]
async fn nested_defers_nonthrowing_preserve_cause() {
    let output = baml_test!(
        r#"
function fail_low() -> string { throw "origin" }

function mid() -> string {
  defer { }
  defer { }
  fail_low() catch (e, ctx) {
    _ => throw "wrap"
  }
}

function main() -> string {
  mid() catch (e, ctx) {
    _ => {
      match (ctx.root_cause().error) {
        let s: string => s
        _ => "LOST: cause chain wiped by a stacked defer re-raise"
      }
    }
  }
}
"#
    );
    assert_eq!(expect_string(output.result.unwrap()), "origin");
}

/// When a defer body itself throws, the full error chain must be preserved:
/// the new error chains onto the error being handled, and pre-existing chain
/// links are not dropped (the defer adds a fresh link on top of a pre-existing
/// chain).
#[tokio::test]
async fn defer_that_throws_still_chains_through_wrap() {
    let output = baml_test!(
        r#"
function fail_low() -> string { throw "origin" }

function mid() -> string {
  defer { throw "cleanup" }
  fail_low() catch (e, ctx) {
    _ => throw "wrap"
  }
}

function main() -> string {
  mid() catch (e, ctx) {
    _ => ctx.to_string()
  }
}
"#
    );
    let rendered = expect_string(output.result.unwrap());
    // Oldest -> newest: origin, wrap, cleanup — all three present, in order.
    let origin_at = rendered.find("origin").expect("missing 'origin' frame");
    let wrap_at = rendered.find("wrap").expect("missing 'wrap' frame");
    let cleanup_at = rendered.find("cleanup").expect("missing 'cleanup' frame");
    assert!(
        origin_at < wrap_at && wrap_at < cleanup_at,
        "cause chain out of order or a link dropped:\n{rendered}"
    );
    assert_eq!(
        rendered
            .matches("During handling of the above error, another error occurred")
            .count(),
        2,
        "expected two chain separators (origin->wrap->cleanup):\n{rendered}"
    );
}

/// ErrorContext rendering runs inside a native call, so it cannot yield to
/// user `ToString` implementations. It must still preserve the qualified
/// thrown class name and recursively render its fields.
#[tokio::test]
async fn class_error_renders_qualified_name_and_structural_fields() {
    let output = baml_test!(
        r#"
class Detail {
  label string

  implements baml.ToString {
    function to_string(self) -> string throws never { "DETAIL OVERRIDE" }
  }
}

class AppError {
  message string
  detail Detail

  implements baml.ToString {
    function to_string(self) -> string throws never { "ERROR OVERRIDE" }
  }
}

function main() -> string {
  (
    throw AppError {
      message: "boom",
      detail: Detail { label: "visible" },
    }
  ) catch (e, ctx) {
    _ => ctx.to_string()
  }
}
"#
    );
    let rendered = expect_string(output.result.unwrap());
    assert!(
        rendered.contains(
            r#"user.AppError { message: "boom", detail: user.Detail { label: "visible" } }"#
        ),
        "class error lost its identity or fields:\n{rendered}"
    );
    assert!(
        !rendered.contains("OVERRIDE"),
        "ErrorContext unexpectedly dispatched a user ToString override:\n{rendered}"
    );
}

/// Arbitrary thrown values are legal. Their context should retain the value's
/// structural representation rather than replacing it with `<error>`.
#[tokio::test]
async fn non_class_error_renders_structurally() {
    let output = baml_test!(
        r#"
function main() -> string {
  (throw {"first": "alpha", "second": "beta"}) catch (e, ctx) {
    _ => ctx.to_string()
  }
}
"#
    );
    let rendered = expect_string(output.result.unwrap());
    assert!(
        rendered.contains(r#"{"first": "alpha", "second": "beta"}"#),
        "non-class error was not rendered structurally:\n{rendered}"
    );
    assert!(
        !rendered.contains("<error>"),
        "non-class error fell back to the old placeholder:\n{rendered}"
    );
}

#[tokio::test]
async fn structural_error_rendering_truncates_object_cycles() {
    let output = baml_test!(
        r#"
class RecursiveError {
  next RecursiveError?
}

function fail() -> string {
  let error = RecursiveError { next: null }
  error.next = error
  throw error
}

function main() -> string {
  fail() catch (e, ctx) {
    _ => ctx.to_string()
  }
}
"#
    );
    let rendered = expect_string(output.result.unwrap());
    assert!(
        rendered.contains("user.RecursiveError { next: … }"),
        "cyclic class error was not safely truncated:\n{rendered}"
    );
}

#[tokio::test]
async fn structural_error_rendering_enforces_depth_and_node_budgets() {
    let output = baml_test!(
        r#"
class Link {
  next Link?
}

function fail_deep() -> string {
  let root = Link { next: null }
  let current = root
  let i = 0
  while (i < 40) {
    let next = Link { next: null }
    current.next = next
    current = next
    i += 1
  }
  throw root
}

function fail_wide() -> string {
  let values: int[] = []
  let i = 0
  while (i < 300) {
    values.push(i)
    i += 1
  }
  throw values
}

function render_deep() -> string {
  fail_deep() catch (e, ctx) {
    _ => ctx.to_string()
  }
}

function render_wide() -> string {
  fail_wide() catch (e, ctx) {
    _ => ctx.to_string()
  }
}

function main() -> string {
  render_deep() + "|" + render_wide()
}
"#
    );
    let rendered = expect_string(output.result.unwrap());
    let (deep, wide) = rendered
        .split_once('|')
        .expect("expected both bounded renderings");
    assert!(
        deep.contains('…'),
        "deep class error exceeded its budget without truncation:\n{deep}"
    );
    assert!(
        wide.contains('…'),
        "wide array error exceeded its budget without truncation:\n{wide}"
    );
    assert_eq!(
        wide.matches('…').count(),
        1,
        "wide array should use one truncation marker:\n{wide}"
    );
    assert!(
        wide.len() < 2_000,
        "wide array diagnostic should remain bounded, got {} bytes",
        wide.len()
    );
}
