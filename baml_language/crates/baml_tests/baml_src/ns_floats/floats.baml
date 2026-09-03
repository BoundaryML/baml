// Tests for the `float` builtin class (BEP-043).
//
// Assertion strategy:
//   - Boolean predicates (is_nan, is_infinite, is_finite): asserted directly.
//   - Exact float values (integer-valued results): `assert.equal`.
//   - Approximate float values (transcendental results): inline epsilon check
//     `assert.is_true(((result) - (expected)).abs() < eps)`.
//   - NaN results: assert.is_true(result.is_nan()).
//
// Random tests use property/coverage checks (looping N samples) instead of
// asserting exact values; they are included at the bottom of this file.

// ─── is_nan ───────────────────────────────────────────────────────────────────

test "float_is_nan_true" {
    assert.is_true(float.nan().is_nan())
}

test "float_is_nan_false_for_zero" {
    assert.is_true(!(0.0).is_nan())
}

test "float_is_nan_false_for_finite" {
    assert.is_true(!(3.14).is_nan())
}

test "float_is_nan_false_for_infinity" {
    assert.is_true(!float.inf().is_nan())
}

// ─── is_infinite ──────────────────────────────────────────────────────────────

test "float_is_infinite_true_positive" {
    assert.is_true(float.inf().is_infinite())
}

test "float_is_infinite_true_negative" {
    assert.is_true((-float.inf()).is_infinite())
}

test "float_is_infinite_false_for_large_finite" {
    assert.is_true(!(1000000.0 * 1000000.0).is_infinite())
}

test "float_is_infinite_false_for_nan" {
    assert.is_true(!float.nan().is_infinite())
}

// ─── is_finite ────────────────────────────────────────────────────────────────

test "float_is_finite_true_for_zero" {
    assert.is_true((0.0).is_finite())
}

test "float_is_finite_true_for_normal" {
    assert.is_true((3.14).is_finite())
}

test "float_is_finite_true_for_negative_zero" {
    assert.is_true((-0.0).is_finite())
}

test "float_is_finite_false_for_nan" {
    assert.is_true(!float.nan().is_finite())
}

test "float_is_finite_false_for_infinity" {
    assert.is_true(!float.inf().is_finite())
}

test "float_is_finite_false_for_negative_infinity" {
    assert.is_true(!(-float.inf()).is_finite())
}

// ─── Constants ────────────────────────────────────────────────────────────────

// Verify pi via the identity cos(π) == -1 (cos is computed independently of the
// pi constant).
test "float_pi_value" {
    assert.is_true(((float.pi().cos()) - (-1.0)).abs() < 0.000000000001)
}

// e has no simple algebraic identity among the available float ops (no exp/ln,
// and log_base(base) == 1 for any base), so compare against the constant.
test "float_e_value" {
    assert.is_true(((float.e()) - 2.718281828459045).abs() < 0.000000000001)
}

// golden_ratio = (1 + sqrt(5)) / 2.
test "float_golden_ratio_value" {
    assert.is_true(((float.golden_ratio()) - ((1.0 + (5.0).sqrt()) / 2.0)).abs() < 0.000000000001)
}

test "float_nan_is_a_nan" {
    assert.is_true(float.nan().is_nan())
}

test "float_inf_is_positive_infinity" {
    assert.is_true(float.inf().is_infinite())
}

test "float_inf_is_positive_not_negative" {
    assert.is_true(float.inf() > 0.0)
}

test "float_negative_inf_via_unary_minus" {
    assert.is_true((-float.inf()).is_infinite())
}

test "float_negative_inf_is_negative" {
    assert.is_true(-float.inf() < 0.0)
}

// ─── abs ──────────────────────────────────────────────────────────────────────

test "float_abs_negative" {
    assert.is_true(((-3.5).abs() - 3.5).abs() < 0.000000000001)
}

test "float_abs_positive" {
    assert.is_true(((3.5).abs() - 3.5).abs() < 0.000000000001)
}

// Note: float_abs_negative_zero (sign-of-zero) is at the end of this file —
// the test registry stays append-only.

test "float_abs_nan_is_nan" {
    assert.is_true(float.nan().abs().is_nan())
}

test "float_abs_inf_is_inf" {
    assert.is_true((-float.inf()).abs().is_infinite())
}

test "float_abs_inf_becomes_positive" {
    assert.is_true((-float.inf()).abs() > 0.0)
}

// ─── min / max ────────────────────────────────────────────────────────────────

test "float_min_basic" {
    assert.is_true(((3.0).min(5.0) - 3.0).abs() < 0.000000000001)
}

// NaN is the largest float in BAML's total order, so it loses every `min` it
// does not tie. Both receiver forms agree, and both agree with `Compare.min`.
test "float_min_with_nan_keeps_the_number" {
    assert.equal((3.0).min(float.nan()), 3.0)
}

test "float_min_nan_receiver_keeps_the_number" {
    assert.equal(float.nan().min(3.0), 3.0)
}

test "float_min_both_nan_is_nan" {
    assert.is_true(float.nan().min(float.nan()).is_nan())
}

test "float_max_basic" {
    assert.is_true(((3.0).max(5.0) - 5.0).abs() < 0.000000000001)
}

// The mirror of `min`: NaN is the largest float, so it wins every `max`.
test "float_max_with_nan_is_nan" {
    assert.is_true((3.0).max(float.nan()).is_nan())
}

test "float_max_nan_receiver_is_nan" {
    assert.is_true(float.nan().max(3.0).is_nan())
}

// A literal receiver and a `float`-typed one must resolve to the same `max`.
function float_max_via_local() -> float throws never {
    let a: float = 3.0
    a.max(float.nan())
}

test "float_max_receiver_form_does_not_change_the_answer" {
    assert.is_true(float_max_via_local().is_nan())
}

// ─── clamp ────────────────────────────────────────────────────────────────────

test "float_clamp_in_range" {
    assert.is_true(((5.0).clamp(0.0, 10.0) - 5.0).abs() < 0.000000000001)
}

test "float_clamp_below" {
    assert.is_true(((-3.0).clamp(0.0, 10.0) - 0.0).abs() < 0.000000000001)
}

test "float_clamp_above" {
    assert.is_true(((15.0).clamp(0.0, 10.0) - 10.0).abs() < 0.000000000001)
}

// NaN clamps down to `max` rather than propagating: it is the largest float, so
// the upper clamp catches it.
test "float_clamp_nan_clamps_to_max" {
    assert.is_true(((float.nan()).clamp(0.0, 10.0) - 10.0).abs() < 0.000000000001)
}

// ─── floor / ceil / round / trunc / fract ─────────────────────────────────────

test "float_floor_positive_fraction" {
    assert.is_true(((3.7).floor() - 3.0).abs() < 0.000000000001)
}

test "float_floor_negative_fraction" {
    assert.is_true(((-1.5).floor() - (-2.0)).abs() < 0.000000000001)
}

test "float_floor_integer_unchanged" {
    assert.is_true(((3.0).floor() - 3.0).abs() < 0.000000000001)
}

test "float_ceil_positive_fraction" {
    assert.is_true(((3.2).ceil() - 4.0).abs() < 0.000000000001)
}

test "float_ceil_negative_fraction" {
    assert.is_true(((-1.5).ceil() - (-1.0)).abs() < 0.000000000001)
}

test "float_round_half_up" {
    assert.is_true(((1.5).round() - 2.0).abs() < 0.000000000001)
}

test "float_round_negative_half_away_from_zero" {
    assert.is_true(((-1.5).round() - (-2.0)).abs() < 0.000000000001)
}

test "float_round_below_half" {
    assert.is_true(((1.4).round() - 1.0).abs() < 0.000000000001)
}

test "float_round_above_half" {
    assert.is_true(((1.6).round() - 2.0).abs() < 0.000000000001)
}

// Half-away-from-zero: 2.5 → 3.0 (not banker's rounding 2.0).
test "float_round_two_point_five_is_three" {
    assert.is_true(((2.5).round() - 3.0).abs() < 0.000000000001)
}

test "float_round_negative_two_point_five_is_neg_three" {
    assert.is_true(((-2.5).round() - (-3.0)).abs() < 0.000000000001)
}

test "float_trunc_positive" {
    assert.is_true(((3.7).trunc() - 3.0).abs() < 0.000000000001)
}

test "float_trunc_negative" {
    assert.is_true(((-3.7).trunc() - (-3.0)).abs() < 0.000000000001)
}

test "float_fract_positive" {
    assert.is_true(((3.75).fract() - 0.75).abs() < 0.000000000001)
}

test "float_fract_negative" {
    assert.is_true(((-3.75).fract() - (-0.75)).abs() < 0.000000000001)
}

test "float_fract_integer_is_zero" {
    assert.is_true(((5.0).fract() - 0.0).abs() < 0.000000000001)
}

test "float_floor_nan_propagates" {
    assert.is_true(float.nan().floor().is_nan())
}

// ─── ifloor / iceil / iround / itrunc ─────────────────────────────────────────

test "float_ifloor_positive" {
    assert.equal((3.7).ifloor(), 3)
}

test "float_ifloor_negative" {
    assert.equal((-1.5).ifloor(), -2)
}

test "float_iceil_positive" {
    assert.equal((3.2).iceil(), 4)
}

test "float_iceil_negative" {
    assert.equal((-1.5).iceil(), -1)
}

test "float_iround_half_up" {
    assert.equal((1.5).iround(), 2)
}

test "float_iround_negative_half" {
    assert.equal((-1.5).iround(), -2)
}

test "float_iround_below_half" {
    assert.equal((1.4).iround(), 1)
}

test "float_itrunc_positive" {
    assert.equal((3.7).itrunc(), 3)
}

test "float_itrunc_negative" {
    assert.equal((-3.7).itrunc(), -3)
}

// -2^62 (BAML int.min_value()) is exactly representable as f64.
test "float_iround_at_i63_min_is_ok" {
    assert.equal((-4611686018427387904.0).iround(), -4611686018427387904)
}

test "float_iround_too_large_throws" {
    {
        (9223372036854775808.0).iround();
        baml.sys.panic("expected (9223372036854775808.0).iround() to throw")
    } catch (e) {
        baml.errors.InvalidArgument => null
    }
}

test "float_iround_too_small_throws" {
    {
        (-9223372036854775808.0 * 2.0).iround();
        baml.sys.panic("expected overflow iround to throw")
    } catch (e) {
        baml.errors.InvalidArgument => null
    }
}

test "float_itrunc_nan_throws" {
    {
        float.nan().itrunc();
        baml.sys.panic("expected float.nan().itrunc() to throw")
    } catch (e) {
        baml.errors.InvalidArgument => null
    }
}

test "float_itrunc_inf_throws" {
    {
        float.inf().itrunc();
        baml.sys.panic("expected float.inf().itrunc() to throw")
    } catch (e) {
        baml.errors.InvalidArgument => null
    }
}

test "float_iround_nan_throws" {
    {
        float.nan().iround();
        baml.sys.panic("expected float.nan().iround() to throw")
    } catch (e) {
        baml.errors.InvalidArgument => null
    }
}

test "float_iround_inf_throws" {
    {
        float.inf().iround();
        baml.sys.panic("expected float.inf().iround() to throw")
    } catch (e) {
        baml.errors.InvalidArgument => null
    }
}

test "float_ifloor_nan_throws" {
    {
        float.nan().ifloor();
        baml.sys.panic("expected float.nan().ifloor() to throw")
    } catch (e) {
        baml.errors.InvalidArgument => null
    }
}

test "float_ifloor_inf_throws" {
    {
        float.inf().ifloor();
        baml.sys.panic("expected float.inf().ifloor() to throw")
    } catch (e) {
        baml.errors.InvalidArgument => null
    }
}

test "float_ifloor_neg_inf_throws" {
    {
        (-float.inf()).ifloor();
        baml.sys.panic("expected (-float.inf()).ifloor() to throw")
    } catch (e) {
        baml.errors.InvalidArgument => null
    }
}

test "float_iceil_nan_throws" {
    {
        float.nan().iceil();
        baml.sys.panic("expected float.nan().iceil() to throw")
    } catch (e) {
        baml.errors.InvalidArgument => null
    }
}

test "float_iceil_inf_throws" {
    {
        float.inf().iceil();
        baml.sys.panic("expected float.inf().iceil() to throw")
    } catch (e) {
        baml.errors.InvalidArgument => null
    }
}

// ─── sqrt / pow / log / hypot ─────────────────────────────────────────────────

test "float_sqrt_basic_4" {
    assert.is_true(((4.0).sqrt() - 2.0).abs() < 0.000000000001)
}

// sqrt(2) is irrational; verify the defining property sqrt(2)*sqrt(2) == 2
// rather than a hardcoded decimal.
test "float_sqrt_basic_2" {
    assert.is_true(((2.0).sqrt() * (2.0).sqrt() - 2.0).abs() < 0.000000000001)
}

test "float_sqrt_zero" {
    assert.is_true(((0.0).sqrt() - 0.0).abs() < 0.000000000001)
}

test "float_sqrt_negative_is_nan" {
    assert.is_true((-1.0).sqrt().is_nan())
}

test "float_pow_integer" {
    assert.is_true(((2.0).pow(10.0) - 1024.0).abs() < 0.000000001)
}

test "float_pow_negative_exp" {
    assert.is_true(((2.0).pow(-1.0) - 0.5).abs() < 0.000000000001)
}

// (2.0)^0.5 = sqrt(2) — compare to (2.0).sqrt().
test "float_pow_fractional_exp" {
    assert.is_true(((2.0).pow(0.5) - (2.0).sqrt()).abs() < 0.000000000001)
}

test "float_pow_negative_base_fractional_is_nan" {
    assert.is_true((-1.0).pow(0.5).is_nan())
}

// IEEE 754-2008: 1^anything (including NaN) is 1.
test "float_pow_one_to_nan_is_one" {
    assert.equal((1.0).pow(float.nan()), 1.0)
}

// IEEE 754-2008: x^0 (including NaN base) is 1.
test "float_pow_nan_to_zero_is_one" {
    assert.equal(float.nan().pow(0.0), 1.0)
}

test "float_pow_nan_to_one_is_nan" {
    assert.is_true(float.nan().pow(1.0).is_nan())
}

test "float_log_base_10" {
    assert.is_true(((1000.0).log(10.0) - 3.0).abs() < 0.000000000001)
}

test "float_log_base_2" {
    assert.is_true(((8.0).log(2.0) - 3.0).abs() < 0.000000000001)
}

test "float_log_negative_self_is_nan" {
    assert.is_true((-1.0).log(10.0).is_nan())
}

test "float_hypot_345" {
    assert.is_true(((3.0).hypot(4.0) - 5.0).abs() < 0.000000000001)
}

test "float_hypot_zero" {
    assert.is_true(((0.0).hypot(0.0) - 0.0).abs() < 0.000000000001)
}

// ─── Trig (radians) ───────────────────────────────────────────────────────────

test "float_sin_zero" {
    assert.is_true(((0.0).sin() - 0.0).abs() < 0.000000000001)
}

test "float_sin_pi_is_zero" {
    assert.is_true((float.pi().sin() - 0.0).abs() < 0.0000000001)
}

test "float_sin_half_pi_is_one" {
    assert.is_true(((float.pi() / 2.0).sin() - 1.0).abs() < 0.000000000001)
}

test "float_cos_zero_is_one" {
    assert.is_true(((0.0).cos() - 1.0).abs() < 0.000000000001)
}

test "float_cos_pi_is_neg_one" {
    assert.is_true((float.pi().cos() - (-1.0)).abs() < 0.000000000001)
}

test "float_tan_zero" {
    assert.is_true(((0.0).tan() - 0.0).abs() < 0.000000000001)
}

test "float_tan_pi_quarter_is_one" {
    assert.is_true(((float.pi() / 4.0).tan() - 1.0).abs() < 0.000000000001)
}

// asin(1) = π/2.
test "float_asin_one_is_half_pi" {
    assert.is_true(((1.0).asin() - (float.pi() / 2.0)).abs() < 0.000000000001)
}

test "float_asin_out_of_range_is_nan" {
    assert.is_true((2.0).asin().is_nan())
}

// acos(0) = π/2.
test "float_acos_zero_is_half_pi" {
    assert.is_true(((0.0).acos() - (float.pi() / 2.0)).abs() < 0.000000000001)
}

test "float_acos_out_of_range_is_nan" {
    assert.is_true((-2.0).acos().is_nan())
}

// atan(1) = π/4.
test "float_atan_one_is_quarter_pi" {
    assert.is_true(((1.0).atan() - (float.pi() / 4.0)).abs() < 0.000000000001)
}

test "float_atan2_one_one_is_quarter_pi" {
    assert.is_true(((1.0).atan2(1.0) - (float.pi() / 4.0)).abs() < 0.000000000001)
}

test "float_atan2_one_zero_is_half_pi" {
    assert.is_true(((1.0).atan2(0.0) - (float.pi() / 2.0)).abs() < 0.000000000001)
}

test "float_atan2_zero_neg_one_is_pi" {
    assert.is_true(((0.0).atan2(-1.0) - float.pi()).abs() < 0.000000000001)
}

test "float_sinh_zero" {
    assert.is_true(((0.0).sinh() - 0.0).abs() < 0.000000000001)
}

test "float_cosh_zero_is_one" {
    assert.is_true(((0.0).cosh() - 1.0).abs() < 0.000000000001)
}

test "float_tanh_large_input_approaches_one" {
    assert.is_true(((10.0).tanh() - 1.0).abs() < 0.000001)
}

test "float_asinh_zero" {
    assert.is_true(((0.0).asinh() - 0.0).abs() < 0.000000000001)
}

test "float_acosh_one_is_zero" {
    assert.is_true(((1.0).acosh() - 0.0).abs() < 0.000000000001)
}

test "float_acosh_below_one_is_nan" {
    assert.is_true((0.5).acosh().is_nan())
}

test "float_atanh_zero" {
    assert.is_true(((0.0).atanh() - 0.0).abs() < 0.000000000001)
}

test "float_atanh_out_of_range_is_nan" {
    assert.is_true((2.0).atanh().is_nan())
}

// ─── to_radians / to_degrees ──────────────────────────────────────────────────

// 180° in radians = π.
test "float_to_radians_180_is_pi" {
    assert.is_true(((180.0).to_radians() - float.pi()).abs() < 0.000000000001)
}

test "float_to_radians_zero" {
    assert.is_true(((0.0).to_radians() - 0.0).abs() < 0.000000000001)
}

// π radians = 180°.
test "float_to_degrees_pi_is_180" {
    assert.is_true((float.pi().to_degrees() - 180.0).abs() < 0.000000000001)
}

// Round trip: 90° → radians → degrees → 90°.
test "float_to_radians_round_trip" {
    assert.is_true(((90.0).to_radians().to_degrees() - 90.0).abs() < 0.000000000001)
}

// ─── parse ────────────────────────────────────────────────────────────────────

test "float_parse_decimal" {
    assert.is_true((float.parse("1.5") - 1.5).abs() < 0.000000000001)
}

test "float_parse_negative" {
    assert.is_true((float.parse("-0.25") - (-0.25)).abs() < 0.000000000001)
}

test "float_parse_explicit_plus" {
    assert.is_true((float.parse("+0.5") - 0.5).abs() < 0.000000000001)
}

test "float_parse_integer" {
    assert.is_true((float.parse("42") - 42.0).abs() < 0.000000000001)
}

test "float_parse_zero" {
    assert.is_true((float.parse("0") - 0.0).abs() < 0.000000000001)
}

test "float_parse_scientific_lowercase" {
    assert.is_true((float.parse("1e3") - 1000.0).abs() < 0.000000001)
}

test "float_parse_scientific_uppercase_negative_exp" {
    assert.is_true((float.parse("-1.5E-3") - (-0.0015)).abs() < 0.000000000001)
}

test "float_parse_inf" {
    assert.is_true(float.parse("inf").is_infinite())
}

test "float_parse_inf_is_positive" {
    assert.is_true(float.parse("inf") > 0.0)
}

test "float_parse_infinity_word" {
    assert.is_true(float.parse("infinity").is_infinite())
}

test "float_parse_infinity_word_is_positive" {
    assert.is_true(float.parse("infinity") > 0.0)
}

test "float_parse_negative_inf" {
    assert.is_true(float.parse("-inf").is_infinite())
}

test "float_parse_negative_inf_is_negative" {
    assert.is_true(float.parse("-inf") < 0.0)
}

test "float_parse_nan_lowercase" {
    assert.is_true(float.parse("nan").is_nan())
}

test "float_parse_nan_mixed_case" {
    assert.is_true(float.parse("NaN").is_nan())
}

test "float_parse_empty_throws" {
    {
        float.parse("");
        baml.sys.panic("expected float.parse(\"\") to throw")
    } catch (e) {
        baml.errors.ParseError => null
    }
}

test "float_parse_alpha_throws" {
    {
        float.parse("hello");
        baml.sys.panic("expected float.parse(\"hello\") to throw")
    } catch (e) {
        baml.errors.ParseError => null
    }
}

test "float_parse_trailing_garbage_throws" {
    {
        float.parse("1.5abc");
        baml.sys.panic("expected float.parse(\"1.5abc\") to throw")
    } catch (e) {
        baml.errors.ParseError => null
    }
}

test "float_parse_whitespace_throws" {
    {
        float.parse(" 1.0 ");
        baml.sys.panic("expected float.parse(\" 1.0 \") to throw")
    } catch (e) {
        baml.errors.ParseError => null
    }
}

test "float_parse_just_sign_throws" {
    {
        float.parse("-");
        baml.sys.panic("expected float.parse(\"-\") to throw")
    } catch (e) {
        baml.errors.ParseError => null
    }
}

test "float_parse_round_trip_in_arithmetic" {
    assert.is_true((float.parse("0.5") + float.parse("0.25") - 0.75).abs() < 0.000000000001)
}

// ─── random ───────────────────────────────────────────────────────────────────
//
// `float.random()` returns a uniform float in [0.0, 1.0). We can't assert on
// exact values, so we sample many times and verify structural properties.
// All sampling logic lives in helper functions (test-block locals are boxed).

// float_random_returns_a_float: single sample, verify it's in [0.0, 1.0).
function float_random_returns_a_float() -> bool {
    let r = float.random();
    r >= 0.0 && r < 1.0
}

test "float_random_returns_a_float" {
    assert.is_true(float_random_returns_a_float())
}

// float_random_in_unit_interval_and_covers_both_halves: 1000 samples.
// All must be in [0.0, 1.0), and at least 350 must be < 0.5 AND at least 350
// must be >= 0.5. With 1000 uniform samples, each count is Binomial(1000, 0.5).
// P(count < 350) = P(Z < -10) ≈ 0 — effectively impossible.
function float_random_in_unit_interval_and_covers_both_halves() -> bool {
    let ok = true;
    let below = 0;
    let above = 0;
    let i = 0;
    while (i < 1000) {
        let r = float.random();
        if (r < 0.0 || r >= 1.0) {
            ok = false
        } else {
            null
        }
        if (r < 0.5) {
            below = below + 1
        } else {
            above = above + 1
        }
        i = i + 1
    }
    ok && below >= 350 && above >= 350
}

test "float_random_in_unit_interval_and_covers_both_halves" {
    assert.is_true(float_random_in_unit_interval_and_covers_both_halves())
}

// float_random_distinct_values: 100 samples should span a wide range.
// The range (max - min) of 100 uniform samples from [0, 1) has expected value
// ≈ 0.98; P(range < 0.5) = 2*(0.5)^100 ≈ 10^-30 — effectively impossible.
// We use the range as a proxy for distinctness without needing float equality.
function float_random_distinct_values() -> bool {
    let min_seen = 1.0;
    let max_seen = 0.0;
    let i = 0;
    while (i < 100) {
        let r = float.random();
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
    (max_seen - min_seen) > 0.5
}

test "float_random_distinct_values" {
    assert.is_true(float_random_distinct_values())
}

class FixedFloatRng {
    draw: int,

    implements baml.random.Rng {
        function random(self, bytes: int) -> uint8array {
            let empty = b"";
            empty
        }

        function random_int(self) -> int {
            self.draw
        }
    }
}

test "float_random_uses_rng" {
    assert.equal(float.random(rng = FixedFloatRng { draw: 0 }), 0.0);
    assert.equal(float.random(rng = FixedFloatRng { draw: int.min_value() }), 0.5);
    let high = float.random(rng = FixedFloatRng { draw: -1 });
    assert.is_true(high > 0.99 && high < 1.0)
}

// ─── Equality (== / !=) ───────────────────────────────────────────────────────
//
// Float `==`/`!=` follow IEEE 754 except for NaN, where BAML's reflexive
// `Equals` wins: NaN == NaN is true (and every NaN equals every other), while
// +0.0 == -0.0, +inf == +inf, +inf != -inf are unchanged. int/float mix is
// allowed (int widened to f64); float-vs-bigint is the one rejected pairing
// (compile error E0004 — bigint past 2^53 can't round-trip f64).
//
// Two distinct code paths are exercised: the compiler constant-folds `==` on two
// float literals (try_fold_binary), while a non-literal operand forces the VM
// float-compare opcode. Cases below cover both so a divergence would be caught.

// Constant-folded path (both operands literals — folded by the compiler)

test "float_eq_float_eq_int_literal_fold" {
    // `float` and `int` are distinct concrete types, so `==` is always false
    // (no implicit coercion); the compiler also warns it is provably disjoint.
    assert.is_true((3.0 == 3) == false)
}

test "float_eq_float_neq_int_distinct_fold" {
    assert.is_true(2.5 != 2)
}

test "float_eq_fold_ordinary_equal" {
    assert.is_true(1.5 == 1.5)
}

test "float_eq_fold_ordinary_unequal_false" {
    assert.is_true((1.5 == 2.5) == false)
}

test "float_eq_int_eq_float_literal_fold" {
    assert.is_true((2 == 2.0) == false)
}

test "float_eq_large_integer_valued_fold" {
    assert.is_true((1000000.0 == 1000000) == false)
}

test "float_eq_neg_zero_lit_equals_neg_zero_lit_fold" {
    assert.is_true(-0.0 == -0.0)
}

test "float_eq_positive_zero_equals_negative_zero_fold" {
    assert.is_true(0.0 == -0.0)
}

// Folded or runtime depending on operand shape

test "float_eq_null_eq_float_symmetric_false" {
    assert.is_true((null == 2.0) == false)
}

test "float_eq_two_pow_53_cross_type_false" {
    // Even where the int is exactly representable as a float, the two are
    // distinct concrete types, so `==` is false (and provably disjoint).
    assert.is_true((9007199254740992.0 == 9007199254740992) == false)
}

// Runtime VM path (≥1 non-literal operand forces the float-eq opcode)

test "float_eq_classic_imprecision_sum" {
    assert.is_true(0.1 + 0.2 != 0.3)
}

test "float_eq_div_third_eq_shortest" {
    assert.is_true(1.0 / 3.0 == 0.3333333333333333)
}

test "float_eq_div_third_ne_truncated" {
    assert.is_true(1.0 / 3.0 != 0.333)
}

test "float_eq_float_eq_null_is_false" {
    assert.is_true((float.parse("1.5") == null) == false)
}

test "float_eq_float_neq_null_is_true" {
    assert.is_true(float.parse("1.5") != null)
}

test "float_eq_half_underflow_to_zero" {
    assert.is_true((float.parse("5e-324") / 2.0) == 0.0)
}

// A NaN produced by arithmetic still equals itself — `Equals` is reflexive, and
// the NaN's sign and payload (which differ by CPU architecture) are not
// observable through comparison.
test "float_eq_inf_minus_inf_is_nan_eq_self" {
    assert.is_true((float.inf() - float.inf()) == (float.inf() - float.inf()))
}

test "float_eq_inf_plus_one_equals_inf" {
    assert.is_true(float.inf() == (float.inf() + 1.0))
}

test "float_eq_large_magnitude_precision_loss" {
    assert.is_true(float.parse("1e16") + 1.0 == float.parse("1e16"))
}

test "float_eq_max_finite_ne_inf" {
    assert.is_true(float.parse("1.7976931348623157e308") != float.inf())
}

test "float_eq_mul_imprecision_eq_rounded" {
    assert.is_true(0.1 * 3.0 == 0.30000000000000004)
}

test "float_eq_mul_imprecision_ne_point3" {
    assert.is_true(0.1 * 3.0 != 0.3)
}

test "float_eq_nan_arith_propagates_eq_self" {
    assert.is_true((float.nan() + 1.0) == (float.nan() + 1.0))
}

test "float_eq_nan_const_not_equal_number" {
    assert.is_true((float.nan() == 1.5) == false)
}

test "float_eq_nan_eq_null_is_false" {
    assert.is_true((float.nan() == null) == false)
}

test "float_eq_nan_ne_number_is_true" {
    assert.is_true(float.nan() != 0.0)
}

test "float_eq_nan_ne_self_is_false" {
    assert.is_true(!(float.parse("nan") != float.parse("nan")))
}

test "float_eq_nan_not_equal_infinity" {
    assert.is_true((float.nan() == float.inf()) == false)
}

test "float_eq_nan_equal_to_self" {
    assert.is_true(float.parse("nan") == float.parse("nan"))
}

test "float_eq_neg_inf_equals_neg_inf" {
    assert.is_true(-float.inf() == -float.inf())
}

test "float_eq_neg_zero_from_division" {
    assert.is_true((-1.0 / float.inf()) == 0.0)
}

test "float_eq_neg_zero_ne_pos_zero_runtime_false" {
    assert.is_true((float.parse("-0.0") != 0.0) == false)
}

test "float_eq_overflow_to_inf" {
    assert.is_true(float.parse("1.0e308") * 10.0 == float.inf())
}

test "float_eq_parse_inf_equals_inf_const" {
    assert.is_true(float.parse("inf") == float.inf())
}

test "float_eq_parse_inf_ne_neg_inf" {
    assert.is_true(float.parse("inf") != float.parse("-inf"))
}

test "float_eq_parse_neg_inf_equals_neg_inf_const" {
    assert.is_true(float.parse("-inf") == -float.inf())
}

test "float_eq_parse_roundtrip" {
    assert.is_true(float.parse("1.5") == 1.5)
}

test "float_eq_parse_roundtrip_imprecise_all_runtime" {
    assert.is_true(float.parse("0.1") + float.parse("0.2") != float.parse("0.3"))
}

test "float_eq_pi_eq_pi" {
    assert.is_true(float.pi() == float.pi())
}

test "float_eq_pi_ne_truncated" {
    assert.is_true(float.pi() != 3.14)
}

test "float_eq_pos_inf_equals_pos_inf" {
    assert.is_true(float.inf() == float.inf())
}

test "float_eq_pos_inf_ne_neg_inf" {
    assert.is_true(float.inf() != -float.inf())
}

test "float_eq_runtime_float_eq_int" {
    assert.is_true((float.parse("2") == 2) == false)
}

test "float_eq_runtime_float_neq_int_distinct" {
    assert.is_true(float.parse("2.5") != 2)
}

test "float_eq_runtime_int_eq_float" {
    assert.is_true((5 == float.parse("5.0")) == false)
}

test "float_eq_runtime_int_neq_float_distinct" {
    assert.is_true(3 != float.parse("3.5"))
}

test "float_eq_runtime_ordinary_unequal_false" {
    assert.is_true((float.parse("1.5") == 2.5) == false)
}

test "float_eq_runtime_signed_zero" {
    assert.is_true(float.parse("-0.0") == 0.0)
}

test "float_eq_self_equal_nonliteral" {
    assert.is_true(float.parse("0.1") == float.parse("0.1"))
}

test "float_eq_sqrt_irrational_ne_two" {
    assert.is_true((2.0).sqrt() * (2.0).sqrt() != 2.0)
}

test "float_eq_sqrt_irrational_property" {
    assert.is_true(((2.0).sqrt() * (2.0).sqrt()) == 2.0000000000000004)
}

test "float_eq_subnormal_nonzero" {
    assert.is_true(float.parse("5e-324") != 0.0)
}

test "float_eq_sum_equals_exact_rounded" {
    assert.is_true(0.1 + 0.2 == 0.30000000000000004)
}

// ─── NaN ordering (BAML: a total order, with NaN above every number) ──────────
// `float` implements `baml.ops.Compare`, which is a *total* order, so NaN cannot
// be left unordered the way IEEE leaves it. NaN sits above every number and is
// equal to itself, which is what makes `nan <= nan` / `nan >= nan` true.
test "nan_comparison" {
    assert.is_true(!(float.nan() < 1.0))
    assert.is_true(1.0 < float.nan())
    assert.is_true(float.nan() > 1.0)
    assert.is_true(!(float.nan() < float.nan()))
    assert.is_true(float.nan() <= float.nan())
    assert.is_true(float.nan() >= float.nan())
    assert.is_true(float.nan() > float.inf())
    assert.is_true(float.nan() == float.nan())
}

// `Equals` is reflexive, so — unlike IEEE — no float is unequal to itself, and
// `x != x` is never a NaN test. Use `is_nan()`.
test "float_eq_is_reflexive_for_nan" {
    assert.is_true(!(float.nan() != float.nan()))
    assert.is_true(float.nan().eq(float.nan()))
}

// Signed zeros stay one value, as under IEEE.
test "float_signed_zeros_are_equal" {
    assert.is_true(-0.0 == 0.0)
    assert.is_true(!(-0.0 < 0.0))
    assert.is_true(-0.0 <= 0.0)
}

// The same via the `Compare` *method* surface (`lt`/`le`/`gt`/`ge`), which the
// interface derives from `cmp`.
test "float_nan_lt_method_is_false" {
    assert.is_true(!float.nan().lt(1.0))
}

test "float_nan_ge_method_is_true" {
    assert.is_true(float.nan().ge(float.nan()))
}

// ─── regression: bare-literal method receiver (B-614) ─────────────────────────
//
// `2.5.floor()` on a bare float literal used to lower its receiver to a Missing
// expr and fail to type-check. Appended at the end so the test registry stays
// append-only.
test "float_floor_bare_literal" {
    assert.equal(2.5.floor(), 2.0)
}

// ─── underscore separators ────────────────────────────────────────────────────
// `_` digit separators are allowed in the mantissa, fraction, and exponent
// (the exponent must start with a digit). Underscores are stripped during
// AST lowering, so downstream f64 parsing never sees them.

test "float_underscore_mantissa" {
    assert.equal(1_000.5, 1000.5)
}

test "float_underscore_fraction" {
    assert.equal(1.000_1, 1.0001)
}

test "float_underscore_exponent" {
    assert.equal(1e1_0, 1e10)
}

test "float_underscore_everywhere" {
    assert.equal(1_234.567_8e1_0, 1234.5678e10)
}

// ─── abs: sign of zero ────────────────────────────────────────────────────────
// BAML has no is_sign_positive(), but float.to_string() preserves the sign of
// zero ("-0.0" vs "0.0"), so the sign check is observable through formatting.
// The first assert is a baseline guard: it proves `-0.0` really reaches the VM
// as negative zero, so the abs assertion cannot pass vacuously.
test "float_abs_negative_zero" {
    assert.equal((-0.0).to_string(), "-0.0");
    assert.equal((-0.0).abs(), 0.0);
    assert.equal((-0.0).abs().to_string(), "0.0")
}
