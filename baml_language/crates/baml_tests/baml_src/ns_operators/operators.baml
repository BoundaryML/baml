// Unified tests for operators: arithmetic, bitwise, unary, comparison, logical,
// and compound assignment.
//
// Converted from crates/baml_tests/tests/operators.rs. Each Rust test becomes a
// `test` block with the `function main()` body inlined. Results are compared
// with `assert.equal`.
//
// Compound-assignment tests read the final value of a mutated local (`x`).
// In a test block, a local compares unequal to a literal (VM bug), so these
// tests bind the expected value to a second local — two locals do compare
// correctly — rather than comparing against a literal.

// ─── Arithmetic ──────────────────────────────────────────────────────────────

test "add" {
    assert.equal(1 + 2, 3)
}

test "subtract" {
    assert.equal(1 - 2, -1)
}

test "multiply" {
    assert.equal(1 * 2, 2)
}

test "divide" {
    assert.equal(10 / 2, 5)
}

test "modulo" {
    assert.equal(10 % 3, 1)
}

// ─── Bitwise ─────────────────────────────────────────────────────────────────

test "bitwise_and" {
    assert.equal(10 & 3, 2)
}

test "bitwise_or" {
    assert.equal(10 | 3, 11)
}

test "bitwise_xor" {
    assert.equal(10 ^ 3, 9)
}

test "shift_left" {
    assert.equal(10 << 3, 80)
}

test "shift_right" {
    assert.equal(10 >> 3, 1)
}

// Bitwise NOT (`~`) is two's-complement complement: `~x == -x - 1`. Regression
// for B-766, where `~` silently lowered to the identity (`~x == x`).
test "bitwise_not" {
    assert.equal(~0, -1)
    assert.equal(~1, -2)
    assert.equal(~255, -256)
    assert.equal(~(-1), 0)
}

// ─── Unary ───────────────────────────────────────────────────────────────────

test "unary_negate" {
    assert.equal(-1, -1)
}

test "unary_not" {
    assert.is_true(!(!true))
}

// `let x = -5; x` — local read, so bind expected to a local too.
test "negative_int_in_let" {
    let x = -5;
    let expected = -5;
    assert.equal(x, expected)
}

test "negative_int_arithmetic" {
    assert.equal(-5 + 3, -2)
}

test "negative_int_comparison" {
    assert.is_true(-5 < 0)
}

// `let x = -2.5; x` — local read, so bind expected to a local too.
test "negative_float_in_let" {
    let x = -2.5;
    let expected = -2.5;
    assert.equal(x, expected)
}

test "double_negation" {
    assert.equal(--5, 5)
}

// `let x = -10; --x` — `x` is a test-block local (boxed); applying unary `-`
// to a boxed `Object(Any)` triggers a VM error. Move into a helper where locals
// are not boxed.
function ops_double_neg_var() -> int {
    let x = -10;
    --x
}

test "double_negation_variable" {
    assert.equal(ops_double_neg_var(), -10)
}

// ─── Comparison ──────────────────────────────────────────────────────────────

test "equal" {
    assert.is_true(1 != 2)
}

test "not_equal" {
    assert.is_true(1 != 2)
}

test "greater_than" {
    assert.is_true(!(1 > 2))
}

test "greater_than_or_equal" {
    assert.is_true(!(1 >= 2))
}

test "less_than" {
    assert.is_true(1 < 2)
}

test "less_than_or_equal" {
    assert.is_true(1 <= 2)
}

// ─── Logical ─────────────────────────────────────────────────────────────────

test "logical_and" {
    assert.is_true(!(true && false))
}

test "logical_or" {
    assert.is_true(true || false)
}

// Helper called by short_circuit tests — named with prefix to avoid collisions.
function ops_ret_bool() -> bool {
    true
}

test "short_circuit_and" {
    assert.is_true(true && ops_ret_bool())
}

test "short_circuit_or" {
    assert.is_true(true || ops_ret_bool())
}

// ─── Compound assignment ──────────────────────────────────────────────────────
// Each compound-assignment test ends with a bare `x` read (local). In a test
// block, a local compares unequal to a literal, so we bind the expected value
// to a second local and compare the two locals.

test "assign_add" {
    let x = 1;
    x += 2;
    let expected = 3;
    assert.equal(x, expected)
}

test "assign_subtract" {
    let x = 1;
    x -= 2;
    let expected = -1;
    assert.equal(x, expected)
}

test "assign_multiply" {
    let x = 1;
    x *= 2;
    let expected = 2;
    assert.equal(x, expected)
}

test "assign_divide" {
    let x = 10;
    x /= 2;
    let expected = 5;
    assert.equal(x, expected)
}

test "assign_modulo" {
    let x = 10;
    x %= 3;
    let expected = 1;
    assert.equal(x, expected)
}

test "assign_bitwise_and" {
    let x = 10;
    x &= 3;
    let expected = 2;
    assert.equal(x, expected)
}

test "assign_bitwise_or" {
    let x = 10;
    x |= 3;
    let expected = 11;
    assert.equal(x, expected)
}

test "assign_bitwise_xor" {
    let x = 10;
    x ^= 3;
    let expected = 9;
    assert.equal(x, expected)
}

// ─── Operator dispatch through `baml.ops` interfaces ──────────────────────────
// User types and unions dispatch `+` / `-` through the matching interface impl,
// resolved at runtime from the operands' concrete types.

class Vec2 {
    x int
    y int
    implements baml.ops.Add<Vec2> {
        type Output = Vec2
        function add(self, rhs: Vec2) -> Vec2 throws never {
            Vec2 { x: self.x + rhs.x, y: self.y + rhs.y }
        }
    }
    implements baml.ops.Negate {
        type Output = Vec2
        function neg(self) -> Vec2 throws never {
            Vec2 { x: -self.x, y: -self.y }
        }
    }
}

function ops_add_vecs(a: Vec2, b: Vec2) -> Vec2 {
    a + b
}

function ops_neg_vec(a: Vec2) -> Vec2 {
    -a
}

test "operator_add_user_class" {
    assert.equal(ops_add_vecs(Vec2 { x: 1, y: 2 }, Vec2 { x: 3, y: 4 }), Vec2 { x: 4, y: 6 })
}

test "operator_negate_user_class" {
    assert.equal(ops_neg_vec(Vec2 { x: 1, y: -2 }), Vec2 { x: -1, y: 2 })
}

// Left operand is a union: the impl is chosen from its runtime concrete type.
class Meters {
    v int
    implements baml.ops.Add<int> {
        type Output = Meters
        function add(self, rhs: int) -> Meters throws never {
            Meters { v: self.v + rhs }
        }
    }
}

class Feet {
    v int
    implements baml.ops.Add<int> {
        type Output = Feet
        function add(self, rhs: int) -> Feet throws never {
            Feet { v: self.v + rhs }
        }
    }
}

function ops_add_to_unit(u: Meters | Feet, n: int) -> Meters | Feet {
    u + n
}

test "operator_union_dispatch" {
    assert.equal(ops_add_to_unit(Meters { v: 1 }, 2), Meters { v: 3 })
    assert.equal(ops_add_to_unit(Feet { v: 10 }, 5), Feet { v: 15 })
}

// Compound assignment desugars to the same interface dispatch: `v += w` on a
// user type must route through `__union_add`, not the primitive opcode.
function ops_compound_add_vec() -> Vec2 {
    let v = Vec2 { x: 1, y: 2 };
    v += Vec2 { x: 10, y: 20 };
    v
}

test "operator_compound_assign_user_class" {
    assert.equal(ops_compound_add_vec(), Vec2 { x: 11, y: 22 })
}

// ─── User-type dispatch: the four remaining binary drivers ────────────────────
// Sub/Mul/Div/Rem dispatch by runtime string names (`__union_sub` -> "Subtract"
// -> "sub"); nothing but these tests ties those names to math.baml.

class Scaled {
    v int
    implements baml.ops.Subtract<Scaled> {
        type Output = Scaled
        function sub(self, rhs: Scaled) -> Scaled throws never {
            Scaled { v: self.v - rhs.v }
        }
    }
    implements baml.ops.Multiply<Scaled> {
        type Output = Scaled
        function mul(self, rhs: Scaled) -> Scaled throws never {
            Scaled { v: self.v * rhs.v }
        }
    }
    implements baml.ops.Divide<Scaled> {
        type Output = Scaled
        function div(self, rhs: Scaled) -> Scaled throws never {
            Scaled { v: self.v / rhs.v }
        }
    }
    implements baml.ops.Remainder<Scaled> {
        type Output = Scaled
        function rem(self, rhs: Scaled) -> Scaled throws never {
            Scaled { v: self.v % rhs.v }
        }
    }
}

function ops_scaled_sub(a: Scaled, b: Scaled) -> Scaled { a - b }
function ops_scaled_mul(a: Scaled, b: Scaled) -> Scaled { a * b }
function ops_scaled_div(a: Scaled, b: Scaled) -> Scaled { a / b }
function ops_scaled_rem(a: Scaled, b: Scaled) -> Scaled { a % b }

test "operator_sub_mul_div_rem_user_class" {
    assert.equal(ops_scaled_sub(Scaled { v: 10 }, Scaled { v: 3 }), Scaled { v: 7 })
    assert.equal(ops_scaled_mul(Scaled { v: 10 }, Scaled { v: 3 }), Scaled { v: 30 })
    assert.equal(ops_scaled_div(Scaled { v: 10 }, Scaled { v: 3 }), Scaled { v: 3 })
    assert.equal(ops_scaled_rem(Scaled { v: 10 }, Scaled { v: 3 }), Scaled { v: 1 })
}

// ─── Multi-instantiation dispatch: impl chosen by the rhs runtime type ────────
// Poly implements Add at two instantiations; if dispatch regressed to
// first-applicable-rule selection (instead of the exact iface-args match in
// resolve_implements_rule), one of these would call the wrong impl.

class Poly {
    v int
    implements baml.ops.Add<int> {
        type Output = Poly
        function add(self, rhs: int) -> Poly throws never {
            Poly { v: self.v + rhs }
        }
    }
    implements baml.ops.Add<float> {
        type Output = Poly
        function add(self, rhs: float) -> Poly throws never {
            // Distinct from the Add<int> impl so a wrong-specialization
            // dispatch produces a visibly different value.
            Poly { v: self.v + 1000 }
        }
    }
}

function ops_poly_add_int(p: Poly, n: int) -> Poly { p + n }
function ops_poly_add_float(p: Poly, f: float) -> Poly { p + f }

test "operator_multi_instantiation_selects_by_rhs_type" {
    assert.equal(ops_poly_add_int(Poly { v: 1 }, 5), Poly { v: 6 })
    assert.equal(ops_poly_add_float(Poly { v: 1 }, 0.5), Poly { v: 1001 })
}

// ─── Output != Self at runtime ────────────────────────────────────────────────

class Counter {
    n int
    implements baml.ops.Add<int> {
        type Output = int
        function add(self, rhs: int) -> int throws never { self.n + rhs }
    }
}

function ops_counter_add(c: Counter, n: int) -> int { c + n }

test "operator_output_differs_from_self" {
    assert.equal(ops_counter_add(Counter { n: 2 }, 1), 3)
}

// Negate's Output can also differ from Self (like Rust's Neg::Output).
class Debt {
    amount int
    implements baml.ops.Negate {
        type Output = int
        function neg(self) -> int throws never { 0 - self.amount }
    }
}

function ops_neg_debt(d: Debt) -> int { -d }

test "operator_negate_output_differs_from_self" {
    assert.equal(ops_neg_debt(Debt { amount: 5 }), -5)
}

// ─── Driver/opcode parity for primitives ──────────────────────────────────────
// A bounded-generic operand keeps MIR off the specialized opcodes, so the
// arithmetic below dispatches through baml.ops.__union_* to the native
// primitive impls in ops_math.rs. These pin that the driver path agrees with
// the opcode path: same values, and the same catchable panics.

function drv_add<T extends baml.ops.Add<T, Output = T>>(a: T, b: T) -> T { a + b }
function drv_sub<T extends baml.ops.Subtract<T, Output = T>>(a: T, b: T) -> T { a - b }
function drv_mul<T extends baml.ops.Multiply<T, Output = T>>(a: T, b: T) -> T { a * b }
function drv_div<T extends baml.ops.Divide<T, Output = T>>(a: T, b: T) -> T { a / b }
function drv_rem<T extends baml.ops.Remainder<T, Output = T>>(a: T, b: T) -> T { a % b }
function drv_neg<T extends baml.ops.Negate<Output = T>>(a: T) -> T { -a }
function drv_add_int_rhs<T extends baml.ops.Add<int, Output = T>>(a: T, b: int) -> T { a + b }
function drv_div_int_rhs<T extends baml.ops.Divide<int, Output = T>>(a: T, b: int) -> T { a / b }

// Monomorphic wrappers pin T to the primitive (literal args could otherwise
// infer a literal-typed T, which no interface bound admits).
function drv_int_add(a: int, b: int) -> int { drv_add(a, b) }
function drv_int_sub(a: int, b: int) -> int { drv_sub(a, b) }
function drv_int_mul(a: int, b: int) -> int { drv_mul(a, b) }
function drv_int_div(a: int, b: int) -> int { drv_div(a, b) }
function drv_int_rem(a: int, b: int) -> int { drv_rem(a, b) }
function drv_int_neg(a: int) -> int { drv_neg(a) }
function drv_float_add(a: float, b: float) -> float { drv_add(a, b) }
function drv_float_div(a: float, b: float) -> float { drv_div(a, b) }
function drv_float_neg(a: float) -> float { drv_neg(a) }
function drv_bigint_add(a: bigint, b: bigint) -> bigint { drv_add(a, b) }
function drv_bigint_div(a: bigint, b: bigint) -> bigint { drv_div(a, b) }
function drv_string_add(a: string, b: string) -> string { drv_add(a, b) }
function drv_float_add_int(a: float, b: int) -> float { drv_add_int_rhs(a, b) }
function drv_bigint_add_int(a: bigint, b: int) -> bigint { drv_add_int_rhs(a, b) }

test "driver_int_arithmetic_matches_opcodes" {
    assert.equal(drv_int_add(1, 2), 1 + 2)
    assert.equal(drv_int_sub(1, 2), 1 - 2)
    assert.equal(drv_int_mul(3, 4), 3 * 4)
    assert.equal(drv_int_div(10, 3), 10 / 3)
    assert.equal(drv_int_rem(10, 3), 10 % 3)
    assert.equal(drv_int_neg(5), -5)
}

test "driver_float_arithmetic_matches_opcodes" {
    assert.equal(drv_float_add(1.5, 2.25), 1.5 + 2.25)
    assert.equal(drv_float_div(1.0, 4.0), 1.0 / 4.0)
    assert.equal(drv_float_neg(2.5), -2.5)
}

test "driver_bigint_arithmetic_matches_opcodes" {
    assert.equal(drv_bigint_add(10n, 3n), 10n + 3n)
    assert.equal(drv_bigint_div(10n, 3n), 10n / 3n)
}

test "driver_string_add_matches_opcode" {
    assert.equal(drv_string_add("hello", " world"), "hello" + " world")
}

test "driver_mixed_widening_matches_opcodes" {
    assert.equal(drv_float_add_int(1.5, 2), 1.5 + 2)
    assert.equal(drv_bigint_add_int(10n, 3), 10n + 3)
}

// Mixed-kind primitive unions dispatch through the driver: the old fast-path
// promotion typed `(int | float) + int` as `float` while the runtime value
// could be an `int` — UB in the specialized float opcodes.
function ops_pick_int_or_float(n: int) -> int | float {
    if n > 0 { 1 } else { 2.5 }
}

function ops_mixed_union_add(n: int) -> int | float {
    ops_pick_int_or_float(n) + 1
}

test "operator_mixed_primitive_union_add" {
    assert.equal(ops_mixed_union_add(1), 2)
    assert.equal(ops_mixed_union_add(-1), 3.5)
}

// Panic parity (B-266): the driver natives must throw the same catchable
// panics the opcodes do — not wrap, not return IEEE infinities for int-like
// failures.

function drv_catch_add_overflow() -> int {
    drv_int_add(baml.Int.max_value(), 1) catch (e) { baml.panics.IntegerOverflow => -1 }
}
function drv_catch_mul_overflow() -> int {
    drv_int_mul(baml.Int.max_value(), 2) catch (e) { baml.panics.IntegerOverflow => -1 }
}
function drv_catch_neg_overflow() -> int {
    drv_int_neg(baml.Int.min_value()) catch (e) { baml.panics.IntegerOverflow => -1 }
}
function drv_catch_div_min_by_minus_one() -> int {
    drv_int_div(baml.Int.min_value(), -1) catch (e) { baml.panics.IntegerOverflow => -1 }
}
function drv_catch_int_div_zero() -> int {
    drv_int_div(1, 0) catch (e) { baml.panics.DivisionByZero => -1 }
}
function drv_catch_int_rem_zero() -> int {
    drv_int_rem(1, 0) catch (e) { baml.panics.DivisionByZero => -1 }
}
function drv_catch_float_div_zero() -> float {
    drv_float_div(1.0, 0.0) catch (e) { baml.panics.DivisionByZero => -1.0 }
}
function drv_catch_bigint_div_zero() -> bigint {
    drv_bigint_div(1n, 0n) catch (e) { baml.panics.DivisionByZero => -1n }
}

test "driver_panics_match_opcodes" {
    assert.equal(drv_catch_add_overflow(), -1)
    assert.equal(drv_catch_mul_overflow(), -1)
    assert.equal(drv_catch_neg_overflow(), -1)
    assert.equal(drv_catch_div_min_by_minus_one(), -1)
    assert.equal(drv_catch_int_div_zero(), -1)
    assert.equal(drv_catch_int_rem_zero(), -1)
    assert.equal(drv_catch_float_div_zero(), -1.0)
    assert.equal(drv_catch_bigint_div_zero(), -1n)
}

// ─── Ordering dispatch through baml.ops.Compare ───────────────────────────────
// `<` `<=` `>` `>=` on operands the comparison opcodes cannot order lower to a
// virtual call on baml.ops.Compare. `bool` needs no user code to reach this:
// the stdlib declares `implement Compare for bool`, and before the interface
// lowering existed `true < false` aborted with an uncatchable VM internal error.

function ord_bool_lt(a: bool, b: bool) -> bool { a < b }
function ord_bool_le(a: bool, b: bool) -> bool { a <= b }
function ord_bool_gt(a: bool, b: bool) -> bool { a > b }
function ord_bool_ge(a: bool, b: bool) -> bool { a >= b }

test "ordering_on_bool_dispatches_compare" {
    assert.is_true(ord_bool_lt(false, true))
    assert.is_true(!ord_bool_lt(true, false))
    assert.is_true(!ord_bool_lt(true, true))
    assert.is_true(ord_bool_gt(true, false))
    assert.is_true(!ord_bool_gt(false, true))
    // `le`/`ge` come from the Compare defaults, so they are reflexive.
    assert.is_true(ord_bool_le(false, false))
    assert.is_true(ord_bool_ge(true, true))
    assert.is_true(ord_bool_le(false, true))
    assert.is_true(!ord_bool_ge(false, true))
}

// A user class implementing only the required `lt`; `le`/`gt`/`ge` resolve to the
// interface defaults, which are merged into the impl's method table at bake time.
class Ranked {
    rank int
    implements baml.ops.Equals {
        function eq(self, other: Self) -> bool throws never { self.rank == other.rank }
    }
    implements baml.ops.Compare {
        function lt(self, other: Self) -> bool throws never { self.rank < other.rank }
    }
}

function ord_rank_lt(a: Ranked, b: Ranked) -> bool { a < b }
function ord_rank_le(a: Ranked, b: Ranked) -> bool { a <= b }
function ord_rank_gt(a: Ranked, b: Ranked) -> bool { a > b }
function ord_rank_ge(a: Ranked, b: Ranked) -> bool { a >= b }

test "ordering_on_user_class_dispatches_compare" {
    assert.is_true(ord_rank_lt(Ranked { rank: 1 }, Ranked { rank: 2 }))
    assert.is_true(!ord_rank_lt(Ranked { rank: 2 }, Ranked { rank: 1 }))
    assert.is_true(ord_rank_le(Ranked { rank: 1 }, Ranked { rank: 1 }))
    assert.is_true(ord_rank_gt(Ranked { rank: 2 }, Ranked { rank: 1 }))
    assert.is_true(ord_rank_ge(Ranked { rank: 1 }, Ranked { rank: 1 }))
    assert.is_true(!ord_rank_ge(Ranked { rank: 1 }, Ranked { rank: 2 }))
}

// Each operator dispatches its own method, so an override beats the default it
// replaces. `gt` here is deliberately inconsistent with `!le`: the default would
// make `a > a` false, the override makes it true. This fails if `>` is lowered as
// `!(a <= b)` or as a swapped `b.lt(a)`.
class OddOrder {
    n int
    implements baml.ops.Equals {
        function eq(self, other: Self) -> bool throws never { self.n == other.n }
    }
    implements baml.ops.Compare {
        function lt(self, other: Self) -> bool throws never { self.n < other.n }
        function gt(self, other: Self) -> bool throws never { true }
    }
}

function ord_odd_gt(a: OddOrder, b: OddOrder) -> bool { a > b }
function ord_odd_le(a: OddOrder, b: OddOrder) -> bool { a <= b }

test "ordering_override_beats_default" {
    assert.is_true(ord_odd_gt(OddOrder { n: 1 }, OddOrder { n: 1 }))
    // The un-overridden `le` still follows the default (`lt || eq`).
    assert.is_true(ord_odd_le(OddOrder { n: 1 }, OddOrder { n: 1 }))
}

// A bounded-generic operand keeps MIR off the specialized opcodes, like the
// arithmetic drv_* block above — though the route differs: arithmetic reaches a
// baml.ops.__union_* driver function, while ordering dispatches baml.ops.Compare
// directly. Either way the impl can only come from the runtime instantiation.
// Monomorphic wrappers pin T (literal args could otherwise infer a literal-typed
// T, which no interface bound admits).
function ord_lt<T extends baml.ops.Compare>(a: T, b: T) -> bool { a < b }
function ord_le<T extends baml.ops.Compare>(a: T, b: T) -> bool { a <= b }
function ord_gt<T extends baml.ops.Compare>(a: T, b: T) -> bool { a > b }
function ord_ge<T extends baml.ops.Compare>(a: T, b: T) -> bool { a >= b }

function ord_int_lt(a: int, b: int) -> bool { ord_lt(a, b) }
function ord_int_le(a: int, b: int) -> bool { ord_le(a, b) }
function ord_int_gt(a: int, b: int) -> bool { ord_gt(a, b) }
function ord_int_ge(a: int, b: int) -> bool { ord_ge(a, b) }
function ord_float_lt(a: float, b: float) -> bool { ord_lt(a, b) }
function ord_float_ge(a: float, b: float) -> bool { ord_ge(a, b) }
function ord_string_lt(a: string, b: string) -> bool { ord_lt(a, b) }
function ord_bigint_lt(a: bigint, b: bigint) -> bool { ord_lt(a, b) }
function ord_rank_generic_lt(a: Ranked, b: Ranked) -> bool { ord_lt(a, b) }

// Named for the generic (interface) route rather than a driver: unlike the
// arithmetic block above, ordering reaches no baml.ops driver function.
test "generic_ordering_matches_opcodes" {
    assert.equal(ord_int_lt(1, 2), 1 < 2)
    assert.equal(ord_int_lt(2, 1), 2 < 1)
    assert.equal(ord_int_le(1, 1), 1 <= 1)
    assert.equal(ord_int_gt(2, 1), 2 > 1)
    assert.equal(ord_int_ge(1, 2), 1 >= 2)
    assert.equal(ord_float_lt(1.5, 2.5), 1.5 < 2.5)
    assert.equal(ord_string_lt("a", "b"), "a" < "b")
    assert.equal(ord_bigint_lt(1n, 2n), 1n < 2n)
    // NaN is unordered in every direction on both paths.
    assert.is_true(!ord_float_lt(float.nan(), 1.0))
    assert.is_true(!ord_float_ge(float.nan(), 1.0))
    assert.is_true(!(float.nan() < 1.0))
    assert.is_true(!(float.nan() >= 1.0))
    // The generic path reaches a user class too.
    assert.is_true(ord_rank_generic_lt(Ranked { rank: 1 }, Ranked { rank: 2 }))
}

// ─── Ordering: panics propagate and unwind, like the arithmetic drivers ───────
// The virtual call carries the enclosing catch's unwind edge. `Compare` methods
// are `throws never`, so the only thing that can escape a user `lt` is a panic —
// which must still reach an enclosing catch arm, including through an inherited
// default that nests another dispatch (`le` = `lt || eq`).

class Boom {
    n int
    implements baml.ops.Equals {
        function eq(self, other: Self) -> bool throws never { self.n == other.n }
    }
    implements baml.ops.Compare {
        function lt(self, other: Self) -> bool throws never { (1 / self.n) < other.n }
    }
}

function ord_catch_lt() -> bool {
    (Boom { n: 0 } < Boom { n: 1 }) catch (e) { baml.panics.DivisionByZero => true }
}
function ord_catch_le() -> bool {
    (Boom { n: 0 } <= Boom { n: 1 }) catch (e) { baml.panics.DivisionByZero => true }
}
function ord_catch_gt() -> bool {
    (Boom { n: 0 } > Boom { n: 1 }) catch (e) { baml.panics.DivisionByZero => true }
}
function ord_catch_ge() -> bool {
    (Boom { n: 0 } >= Boom { n: 1 }) catch (e) { baml.panics.DivisionByZero => true }
}

test "ordering_panics_unwind_to_catch" {
    assert.is_true(ord_catch_lt())
    assert.is_true(ord_catch_le())
    assert.is_true(ord_catch_gt())
    assert.is_true(ord_catch_ge())
}

// ─── Ordering into non-local destinations ────────────────────────────────────
// The call terminator's destination must be a local, so a field / index /
// captured target is routed through a temp and assigned through on resume.

class FlagBox { flag bool }

function ord_into_field() -> bool {
    let b = FlagBox { flag: false }
    b.flag = (Ranked { rank: 1 } < Ranked { rank: 2 })
    b.flag
}
function ord_into_index() -> bool {
    let xs = [false, false]
    xs[0] = (Ranked { rank: 1 } < Ranked { rank: 2 })
    xs[0]
}
function ord_into_capture() -> bool {
    let out = false
    let set = () -> null throws never { out = (false < true); null }
    set()
    out
}

test "ordering_into_projection_destinations" {
    assert.is_true(ord_into_field())
    assert.is_true(ord_into_index())
    // Each target starts `false` and is written `true`, so a dropped write fails.
    assert.is_true(ord_into_capture())
}

// ─── Ordering in condition and short-circuit position ────────────────────────
// The dispatch splits the block, so the branch terminator lands in the resume
// block rather than the one the operands were lowered into.

// The class literals are bound first: a bare `Ranked { … }` in condition position
// would have its brace parsed as the `if` body.
function ord_as_if_cond() -> int {
    let lo = Ranked { rank: 1 }
    let hi = Ranked { rank: 2 }
    if lo < hi { 10 } else { 20 }
}
function ord_as_while_cond() -> int {
    let n = 0
    let a = false
    while a < true { n += 1; a = true }
    n
}
function ord_in_short_circuit() -> bool {
    (Ranked { rank: 1 } < Ranked { rank: 2 }) && (true > false)
}

test "ordering_in_condition_position" {
    assert.equal(ord_as_if_cond(), 10)
    assert.equal(ord_as_while_cond(), 1)
    assert.is_true(ord_in_short_circuit())
}

// ─── Ordering with a `Self` receiver and on generic instantiations ───────────
// `Self` inside an interface default body and a generic class are two of the
// three receiver shapes the virtual-call terminator exists for.

interface Ordered requires baml.ops.Compare {
    function precedes(self, other: Self) -> bool throws never { self < other }
}
class Tier {
    level int
    implements baml.ops.Equals {
        function eq(self, other: Self) -> bool throws never { self.level == other.level }
    }
    implements baml.ops.Compare {
        function lt(self, other: Self) -> bool throws never { self.level < other.level }
    }
    implements Ordered {}
}

class Wrapped<T> {
    weight int
    implements baml.ops.Equals {
        function eq(self, other: Self) -> bool throws never { self.weight == other.weight }
    }
    implements baml.ops.Compare {
        function lt(self, other: Self) -> bool throws never { self.weight < other.weight }
    }
}

function ord_self_in_default(a: Tier, b: Tier) -> bool { a.precedes(b) }
function ord_generic_int(a: Wrapped<int>, b: Wrapped<int>) -> bool { a < b }
function ord_generic_string(a: Wrapped<string>, b: Wrapped<string>) -> bool { a < b }

test "ordering_on_self_and_generic_receivers" {
    assert.is_true(ord_self_in_default(Tier { level: 1 }, Tier { level: 2 }))
    assert.is_true(!ord_self_in_default(Tier { level: 2 }, Tier { level: 1 }))
    assert.is_true(ord_generic_int(Wrapped<int> { weight: 1 }, Wrapped<int> { weight: 2 }))
    assert.is_true(ord_generic_string(Wrapped<string> { weight: 1 }, Wrapped<string> { weight: 2 }))
}

// The literal form named in the lowering's own comments: `true < false` compiles
// to a dispatch whose receiver operand is a constant, not a local.
test "ordering_on_bool_literals" {
    assert.is_true(!(true < false))
    assert.is_true(false < true)
    assert.is_true(true >= true)
    assert.is_true(false <= false)
}
