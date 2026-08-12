// Unified tests for for-in loops and C-style for loops.
//
// Converted from crates/baml_tests/tests/for_loops.rs. Each Rust test becomes
// a `test` block. Because loop bodies accumulate into locals (which are boxed in
// the test-block VM), all loop logic lives in top-level helper functions whose
// call results compare correctly against literals.
//
// Skipped (ignored upstream):
//   - c_for_with_break_continue: #[ignore = "compiler2: C-style for condition evaluated with TypeError"]
//   - c_for_only_condition:      #[ignore = "compiler2: C-style for loop uses SysOp where Callable expected"]
//   - c_for_endless_break:       #[ignore = "compiler2: C-style for loop uses SysOp where Callable expected"]

// ── for_loop_sum ──────────────────────────────────────────────────────────────

function for_loops_sum(xs: int[]) -> int {
    let result = 0;

    for (let x in xs) {
        result += x;
    }

    result
}

test "for_loop_sum" {
    assert.equal(for_loops_sum([1, 2, 3, 4]), 10)
}

// ── for_loop_with_break ───────────────────────────────────────────────────────

function for_loops_for_with_break(xs: int[]) -> int {
    let result = 0;

    for (let x in xs) {
        if (x > 10) {
            break;
        }
        result += x;
    }

    result
}

test "for_loop_with_break" {
    assert.equal(for_loops_for_with_break([3, 4, 11, 100]), 7)
}

// ── for_loop_with_continue ────────────────────────────────────────────────────

function for_loops_for_with_continue(xs: int[]) -> int {
    let result = 0;

    for (let x in xs) {
        if (x > 10) {
            continue;
        }
        result += x;
    }

    result
}

test "for_loop_with_continue" {
    assert.equal(for_loops_for_with_continue([5, 20, 6]), 11)
}

// ── for_loop_nested ───────────────────────────────────────────────────────────

function for_loops_nested_for(arr_a: int[], arr_b: int[]) -> int {
    let result = 0;

    for (let a in arr_a) {
        for (let b in arr_b) {
            result += a * b;
        }
    }

    result
}

test "for_loop_nested" {
    assert.equal(for_loops_nested_for([1, 2], [3, 4]), 21)
}

// ── c_for_sum_to_ten ──────────────────────────────────────────────────────────

function for_loops_c_for_sum_to_ten() -> int {
    let s = 0;

    for (let i = 1; i <= 10; i += 1) {
        s += i;
    }

    s
}

test "c_for_sum_to_ten" {
    assert.equal(for_loops_c_for_sum_to_ten(), 55)
}

// ── c_for_with_break_continue ────────────────────────────────────────────────
//
// BUG: UserPanic("parse error") — C-style for with an empty condition slot
// (`for (init;; step)`) compiles but panics at runtime: the empty condition
// is lowered as `baml.sys.panic("parse error")`.
//
// function c_for_with_break_continue() -> int {
//     let s = 0;
//
//     for (let i = 0; ; s += i) {
//         i += 1;
//         if (i > 10) {
//             let x = 0;
//             break;
//         }
//         if (i == 5) {
//             continue;
//         }
//     }
//
//     s
// }
//
// test "c_for_with_break_continue" {
//     assert.equal(c_for_with_break_continue(), 55)
// }

// ── c_for_only_condition ──────────────────────────────────────────────────────
//
// BUG: UserPanic("missing statement") — C-style for with empty init and step
// (`for (; cond;)`) throws UserPanic("missing statement") at runtime.
//
// function c_for_only_condition() -> int {
//     let s = 0;
//
//     for (; false;) {
//     }
//
//     s
// }
//
// test "c_for_only_condition" {
//     assert.equal(c_for_only_condition(), 0)
// }

// ── c_for_endless_break ───────────────────────────────────────────────────────
//
// BUG: UserPanic("missing statement") — C-style for with all slots empty
// (`for (;;)`) throws UserPanic("missing statement") at runtime.
//
// function c_for_endless_break() -> int {
//     let s = 0;
//
//     for (;;) {
//         break;
//     }
//
//     s
// }
//
// test "c_for_endless_break" {
//     assert.equal(c_for_endless_break(), 0)
// }

// ── for_loop_over_let_variable ────────────────────────────────────────────────

function for_loops_sum_let_var() -> int {
    let xs = [1, 2, 3];
    let sum = 0;

    for (let x in xs) {
        sum += x;
    }

    sum
}

test "for_loop_over_let_variable" {
    assert.equal(for_loops_sum_let_var(), 6)
}

// ── for_loop_over_let_variable_no_parens ──────────────────────────────────────

function for_loops_sum_let_var_no_parens() -> int {
    let xs = [10, 20, 30];
    let sum = 0;

    for let x in xs {
        sum += x;
    }

    sum
}

test "for_loop_over_let_variable_no_parens" {
    assert.equal(for_loops_sum_let_var_no_parens(), 60)
}

// ── for_loop_final_if_without_semicolon_can_return ────────────────────────────

function for_loops_first_even(xs: int[]) -> int {
    for (let x in xs) {
        if (x % 2 == 0) {
            return x;
        }
    }

    -1
}

test "for_loop_final_if_without_semicolon_can_return" {
    assert.equal(for_loops_first_even([1, 3, 4, 7]), 4)
}

// ── for_loop_final_if_else_without_semicolon_can_mutate ──────────────────────

function for_loops_render(xs: string[]) -> string {
    let result = "";

    for (let x in xs) {
        if (x == "") {
            result += ".";
        } else {
            result += x;
        }
    }

    result
}

test "for_loop_final_if_else_without_semicolon_can_mutate" {
    assert.equal(for_loops_render(["x", "", "o"]), "x.o")
}

// ── nested_for_loop_final_if_without_semicolon_can_return ────────────────────

function for_loops_has_empty_cell(grid: string[][]) -> bool {
    for (let row in grid) {
        for (let cell in row) {
            if (cell == "") {
                return true;
            }
        }
    }

    false
}

test "nested_for_loop_final_if_without_semicolon_can_return" {
    assert.is_true(for_loops_has_empty_cell([["x", "o"], ["", "x"]]))
}
