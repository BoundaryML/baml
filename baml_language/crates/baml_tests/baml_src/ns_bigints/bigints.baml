// Tests for the `bigint` builtin class (BEP-022).
//
// Converted from crates/baml_tests/tests/bigints.rs. Mirrors ns_ints/ints.baml:
// pure expressions are asserted inline; tests that use locals (let bindings,
// compound assignment, lambdas, generics with receivers) go through a top-level
// helper function to avoid the test-block local-boxing VM bug. Value results
// are compared with `assert.equal`; boolean results are asserted directly.
//
// Throw tests use the `{ <body>; baml.sys.panic("…") } catch (e) { <Type> => null }`
// pattern. For throws raised by a *pure operator* (`/`, `%`, `<<`, `>>`, `*`),
// the operator expression is the RETURN value of a helper function — a bare
// operator statement is dead-code-eliminated and never throws. Method/function
// calls (`bigint.parse`, `.isqrt`, `.ilog`, `bigint.random`, `.pow`) are kept
// inline since calls are not eliminated.
//
// Throw types (from crates/bex_vm/src/package_baml/bigint.rs):
//   parse invalid/empty                  -> baml.errors.ParseError
//   isqrt negative, ilog domain, random,
//   to_int out of range                  -> baml.errors.InvalidArgument
//   div/mod by zero                      -> baml.panics.DivisionByZero
//   negative bit shift                   -> baml.panics.NegativeBitShift
//   pow/shl/mul oversize                 -> baml.panics.AllocFailure
//
// LEFT IN RUST (FFI boundary — pass host BexExternalValue args, not expressible
// from a BAML call site; out-of-range host ints/bigints cannot be written as
// BAML literals):
//   - test_bigint_param_accepts_int_from_host
//   - test_int_param_accepts_bigint_from_host_in_range
//   - test_int_param_rejects_bigint_overflow
//   - test_int_param_rejects_bigint_above_i63
//   - test_int_param_rejects_host_int_above_i63
//   - test_int_param_accepts_max_i63
//
// NOTE — within-file duplicates preserved 1:1 with the Rust source (identical
// input + assertion, kept in separate sections there as regression markers):
//   bigint_pow_large == test_bigint_pow_normal_works            ((2n).pow(256n))
//   bigint_ilog_base2 == test_bigint_ilog_base_two              ((1024n).ilog(2n))
//   bigint_ilog_base10 == test_bigint_ilog_base_ten             ((1000n).ilog(10n))
//   bigint_pow_zero_base_zero_exp == test_bigint_pow_zero_zero  ((0n).pow(0n))
//   test_bigint_constant_fold_negative_shift_refused == test_bigint_shr_negative_shift (1n >> -1n)

// ─── bigint compound-assign & generics (one `int` operand allowed) ───────────

function bigint_compound_assign_add_fn() -> bigint {
    let x: bigint = 0n;
    x += 1;
    x
}

test "test_bigint_compound_assign_int_add" {
    assert.equal(bigint_compound_assign_add_fn(), 1n)
}

function bigint_compound_assign_mul_fn() -> bigint {
    let x: bigint = 5n;
    x *= 2;
    x
}

test "test_bigint_compound_assign_int_mul" {
    assert.equal(bigint_compound_assign_mul_fn(), 10n)
}

function bigint_nonliteral_inc() -> int { 5 }

function bigint_compound_assign_nonliteral_fn() -> bigint {
    let x: bigint = 100n;
    x += -7;
    x += 2 + 3;
    x += bigint_nonliteral_inc();
    x -= 1;
    x
}

test "test_bigint_compound_assign_int_nonliteral_rhs" {
    assert.equal(bigint_compound_assign_nonliteral_fn(), 102n)
}

function bigint_identity<T>(x: T) -> T { x }

function bigint_identity_caller() -> bigint { bigint_identity<bigint>(1n) }

test "test_bigint_generic_fn_explicit_type_arg" {
    assert.equal(bigint_identity_caller(), 1n)
}

class BigintBox<T> {
    v T
    function set(self, x: T) -> T { self.v = x; x }
}

function bigint_box_caller() -> bigint {
    let b = BigintBox<bigint> { v: 0n };
    b.set(7n)
}

test "test_bigint_generic_method_class_type_arg" {
    assert.equal(bigint_box_caller(), 7n)
}

// ─── generic-path bigint comparison (union operand → generic CmpOp) ──────────

function bigint_pick() -> int | bigint { 2n }

test "test_bigint_generic_cmp_eq_bigint_true" {
    // `bigint_pick()` is `int | bigint` but returns the bigint `2n`; comparing to
    // a matching `bigint` literal is `true` (same concrete type and value).
    assert.is_true(bigint_pick() == 2n)
}

test "test_bigint_generic_cmp_eq_int_false" {
    assert.is_true((bigint_pick() == 3) == false)
}

function bigint_pick_a() -> int | bigint { 7n }
function bigint_pick_b() -> int | bigint { 7n }

test "test_bigint_generic_cmp_bigint_bigint" {
    assert.is_true(bigint_pick_a() == bigint_pick_b())
}

// ─── Bigint × Bigint constant folding (TIR `try_fold_binary`) ────────────────

function bigint_fold_get_three() -> 3n { 1n + 2n }

test "test_bigint_constant_fold_add_narrows_to_literal_type" {
    assert.equal(bigint_fold_get_three(), 3n)
}

function bigint_fold_get_twelve() -> 12n { 3n * 4n }

test "test_bigint_constant_fold_mul_narrows" {
    assert.equal(bigint_fold_get_twelve(), 12n)
}

function bigint_fold_is_less() -> true { 1n < 2n }

test "test_bigint_constant_fold_comparison_to_bool" {
    assert.is_true(bigint_fold_is_less())
}

// `1n >> -1n` must NOT fold (folder returns None); the body compiles at the
// unnarrowed `bigint` return type and the runtime raises NegativeBitShift.
function bigint_neg_shr_fn() -> bigint { 1n >> -1n }

test "test_bigint_constant_fold_negative_shift_refused" {
    {
        bigint_neg_shr_fn();
        baml.sys.panic("expected 1n >> -1n to throw NegativeBitShift")
    } catch (e) {
        baml.panics.NegativeBitShift => null
    }
}

function bigint_compound_assign_lambda_fn() -> bigint {
    let small = 5;
    let f = () -> bigint {
        let local_big: bigint = 0n;
        local_big += small;
        local_big
    };
    f()
}

test "test_bigint_compound_assign_int_in_lambda" {
    assert.equal(bigint_compound_assign_lambda_fn(), 5n)
}

// ─── literal syntax ─────────────────────────────────────────────────────────

test "bigint_literal_small" {
    assert.equal(42n, 42n)
}

test "bigint_literal_large" {
    assert.equal(99999999999999999999n, 99999999999999999999n)
}

test "bigint_literal_negative" {
    assert.equal(-7n, -7n)
}

test "bigint_literal_zero" {
    assert.equal(0n, 0n)
}

function bigint_literal_let_binding_fn() -> bigint {
    let x = 42n;
    x
}

test "bigint_literal_let_binding" {
    assert.equal(bigint_literal_let_binding_fn(), 42n)
}

// ─── parse ────────────────────────────────────────────────────────────────────

test "bigint_parse_small" {
    assert.equal(bigint.parse("12345"), 12345n)
}

test "bigint_parse_large" {
    assert.equal(bigint.parse("99999999999999999999"), 99999999999999999999n)
}

test "bigint_parse_negative" {
    assert.equal(bigint.parse("-7"), -7n)
}

test "bigint_parse_zero" {
    assert.equal(bigint.parse("0"), 0n)
}

test "bigint_parse_invalid_throws" {
    {
        bigint.parse("not-a-number");
        baml.sys.panic("expected bigint.parse(\"not-a-number\") to throw")
    } catch (e) {
        baml.errors.ParseError => null
    }
}

test "bigint_parse_empty_throws" {
    {
        bigint.parse("");
        baml.sys.panic("expected bigint.parse(\"\") to throw")
    } catch (e) {
        baml.errors.ParseError => null
    }
}

// ─── abs ──────────────────────────────────────────────────────────────────────

test "bigint_abs_positive" {
    assert.equal((3n).abs(), 3n)
}

test "bigint_abs_negative" {
    assert.equal((-7n).abs(), 7n)
}

test "bigint_abs_zero" {
    assert.equal((0n).abs(), 0n)
}

test "bigint_abs_large_negative" {
    assert.equal((-99999999999999999999n).abs(), 99999999999999999999n)
}

// ─── min / max ────────────────────────────────────────────────────────────────

test "bigint_min_self_smaller" {
    assert.equal((3n).min(5n), 3n)
}

test "bigint_min_other_smaller" {
    assert.equal((5n).min(3n), 3n)
}

test "bigint_min_equal" {
    assert.equal((3n).min(3n), 3n)
}

test "bigint_min_negative" {
    assert.equal((-2n).min(0n), -2n)
}

test "bigint_max_self_larger" {
    assert.equal((5n).max(3n), 5n)
}

test "bigint_max_other_larger" {
    assert.equal((3n).max(5n), 5n)
}

test "bigint_max_equal" {
    assert.equal((3n).max(3n), 3n)
}

// ─── clamp ────────────────────────────────────────────────────────────────────

test "bigint_clamp_in_range" {
    assert.equal((5n).clamp(0n, 10n), 5n)
}

test "bigint_clamp_below_min" {
    assert.equal((-3n).clamp(0n, 10n), 0n)
}

test "bigint_clamp_above_max" {
    assert.equal((15n).clamp(0n, 10n), 10n)
}

test "bigint_clamp_at_min" {
    assert.equal((0n).clamp(0n, 10n), 0n)
}

test "bigint_clamp_at_max" {
    assert.equal((10n).clamp(0n, 10n), 10n)
}

// ─── isqrt ────────────────────────────────────────────────────────────────────

test "bigint_isqrt_perfect_square" {
    assert.equal((16n).isqrt(), 4n)
}

test "bigint_isqrt_non_perfect_square" {
    assert.equal((10n).isqrt(), 3n)
}

test "bigint_isqrt_zero" {
    assert.equal((0n).isqrt(), 0n)
}

test "bigint_isqrt_one" {
    assert.equal((1n).isqrt(), 1n)
}

test "bigint_isqrt_negative_throws" {
    {
        (-1n).isqrt();
        baml.sys.panic("expected (-1n).isqrt() to throw")
    } catch (e) {
        baml.errors.InvalidArgument => null
    }
}

// ─── pow ──────────────────────────────────────────────────────────────────────

test "bigint_pow_basic" {
    assert.equal((2n).pow(10n), 1024n)
}

test "bigint_pow_zero_exp" {
    assert.equal((2n).pow(0n), 1n)
}

test "bigint_pow_zero_base_zero_exp" {
    assert.equal((0n).pow(0n), 1n)
}

test "bigint_pow_negative_base" {
    assert.equal((-2n).pow(3n), -8n)
}

test "bigint_pow_negative_exp_returns_zero" {
    assert.equal((2n).pow(-1n), 0n)
}

// 2^256 — exact 78-digit value.
test "bigint_pow_large" {
    assert.equal(
        (2n).pow(256n),
        115792089237316195423570985008687907853269984665640564039457584007913129639936n,
    )
}

// ─── ilog ─────────────────────────────────────────────────────────────────────

test "bigint_ilog_base10" {
    assert.equal((1000n).ilog(10n), 3n)
}

test "bigint_ilog_base2" {
    assert.equal((1024n).ilog(2n), 10n)
}

test "bigint_ilog_one_returns_zero" {
    assert.equal((1n).ilog(10n), 0n)
}

test "bigint_ilog_zero_throws" {
    {
        (0n).ilog(10n);
        baml.sys.panic("expected (0n).ilog(10n) to throw")
    } catch (e) {
        baml.errors.InvalidArgument => null
    }
}

test "bigint_ilog_negative_throws" {
    {
        (-5n).ilog(10n);
        baml.sys.panic("expected (-5n).ilog(10n) to throw")
    } catch (e) {
        baml.errors.InvalidArgument => null
    }
}

test "bigint_ilog_base_one_throws" {
    {
        (10n).ilog(1n);
        baml.sys.panic("expected (10n).ilog(1n) to throw")
    } catch (e) {
        baml.errors.InvalidArgument => null
    }
}

// ─── to_int ───────────────────────────────────────────────────────────────────

test "bigint_to_int_positive" {
    assert.equal((42n).to_int(), 42)
}

test "bigint_to_int_negative" {
    assert.equal((-7n).to_int(), -7)
}

test "bigint_to_int_zero" {
    assert.equal((0n).to_int(), 0)
}

// int is 63-bit signed, so the boundaries are ±2^62 — narrower than a machine
// 64-bit integer's. Both ends must round-trip exactly.
test "bigint_to_int_at_max_value" {
    assert.equal((4611686018427387903n).to_int(), int.max_value())
}

test "bigint_to_int_at_min_value" {
    assert.equal((-4611686018427387904n).to_int(), int.min_value())
}

// One past each boundary: these still fit an i64, so they exercise the i63
// range check rather than the i64 conversion.
test "bigint_to_int_above_max_throws" {
    {
        (4611686018427387904n).to_int();
        baml.sys.panic("expected (4611686018427387904n).to_int() to throw")
    } catch (e) {
        baml.errors.InvalidArgument => null
    }
}

test "bigint_to_int_below_min_throws" {
    {
        (-4611686018427387905n).to_int();
        baml.sys.panic("expected (-4611686018427387905n).to_int() to throw")
    } catch (e) {
        baml.errors.InvalidArgument => null
    }
}

// Far past i64 as well, exercising the conversion failure itself.
test "bigint_to_int_far_out_of_range_throws" {
    {
        (10n).pow(30n).to_int();
        baml.sys.panic("expected (10n).pow(30n).to_int() to throw")
    } catch (e) {
        baml.errors.InvalidArgument => null
    }
}

// ─── random ───────────────────────────────────────────────────────────────────

// [0, 1) contains exactly one value: 0.
test "bigint_random_single_element_range" {
    assert.equal(bigint.random(0n, 1n), 0n)
}

// Sample from [10, 20); result must be in that range.
function bigint_random_in_range_fn() -> bool {
    let r = bigint.random(10n, 20n);
    r >= 10n && r < 20n
}

test "bigint_random_in_range" {
    assert.is_true(bigint_random_in_range_fn())
}

test "bigint_random_empty_range_throws" {
    {
        bigint.random(5n, 5n);
        baml.sys.panic("expected bigint.random(5n, 5n) to throw")
    } catch (e) {
        baml.errors.InvalidArgument => null
    }
}

test "bigint_random_lower_greater_throws" {
    {
        bigint.random(10n, 0n);
        baml.sys.panic("expected bigint.random(10n, 0n) to throw")
    } catch (e) {
        baml.errors.InvalidArgument => null
    }
}

// ─── equality and ordering ────────────────────────────────────────────────────

test "test_bigint_eq_same_alloc" {
    assert.is_true(42n == 42n)
}

function bigint_eq_distinct_allocs_fn() -> bool {
    let a: bigint = 42n;
    let b: bigint = bigint.parse("42");
    a == b
}

test "test_bigint_eq_distinct_allocs" {
    assert.is_true(bigint_eq_distinct_allocs_fn())
}

test "test_bigint_ordering_lt" {
    assert.is_true(3n < 5n)
}

test "test_bigint_ordering_gt_eq" {
    assert.is_true(5n >= 5n)
}

test "test_bigint_ordering_ne" {
    assert.is_true(42n != 43n)
}

// Values larger than i64::MAX — verifies BigInt arithmetic, not i64.
test "test_bigint_ordering_large" {
    assert.is_true(99999999999999999999n > 99999999999999999998n)
}

// ─── typed arithmetic and bitwise ─────────────────────────────────────────────

test "test_bigint_arithmetic_add" {
    assert.is_true(2n + 3n == 5n)
}

test "test_bigint_arithmetic_sub" {
    assert.is_true(5n - 3n == 2n)
}

test "test_bigint_arithmetic_mul" {
    assert.is_true(2n * 3n == 6n)
}

test "test_bigint_arithmetic_div" {
    assert.is_true(7n / 2n == 3n)
}

test "test_bigint_arithmetic_mod" {
    assert.is_true(7n % 2n == 1n)
}

test "test_bigint_bitwise_and" {
    assert.is_true((15n & 240n) == 0n)
}

test "test_bigint_bitwise_or" {
    assert.is_true((15n | 240n) == 255n)
}

test "test_bigint_bitwise_xor" {
    assert.is_true((170n ^ 255n) == 85n)
}

test "test_bigint_bitwise_shl" {
    assert.is_true((1n << 4n) == 16n)
}

test "test_bigint_bitwise_shr" {
    assert.is_true((256n >> 4n) == 16n)
}

// num-bigint uses two's-complement for bitwise ops: -1 & 0xFF == 0xFF.
test "test_bigint_bitwise_negative" {
    assert.is_true(((-1n) & 255n) == 255n)
}

function bigint_div_by_zero_fn() -> bigint { 1n / 0n }

test "test_bigint_div_by_zero" {
    {
        bigint_div_by_zero_fn();
        baml.sys.panic("expected 1n / 0n to throw")
    } catch (e) {
        baml.panics.DivisionByZero => null
    }
}

function bigint_mod_by_zero_fn() -> bigint { 1n % 0n }

test "test_bigint_mod_by_zero" {
    {
        bigint_mod_by_zero_fn();
        baml.sys.panic("expected 1n % 0n to throw")
    } catch (e) {
        baml.panics.DivisionByZero => null
    }
}

// (2n).pow(64n) + 1n = 18446744073709551617, which does not fit in an i64.
test "test_bigint_arithmetic_huge" {
    assert.equal((2n).pow(64n) + 1n, 18446744073709551617n)
}

// ─── mixed `int OP bigint` arithmetic ─────────────────────────────────────────

test "test_bigint_int_mixed_add_left" {
    assert.is_true(2n + 3 == 5n)
}

test "test_bigint_int_mixed_add_right" {
    assert.is_true(3 + 2n == 5n)
}

test "test_bigint_int_mixed_mul" {
    assert.is_true(2n * 3 == 6n)
}

test "test_bigint_int_mixed_shl" {
    assert.is_true((1n << 4) == 16n)
}

test "test_bigint_int_mixed_bitand_neg_one_with_mask" {
    assert.is_true(((-1n) & 255) == 255n)
}

test "test_bigint_int_mixed_bitor" {
    assert.is_true((1n | 2) == 3n)
}

test "test_bigint_int_mixed_bitxor" {
    assert.is_true((5n ^ 3) == 6n)
}

test "test_bigint_int_mixed_shr" {
    assert.is_true((16n >> 2) == 4n)
}

// Ordering across distinct concrete types (`bigint` vs `int`) is a compile error:
// the two are atomic, with no common type implementing `Compare`. Convert explicitly
// to compare. (Cross-type ordering errors are pinned in the diagnostic-error tests.)

test "test_bigint_int_mixed_comparison_eq" {
    // `int` and `bigint` are distinct concrete types, so `==` is always false
    // (and the compiler warns it is provably disjoint) — no implicit coercion.
    assert.is_true((5 == 5n) == false)
}

// ─── AllocFailure / NegativeBitShift guards ───────────────────────────────────

// (2n).pow(1_000_000_000n) would allocate ~125 MB; AllocFailure is raised before
// any allocation is attempted.
test "test_bigint_pow_alloc_failure" {
    {
        (2n).pow(1000000000n);
        baml.sys.panic("expected oversized pow to raise AllocFailure")
    } catch (e) {
        baml.panics.AllocFailure => null
    }
}

function bigint_shl_alloc_failure_fn() -> bigint { 1n << 1000000000n }

test "test_bigint_shl_alloc_failure" {
    {
        bigint_shl_alloc_failure_fn();
        baml.sys.panic("expected oversized shl to raise AllocFailure")
    } catch (e) {
        baml.panics.AllocFailure => null
    }
}

function bigint_shl_negative_shift_fn() -> bigint { 1n << -1n }

test "test_bigint_shl_negative_shift" {
    {
        bigint_shl_negative_shift_fn();
        baml.sys.panic("expected negative shl to raise NegativeBitShift")
    } catch (e) {
        baml.panics.NegativeBitShift => null
    }
}

function bigint_shr_negative_shift_fn() -> bigint { 1n >> -1n }

test "test_bigint_shr_negative_shift" {
    {
        bigint_shr_negative_shift_fn();
        baml.sys.panic("expected negative shr to raise NegativeBitShift")
    } catch (e) {
        baml.panics.NegativeBitShift => null
    }
}

// Two operands each ≈ MAX_BIGINT_BITS / 2; their product exceeds the per-alloc
// cap, so the pre-flight check rejects before materializing `a * b`.
function bigint_mul_preflight_fn() -> bigint {
    let a = (2n).pow(200000000n);
    let b = (2n).pow(200000000n);
    a * b
}

test "test_bigint_mul_pre_flight_alloc_failure" {
    {
        bigint_mul_preflight_fn();
        baml.sys.panic("expected oversized mul to raise AllocFailure")
    } catch (e) {
        baml.panics.AllocFailure => null
    }
}

// (2n).pow(256n) is a 78-digit number — well within MAX_BIGINT_BITS.
test "test_bigint_pow_normal_works" {
    assert.equal(
        (2n).pow(256n),
        115792089237316195423570985008687907853269984665640564039457584007913129639936n,
    )
}

// ─── pow edge cases (negative exp + small bases) ──────────────────────────────

test "test_bigint_pow_neg_exp_one" {
    assert.equal((1n).pow(-1n), 0n)
}

test "test_bigint_pow_neg_exp_neg_one" {
    assert.equal((-1n).pow(-2n), 0n)
}

test "test_bigint_pow_neg_exp_zero_base" {
    assert.equal((0n).pow(-1n), 0n)
}

// 0^0 == 1 by convention.
test "test_bigint_pow_zero_zero" {
    assert.equal((0n).pow(0n), 1n)
}

test "test_bigint_pow_one_huge_exponent" {
    assert.equal((1n).pow(1000000000n), 1n)
}

test "test_bigint_pow_neg_one_huge_even" {
    assert.equal((-1n).pow(1000000000n), 1n)
}

test "test_bigint_pow_neg_one_huge_odd" {
    assert.equal((-1n).pow(1000000001n), -1n)
}

test "test_bigint_pow_zero_huge_exponent" {
    assert.equal((0n).pow(1000000000n), 0n)
}

// ─── ilog efficiency regression (large input, base 2 and general) ─────────────

test "test_bigint_ilog_base_two" {
    assert.equal((1024n).ilog(2n), 10n)
}

test "test_bigint_ilog_base_ten" {
    assert.equal((1000n).ilog(10n), 3n)
}

// Binary-search path on a value where the linear approach would take ~1M iters.
test "test_bigint_ilog_large_input" {
    assert.equal(((2n).pow(1000n)).ilog(2n), 1000n)
}

// ilog floors (rounds down).
test "test_bigint_ilog_floor" {
    assert.equal((1023n).ilog(2n), 9n)
}

// ─── to_json ──────────────────────────────────────────────────────────────────
//
// Bigint values exceed JSON's safe-integer range, so to_json emits a decimal
// string.

test "test_bigint_to_json_returns_string" {
    assert.equal((42n).to_json(), "42")
}

test "test_bigint_to_json_large" {
    assert.equal((99999999999999999999n).to_json(), "99999999999999999999")
}

// ─── regression: bare-literal method receiver (B-614) ─────────────────────────
//
// `42n.to_string()` on a bare bigint literal used to lower its receiver to a
// Missing expr and panic at runtime. Appended at the end so the test registry
// stays append-only.
test "bigint_to_string_bare_literal" {
    assert.equal(42n.to_string(), "42")
}

// ─── literal bases and underscore separators ─────────────────────────────────
// Bigint literals support the same 0x/0o/0b prefixes and `_` separators as
// int literals, with the trailing `n` suffix.

test "bigint_hex_literal" {
    assert.equal(0xFFn, 255n)
}

test "bigint_octal_literal" {
    assert.equal(0o755n, 493n)
}

test "bigint_binary_literal" {
    assert.equal(0b1010n, 10n)
}

test "bigint_underscore_separators" {
    assert.equal(1_000n, 1000n)
}

test "bigint_hex_underscores" {
    assert.equal(0xFF_FFn, 65535n)
}

test "bigint_negative_prefixed" {
    assert.equal(-0xFFn, -255n)
}

// 2^80 - 1: prefixed bigint literals are not bounded by the i64 (or i63)
// limits that apply to int literals.
test "bigint_hex_beyond_int_range" {
    assert.equal(0xFFFF_FFFF_FFFF_FFFF_FFFFn, 1208925819614629174706175n)
}
