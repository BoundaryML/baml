// Runtime regression tests for the `baml.ops` bitwise interfaces (B-1075):
// the OPERATOR path (specialized bytecode) and the GENERIC-BOUND path (the
// `$rust_function` impls in `ops_bitwise.rs`) must agree - the impls define
// the canonical semantics the opcodes match, like `math.baml`'s.

// ─── operator path (specialized bytecode) ────────────────────────────────────

function int_ops() -> bool {
    (6 & 3) == 2 && (6 | 3) == 7 && (6 ^ 3) == 5 && (1 << 4) == 16 && (-16 >> 2) == -4
}

test "bitwise_int_operators" {
    assert.is_true(int_ops())
}

function bigint_ops() -> bool {
    (6n & 3n) == 2n
        && (6n | 3n) == 7n
        && (6n ^ 3n) == 5n
        && (1n << 70n) == 1180591620717411303424n
        && (1180591620717411303424n >> 70n) == 1n
}

test "bitwise_bigint_operators" {
    assert.is_true(bigint_ops())
}

function mixed_widens() -> bool {
    // Any int/bigint mix widens the int operand to bigint, matching the
    // bytecode's operand widening and the impls' Output types.
    (6 & 3n) == 2n && (6n & 3) == 2n
}

test "bitwise_mixed_widens_to_bigint" {
    assert.is_true(mixed_widens())
}

// ─── generic-bound path (native `$rust_function` dispatch) ───────────────────

function band<T extends baml.ops.BitAnd<int>>(x: T, y: int) -> T.Output throws never {
    x.bit_and(y)
}

function bshl<T extends baml.ops.ShiftLeft<int>>(x: T, y: int) -> T.Output throws never {
    x.shl(y)
}

test "bitwise_generic_bound_dispatch" {
    // Turbofish: TIR's concreteness gate does not widen a fresh literal
    // argument before judging the bound.
    assert.equal(band<int>(6, 3), 2);
    assert.is_true(band<bigint>(6n, 3) == 2n);
    assert.equal(bshl<int>(1, 4), 16);
    assert.is_true(bshl<bigint>(1n, 70) == 1180591620717411303424n)
}
