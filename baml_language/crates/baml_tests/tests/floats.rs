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

// ─── abs ──────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn float_abs_negative_zero() {
    let n = run_float("function main() -> float { (-0.0).abs() }").await;
    assert_eq!(n, 0.0);
    assert!(n.is_sign_positive());
}
