//! Tests for the `float` builtin class (BEP-043).

use baml_tests::baml_test;
use bex_engine::BexExternalValue;

async fn run_float(src: &str) -> f64 {
    let output = baml_test!(baml: src, entry: "main");
    match output.result {
        Ok(BexExternalValue::Float(f)) => f,
        other => panic!("expected float, got {other:?}"),
    }
}

async fn run_bool(src: &str) -> bool {
    let output = baml_test!(baml: src, entry: "main");
    match output.result {
        Ok(BexExternalValue::Bool(b)) => b,
        other => panic!("expected bool, got {other:?}"),
    }
}

async fn run_int(src: &str) -> i64 {
    let output = baml_test!(baml: src, entry: "main");
    match output.result {
        Ok(BexExternalValue::Int(n)) => n,
        other => panic!("expected int, got {other:?}"),
    }
}

fn approx(a: f64, b: f64, eps: f64) {
    assert!(
        (a - b).abs() < eps,
        "expected {a} ≈ {b} (within {eps}), diff = {}",
        (a - b).abs()
    );
}

async fn expect_throw(src: &str) {
    let output = baml_test!(baml: src, entry: "main");
    let Err(bex_engine::EngineError::UnhandledThrow { .. }) = &output.result else {
        panic!("expected UnhandledThrow, got: {:?}", output.result);
    };
}

// ─── is_nan / is_infinite / is_finite ─────────────────────────────────────────

#[tokio::test]
async fn float_is_nan_true() {
    assert!(run_bool("function main() -> bool { float.nan().is_nan() }").await);
}

#[tokio::test]
async fn float_is_nan_false_for_zero() {
    assert!(!run_bool("function main() -> bool { (0.0).is_nan() }").await);
}

#[tokio::test]
async fn float_is_nan_false_for_finite() {
    assert!(!run_bool("function main() -> bool { (3.14).is_nan() }").await);
}

#[tokio::test]
async fn float_is_nan_false_for_infinity() {
    assert!(!run_bool("function main() -> bool { float.inf().is_nan() }").await);
}

#[tokio::test]
async fn float_is_infinite_true_positive() {
    assert!(run_bool("function main() -> bool { float.inf().is_infinite() }").await);
}

#[tokio::test]
async fn float_is_infinite_true_negative() {
    assert!(run_bool("function main() -> bool { (-float.inf()).is_infinite() }").await);
}

#[tokio::test]
async fn float_is_infinite_false_for_large_finite() {
    // BAML does not (yet) parse scientific notation; use a value built from
    // arithmetic. 1_000_000.0 ** 2 = 1e12, comfortably finite.
    assert!(!run_bool("function main() -> bool { (1000000.0 * 1000000.0).is_infinite() }").await);
}

#[tokio::test]
async fn float_is_infinite_false_for_nan() {
    assert!(!run_bool("function main() -> bool { float.nan().is_infinite() }").await);
}

#[tokio::test]
async fn float_is_finite_true_for_zero() {
    assert!(run_bool("function main() -> bool { (0.0).is_finite() }").await);
}

#[tokio::test]
async fn float_is_finite_true_for_normal() {
    assert!(run_bool("function main() -> bool { (3.14).is_finite() }").await);
}

#[tokio::test]
async fn float_is_finite_true_for_negative_zero() {
    assert!(run_bool("function main() -> bool { (-0.0).is_finite() }").await);
}

#[tokio::test]
async fn float_is_finite_false_for_nan() {
    assert!(!run_bool("function main() -> bool { float.nan().is_finite() }").await);
}

#[tokio::test]
async fn float_is_finite_false_for_infinity() {
    assert!(!run_bool("function main() -> bool { float.inf().is_finite() }").await);
}

#[tokio::test]
async fn float_is_finite_false_for_negative_infinity() {
    assert!(!run_bool("function main() -> bool { (-float.inf()).is_finite() }").await);
}

// ─── Constants ────────────────────────────────────────────────────────────────

#[tokio::test]
async fn float_pi_value() {
    let pi = run_float("function main() -> float { float.pi() }").await;
    assert!((pi - std::f64::consts::PI).abs() < 1e-12);
}

#[tokio::test]
async fn float_e_value() {
    let e = run_float("function main() -> float { float.e() }").await;
    assert!((e - std::f64::consts::E).abs() < 1e-12);
}

#[tokio::test]
async fn float_golden_ratio_value() {
    let phi = run_float("function main() -> float { float.golden_ratio() }").await;
    let expected = (1.0 + 5.0_f64.sqrt()) / 2.0;
    assert!((phi - expected).abs() < 1e-12);
}

#[tokio::test]
async fn float_nan_is_a_nan() {
    let n = run_float("function main() -> float { float.nan() }").await;
    assert!(n.is_nan());
}

#[tokio::test]
async fn float_inf_is_positive_infinity() {
    let i = run_float("function main() -> float { float.inf() }").await;
    assert!(i.is_infinite() && i > 0.0);
}

#[tokio::test]
async fn float_negative_inf_via_unary_minus() {
    let i = run_float("function main() -> float { -float.inf() }").await;
    assert!(i.is_infinite() && i < 0.0);
}

// ─── abs / min / max / clamp ──────────────────────────────────────────────────

#[tokio::test]
async fn float_abs_negative() {
    approx(
        run_float("function main() -> float { (-3.5).abs() }").await,
        3.5,
        1e-12,
    );
}

#[tokio::test]
async fn float_abs_positive() {
    approx(
        run_float("function main() -> float { (3.5).abs() }").await,
        3.5,
        1e-12,
    );
}

#[tokio::test]
async fn float_abs_negative_zero() {
    let n = run_float("function main() -> float { (-0.0).abs() }").await;
    assert_eq!(n, 0.0);
    assert!(n.is_sign_positive());
}

#[tokio::test]
async fn float_abs_nan_is_nan() {
    let n = run_float("function main() -> float { float.nan().abs() }").await;
    assert!(n.is_nan());
}

#[tokio::test]
async fn float_abs_inf_is_inf() {
    let n = run_float("function main() -> float { (-float.inf()).abs() }").await;
    assert!(n.is_infinite() && n > 0.0);
}

#[tokio::test]
async fn float_min_basic() {
    approx(
        run_float("function main() -> float { (3.0).min(5.0) }").await,
        3.0,
        1e-12,
    );
}

#[tokio::test]
async fn float_min_with_nan_suppresses_nan() {
    let n = run_float("function main() -> float { (3.0).min(float.nan()) }").await;
    assert_eq!(n, 3.0);
}

#[tokio::test]
async fn float_min_nan_receiver_suppresses_nan() {
    // Symmetric: NaN as `self`, finite as `other`.
    let n = run_float("function main() -> float { float.nan().min(3.0) }").await;
    assert_eq!(n, 3.0);
}

#[tokio::test]
async fn float_min_both_nan_is_nan() {
    let n = run_float("function main() -> float { float.nan().min(float.nan()) }").await;
    assert!(n.is_nan());
}

#[tokio::test]
async fn float_max_basic() {
    approx(
        run_float("function main() -> float { (3.0).max(5.0) }").await,
        5.0,
        1e-12,
    );
}

#[tokio::test]
async fn float_max_with_nan_suppresses_nan() {
    let n = run_float("function main() -> float { (3.0).max(float.nan()) }").await;
    assert_eq!(n, 3.0);
}

#[tokio::test]
async fn float_max_nan_receiver_suppresses_nan() {
    // Symmetric: NaN as `self`, finite as `other`.
    let n = run_float("function main() -> float { float.nan().max(3.0) }").await;
    assert_eq!(n, 3.0);
}

#[tokio::test]
async fn float_clamp_in_range() {
    approx(
        run_float("function main() -> float { (5.0).clamp(0.0, 10.0) }").await,
        5.0,
        1e-12,
    );
}

#[tokio::test]
async fn float_clamp_below() {
    approx(
        run_float("function main() -> float { (-3.0).clamp(0.0, 10.0) }").await,
        0.0,
        1e-12,
    );
}

#[tokio::test]
async fn float_clamp_above() {
    approx(
        run_float("function main() -> float { (15.0).clamp(0.0, 10.0) }").await,
        10.0,
        1e-12,
    );
}

// ─── floor / ceil / round / trunc / fract ─────────────────────────────────────

#[tokio::test]
async fn float_floor_positive_fraction() {
    approx(
        run_float("function main() -> float { (3.7).floor() }").await,
        3.0,
        1e-12,
    );
}

#[tokio::test]
async fn float_floor_negative_fraction() {
    approx(
        run_float("function main() -> float { (-1.5).floor() }").await,
        -2.0,
        1e-12,
    );
}

#[tokio::test]
async fn float_floor_integer_unchanged() {
    approx(
        run_float("function main() -> float { (3.0).floor() }").await,
        3.0,
        1e-12,
    );
}

#[tokio::test]
async fn float_ceil_positive_fraction() {
    approx(
        run_float("function main() -> float { (3.2).ceil() }").await,
        4.0,
        1e-12,
    );
}

#[tokio::test]
async fn float_ceil_negative_fraction() {
    approx(
        run_float("function main() -> float { (-1.5).ceil() }").await,
        -1.0,
        1e-12,
    );
}

#[tokio::test]
async fn float_round_half_up() {
    approx(
        run_float("function main() -> float { (1.5).round() }").await,
        2.0,
        1e-12,
    );
}

#[tokio::test]
async fn float_round_negative_half_away_from_zero() {
    approx(
        run_float("function main() -> float { (-1.5).round() }").await,
        -2.0,
        1e-12,
    );
}

#[tokio::test]
async fn float_round_below_half() {
    approx(
        run_float("function main() -> float { (1.4).round() }").await,
        1.0,
        1e-12,
    );
}

#[tokio::test]
async fn float_round_above_half() {
    approx(
        run_float("function main() -> float { (1.6).round() }").await,
        2.0,
        1e-12,
    );
}

#[tokio::test]
async fn float_round_two_point_five_is_three() {
    // Pins half-away-from-zero (→ 3.0) — banker's rounding would give 2.0.
    approx(
        run_float("function main() -> float { (2.5).round() }").await,
        3.0,
        1e-12,
    );
}

#[tokio::test]
async fn float_round_negative_two_point_five_is_neg_three() {
    approx(
        run_float("function main() -> float { (-2.5).round() }").await,
        -3.0,
        1e-12,
    );
}

#[tokio::test]
async fn float_trunc_positive() {
    approx(
        run_float("function main() -> float { (3.7).trunc() }").await,
        3.0,
        1e-12,
    );
}

#[tokio::test]
async fn float_trunc_negative() {
    approx(
        run_float("function main() -> float { (-3.7).trunc() }").await,
        -3.0,
        1e-12,
    );
}

#[tokio::test]
async fn float_fract_positive() {
    approx(
        run_float("function main() -> float { (3.75).fract() }").await,
        0.75,
        1e-12,
    );
}

#[tokio::test]
async fn float_fract_negative() {
    approx(
        run_float("function main() -> float { (-3.75).fract() }").await,
        -0.75,
        1e-12,
    );
}

#[tokio::test]
async fn float_fract_integer_is_zero() {
    approx(
        run_float("function main() -> float { (5.0).fract() }").await,
        0.0,
        1e-12,
    );
}

#[tokio::test]
async fn float_floor_nan_propagates() {
    let n = run_float("function main() -> float { float.nan().floor() }").await;
    assert!(n.is_nan());
}

// ─── ifloor / iceil / iround / itrunc ─────────────────────────────────────────

#[tokio::test]
async fn float_ifloor_basic() {
    assert_eq!(
        run_int("function main() -> int { (3.7).ifloor() }").await,
        3
    );
    assert_eq!(
        run_int("function main() -> int { (-1.5).ifloor() }").await,
        -2
    );
}

#[tokio::test]
async fn float_iceil_basic() {
    assert_eq!(run_int("function main() -> int { (3.2).iceil() }").await, 4);
    assert_eq!(
        run_int("function main() -> int { (-1.5).iceil() }").await,
        -1
    );
}

#[tokio::test]
async fn float_iround_basic() {
    assert_eq!(
        run_int("function main() -> int { (1.5).iround() }").await,
        2
    );
    assert_eq!(
        run_int("function main() -> int { (-1.5).iround() }").await,
        -2
    );
    assert_eq!(
        run_int("function main() -> int { (1.4).iround() }").await,
        1
    );
}

#[tokio::test]
async fn float_itrunc_basic() {
    assert_eq!(
        run_int("function main() -> int { (3.7).itrunc() }").await,
        3
    );
    assert_eq!(
        run_int("function main() -> int { (-3.7).itrunc() }").await,
        -3
    );
}

#[tokio::test]
async fn float_iround_at_i64_min_is_ok() {
    // -2^63 is exactly representable as f64.
    assert_eq!(
        run_int("function main() -> int { (-9223372036854775808.0).iround() }").await,
        i64::MIN
    );
}

#[tokio::test]
async fn float_iround_too_large_throws() {
    // 2^63 (one past i64::MAX) is exactly representable as f64; must throw.
    expect_throw("function main() -> int { (9223372036854775808.0).iround() }").await;
}

#[tokio::test]
async fn float_iround_too_small_throws() {
    // 2 * i64::MIN is well outside the range.
    expect_throw("function main() -> int { (-9223372036854775808.0 * 2.0).iround() }").await;
}

#[tokio::test]
async fn float_itrunc_nan_throws() {
    expect_throw("function main() -> int { float.nan().itrunc() }").await;
}

#[tokio::test]
async fn float_itrunc_inf_throws() {
    expect_throw("function main() -> int { float.inf().itrunc() }").await;
}

#[tokio::test]
async fn float_iround_nan_throws() {
    expect_throw("function main() -> int { float.nan().iround() }").await;
}

#[tokio::test]
async fn float_iround_inf_throws() {
    expect_throw("function main() -> int { float.inf().iround() }").await;
}

#[tokio::test]
async fn float_ifloor_nan_throws() {
    expect_throw("function main() -> int { float.nan().ifloor() }").await;
}

#[tokio::test]
async fn float_ifloor_inf_throws() {
    expect_throw("function main() -> int { float.inf().ifloor() }").await;
}

#[tokio::test]
async fn float_ifloor_neg_inf_throws() {
    expect_throw("function main() -> int { (-float.inf()).ifloor() }").await;
}

#[tokio::test]
async fn float_iceil_nan_throws() {
    expect_throw("function main() -> int { float.nan().iceil() }").await;
}

#[tokio::test]
async fn float_iceil_inf_throws() {
    expect_throw("function main() -> int { float.inf().iceil() }").await;
}

// ─── sqrt / pow / log / hypot ─────────────────────────────────────────────────

#[tokio::test]
async fn float_sqrt_basic() {
    approx(
        run_float("function main() -> float { (4.0).sqrt() }").await,
        2.0,
        1e-12,
    );
    approx(
        run_float("function main() -> float { (2.0).sqrt() }").await,
        std::f64::consts::SQRT_2,
        1e-12,
    );
}

#[tokio::test]
async fn float_sqrt_zero() {
    approx(
        run_float("function main() -> float { (0.0).sqrt() }").await,
        0.0,
        1e-12,
    );
}

#[tokio::test]
async fn float_sqrt_negative_is_nan() {
    let n = run_float("function main() -> float { (-1.0).sqrt() }").await;
    assert!(n.is_nan());
}

#[tokio::test]
async fn float_pow_integer() {
    approx(
        run_float("function main() -> float { (2.0).pow(10.0) }").await,
        1024.0,
        1e-9,
    );
}

#[tokio::test]
async fn float_pow_negative_exp() {
    approx(
        run_float("function main() -> float { (2.0).pow(-1.0) }").await,
        0.5,
        1e-12,
    );
}

#[tokio::test]
async fn float_pow_fractional_exp() {
    approx(
        run_float("function main() -> float { (2.0).pow(0.5) }").await,
        std::f64::consts::SQRT_2,
        1e-12,
    );
}

#[tokio::test]
async fn float_pow_negative_base_fractional_is_nan() {
    let n = run_float("function main() -> float { (-1.0).pow(0.5) }").await;
    assert!(n.is_nan());
}

#[tokio::test]
async fn float_pow_one_to_nan_is_one() {
    // IEEE 754-2008 special case: 1^anything (including NaN) is 1.
    let n = run_float("function main() -> float { (1.0).pow(float.nan()) }").await;
    assert_eq!(n, 1.0);
}

#[tokio::test]
async fn float_pow_nan_to_zero_is_one() {
    // IEEE 754-2008 special case: x^0 (including NaN) is 1.
    let n = run_float("function main() -> float { float.nan().pow(0.0) }").await;
    assert_eq!(n, 1.0);
}

#[tokio::test]
async fn float_pow_nan_to_one_is_nan() {
    let n = run_float("function main() -> float { float.nan().pow(1.0) }").await;
    assert!(n.is_nan());
}

#[tokio::test]
async fn float_log_base_10() {
    approx(
        run_float("function main() -> float { (1000.0).log(10.0) }").await,
        3.0,
        1e-12,
    );
}

#[tokio::test]
async fn float_log_base_2() {
    approx(
        run_float("function main() -> float { (8.0).log(2.0) }").await,
        3.0,
        1e-12,
    );
}

#[tokio::test]
async fn float_log_negative_self_is_nan() {
    let n = run_float("function main() -> float { (-1.0).log(10.0) }").await;
    assert!(n.is_nan());
}

#[tokio::test]
async fn float_hypot_345() {
    approx(
        run_float("function main() -> float { (3.0).hypot(4.0) }").await,
        5.0,
        1e-12,
    );
}

#[tokio::test]
async fn float_hypot_zero() {
    approx(
        run_float("function main() -> float { (0.0).hypot(0.0) }").await,
        0.0,
        1e-12,
    );
}

// ─── Trig (radians) ───────────────────────────────────────────────────────────

#[tokio::test]
async fn float_sin_zero() {
    approx(
        run_float("function main() -> float { (0.0).sin() }").await,
        0.0,
        1e-12,
    );
}

#[tokio::test]
async fn float_sin_pi_is_zero() {
    approx(
        run_float("function main() -> float { float.pi().sin() }").await,
        0.0,
        1e-10,
    );
}

#[tokio::test]
async fn float_sin_half_pi_is_one() {
    approx(
        run_float("function main() -> float { (float.pi() / 2.0).sin() }").await,
        1.0,
        1e-12,
    );
}

#[tokio::test]
async fn float_cos_zero_is_one() {
    approx(
        run_float("function main() -> float { (0.0).cos() }").await,
        1.0,
        1e-12,
    );
}

#[tokio::test]
async fn float_cos_pi_is_neg_one() {
    approx(
        run_float("function main() -> float { float.pi().cos() }").await,
        -1.0,
        1e-12,
    );
}

#[tokio::test]
async fn float_tan_zero() {
    approx(
        run_float("function main() -> float { (0.0).tan() }").await,
        0.0,
        1e-12,
    );
}

#[tokio::test]
async fn float_tan_pi_quarter_is_one() {
    approx(
        run_float("function main() -> float { (float.pi() / 4.0).tan() }").await,
        1.0,
        1e-12,
    );
}

#[tokio::test]
async fn float_asin_one_is_half_pi() {
    approx(
        run_float("function main() -> float { (1.0).asin() }").await,
        std::f64::consts::FRAC_PI_2,
        1e-12,
    );
}

#[tokio::test]
async fn float_asin_out_of_range_is_nan() {
    let n = run_float("function main() -> float { (2.0).asin() }").await;
    assert!(n.is_nan());
}

#[tokio::test]
async fn float_acos_zero_is_half_pi() {
    approx(
        run_float("function main() -> float { (0.0).acos() }").await,
        std::f64::consts::FRAC_PI_2,
        1e-12,
    );
}

#[tokio::test]
async fn float_acos_out_of_range_is_nan() {
    let n = run_float("function main() -> float { (-2.0).acos() }").await;
    assert!(n.is_nan());
}

#[tokio::test]
async fn float_atan_one_is_quarter_pi() {
    approx(
        run_float("function main() -> float { (1.0).atan() }").await,
        std::f64::consts::FRAC_PI_4,
        1e-12,
    );
}

#[tokio::test]
async fn float_atan2_basic() {
    approx(
        run_float("function main() -> float { (1.0).atan2(1.0) }").await,
        std::f64::consts::FRAC_PI_4,
        1e-12,
    );
    approx(
        run_float("function main() -> float { (1.0).atan2(0.0) }").await,
        std::f64::consts::FRAC_PI_2,
        1e-12,
    );
    approx(
        run_float("function main() -> float { (0.0).atan2(-1.0) }").await,
        std::f64::consts::PI,
        1e-12,
    );
}

#[tokio::test]
async fn float_sinh_zero() {
    approx(
        run_float("function main() -> float { (0.0).sinh() }").await,
        0.0,
        1e-12,
    );
}

#[tokio::test]
async fn float_cosh_zero_is_one() {
    approx(
        run_float("function main() -> float { (0.0).cosh() }").await,
        1.0,
        1e-12,
    );
}

#[tokio::test]
async fn float_tanh_large_input_approaches_one() {
    approx(
        run_float("function main() -> float { (10.0).tanh() }").await,
        1.0,
        1e-6,
    );
}

#[tokio::test]
async fn float_asinh_zero() {
    approx(
        run_float("function main() -> float { (0.0).asinh() }").await,
        0.0,
        1e-12,
    );
}

#[tokio::test]
async fn float_acosh_one_is_zero() {
    approx(
        run_float("function main() -> float { (1.0).acosh() }").await,
        0.0,
        1e-12,
    );
}

#[tokio::test]
async fn float_acosh_below_one_is_nan() {
    let n = run_float("function main() -> float { (0.5).acosh() }").await;
    assert!(n.is_nan());
}

#[tokio::test]
async fn float_atanh_zero() {
    approx(
        run_float("function main() -> float { (0.0).atanh() }").await,
        0.0,
        1e-12,
    );
}

#[tokio::test]
async fn float_atanh_out_of_range_is_nan() {
    let n = run_float("function main() -> float { (2.0).atanh() }").await;
    assert!(n.is_nan());
}

// ─── to_radians / to_degrees ──────────────────────────────────────────────────

#[tokio::test]
async fn float_to_radians_180_is_pi() {
    approx(
        run_float("function main() -> float { (180.0).to_radians() }").await,
        std::f64::consts::PI,
        1e-12,
    );
}

#[tokio::test]
async fn float_to_radians_zero() {
    approx(
        run_float("function main() -> float { (0.0).to_radians() }").await,
        0.0,
        1e-12,
    );
}

#[tokio::test]
async fn float_to_degrees_pi_is_180() {
    approx(
        run_float("function main() -> float { float.pi().to_degrees() }").await,
        180.0,
        1e-12,
    );
}

#[tokio::test]
async fn float_to_radians_round_trip() {
    // (90 deg → π/2 rad → 90 deg) should be self-inverse.
    approx(
        run_float("function main() -> float { (90.0).to_radians().to_degrees() }").await,
        90.0,
        1e-12,
    );
}

// ─── parse ────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn float_parse_decimal() {
    approx(
        run_float(r#"function main() -> float { float.parse("1.5") }"#).await,
        1.5,
        1e-12,
    );
}

#[tokio::test]
async fn float_parse_negative() {
    approx(
        run_float(r#"function main() -> float { float.parse("-0.25") }"#).await,
        -0.25,
        1e-12,
    );
}

#[tokio::test]
async fn float_parse_explicit_plus() {
    approx(
        run_float(r#"function main() -> float { float.parse("+0.5") }"#).await,
        0.5,
        1e-12,
    );
}

#[tokio::test]
async fn float_parse_integer() {
    approx(
        run_float(r#"function main() -> float { float.parse("42") }"#).await,
        42.0,
        1e-12,
    );
}

#[tokio::test]
async fn float_parse_zero() {
    approx(
        run_float(r#"function main() -> float { float.parse("0") }"#).await,
        0.0,
        1e-12,
    );
}

#[tokio::test]
async fn float_parse_scientific_lowercase() {
    approx(
        run_float(r#"function main() -> float { float.parse("1e3") }"#).await,
        1000.0,
        1e-9,
    );
}

#[tokio::test]
async fn float_parse_scientific_uppercase_negative_exp() {
    approx(
        run_float(r#"function main() -> float { float.parse("-1.5E-3") }"#).await,
        -0.0015,
        1e-12,
    );
}

#[tokio::test]
async fn float_parse_inf() {
    let n = run_float(r#"function main() -> float { float.parse("inf") }"#).await;
    assert!(n.is_infinite() && n > 0.0);
}

#[tokio::test]
async fn float_parse_infinity_word() {
    let n = run_float(r#"function main() -> float { float.parse("infinity") }"#).await;
    assert!(n.is_infinite() && n > 0.0);
}

#[tokio::test]
async fn float_parse_negative_inf() {
    let n = run_float(r#"function main() -> float { float.parse("-inf") }"#).await;
    assert!(n.is_infinite() && n < 0.0);
}

#[tokio::test]
async fn float_parse_nan_lowercase() {
    let n = run_float(r#"function main() -> float { float.parse("nan") }"#).await;
    assert!(n.is_nan());
}

#[tokio::test]
async fn float_parse_nan_mixed_case() {
    let n = run_float(r#"function main() -> float { float.parse("NaN") }"#).await;
    assert!(n.is_nan());
}

#[tokio::test]
async fn float_parse_empty_throws() {
    expect_throw(r#"function main() -> float { float.parse("") }"#).await;
}

#[tokio::test]
async fn float_parse_alpha_throws() {
    expect_throw(r#"function main() -> float { float.parse("hello") }"#).await;
}

#[tokio::test]
async fn float_parse_trailing_garbage_throws() {
    expect_throw(r#"function main() -> float { float.parse("1.5abc") }"#).await;
}

#[tokio::test]
async fn float_parse_whitespace_throws() {
    expect_throw(r#"function main() -> float { float.parse(" 1.0 ") }"#).await;
}

#[tokio::test]
async fn float_parse_just_sign_throws() {
    expect_throw(r#"function main() -> float { float.parse("-") }"#).await;
}

#[tokio::test]
async fn float_parse_round_trip_in_arithmetic() {
    approx(
        run_float(r#"function main() -> float { float.parse("0.5") + float.parse("0.25") }"#).await,
        0.75,
        1e-12,
    );
}

// ─── random ───────────────────────────────────────────────────────────────────
//
// We can't assert exact values, so we sample many times and verify:
//   - every sample is in [0.0, 1.0)
//   - over enough samples we cover the unit interval well (mean ≈ 0.5, both
//     halves are reached)

async fn run_float_array(src: &str) -> Vec<f64> {
    let output = baml_test!(baml: src, entry: "main");
    match output.result {
        Ok(BexExternalValue::Array { items, .. }) => items
            .into_iter()
            .map(|item| match item {
                BexExternalValue::Float(f) => f,
                other => panic!("expected float element, got {other:?}"),
            })
            .collect(),
        other => panic!("expected array, got {other:?}"),
    }
}

#[tokio::test]
async fn float_random_in_unit_interval_and_covers_both_halves() {
    let samples = run_float_array(
        r#"
        function main() -> float[] {
            let acc: float[] = [];
            let i = 0;
            while (i < 1000) {
                let _ = acc.push(float.random());
                i = i + 1;
            }
            acc
        }
        "#,
    )
    .await;
    assert_eq!(samples.len(), 1000);
    let mut min_seen = f64::INFINITY;
    let mut max_seen = f64::NEG_INFINITY;
    let mut below_half = 0;
    let mut above_half = 0;
    for &x in &samples {
        assert!(
            (0.0..1.0).contains(&x),
            "float.random() returned {x}, out of [0.0, 1.0)"
        );
        min_seen = min_seen.min(x);
        max_seen = max_seen.max(x);
        if x < 0.5 {
            below_half += 1;
        } else {
            above_half += 1;
        }
    }
    // Both halves should each get about 500 samples; tolerate ±150.
    assert!(
        below_half >= 350,
        "expected ≥350 samples below 0.5, got {below_half}"
    );
    assert!(
        above_half >= 350,
        "expected ≥350 samples ≥ 0.5, got {above_half}"
    );
    assert!(max_seen < 1.0, "upper bound is exclusive — saw {max_seen}");
    assert!(min_seen >= 0.0, "lower bound is inclusive — saw {min_seen}");
}

#[tokio::test]
async fn float_random_distinct_values() {
    // Across 100 samples drawn from f64's continuous range, observing fewer
    // than 90 distinct values would indicate the RNG is broken.
    let samples = run_float_array(
        r#"
        function main() -> float[] {
            let acc: float[] = [];
            let i = 0;
            while (i < 100) {
                let _ = acc.push(float.random());
                i = i + 1;
            }
            acc
        }
        "#,
    )
    .await;
    let distinct: std::collections::HashSet<u64> = samples.iter().map(|f| f.to_bits()).collect();
    assert!(
        distinct.len() >= 90,
        "expected ≥90 distinct values out of 100 samples, got {}",
        distinct.len()
    );
}

#[tokio::test]
async fn float_random_returns_a_float() {
    let n = run_float("function main() -> float { float.random() }").await;
    assert!((0.0..1.0).contains(&n));
}
