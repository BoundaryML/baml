// BEP-042 Part 3 — `Context` cause-chain tests.
//
// Cause is computed lazily at the throw funnel: a throw whose PC lies in a
// handler body chains onto that handler's caught error (`vm.rs`
// `find_cause_context` + the handler-body extent on `ExceptionTableEntry`,
// built from the catch region's `handler_body` blocks).
//
// Test functions use a unique `ec_<test>_` prefix because the corpus shares
// one namespace. Some tests use `..._check() -> bool` helpers instead of
// directly returning values (locals must not live in test blocks — local-boxing
// VM bug), built with `string.includes`, `string.index_of`, and
// `split(sep).length()`.

// ─── single_error_root_cause_is_self ──────────────────────────────────────────
// Shipped single-link behavior: a lone caught error has no cause, so
// `root_cause()` is the error itself.

function ec_single_boom() -> string { throw "kaboom" }

function ec_single_main() -> string {
  ec_single_boom() catch (e, ctx) {
    _ => {
      match (ctx.root_cause().error) {
        let s: string => s
        _ => "unexpected non-string error"
      }
    }
  }
}

test "single_error_root_cause_is_self" {
    assert.equal(ec_single_main(), "kaboom")
}

// ─── nested_catch_chains_to_handled_error ─────────────────────────────────────
// Nested `catch`: throwing a different error while handling one chains the
// new error onto the error being handled (Python `__context__`-style).

function ec_nested_fail_a() -> string { throw "A" }
function ec_nested_fail_b() -> string { throw "B" }

function ec_nested_main() -> string {
  ec_nested_fail_a() catch (e, ctx) {
    _ => {
      // B is thrown while A is being handled -> B.cause == A.
      ec_nested_fail_b() catch (e2, ctx2) {
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

test "nested_catch_chains_to_handled_error" {
    assert.equal(ec_nested_main(), "A")
}

// ─── hazard_a_nested_construct_in_catch_arm_chains_to_outer_error ─────────────
// HAZARD A — handler-body PC coverage.
//
// A throw inside a `catch` arm body must chain onto the error that arm is
// handling, even when the arm contains out-of-line constructs (a nested
// `defer` pad and a nested `catch`) that fragment the handler body across
// basic blocks.

function ec_hazard_a_fail_outer() -> string { throw "E" }
function ec_hazard_a_fail_inner() -> string { throw "Y" }

function ec_hazard_a_main() -> string {
  ec_hazard_a_fail_outer() catch (e, ctx) {
    _ => {
      defer { }
      ec_hazard_a_fail_inner() catch (e2, ctx2) {
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

test "hazard_a_nested_construct_in_catch_arm_chains_to_outer_error" {
    assert.equal(ec_hazard_a_main(), "E")
}

// ─── no_match_rethrow_does_not_self_chain ─────────────────────────────────────
// A *rethrow* must not graft a spurious cause link. When a `catch` has no
// matching arm, the error re-raises (the no-match fall-through, which the VM
// flags as a rethrow). The re-raise executes from the catch's own dispatch
// block, which lies inside that catch's handler body — so without the rethrow
// guard the cause pre-walk would chain the re-raised "B" onto the very error
// the catch is examining (also "B"), a self-link. Because nothing was being
// handled when "B" arose, its `cause` must stay empty.

function ec_rethrow_fail_b() -> string { throw "B" }

function ec_rethrow_main() -> string {
  // The inner catch binds `ictx`, so "B" is materialized into its context
  // slot. The arm does not match the string "B", so "B" re-raises from the
  // dispatch block — which must NOT chain "B" onto its own context.
  (
    ec_rethrow_fail_b() catch (inner, ictx) {
      baml.errors.InvalidArgument => { "unreachable" }
    }
  ) catch (e2, ctx2) {
    _ => {
      match (ctx2.cause) {
        null => "no cause"
        let c: baml.errors.Context => "MISCHAINED: rethrow self-linked"
      }
    }
  }
}

test "no_match_rethrow_does_not_self_chain" {
    assert.equal(ec_rethrow_main(), "no cause")
}

// ─── throw_after_catch_in_layout_gap_does_not_chain ───────────────────────────
// Per-block handler-body coverage. A throw in code laid out in the PC *gap*
// between a catch arm's fragmented blocks must not be attributed to that
// catch. The first catch fully handles "A"; its arm is fragmented by a nested
// `defer` whose pad is laid out out-of-line, so the later `throw "C"` sits
// between the arm's main block and that pad. A single `[handler_pc, max_end)`
// span would over-cover it and mis-chain "C" onto "A"; per-block ranges keep
// it outside, so "C" has no cause.

function ec_gap_fail_a() -> string { throw "A" }
function ec_gap_fail_c() -> string { throw "C" }

function ec_gap_main() -> string {
  let recovered = ec_gap_fail_a() catch (e, ctx) {
    _ => {
      defer { }
      "recovered"
    }
  }
  // "A" is fully handled; this is a brand-new failure, not "during handling
  // of A". Its context must have no cause.
  ec_gap_fail_c() catch (e2, ctx2) {
    _ => {
      match (ctx2.cause) {
        null => recovered
        let c: baml.errors.Context => "MISCHAINED: gap over-covered"
      }
    }
  }
}

test "throw_after_catch_in_layout_gap_does_not_chain" {
    assert.equal(ec_gap_main(), "recovered")
}

// ─── sibling_defers_that_throw_chain_to_root_cause ────────────────────────────
// Sibling defers that throw while a scope unwinds form a cause chain. `scope`
// throws "X"; three sibling defers each throw while unwinding. They run LIFO
// (C, then B, then A), and each throw is "during handling of" the previous
// in-flight error, so the surviving error "A" chains A -> B -> C -> X.
// `root_cause` walks to the original failure "X".

function ec_sibling_scope() -> string {
  defer { throw "A" }
  defer { throw "B" }
  defer { throw "C" }
  throw "X"
}

function ec_sibling_main() -> string {
  ec_sibling_scope() catch (e, ctx) {
    _ => {
      match (ctx.root_cause().error) {
        let s: string => s
        _ => "no root cause found"
      }
    }
  }
}

test "sibling_defers_that_throw_chain_to_root_cause" {
    assert.equal(ec_sibling_main(), "X")
}

// ─── hazard_b_unused_binding_slot_liveness_preserves_cause ────────────────────
// HAZARD B — `error_local` / context-slot liveness across the whole handler
// body. The outer handler binds `ctx` but never reads it, so the cause
// pre-walk's runtime read of the context slot must keep it alive (handled via
// the analysis use-injection in `analysis.rs` + `optimize.rs`).

function ec_hazard_b_fail_outer() -> string { throw "E" }
function ec_hazard_b_fail_inner() -> string { throw "Y" }

function ec_hazard_b_main() -> string {
  ec_hazard_b_fail_outer() catch (e, ctx) {
    _ => {
      let a = 1 + 2
      let b = a * a
      let c = b + a
      ec_hazard_b_fail_inner() catch (e2, ctx2) {
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

test "hazard_b_unused_binding_slot_liveness_preserves_cause" {
    assert.equal(ec_hazard_b_main(), "E")
}

// ─── defer_on_unwind_preserves_cause_chain ────────────────────────────────────
// B-611 regression — a non-throwing `defer` on the unwind frame must NOT wipe
// the propagating error's cause chain.
//
// `mid` catches "origin" and throws "wrap" (so wrap.cause == origin). A
// `defer {}` armed in `mid` routes that expression-position throw through its
// landing pad, which replays the (empty) defer and then re-raises "wrap"
// unchanged. The pad's transparent re-raise preserves the cause computed at
// wrap's throw site (`thrown_value_causes` in `vm.rs`), so `root_cause`
// walks past "wrap" to the original "origin".

function ec_deferunwind_fail_low() -> string { throw "origin" }

function ec_deferunwind_mid() -> string {
  defer { }
  ec_deferunwind_fail_low() catch (e, ctx) {
    _ => throw "wrap"
  }
}

function ec_deferunwind_main() -> string {
  ec_deferunwind_mid() catch (e, ctx) {
    _ => {
      match (ctx.root_cause().error) {
        let s: string => s
        _ => "LOST: cause chain wiped by the defer re-raise"
      }
    }
  }
}

test "defer_on_unwind_preserves_cause_chain" {
    assert.equal(ec_deferunwind_main(), "origin")
}

// ─── defer_on_unwind_preserves_to_string_chain ────────────────────────────────
// B-611 — the rendered chain must keep its "During handling of..." section
// across the defer re-raise. Same shape as above, but return `to_string`: the
// full chain (origin, then wrap) must survive, not just the surviving "wrap".

function ec_tostring_fail_low() -> string { throw "origin" }

function ec_tostring_mid() -> string {
  defer { }
  ec_tostring_fail_low() catch (e, ctx) {
    _ => throw "wrap"
  }
}

function ec_tostring_main() -> string {
  ec_tostring_mid() catch (e, ctx) {
    _ => ctx.to_string()
  }
}

function ec_tostring_check() -> bool {
  let rendered = ec_tostring_main();
  // missing chain separator => the defer re-raise dropped the cause
  let has_sep = rendered.includes("During handling of the above error, another error occurred");
  let has_origin = rendered.includes("origin");
  let has_wrap = rendered.includes("wrap");
  has_sep && has_origin && has_wrap
}

test "defer_on_unwind_preserves_to_string_chain" {
    assert.is_true(ec_tostring_check())
}

// ─── defer_with_body_preserves_cause_chain ────────────────────────────────────
// B-611 — a NON-empty (but non-throwing) defer body must behave the same: it
// is the pad's transparent re-raise, not empty-block elimination, that must
// preserve the cause. The defer mutates an outer local (a real, observable
// body) yet does not throw.

function ec_deferbody_fail_low() -> string { throw "origin" }

function ec_deferbody_mid() -> string {
  let marker = "start"
  defer { marker = marker + "-cleanup" }
  ec_deferbody_fail_low() catch (e, ctx) {
    _ => throw "wrap"
  }
}

function ec_deferbody_main() -> string {
  ec_deferbody_mid() catch (e, ctx) {
    _ => {
      match (ctx.root_cause().error) {
        let s: string => s
        _ => "LOST: cause chain wiped by the defer re-raise"
      }
    }
  }
}

test "defer_with_body_preserves_cause_chain" {
    assert.equal(ec_deferbody_main(), "origin")
}

// ─── nested_defers_nonthrowing_preserve_cause ─────────────────────────────────
// B-611 — two stacked non-throwing defers on the unwind frame each add a
// transparent re-raise; the cause must survive all of them.

function ec_stackdefers_fail_low() -> string { throw "origin" }

function ec_stackdefers_mid() -> string {
  defer { }
  defer { }
  ec_stackdefers_fail_low() catch (e, ctx) {
    _ => throw "wrap"
  }
}

function ec_stackdefers_main() -> string {
  ec_stackdefers_mid() catch (e, ctx) {
    _ => {
      match (ctx.root_cause().error) {
        let s: string => s
        _ => "LOST: cause chain wiped by a stacked defer re-raise"
      }
    }
  }
}

test "nested_defers_nonthrowing_preserve_cause" {
    assert.equal(ec_stackdefers_main(), "origin")
}

// ─── defer_that_throws_still_chains_through_wrap ──────────────────────────────
// B-611 regression guard — a defer whose body ITSELF throws must still build
// the full chain. `mid` catches "origin" and throws "wrap"; the armed defer
// then throws "cleanup" while "wrap" is unwinding. The result chains
// cleanup -> wrap -> origin (cleanup is "during handling of" wrap, wrap is
// "during handling of" origin). `root_cause` reaches "origin" and the middle
// "wrap" link is not dropped — confirming the fix preserves a pre-existing
// chain even when the defer contributes a fresh link on top of it.

function ec_deferthrow_fail_low() -> string { throw "origin" }

function ec_deferthrow_mid() -> string {
  defer { throw "cleanup" }
  ec_deferthrow_fail_low() catch (e, ctx) {
    _ => throw "wrap"
  }
}

function ec_deferthrow_main() -> string {
  ec_deferthrow_mid() catch (e, ctx) {
    _ => ctx.to_string()
  }
}

function ec_deferthrow_check() -> bool {
  let rendered = ec_deferthrow_main();
  // Match error lines, not names such as this test's "through_wrap" stack frame.
  let origin_at = rendered.index_of("\norigin") ?? -1;
  let wrap_at = rendered.index_of("\nwrap") ?? -1;
  let cleanup_at = rendered.index_of("\ncleanup") ?? -1;
  let all_present = origin_at >= 0 && wrap_at >= 0 && cleanup_at >= 0;
  let ordered = origin_at < wrap_at && wrap_at < cleanup_at;
  // Two chain separators (origin->wrap->cleanup) => split yields 3 pieces.
  let pieces = rendered.split("During handling of the above error, another error occurred").length();
  all_present && ordered && pieces == 3
}

test "defer_that_throws_still_chains_through_wrap" {
    assert.is_true(ec_deferthrow_check())
}
