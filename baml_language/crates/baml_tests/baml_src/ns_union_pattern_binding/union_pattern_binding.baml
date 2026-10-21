// Regression tests for union-typed refutable pattern bindings (`x: A | B`).
//
// A binding ascribed a union type is refutable: it matches when the scrutinee's
// runtime type is *any* member of the union, and binds the value. The runtime
// `is`-type test must therefore check every member.
//
// Regression: the union multi-target branch in TIR pattern lowering
// (`lower_pat_dispatch`) lowered the pattern once per scrutinee union member and
// recorded each member's type into `pattern_types[subpat]` last-write-wins, so
// only the LAST member survived. MIR reads that per-PatId entry to emit the
// runtime test, so `x: A | B` tested only `B` — a value of the first member `A`
// never matched (the loop fell through / `else` was taken). The fix joins the
// per-member types so the recorded type is the full `A | B`.
//
// Converted from crates/bex_engine/tests/union_pattern_binding.rs. Every Rust
// source declared the same classes, so they are declared once here; each test's
// `function f()` is renamed uniquely since one namespace can't declare the same
// function twice, and the trivial `function main()` wrapper is folded into the
// `test` block's assertion.

// ─── shared declarations ─────────────────────────────────────────────────────

// `x.v` on `WlA | WlB` reads a shared member across a union, which is reachable
// only through an interface both arms implement.
interface WlHasV { v int }
class WlA { v int  implements WlHasV {} }
class WlB { v int  implements WlHasV {} }
class WlStop {}

// ─── let-else with a union-typed binding ─────────────────────────────────────

// `let x: WlA | WlB = cur else { ... }` where `cur` is the FIRST union member.
// Must bind (result 3), not fall through to `else`.
function upb_let_else_first_member() -> int {
    let cur: WlA | WlB | WlStop = WlA { v: 3 };
    let x: WlA | WlB = cur else { return 0; };
    x.v
}

test "union_typed_let_else_binds_first_member" {
    assert.equal(upb_let_else_first_member(), 3)
}

// Same, with `cur` being the SECOND union member (always worked; guards the fix
// against regressing the last member).
function upb_let_else_second_member() -> int {
    let cur: WlA | WlB | WlStop = WlB { v: 7 };
    let x: WlA | WlB = cur else { return 0; };
    x.v
}

test "union_typed_let_else_binds_second_member" {
    assert.equal(upb_let_else_second_member(), 7)
}

// Control: single-type refutable binding (no union) — one member, never
// clobbered.
function upb_single_typed_let_else() -> int {
    let cur: WlA | WlStop = WlA { v: 3 };
    let x: WlA = cur else { return 0; };
    x.v
}

test "single_typed_let_else_binds" {
    assert.equal(upb_single_typed_let_else(), 3)
}

// Control: the `else` branch is genuinely taken when the scrutinee is a
// non-targeted member.
function upb_let_else_non_member() -> int {
    let cur: WlA | WlB | WlStop = WlStop {};
    let x: WlA | WlB = cur else { return -1; };
    x.v
}

test "union_typed_let_else_takes_else_on_non_member" {
    assert.equal(upb_let_else_non_member(), -1)
}

// ─── the same union binding as a match arm ───────────────────────────────────

// Expression position, first member.
function upb_match_arm_first_member() -> int {
    let cur: WlA | WlB | WlStop = WlA { v: 3 };
    match (cur) {
        let x: WlA | WlB => x.v,
        _ => 0,
    }
}

test "union_typed_match_arm_binds_first_member" {
    assert.equal(upb_match_arm_first_member(), 3)
}

// `match` union arm, second member (guards the last member).
function upb_match_arm_second_member() -> int {
    let cur: WlA | WlB | WlStop = WlB { v: 7 };
    match (cur) {
        let x: WlA | WlB => x.v,
        _ => 0,
    }
}

test "union_typed_match_arm_binds_second_member" {
    assert.equal(upb_match_arm_second_member(), 7)
}
