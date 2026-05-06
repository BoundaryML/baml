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
