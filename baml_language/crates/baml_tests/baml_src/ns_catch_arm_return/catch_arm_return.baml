// B-246: a braceless `return` as a `catch`/`match` arm value.
//
// `return` is a diverging expression of type `never` (like `throw`), so it can
// appear directly as an arm value without a surrounding block. When the arm
// runs, control transfers to the enclosing function's exit — it returns from
// the function rather than producing the arm's value. Previously this only
// worked in block form (`_ => { return x }`); the braceless form emitted a
// cascade of misleading parse/type errors.

function car_throws(x: int) -> int throws string {
    if (x == 0) {
        throw "boom"
    }
    x
}

// ── catch arm ────────────────────────────────────────────────────────────────

// On throw the wildcard arm runs and `return -1` exits `catch_arm_returns`
// early; otherwise the catch yields the base value and execution continues.
function catch_arm_returns(x: int) -> int {
    let v = car_throws(x) catch (e) {
        _ => return -1,
    };
    v + 100
}

test "catch_arm_braceless_return_fires_on_throw" {
    // x == 0 throws -> arm runs -> `return -1` exits the function with -1.
    assert.equal(catch_arm_returns(0), -1)
}

test "catch_arm_braceless_return_skipped_when_no_throw" {
    // x == 5 does not throw -> catch yields 5 -> 5 + 100 = 105.
    assert.equal(catch_arm_returns(5), 105)
}

// A typed catch arm value mixed with an early `return`: the `never`-typed
// `return` arm unifies with the `int` arm, so the catch result stays `int`.
function catch_arm_return_mixed(x: int) -> int {
    let v = car_throws(x) catch (e) {
        let s: string => return s.length(),
    };
    v + 100
}

test "catch_arm_braceless_return_mixed_arm_types" {
    // x == 0 throws "boom" (len 4) -> `return 4`.
    assert.equal(catch_arm_return_mixed(0), 4)
}

// ── match arm ────────────────────────────────────────────────────────────────
//
// The same fix lets `return` be a braceless `match` arm value (block form
// already worked).

function match_arm_returns(x: int) -> int {
    let v = match (x) {
        0 => return -1,
        _ => x,
    };
    v + 100
}

test "match_arm_braceless_return_fires" {
    assert.equal(match_arm_returns(0), -1)
}

test "match_arm_braceless_return_other_arm" {
    assert.equal(match_arm_returns(7), 107)
}
