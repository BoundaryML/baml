// Tests for the `int` builtin class (BEP-043).
//
// Converted from crates/baml_tests/tests/ints.rs. Each Rust test becomes a
// `test` block with the `main()` body inlined. Results use `assert.equal`
// for structural comparison.
//
// Pure expressions (no locals) are asserted inline. Tests that use a local
// `n` go through a top-level helper function to avoid the test-block local
// boxing VM bug.
//
// Random tests use property/coverage checks (looping N samples) instead of
// asserting exact values; they are included below in the random section.
// SKIPPED — deterministic throw tests for random (invalid range):
//   (int_random_lower_equals_upper_throws and int_random_lower_greater_than_upper_throws
//    are included below — they are purely deterministic throw checks)

// ─── abs ──────────────────────────────────────────────────────────────────────

test "int_abs_positive" {
    assert.equal((3).abs(), 3)
}

test "int_abs_negative" {
    assert.equal((-7).abs(), 7)
}

test "int_abs_zero" {
    assert.equal((0).abs(), 0)
}

// int.min_value() is -2^62; its abs() is 2^62 which exceeds int.max_value().
test "int_abs_min_value_panics" {
    {
        int.min_value().abs();
        baml.sys.panic("expected int.min_value().abs() to panic")
    } catch (e) {
        baml.panics.IntegerOverflow => null
    }
}

test "int_abs_near_min_value_succeeds" {
    assert.equal((-4611686018427387903).abs(), 4611686018427387903)
}

// ─── min / max ────────────────────────────────────────────────────────────────

test "int_min_basic" {
    assert.equal((3).min(5), 3)
}

test "int_min_other_smaller" {
    assert.equal((5).min(3), 3)
}

test "int_min_equal" {
    assert.equal((3).min(3), 3)
}

test "int_min_negative" {
    assert.equal((-2).min(0), -2)
}

test "int_max_basic" {
    assert.equal((3).max(5), 5)
}

test "int_max_other_smaller" {
    assert.equal((5).max(3), 5)
}

test "int_max_negative" {
    assert.equal((-5).max(-2), -2)
}

// ─── clamp ────────────────────────────────────────────────────────────────────

test "int_clamp_in_range" {
    assert.equal((5).clamp(0, 10), 5)
}

test "int_clamp_below_min" {
    assert.equal((-3).clamp(0, 10), 0)
}

test "int_clamp_above_max" {
    assert.equal((15).clamp(0, 10), 10)
}

test "int_clamp_at_min_boundary" {
    assert.equal((0).clamp(0, 10), 0)
}

test "int_clamp_at_max_boundary" {
    assert.equal((10).clamp(0, 10), 10)
}

// ─── max_value / min_value ────────────────────────────────────────────────────

test "int_max_value_returns_i63_max" {
    assert.equal(int.max_value(), 4611686018427387903)
}

test "int_min_value_returns_i63_min" {
    assert.equal(int.min_value(), -4611686018427387904)
}

test "int_max_min_inverse" {
    assert.is_true(int.max_value() > int.min_value())
}

// ─── parse ────────────────────────────────────────────────────────────────────

test "int_parse_basic" {
    assert.equal(int.parse("42"), 42)
}

test "int_parse_zero" {
    assert.equal(int.parse("0"), 0)
}

test "int_parse_negative" {
    assert.equal(int.parse("-7"), -7)
}

test "int_parse_explicit_plus_sign" {
    assert.equal(int.parse("+42"), 42)
}

test "int_parse_max_value" {
    assert.equal(int.parse("4611686018427387903"), 4611686018427387903)
}

test "int_parse_min_value" {
    assert.equal(int.parse("-4611686018427387904"), -4611686018427387904)
}

test "int_parse_leading_zeros_ok" {
    assert.equal(int.parse("007"), 7)
}

test "int_parse_empty_throws" {
    {
        int.parse("");
        baml.sys.panic("expected int.parse(\"\") to throw")
    } catch (e) {
        baml.errors.ParseError => null
    }
}

test "int_parse_non_digit_throws" {
    {
        int.parse("12a");
        baml.sys.panic("expected int.parse(\"12a\") to throw")
    } catch (e) {
        baml.errors.ParseError => null
    }
}

test "int_parse_alpha_throws" {
    {
        int.parse("hello");
        baml.sys.panic("expected int.parse(\"hello\") to throw")
    } catch (e) {
        baml.errors.ParseError => null
    }
}

test "int_parse_whitespace_throws" {
    {
        int.parse(" 5 ");
        baml.sys.panic("expected int.parse(\" 5 \") to throw")
    } catch (e) {
        baml.errors.ParseError => null
    }
}

test "int_parse_overflow_throws" {
    {
        int.parse("9223372036854775808");
        baml.sys.panic("expected int.parse(\"9223372036854775808\") to throw")
    } catch (e) {
        baml.errors.ParseError => null
    }
}

test "int_parse_underflow_throws" {
    {
        int.parse("-9223372036854775809");
        baml.sys.panic("expected int.parse(\"-9223372036854775809\") to throw")
    } catch (e) {
        baml.errors.ParseError => null
    }
}

test "int_parse_just_sign_throws" {
    {
        int.parse("-");
        baml.sys.panic("expected int.parse(\"-\") to throw")
    } catch (e) {
        baml.errors.ParseError => null
    }
}

test "int_parse_underscore_throws" {
    {
        int.parse("1_000");
        baml.sys.panic("expected int.parse(\"1_000\") to throw")
    } catch (e) {
        baml.errors.ParseError => null
    }
}

test "int_parse_round_trip_via_to_string" {
    assert.equal(int.parse("100") + int.parse("23"), 123)
}

// ─── random ───────────────────────────────────────────────────────────────────
//
// Property-based checks: sample many times and verify structural properties.
// All sampling logic lives in helper functions (test-block locals are boxed).

// int_random_in_range_and_covers_endpoints: 1000 samples from [0, 10).
// All must be in [0, 10) AND both 0 and 9 must appear.
// P(missing endpoint) = (9/10)^1000 ≈ 1.7e-46.
function int_random_in_range_and_covers_endpoints() -> bool {
    let ok = true;
    let saw_lo = false;
    let saw_hi = false;
    let i = 0;
    while (i < 1000) {
        let r = int.random(0, 10);
        if (r < 0 || r >= 10) {
            ok = false
        } else {
            null
        }
        if (r == 0) {
            saw_lo = true
        } else {
            null
        }
        if (r == 9) {
            saw_hi = true
        } else {
            null
        }
        i = i + 1
    }
    ok && saw_lo && saw_hi
}

test "int_random_in_range_and_covers_endpoints" {
    assert.is_true(int_random_in_range_and_covers_endpoints())
}

// int_random_negative_range: 200 samples from [-5, 5). All must be in [-5, 5).
function int_random_negative_range() -> bool {
    let ok = true;
    let i = 0;
    while (i < 200) {
        let r = int.random(-5, 5);
        if (r < -5 || r >= 5) {
            ok = false
        } else {
            null
        }
        i = i + 1
    }
    ok
}

test "int_random_negative_range" {
    assert.is_true(int_random_negative_range())
}

// int_random_single_element_range: [0, 1) has only one outcome: 0.
// 50 samples, all must equal 0.
function int_random_single_element_range() -> bool {
    let ok = true;
    let i = 0;
    while (i < 50) {
        let r = int.random(0, 1);
        if (r != 0) {
            ok = false
        } else {
            null
        }
        i = i + 1
    }
    ok
}

test "int_random_single_element_range" {
    assert.is_true(int_random_single_element_range())
}

// One call spanning the widest representable half-open int range. Just verify
// that the call succeeds and excludes the upper endpoint.
function int_random_widest_range_returns_some_int() -> bool {
    let r = int.random(int.min_value(), int.max_value());
    r >= int.min_value() && r < int.max_value()
}

test "int_random_widest_range_returns_some_int" {
    assert.is_true(int_random_widest_range_returns_some_int())
}

// int_random_distinct_values_in_wide_range: 100 samples from [0, 1000).
// We verify the range (max - min) is large as a proxy for diversity. With 100
// uniform samples over 1000 values, the expected range is ~989 (order statistics
// E[max] ≈ 990, E[min] ≈ 10). P(range < 500) is negligibly small.
function int_random_distinct_values_in_wide_range() -> bool {
    let min_seen = 1000;
    let max_seen = 0;
    let i = 0;
    while (i < 100) {
        let r = int.random(0, 1000);
        if (r < min_seen) {
            min_seen = r
        } else {
            null
        }
        if (r > max_seen) {
            max_seen = r
        } else {
            null
        }
        i = i + 1
    }
    (max_seen - min_seen) >= 500
}

test "int_random_distinct_values_in_wide_range" {
    assert.is_true(int_random_distinct_values_in_wide_range())
}

class FixedBytesRng {
    draw: uint8array,

    implements baml.random.Rng {
        function random(self, bytes: int) -> uint8array {
            self.draw
        }
    }
}

test "random_int_maps_all_i63_bits" {
    let all_ones = FixedBytesRng { draw: b"\xff\xff\xff\xff\xff\xff\xff\xff" };
    let all_zeros = FixedBytesRng { draw: b"\x00\x00\x00\x00\x00\x00\x00\x00" };
    let sign_only = FixedBytesRng { draw: b"\x40\x00\x00\x00\x00\x00\x00\x00" };
    let max = FixedBytesRng { draw: b"\x3f\xff\xff\xff\xff\xff\xff\xff" };
    assert.equal(all_ones.random_int(), -1);
    assert.equal(all_zeros.random_int(), 0);
    assert.equal(sign_only.random_int(), int.min_value());
    assert.equal(max.random_int(), int.max_value())
}

class RejectionThenAcceptedRng {
    calls: int,

    implements baml.random.Rng {
        function random(self, bytes: int) -> uint8array {
            let empty = b"";
            empty
        }

        function random_int(self) -> int {
            self.calls += 1;
            if (self.calls == 1) {
                // The maximum draw is in the three-value rejection tail for
                // [0, 5).
                int.max_value()
            } else {
                // This maps to 4 after rebiasing from the signed i63 domain.
                int.min_value() + 4
            }
        }
    }
}

test "int_random_rejection_boundary" {
    let rng = RejectionThenAcceptedRng { calls: 0 };
    assert.equal(int.random(0, 5, rng = rng), 4);
    assert.equal(rng.calls, 2)
}

// Throw tests (deterministic — included regardless of randomness skip).

test "int_random_lower_equals_upper_throws" {
    {
        int.random(5, 5);
        baml.sys.panic("expected int.random(5, 5) to throw")
    } catch (e) {
        baml.errors.InvalidArgument => null
    }
}

test "int_random_lower_greater_than_upper_throws" {
    {
        int.random(10, 0);
        baml.sys.panic("expected int.random(10, 0) to throw")
    } catch (e) {
        baml.errors.InvalidArgument => null
    }
}

// ─── isqrt ────────────────────────────────────────────────────────────────────

test "int_isqrt_perfect_square" {
    assert.equal((16).isqrt(), 4)
}

test "int_isqrt_non_square_rounds_down" {
    assert.equal((10).isqrt(), 3)
}

test "int_isqrt_zero" {
    assert.equal((0).isqrt(), 0)
}

test "int_isqrt_one" {
    assert.equal((1).isqrt(), 1)
}

test "int_isqrt_large" {
    assert.equal((1000000000000).isqrt(), 1000000)
}

test "int_isqrt_max_value" {
    assert.equal(int.max_value().isqrt(), 2147483647)
}

test "int_isqrt_negative_throws" {
    {
        (-1).isqrt();
        baml.sys.panic("expected (-1).isqrt() to throw")
    } catch (e) {
        baml.errors.InvalidArgument => null
    }
}

test "int_isqrt_min_value_throws" {
    {
        int.min_value().isqrt();
        baml.sys.panic("expected int.min_value().isqrt() to throw")
    } catch (e) {
        baml.errors.InvalidArgument => null
    }
}

// ─── pow ──────────────────────────────────────────────────────────────────────

test "int_pow_basic" {
    assert.equal((2).pow(10), 1024)
}

test "int_pow_zero_exponent" {
    assert.equal((5).pow(0), 1)
}

test "int_pow_zero_to_zero_is_one" {
    assert.equal((0).pow(0), 1)
}

test "int_pow_zero_to_positive_is_zero" {
    assert.equal((0).pow(5), 0)
}

test "int_pow_negative_exponent_returns_zero" {
    assert.equal((2).pow(-1), 0)
}

test "int_pow_negative_base_odd_exp" {
    assert.equal((-2).pow(3), -8)
}

test "int_pow_negative_base_even_exp" {
    assert.equal((-2).pow(4), 16)
}

test "int_pow_one_anything" {
    assert.equal((1).pow(1000000), 1)
}

test "int_pow_negative_one_even" {
    assert.equal((-1).pow(1000000), 1)
}

test "int_pow_negative_one_odd" {
    assert.equal((-1).pow(1000001), -1)
}

// 2^33 = 8589934592, even — result should be 1.
test "int_pow_negative_one_huge_even_exp" {
    assert.equal((-1).pow(8589934592), 1)
}

// Overflow: (10)^100 → saturates to int.max_value().
test "int_pow_overflow_positive_saturates" {
    assert.equal((10).pow(100), 4611686018427387903)
}

// Overflow: (-10)^99 → saturates to int.min_value().
test "int_pow_overflow_negative_odd_saturates_to_min" {
    assert.equal((-10).pow(99), -4611686018427387904)
}

// Overflow: (-10)^100 → saturates to int.max_value().
test "int_pow_overflow_negative_even_saturates_to_max" {
    assert.equal((-10).pow(100), 4611686018427387903)
}

// ─── ilog ─────────────────────────────────────────────────────────────────────

test "int_ilog_base_10" {
    assert.equal((1000).ilog(10), 3)
}

test "int_ilog_base_2_power_of_two" {
    assert.equal((1024).ilog(2), 10)
}

test "int_ilog_rounds_down" {
    assert.equal((999).ilog(10), 2)
}

test "int_ilog_one_is_zero" {
    assert.equal((1).ilog(10), 0)
}

test "int_ilog_self_zero_throws" {
    {
        (0).ilog(10);
        baml.sys.panic("expected (0).ilog(10) to throw")
    } catch (e) {
        baml.errors.InvalidArgument => null
    }
}

test "int_ilog_self_negative_throws" {
    {
        (-5).ilog(10);
        baml.sys.panic("expected (-5).ilog(10) to throw")
    } catch (e) {
        baml.errors.InvalidArgument => null
    }
}

test "int_ilog_base_one_throws" {
    {
        (10).ilog(1);
        baml.sys.panic("expected (10).ilog(1) to throw")
    } catch (e) {
        baml.errors.InvalidArgument => null
    }
}

test "int_ilog_base_zero_throws" {
    {
        (10).ilog(0);
        baml.sys.panic("expected (10).ilog(0) to throw")
    } catch (e) {
        baml.errors.InvalidArgument => null
    }
}

test "int_ilog_base_negative_throws" {
    {
        (10).ilog(-2);
        baml.sys.panic("expected (10).ilog(-2) to throw")
    } catch (e) {
        baml.errors.InvalidArgument => null
    }
}

// ─── Bit operations ───────────────────────────────────────────────────────────

test "int_leading_zeros_zero" {
    assert.equal((0).leading_zeros(), 64)
}

test "int_leading_zeros_one" {
    assert.equal((1).leading_zeros(), 63)
}

test "int_leading_zeros_negative_one_is_zero" {
    assert.equal((-1).leading_zeros(), 0)
}

// 2^32 = 4294967296: 31 leading zeros (bit 32 set out of 64).
test "int_leading_zeros_large_power_of_two" {
    assert.equal((4294967296).leading_zeros(), 31)
}

test "int_leading_ones_zero" {
    assert.equal((0).leading_ones(), 0)
}

test "int_leading_ones_negative_one" {
    assert.equal((-1).leading_ones(), 64)
}

test "int_trailing_zeros_zero" {
    assert.equal((0).trailing_zeros(), 64)
}

test "int_trailing_zeros_one" {
    assert.equal((1).trailing_zeros(), 0)
}

// 8 = 0b1000 → three trailing zeros.
test "int_trailing_zeros_eight" {
    assert.equal((8).trailing_zeros(), 3)
}

test "int_trailing_ones_zero" {
    assert.equal((0).trailing_ones(), 0)
}

// 7 = 0b111.
test "int_trailing_ones_seven" {
    assert.equal((7).trailing_ones(), 3)
}

test "int_trailing_ones_negative_one" {
    assert.equal((-1).trailing_ones(), 64)
}

test "int_count_zeros_of_zero" {
    assert.equal((0).count_zeros(), 64)
}

test "int_count_zeros_of_negative_one" {
    assert.equal((-1).count_zeros(), 0)
}

test "int_count_ones_of_zero" {
    assert.equal((0).count_ones(), 0)
}

test "int_count_ones_of_seven" {
    assert.equal((7).count_ones(), 3)
}

test "int_count_ones_of_negative_one" {
    assert.equal((-1).count_ones(), 64)
}

// For any 64-bit integer, count_zeros + count_ones must equal 64.
// Uses a helper because the local `n` would be boxed in a test block.
function count_zeros_plus_ones(n: int) -> int {
    n.count_zeros() + n.count_ones()
}

test "int_count_zeros_plus_count_ones_equals_64" {
    // 51966 = 0xCAFE — arbitrary mixed bit pattern
    assert.equal(count_zeros_plus_ones(51966), 64)
}

// ─── Operator overflow & boundary arithmetic ────────────────────────────────
//
// `+`, `-`, `*` and unary `-` throw a catchable `baml.panics.IntegerOverflow`
// when the result leaves the i63 range (catch tests live in ns_exceptions).
// Valid arithmetic right up to the boundary must NOT spuriously throw. Helpers
// take runtime args so the ops aren't constant-folded — they run through the
// VM's checked AddInt/SubInt/MulInt opcodes.
function int_add_(a: int, b: int) -> int {
    a + b
}

function int_sub_(a: int, b: int) -> int {
    a - b
}

function int_mul_(a: int, b: int) -> int {
    a * b
}

test "int_add_at_max_boundary_no_overflow" {
    // (MAX - 1) + 1 == MAX
    assert.equal(int_add_(int.max_value() - 1, 1), int.max_value())
}

test "int_sub_at_min_boundary_no_overflow" {
    // (MIN + 1) - 1 == MIN
    assert.equal(int_sub_(int.min_value() + 1, 1), int.min_value())
}

test "int_mul_max_by_one_no_overflow" {
    assert.equal(int_mul_(int.max_value(), 1), int.max_value())
}

test "int_mul_min_by_one_no_overflow" {
    assert.equal(int_mul_(int.min_value(), 1), int.min_value())
}

// ─── regression: bare-literal method receiver (B-614) ─────────────────────────
//
// A method called directly on a bare integer literal (no wrapping parens) used
// to lower its receiver to a Missing expr: `.abs()` failed to type-check and
// `.to_string()` panicked at runtime with "parse error". The receiver must be
// the literal itself. Kept at the end of the file so the test registry stays
// append-only.
test "int_abs_bare_literal" {
    assert.equal(3.abs(), 3)
}

test "int_to_string_bare_literal" {
    assert.equal(7.to_string(), "7")
}

// ─── literal bases and underscore separators ─────────────────────────────────
// Hex (0x), octal (0o), and binary (0b) integer literals, with `_` digit
// separators allowed everywhere Rust allows them. Value parsing/validation
// lives in baml_base::num_lit; invalid forms are covered by the
// diagnostic_errors/invalid_numeric_literals project.

test "int_hex_literal" {
    assert.equal(0xFF, 255)
}

test "int_hex_lowercase_digits" {
    assert.equal(0xcafe, 51966)
}

test "int_hex_mixed_case_digits" {
    assert.equal(0xCaFe, 51966)
}

test "int_octal_literal" {
    assert.equal(0o755, 493)
}

test "int_binary_literal" {
    assert.equal(0b1010, 10)
}

test "int_prefixed_zero" {
    assert.equal(0x0, 0);
    assert.equal(0o0, 0);
    assert.equal(0b0, 0)
}

test "int_negative_prefixed" {
    assert.equal(-0xFF, -255);
    assert.equal(-0b1010, -10);
    assert.equal(-0o755, -493)
}

test "int_underscore_separators" {
    assert.equal(1_000_000, 1000000)
}

test "int_trailing_underscore" {
    assert.equal(1_, 1)
}

test "int_hex_underscores" {
    assert.equal(0xFF_FF, 65535)
}

test "int_binary_underscores" {
    assert.equal(0b10_10, 10)
}

test "int_octal_underscores" {
    assert.equal(0o7_5_5, 493)
}

test "int_underscore_after_prefix" {
    assert.equal(0x_F, 15)
}

// int.max_value() is 2^62 - 1: the full positive i63 range is reachable in
// hex.
test "int_hex_max_value" {
    assert.equal(int.max_value(), 0x3FFF_FFFF_FFFF_FFFF)
}

// Bitwise ops read naturally with hex/binary operands (previously the bit
// patterns had to be spelled in decimal).
test "int_hex_bitwise" {
    assert.equal(0xCAFE & 0xFF, 0xFE);
    assert.equal(0b1100 | 0b0011, 0b1111);
    assert.equal(0b1100 ^ 0b1010, 0b0110)
}

// Maximal munch: a method call on a bare prefixed literal must not absorb
// the dot (`0xFF.to_string()` is INTEGER_LITERAL DOT WORD).
test "int_hex_bare_literal_method" {
    assert.equal(0xFF.to_string(), "255")
}
