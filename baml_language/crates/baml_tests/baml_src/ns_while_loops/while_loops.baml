// Unified tests for while loops, break, and continue.
//
// Converted from crates/baml_tests/tests/while_loops.rs. Each Rust test becomes
// a `test` block. Because loop bodies accumulate into locals (which are boxed in
// the test-block VM), all loop logic lives in top-level helper functions whose
// call results compare correctly against literals.
//
// Naming: several Rust tests share function names (e.g. two `factorial`, two
// `nested`). Each helper here is given a unique, file-scoped name to avoid
// collisions with other baml_src files.

// ── while_loop_gcd ────────────────────────────────────────────────────────────

function while_loops_gcd(a: int, b: int) -> int {
    while (a != b) {
        if (a > b) {
            a = a - b;
        } else {
            b = b - a;
        }
    }

    a
}

test "while_loop_gcd" {
    assert.equal(while_loops_gcd(21, 15), 3)
}

// ── while_with_scope (Fibonacci) ──────────────────────────────────────────────

function while_loops_fib(n: int) -> int {
    let a = 0;
    let b = 1;

    while (n > 0) {
        n -= 1;
        let t = a + b;
        b = a;
        a = t;
    }

    a
}

test "while_with_scope" {
    assert.equal(while_loops_fib(5), 5)
}

// ── while_with_break ──────────────────────────────────────────────────────────

function while_loops_break_at_two() -> int {
    let a = 1;

    while (a < 5) {
        a += 1;

        if (a == 2) {
            break;
        }
    }

    a
}

test "while_with_break" {
    assert.equal(while_loops_break_at_two(), 2)
}

// ── break_factorial ───────────────────────────────────────────────────────────

function while_loops_factorial_break(limit: int) -> int {
    let result = 1;

    while (true) {
        if (limit == 0) {
            break;
        }
        result = result * limit;
        limit = limit - 1;
    }

    result
}

test "break_factorial" {
    assert.equal(while_loops_factorial_break(5), 120)
}

// ── break_nested ──────────────────────────────────────────────────────────────

function while_loops_break_nested() -> int {
    let a = 5;
    while (true) {
        while (true) {
            a = a + 1;
            break;
        }
        a = a + 1;
        break;
    }
    a
}

test "break_nested" {
    assert.equal(while_loops_break_nested(), 7)
}

// ── break_nested_with_variable_conditions ─────────────────────────────────────

function while_loops_break_nested_var(x: bool, y: bool) -> int {
    let a = 5;
    while (x) {
        while (y) {
            a = a + 1;
            break;
        }
        a = a + 1;
        break;
    }
    a
}

test "break_nested_with_variable_conditions" {
    assert.equal(while_loops_break_nested_var(true, true), 7)
}

// ── while_with_conditional_break ──────────────────────────────────────────────

function while_loops_count_down(n: int) -> int {
    let result = 0;
    while (true) {
        result = result + n;
        n = n - 1;
        if (n == 0) {
            break;
        }
    }
    result
}

test "while_with_conditional_break" {
    assert.equal(while_loops_count_down(3), 6)
}

// ── continue_factorial ────────────────────────────────────────────────────────

function while_loops_factorial_continue(limit: int) -> int {
    let result = 1;

    let should_continue = true;
    while (should_continue) {
        result = result * limit;
        limit = limit - 1;

        if (limit != 0) {
            continue;
        } else {
            should_continue = false;
        }
    }

    result
}

test "continue_factorial" {
    assert.equal(while_loops_factorial_continue(5), 120)
}

// ── continue_nested ───────────────────────────────────────────────────────────

function while_loops_continue_nested() -> int {
    let execute = true;
    while (execute) {
        while (false) {
            continue;
        }
        if (false) {
            continue;
        }
        execute = false;
    }
    5
}

test "continue_nested" {
    assert.equal(while_loops_continue_nested(), 5)
}
