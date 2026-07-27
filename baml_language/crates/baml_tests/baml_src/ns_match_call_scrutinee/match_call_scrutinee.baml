// A `match` whose scrutinee is a single-use call result must dispatch on THAT
// value. The if-else-chain switch strategy re-loads the discriminant once per
// comparison, so a stack-carried call result — consumed by the first
// comparison — left pulls 2..N popping whatever sat below on the operand
// stack: a VM type error when the popped slot was type-incompatible (a
// closure local), and a SILENT wrong arm when it was compatible (an
// enum-typed local). The stack-carry analysis now simulates every pull the
// chosen strategy emits (`switch_discriminant_pulls`), rejecting the carry so
// the scrutinee takes a real slot. First-arm hits passed even before the fix
// (they exit at the first comparison) — pinned here so every arm of the chain
// is exercised.

enum Decision {
    Alpha,
    Beta,
    Gamma,
}

function decide(which: int) -> Decision {
    match (which) {
        1 => Decision.Alpha,
        2 => Decision.Beta,
        _ => Decision.Gamma,
    }
}

// The silent shape: an enum-typed local sits below the carried scrutinee, so
// the stray pop used to dispatch on IT instead of the call result.
function pick_with_enum_local(which: int, other: Decision) -> string {
    match (decide(which)) {
        Decision.Alpha => "alpha",
        Decision.Beta => "beta",
        Decision.Gamma => "gamma",
    }
}

test "call_scrutinee_first_arm" {
    assert.equal(pick_with_enum_local(1, Decision.Gamma), "alpha")
}

test "call_scrutinee_second_arm_ignores_enum_local" {
    assert.equal(pick_with_enum_local(2, Decision.Gamma), "beta")
}

test "call_scrutinee_third_arm_ignores_enum_local" {
    assert.equal(pick_with_enum_local(3, Decision.Alpha), "gamma")
}

// The crash shape: the local below the scrutinee is a closure — the stray
// discriminant pop used to raise `expected variant, got closure`.
function pick_with_closure_local(which: int, cb: () -> null throws never) -> string {
    match (decide(which)) {
        Decision.Alpha => "alpha",
        Decision.Beta => "beta",
        Decision.Gamma => "gamma",
    }
}

test "call_scrutinee_second_arm_with_closure_local" {
    assert
        .equal(
            pick_with_closure_local(2, () -> null {
                null
            }),
            "beta",
        )
}

// Int chains hit the same strategy without the `discriminant` instruction —
// the stray pop used to read a stale int local and fall to `_`.
function decide_int() -> int {
    2
}

function pick_int_chain() -> string {
    let stale = 7;
    match (decide_int()) {
        1 => "one",
        2 => "two",
        _ => "other",
    }
}

test "call_scrutinee_int_chain" {
    assert.equal(pick_int_chain(), "two")
}

// ── Controls: the shapes that were sound all along stay sound ───────────────

// Two exhaustive arms emit a single comparison — one pull, carry-compatible.
enum Coin {
    Heads,
    Tails,
}

function flip(which: int) -> Coin {
    match (which) {
        1 => Coin.Heads,
        _ => Coin.Tails,
    }
}

function pick_two_arms(other: Coin) -> string {
    match (flip(2)) {
        Coin.Heads => "heads",
        Coin.Tails => "tails",
    }
}

test "call_scrutinee_two_arm_exhaustive" {
    assert.equal(pick_two_arms(Coin.Heads), "tails")
}

// Five dense arms take the jump-table strategy — one pull, carry-compatible.
enum Hand {
    One,
    Two,
    Three,
    Four,
    Five,
}

function deal(which: int) -> Hand {
    match (which) {
        1 => Hand.One,
        2 => Hand.Two,
        3 => Hand.Three,
        4 => Hand.Four,
        _ => Hand.Five,
    }
}

function pick_jump_table(other: Hand) -> string {
    match (deal(4)) {
        Hand.One => "1",
        Hand.Two => "2",
        Hand.Three => "3",
        Hand.Four => "4",
        Hand.Five => "5",
    }
}

test "call_scrutinee_jump_table" {
    assert.equal(pick_jump_table(Hand.One), "4")
}

// A parameter scrutinee is slot-backed — never stack-carried.
function pick_param(d: Decision, other: Decision) -> string {
    match (d) {
        Decision.Alpha => "alpha",
        Decision.Beta => "beta",
        Decision.Gamma => "gamma",
    }
}

test "param_scrutinee_control" {
    assert.equal(pick_param(Decision.Beta, Decision.Gamma), "beta")
}
