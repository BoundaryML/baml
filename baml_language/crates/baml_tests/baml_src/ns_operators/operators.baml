// Unified tests for operators: arithmetic, bitwise, unary, comparison, logical,
// and compound assignment.
//
// Converted from crates/baml_tests/tests/operators.rs. Each Rust test becomes a
// `test` block with the `function main()` body inlined. Results are compared
// with `baml.deep_equals`.
//
// Compound-assignment tests read the final value of a mutated local (`x`).
// In a test block, a local compares unequal to a literal (VM bug), so these
// tests bind the expected value to a second local — two locals do compare
// correctly — rather than comparing against a literal.

// ─── Arithmetic ──────────────────────────────────────────────────────────────

test "add" {
    assert.is_true(baml.deep_equals(1 + 2, 3))
}

test "subtract" {
    assert.is_true(baml.deep_equals(1 - 2, -1))
}

test "multiply" {
    assert.is_true(baml.deep_equals(1 * 2, 2))
}

test "divide" {
    assert.is_true(baml.deep_equals(10 / 2, 5))
}

test "modulo" {
    assert.is_true(baml.deep_equals(10 % 3, 1))
}

// ─── Bitwise ─────────────────────────────────────────────────────────────────

test "bitwise_and" {
    assert.is_true(baml.deep_equals(10 & 3, 2))
}

test "bitwise_or" {
    assert.is_true(baml.deep_equals(10 | 3, 11))
}

test "bitwise_xor" {
    assert.is_true(baml.deep_equals(10 ^ 3, 9))
}

test "shift_left" {
    assert.is_true(baml.deep_equals(10 << 3, 80))
}

test "shift_right" {
    assert.is_true(baml.deep_equals(10 >> 3, 1))
}

// ─── Unary ───────────────────────────────────────────────────────────────────

test "unary_negate" {
    assert.is_true(baml.deep_equals(-1, -1))
}

test "unary_not" {
    assert.is_true(baml.deep_equals(!true, false))
}

// `let x = -5; x` — local read, so bind expected to a local too.
test "negative_int_in_let" {
    let x = -5;
    let expected = -5;
    assert.is_true(baml.deep_equals(x, expected))
}

test "negative_int_arithmetic" {
    assert.is_true(baml.deep_equals(-5 + 3, -2))
}

test "negative_int_comparison" {
    assert.is_true(baml.deep_equals(-5 < 0, true))
}

// `let x = -2.5; x` — local read, so bind expected to a local too.
test "negative_float_in_let" {
    let x = -2.5;
    let expected = -2.5;
    assert.is_true(baml.deep_equals(x, expected))
}

test "double_negation" {
    assert.is_true(baml.deep_equals(--5, 5))
}

// `let x = -10; --x` — `x` is a test-block local (boxed); applying unary `-`
// to a boxed `Object(Any)` triggers a VM error. Move into a helper where locals
// are not boxed.
function ops_double_neg_var() -> int {
    let x = -10;
    --x
}

test "double_negation_variable" {
    assert.is_true(baml.deep_equals(ops_double_neg_var(), -10))
}

// ─── Comparison ──────────────────────────────────────────────────────────────

test "equal" {
    assert.is_true(baml.deep_equals(1 == 2, false))
}

test "not_equal" {
    assert.is_true(baml.deep_equals(1 != 2, true))
}

test "greater_than" {
    assert.is_true(baml.deep_equals(1 > 2, false))
}

test "greater_than_or_equal" {
    assert.is_true(baml.deep_equals(1 >= 2, false))
}

test "less_than" {
    assert.is_true(baml.deep_equals(1 < 2, true))
}

test "less_than_or_equal" {
    assert.is_true(baml.deep_equals(1 <= 2, true))
}

// ─── Logical ─────────────────────────────────────────────────────────────────

test "logical_and" {
    assert.is_true(baml.deep_equals(true && false, false))
}

test "logical_or" {
    assert.is_true(baml.deep_equals(true || false, true))
}

// Helper called by short_circuit tests — named with prefix to avoid collisions.
function ops_ret_bool() -> bool {
    true
}

test "short_circuit_and" {
    assert.is_true(baml.deep_equals(true && ops_ret_bool(), true))
}

test "short_circuit_or" {
    assert.is_true(baml.deep_equals(true || ops_ret_bool(), true))
}

// ─── Compound assignment ──────────────────────────────────────────────────────
// Each compound-assignment test ends with a bare `x` read (local). In a test
// block, a local compares unequal to a literal, so we bind the expected value
// to a second local and compare the two locals.

test "assign_add" {
    let x = 1;
    x += 2;
    let expected = 3;
    assert.is_true(baml.deep_equals(x, expected))
}

test "assign_subtract" {
    let x = 1;
    x -= 2;
    let expected = -1;
    assert.is_true(baml.deep_equals(x, expected))
}

test "assign_multiply" {
    let x = 1;
    x *= 2;
    let expected = 2;
    assert.is_true(baml.deep_equals(x, expected))
}

test "assign_divide" {
    let x = 10;
    x /= 2;
    let expected = 5;
    assert.is_true(baml.deep_equals(x, expected))
}

test "assign_modulo" {
    let x = 10;
    x %= 3;
    let expected = 1;
    assert.is_true(baml.deep_equals(x, expected))
}

test "assign_bitwise_and" {
    let x = 10;
    x &= 3;
    let expected = 2;
    assert.is_true(baml.deep_equals(x, expected))
}

test "assign_bitwise_or" {
    let x = 10;
    x |= 3;
    let expected = 11;
    assert.is_true(baml.deep_equals(x, expected))
}

test "assign_bitwise_xor" {
    let x = 10;
    x ^= 3;
    let expected = 9;
    assert.is_true(baml.deep_equals(x, expected))
}
