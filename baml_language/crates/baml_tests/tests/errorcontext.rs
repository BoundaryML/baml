//! BEP-042 Part 3 — `ErrorContext` cause-chain acceptance tests.
//!
//! These pin the two correctness hazards of the side-table chaining design
//! (cause computed lazily at the throw funnel from a PC-range
//! `handler_context_table`, no register, no `PopErrorContext` opcode). See
//! `thoughts/antonio/errorcontext-impl-plan.md` Step 4 and
//! `thoughts/antonio/errorcontext-sidetable-vs-opcode.md`.
//!
//! Both are `#[ignore]`d until `ErrorContext` ships: today the second `catch`
//! binding is `baml.errors.StackTrace`, which has no `error`/`cause`/
//! `root_cause()`. Remove the `#[ignore]` as the acceptance gate when the
//! feature lands.

use baml_tests::baml_test;
use bex_engine::BexExternalValue;

#[track_caller]
fn expect_string(v: BexExternalValue) -> String {
    match v {
        BexExternalValue::String(s) => s.to_string(),
        other => panic!("expected String, got {other:?}"),
    }
}

/// HAZARD A — handler-body PC contiguity.
///
/// A throw inside a `catch` arm body must chain onto the error that arm is
/// handling. The arm here contains out-of-line constructs (a nested `defer`
/// pad and a nested `catch`), which fragment the handler body across multiple
/// basic blocks. A naive single `[handler_pc, bb_join)` context-table span
/// would not cover the fragment holding `fail_inner()`'s call site, so the
/// pre-walk would find no enclosing handler and lose the link (`cause = none`).
/// The fix is one `HandlerContextEntry` per handler-body block.
#[tokio::test]
#[ignore = "BEP-042 Part 3 ErrorContext not yet implemented — acceptance test (HAZARD A)"]
async fn hazard_a_nested_construct_in_catch_arm_chains_to_outer_error() {
    let output = baml_test!(
        r#"
function fail_outer() -> string { throw "E" }
function fail_inner() -> string { throw "Y" }

function main() -> string {
  fail_outer() catch (e, ctx) {
    _ => {
      // Out-of-line pad forces the arm body to span several blocks BEFORE the
      // inner throw — the contiguity stressor.
      defer { }
      fail_inner() catch (e2, ctx2) {
        _ => {
          // Y was thrown while E was being handled, so Y's chain must reach E.
          match ctx2.root_cause().error {
            s: string => s
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

/// HAZARD B — `error_local` slot liveness across the whole handler body.
///
/// The outer handler binds `e`/`ctx` but never reads them, so today the
/// caught-error slot only needs to live to (an immediate) last use. The lazy
/// cause pre-walk extends the required lifetime of that slot to the entire
/// handler-body PC range: when `fail_inner()` throws, the pre-walk reads the
/// outer handler's `error_slot` to obtain `E`. If MIR slot-coloring recycled
/// that slot for one of the intervening temps (register pressure below), the
/// pre-walk reads garbage. The fix is to pin `CatchRegion.error_local` live
/// across the handler body (or store the caught value at handler entry).
#[tokio::test]
#[ignore = "BEP-042 Part 3 ErrorContext not yet implemented — acceptance test (HAZARD B)"]
async fn hazard_b_unused_binding_slot_liveness_preserves_cause() {
    let output = baml_test!(
        r#"
function fail_outer() -> string { throw "E" }
function fail_inner() -> string { throw "Y" }

function main() -> string {
  // `e` and `ctx` are bound but never read — the outer caught-error slot is
  // "dead" by ordinary liveness, tempting the allocator to reuse it.
  fail_outer() catch (e, ctx) {
    _ => {
      // Intervening temps create register pressure on the freed slot.
      let a = 1 + 2
      let b = a * a
      let c = b + a
      fail_inner() catch (e2, ctx2) {
        _ => {
          // Despite `e` being unused, E must still be intact as Y's cause.
          match ctx2.root_cause().error {
            s: string => s
            _ => "GARBAGE: outer error_local was recolored before the inner throw"
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
