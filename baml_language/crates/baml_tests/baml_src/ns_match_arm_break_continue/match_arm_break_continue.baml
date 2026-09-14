// B-619: bare `break`/`continue` as a `match` arm value.
//
// `break`/`continue` are diverging expressions of type `never` (like `return`),
// so they may appear as a braceless `match` arm value without a surrounding
// block. Previously only the block form (`0 => { break; }`) was accepted; the
// braceless form emitted `E0010 "Expected expression, found break"`. These
// exercise the fix end-to-end — the loop-control semantics must match the
// block form.

// ── statement position (match result discarded) ──────────────────────────────

// Bare `break` stops the loop at 6; bare `continue` skips the add for 3.
function sum_skip_three_break_at_six() -> int {
    let total = 0;
    let i = 0;
    while (true) {
        i = i + 1;
        match (i) {
            6 => break,
            3 => continue,
            _ => { total = total + i; }
        }
    }
    total
}

test "bare_break_and_continue_arms_control_the_loop" {
    // Adds 1 + 2 + 4 + 5 (skips 3, breaks before 6) = 12.
    assert.equal(sum_skip_three_break_at_six(), 12)
}

// ── value position (never unifies with the other arm) ────────────────────────

// The bare `break` arm sits where the `match` value is bound to `step`; its
// `never` type unifies with the `int` arm, and when it fires control leaves the
// loop before `step` is used.
function countdown_sum(start: int) -> int {
    let total = 0;
    let i = start;
    while (true) {
        let step = match (i) {
            0 => break,
            _ => i,
        };
        total = total + step;
        i = i - 1;
    }
    total
}

test "bare_break_in_value_position_unifies_and_breaks" {
    // 4 + 3 + 2 + 1 = 10, then i == 0 breaks.
    assert.equal(countdown_sum(4), 10)
}

// The bare `continue` arm in value position: its `never` unifies with the `int`
// arm, and when it fires the loop restarts without adding.
function sum_1_to_5_skip_3() -> int {
    let total = 0;
    let i = 0;
    while (i < 5) {
        i = i + 1;
        let add = match (i) {
            3 => continue,
            _ => i,
        };
        total = total + add;
    }
    total
}

test "bare_continue_in_value_position_unifies_and_skips" {
    // 1 + 2 + 4 + 5 (skips 3) = 12.
    assert.equal(sum_1_to_5_skip_3(), 12)
}
