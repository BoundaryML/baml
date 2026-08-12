// Unified tests for if/else expressions, statements, and block expressions.
//
// Converted from crates/baml_tests/tests/if_else.rs. Each Rust test becomes a
// `test` block with the `function main()` body inlined.
//
// Tests whose Rust form passes arguments (`args: { "x" => ... }`) are wrapped
// in a top-level helper function that embeds the argument values as literals;
// the test block calls the helper and compares the result.
//
// Tests that end with a bare local read (`let x = ...; x`) use a second local
// for the expected value — two locals compare correctly in a test block, but a
// local vs. a literal does not (VM bug). Tests with complex local mutation are
// wrapped in top-level helpers where the bug does not apply.

// ─── helpers for parametrised tests ──────────────────────────────────────────

function if_else_identity(x: int) -> int { x }

// if_else_with_parameter: main(b=true) -> if (b) { 1 } else { 2 }
function if_else_with_param_true(b: bool) -> int {
    if (b) { 1 } else { 2 }
}

// if_else_return_expr_with_locals: main(b=true) -> if (b) { let a=1; a } else { let a=2; a }
function if_else_return_locals(b: bool) -> int {
    if (b) {
        let a = 1;
        a
    } else {
        let a = 2;
        a
    }
}

// if_else_assignment_with_param: main(b=true) -> let i = if(b){1} else {2}; i
function if_else_assignment_with_param(b: bool) -> int {
    let i = if (b) { 1 } else { 2 };
    i
}

// if_else_assignment_with_locals: main(b=true) -> let result = if(b){let a=1;a} else {let a=2;a}; result
function if_else_assignment_with_locals_fn(b: bool) -> int {
    let result = if (b) {
        let a = 1;
        a
    } else {
        let a = 2;
        a
    };
    result
}

// else_if_with_parameter: main(a=false, b=true) -> if(a){1} else if(b){2} else {3}
function else_if_with_param(a: bool, b: bool) -> int {
    if (a) {
        1
    } else if (b) {
        2
    } else {
        3
    }
}

// else_if_assignment: main(a=false, b=true) -> let result = if(a){1} else if(b){2} else {3}; result
function else_if_assignment_fn(a: bool, b: bool) -> int {
    let result = if (a) {
        1
    } else if (b) {
        2
    } else {
        3
    };
    result
}

// else_if_return_expr_with_locals: main(a=false, b=true)
function else_if_return_locals(a: bool, b: bool) -> int {
    if (a) {
        let x = 1;
        x
    } else if (b) {
        let y = 2;
        y
    } else {
        let z = 3;
        z
    }
}

// else_if_assignment_with_locals: main(a=false, b=true)
function else_if_assignment_with_locals_fn(a: bool, b: bool) -> int {
    let result = if (a) {
        let x = 1;
        x
    } else if (b) {
        let y = 2;
        y
    } else {
        let z = 3;
        z
    };
    result
}

// if_else_statement: main(b=true) — identity is in-scope as if_else_identity
function if_else_statement_fn(b: bool) -> int {
    let a = 1;
    if (b) {
        let x = 1;
        let y = 2;
        if_else_identity(x);
    } else {
        let x = 3;
        let y = 4;
        if_else_identity(y);
    }
    a
}

// nested_block_with_if: complex local mutation — use helper so locals aren't boxed
function nested_block_with_if_fn() -> int {
    let a = 1;
    {
        let b = 2;
        let c = 3;
        a = b + c;
        if (a == 5) {
            a = 10;
        }
    }
    a
}

// if_without_else: complex local mutation — use helper
function if_without_else_fn() -> int {
    let x = 0;
    if (true) {
        x = 5;
    }
    x
}

// if_without_else_with_local: complex case — use helper
function if_without_else_with_local_fn() -> int {
    let result = 0;
    if (true) {
        let temp = 10;
    }
    result
}

// consecutive_if_without_else: complex local mutation — use helper
function consecutive_if_without_else_fn() -> int {
    let x = 0;
    if (true) { x = 1; }
    if (false) { x = 2; }
    x
}

// block_expression: let a = { let b=1; b }; a — use helper
function block_expression_fn() -> int {
    let a = {
        let b = 1;
        b
    };
    a
}

// ─── Basic if/else ────────────────────────────────────────────────────────────

test "if_else_true_branch" {
    assert.equal(if (true) { 1 } else { 2 }, 1)
}

test "if_else_false_branch" {
    assert.equal(if (false) { 1 } else { 2 }, 2)
}

test "if_else_comparison" {
    assert.equal(if (1 < 2) { 10 } else { 20 }, 10)
}

test "if_else_equality" {
    assert.equal(if (5 == 5) { 100 } else { 200 }, 100)
}

// `let x = if (true) { 42 } else { 0 }; x` — local read, so bind expected.
test "if_else_assign_to_variable" {
    let x = if (true) { 42 } else { 0 };
    let expected = 42;
    assert.equal(x, expected)
}

test "if_else_with_local_in_branches" {
    assert.equal(if (true) { let a = 1; a } else { let b = 2; b }, 1)
}

test "if_else_with_parameter" {
    assert.equal(if_else_with_param_true(true), 1)
}

test "if_else_return_expr_with_locals" {
    assert.equal(if_else_return_locals(true), 1)
}

test "if_else_assignment_with_param" {
    assert.equal(if_else_assignment_with_param(true), 1)
}

test "if_else_assignment_with_locals" {
    assert.equal(if_else_assignment_with_locals_fn(true), 1)
}

// ─── Nested / else-if ────────────────────────────────────────────────────────

test "if_else_nested" {
    assert.equal(if (true) { if (false) { 1 } else { 2 } } else { 3 }, 2)
}

test "else_if_chain" {
    assert.equal(if (false) { 1 } else if (false) { 2 } else { 3 }, 3)
}

// `let x = 5; if (x < 0) {...}` — `x` is a test-block local (boxed as
// Object(Any)), so `x < 0` triggers a VM error. Move into a helper.
function else_if_with_comparisons_fn() -> int {
    let x = 5;
    if (x < 0) {
        0
    } else if (x < 10) {
        1
    } else {
        2
    }
}

test "else_if_with_comparisons" {
    assert.equal(else_if_with_comparisons_fn(), 1)
}

test "else_if_with_parameter" {
    assert.equal(else_if_with_param(false, true), 2)
}

test "else_if_assignment" {
    assert.equal(else_if_assignment_fn(false, true), 2)
}

test "else_if_return_expr_with_locals" {
    assert.equal(else_if_return_locals(false, true), 2)
}

test "else_if_assignment_with_locals" {
    assert.equal(else_if_assignment_with_locals_fn(false, true), 2)
}

// ─── Complex conditions ───────────────────────────────────────────────────────

test "if_else_function_call_in_branch" {
    assert.equal(if (true) { if_else_identity(42) } else { 0 }, 42)
}

test "if_else_arithmetic_condition" {
    assert.equal(if (1 + 1 == 2) { 100 } else { 0 }, 100)
}

test "if_else_logical_and" {
    assert.equal(if (true && true) { 1 } else { 0 }, 1)
}

test "if_else_logical_or" {
    assert.equal(if (false || true) { 1 } else { 0 }, 1)
}

// ─── If-else as expression ────────────────────────────────────────────────────

test "if_else_in_arithmetic" {
    assert.equal(1 + if (true) { 2 } else { 3 }, 3)
}

test "if_else_as_function_arg" {
    assert.equal(if_else_identity(if (false) { 10 } else { 20 }), 20)
}

// `let tmp = if (false) { 10 } else { 20 }; identity(tmp)` — `tmp` is a
// test-block local (boxed); passing it to a function doesn't unbox it. Move
// into a helper function where locals are not boxed.
function if_else_assigned_then_passed_fn() -> int {
    let tmp = if (false) { 10 } else { 20 };
    if_else_identity(tmp)
}

test "if_else_assigned_then_passed_to_call" {
    assert.equal(if_else_assigned_then_passed_fn(), 20)
}

test "parenthesized_if_else_in_arithmetic" {
    assert.equal((if (true) { 1 } else { 2 }) + (if (false) { 3 } else { 4 }), 5)
}

test "chained_if_else_in_arithmetic" {
    assert.equal(if (true) { 1 } else { 2 } + if (false) { 3 } else { 4 }, 5)
}

// ─── If without else ─────────────────────────────────────────────────────────

test "if_without_else" {
    assert.equal(if_without_else_fn(), 5)
}

test "if_without_else_with_local" {
    assert.equal(if_without_else_with_local_fn(), 0)
}

test "consecutive_if_without_else" {
    assert.equal(consecutive_if_without_else_fn(), 1)
}

// ─── Block expressions ────────────────────────────────────────────────────────

test "block_expression" {
    assert.equal(block_expression_fn(), 1)
}

// ─── If-else as statement (with side effects) ─────────────────────────────────

test "if_else_statement" {
    assert.equal(if_else_statement_fn(true), 1)
}

test "nested_block_with_if" {
    assert.equal(nested_block_with_if_fn(), 10)
}
