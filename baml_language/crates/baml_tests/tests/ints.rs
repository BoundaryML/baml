//! Tests for the `int` builtin class (BEP-043).

use baml_tests::baml_test;
use bex_engine::BexExternalValue;

// ─── abs ──────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn int_abs_positive() {
    let output = baml_test!(
        r#"
        function main() -> int { (3).abs() }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(3)));
}

#[tokio::test]
async fn int_abs_negative() {
    let output = baml_test!(
        r#"
        function main() -> int { (-7).abs() }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(7)));
}

#[tokio::test]
async fn int_abs_zero() {
    let output = baml_test!(
        r#"
        function main() -> int { (0).abs() }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(0)));
}

#[tokio::test]
async fn int_abs_min_value_throws() {
    // i64::MIN is -9223372036854775808; its absolute value (2^63) does not fit.
    let output = baml_test!(
        r#"
        function main() -> int { int.min_value().abs() }
    "#
    );
    let Err(bex_engine::EngineError::UnhandledThrow { .. }) = &output.result else {
        panic!("expected UnhandledThrow, got: {:?}", output.result);
    };
}

#[tokio::test]
async fn int_abs_near_min_value_succeeds() {
    // i64::MIN + 1 is representable in absolute form.
    let output = baml_test!(
        r#"
        function main() -> int { (-9223372036854775807).abs() }
    "#
    );
    assert_eq!(
        output.result,
        Ok(BexExternalValue::Int(9_223_372_036_854_775_807))
    );
}

// ─── min / max ────────────────────────────────────────────────────────────────

#[tokio::test]
async fn int_min_basic() {
    let output = baml_test!(
        r#"
        function main() -> int { (3).min(5) }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(3)));
}

#[tokio::test]
async fn int_min_other_smaller() {
    let output = baml_test!(
        r#"
        function main() -> int { (5).min(3) }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(3)));
}

#[tokio::test]
async fn int_min_equal() {
    let output = baml_test!(
        r#"
        function main() -> int { (3).min(3) }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(3)));
}

#[tokio::test]
async fn int_min_negative() {
    let output = baml_test!(
        r#"
        function main() -> int { (-2).min(0) }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(-2)));
}

#[tokio::test]
async fn int_max_basic() {
    let output = baml_test!(
        r#"
        function main() -> int { (3).max(5) }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(5)));
}

#[tokio::test]
async fn int_max_other_smaller() {
    let output = baml_test!(
        r#"
        function main() -> int { (5).max(3) }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(5)));
}

#[tokio::test]
async fn int_max_negative() {
    let output = baml_test!(
        r#"
        function main() -> int { (-5).max(-2) }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(-2)));
}

// ─── clamp ────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn int_clamp_in_range() {
    let output = baml_test!(
        r#"
        function main() -> int { (5).clamp(0, 10) }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(5)));
}

#[tokio::test]
async fn int_clamp_below_min() {
    let output = baml_test!(
        r#"
        function main() -> int { (-3).clamp(0, 10) }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(0)));
}

#[tokio::test]
async fn int_clamp_above_max() {
    let output = baml_test!(
        r#"
        function main() -> int { (15).clamp(0, 10) }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(10)));
}

#[tokio::test]
async fn int_clamp_at_min_boundary() {
    let output = baml_test!(
        r#"
        function main() -> int { (0).clamp(0, 10) }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(0)));
}

#[tokio::test]
async fn int_clamp_at_max_boundary() {
    let output = baml_test!(
        r#"
        function main() -> int { (10).clamp(0, 10) }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(10)));
}

// ─── max_value / min_value ────────────────────────────────────────────────────

#[tokio::test]
async fn int_max_value_returns_i64_max() {
    let output = baml_test!(
        r#"
        function main() -> int { int.max_value() }
    "#
    );
    assert_eq!(
        output.result,
        Ok(BexExternalValue::Int(9_223_372_036_854_775_807))
    );
}

#[tokio::test]
async fn int_min_value_returns_i64_min() {
    let output = baml_test!(
        r#"
        function main() -> int { int.min_value() }
    "#
    );
    assert_eq!(
        output.result,
        Ok(BexExternalValue::Int(-9_223_372_036_854_775_808))
    );
}

#[tokio::test]
async fn int_max_min_inverse() {
    // max_value() + 1 would overflow; min_value() - 1 would underflow.
    // We just verify they round-trip through arithmetic that stays in range.
    let output = baml_test!(
        r#"
        function main() -> bool {
            int.max_value() > int.min_value()
        }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Bool(true)));
}

// ─── Bit operations ───────────────────────────────────────────────────────────

#[tokio::test]
async fn int_leading_zeros_zero() {
    let output = baml_test!(
        r#"
        function main() -> int { (0).leading_zeros() }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(64)));
}

#[tokio::test]
async fn int_leading_zeros_one() {
    let output = baml_test!(
        r#"
        function main() -> int { (1).leading_zeros() }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(63)));
}

#[tokio::test]
async fn int_leading_zeros_negative_one_is_zero() {
    // -1 is all ones in two's complement.
    let output = baml_test!(
        r#"
        function main() -> int { (-1).leading_zeros() }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(0)));
}

#[tokio::test]
async fn int_leading_zeros_large_power_of_two() {
    // 2^32 = 4294967296: 31 leading zeros (bit 32 is set out of 64).
    let output = baml_test!(
        r#"
        function main() -> int { (4294967296).leading_zeros() }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(31)));
}

#[tokio::test]
async fn int_leading_ones_zero() {
    let output = baml_test!(
        r#"
        function main() -> int { (0).leading_ones() }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(0)));
}

#[tokio::test]
async fn int_leading_ones_negative_one() {
    let output = baml_test!(
        r#"
        function main() -> int { (-1).leading_ones() }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(64)));
}

#[tokio::test]
async fn int_trailing_zeros_zero() {
    let output = baml_test!(
        r#"
        function main() -> int { (0).trailing_zeros() }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(64)));
}

#[tokio::test]
async fn int_trailing_zeros_one() {
    let output = baml_test!(
        r#"
        function main() -> int { (1).trailing_zeros() }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(0)));
}

#[tokio::test]
async fn int_trailing_zeros_eight() {
    // 8 = 0b1000 → three trailing zeros.
    let output = baml_test!(
        r#"
        function main() -> int { (8).trailing_zeros() }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(3)));
}

#[tokio::test]
async fn int_trailing_ones_zero() {
    let output = baml_test!(
        r#"
        function main() -> int { (0).trailing_ones() }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(0)));
}

#[tokio::test]
async fn int_trailing_ones_seven() {
    // 7 = 0b111.
    let output = baml_test!(
        r#"
        function main() -> int { (7).trailing_ones() }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(3)));
}

#[tokio::test]
async fn int_trailing_ones_negative_one() {
    let output = baml_test!(
        r#"
        function main() -> int { (-1).trailing_ones() }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(64)));
}

#[tokio::test]
async fn int_count_zeros_of_zero() {
    let output = baml_test!(
        r#"
        function main() -> int { (0).count_zeros() }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(64)));
}

#[tokio::test]
async fn int_count_zeros_of_negative_one() {
    let output = baml_test!(
        r#"
        function main() -> int { (-1).count_zeros() }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(0)));
}

#[tokio::test]
async fn int_count_ones_of_zero() {
    let output = baml_test!(
        r#"
        function main() -> int { (0).count_ones() }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(0)));
}

#[tokio::test]
async fn int_count_ones_of_seven() {
    let output = baml_test!(
        r#"
        function main() -> int { (7).count_ones() }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(3)));
}

#[tokio::test]
async fn int_count_ones_of_negative_one() {
    let output = baml_test!(
        r#"
        function main() -> int { (-1).count_ones() }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(64)));
}

#[tokio::test]
async fn int_count_zeros_plus_count_ones_equals_64() {
    // For any 64-bit integer, count_zeros + count_ones must equal 64.
    let output = baml_test!(
        r#"
        function main() -> int {
            let n = 51966;  // 0xCAFE — arbitrary mixed bit pattern
            n.count_zeros() + n.count_ones()
        }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(64)));
}
