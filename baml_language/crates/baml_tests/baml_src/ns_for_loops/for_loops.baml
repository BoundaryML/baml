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

// ── for_loop_joined_map_arm (B-1576) ─────────────────────────────────────────
//
// Regression: an `if`/`else` binding whose arms are a plain array and an
// `Array.map` lambda joins while the map's element var is still unsolved,
// so the binding records a syntactic union of lists. The for-loop's element
// demand must judge the canonical spelling once forcing grounds it; 0.17.0
// aborted here (an `Error` element with zero diagnostics reached runtime
// lowering) while 0.16.0 compiled. Sum in the body so the element must be
// `int`, not a leftover projection.

function for_loops_joined_map_arm(xs: int[], b: bool) -> int {
    let ys = if (b) {
        [0]
    } else {
        xs.map((x: int) -> int {
            x + 1
        })
    };
    let n = 0;

    for (let y in ys) {
        n = n + y;
    }

    n
}

test "for_loop_joined_map_arm_plain_branch" {
    assert.equal(for_loops_joined_map_arm([1, 2, 3], true), 0)
}

test "for_loop_joined_map_arm_map_branch" {
    assert.equal(for_loops_joined_map_arm([1, 2, 3], false), 9)
}

// Arm order flipped: the map arm joins first, the ground arm second.

function for_loops_joined_map_arm_first(xs: int[], b: bool) -> int {
    let ys = if (b) {
        xs.map((x: int) -> int {
            x + 1
        })
    } else {
        [0]
    };
    let n = 0;

    for (let y in ys) {
        n = n + y;
    }

    n
}

test "for_loop_joined_map_arm_first" {
    assert.equal(for_loops_joined_map_arm_first([1, 2, 3], true), 9)
}

// Both arms are map lambdas: two element vars join, neither ground.

function for_loops_joined_both_map_arms(xs: int[], b: bool) -> int {
    let ys = if (b) {
        xs.map((x: int) -> int {
            x * 2
        })
    } else {
        xs.map((x: int) -> int {
            x + 1
        })
    };
    let n = 0;

    for (let y in ys) {
        n = n + y;
    }

    n
}

test "for_loop_joined_both_map_arms" {
    assert.equal(for_loops_joined_both_map_arms([1, 2, 3], true), 12)
}

// ── for_loop_generic_iterable_bound (B-1576) ─────────────────────────────────
//
// The same abort hit every `for` over a `T extends baml.iter.Iterable`
// collection: the element is the rigid projection `(T as Iterable).Item`,
// which the runtime carries for per-receiver dispatch - it must not be
// erased to an error for being irreducible at compile time.

function for_loops_count_bounded<T extends baml.iter.Iterable>(c: T) -> int {
    let n = 0;

    for (let y in c) {
        n = n + 1;
    }

    n
}

test "for_loop_generic_iterable_bound_over_list" {
    assert.equal(for_loops_count_bounded([5, 6, 7]), 3)
}

test "for_loop_generic_iterable_bound_over_string" {
    assert.equal(for_loops_count_bounded("abc"), 3)
}
