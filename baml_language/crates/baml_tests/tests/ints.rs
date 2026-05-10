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

// ─── parse ────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn int_parse_basic() {
    let output = baml_test!(
        r#"
        function main() -> int { int.parse("42") }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(42)));
}

#[tokio::test]
async fn int_parse_zero() {
    let output = baml_test!(
        r#"
        function main() -> int { int.parse("0") }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(0)));
}

#[tokio::test]
async fn int_parse_negative() {
    let output = baml_test!(
        r#"
        function main() -> int { int.parse("-7") }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(-7)));
}

#[tokio::test]
async fn int_parse_explicit_plus_sign() {
    let output = baml_test!(
        r#"
        function main() -> int { int.parse("+42") }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(42)));
}

#[tokio::test]
async fn int_parse_max_value() {
    let output = baml_test!(
        r#"
        function main() -> int { int.parse("9223372036854775807") }
    "#
    );
    assert_eq!(
        output.result,
        Ok(BexExternalValue::Int(9_223_372_036_854_775_807))
    );
}

#[tokio::test]
async fn int_parse_min_value() {
    let output = baml_test!(
        r#"
        function main() -> int { int.parse("-9223372036854775808") }
    "#
    );
    assert_eq!(
        output.result,
        Ok(BexExternalValue::Int(-9_223_372_036_854_775_808))
    );
}

#[tokio::test]
async fn int_parse_leading_zeros_ok() {
    let output = baml_test!(
        r#"
        function main() -> int { int.parse("007") }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(7)));
}

#[tokio::test]
async fn int_parse_empty_throws() {
    let output = baml_test!(
        r#"
        function main() -> int { int.parse("") }
    "#
    );
    let Err(bex_engine::EngineError::UnhandledThrow { .. }) = &output.result else {
        panic!("expected UnhandledThrow, got: {:?}", output.result);
    };
}

#[tokio::test]
async fn int_parse_non_digit_throws() {
    let output = baml_test!(
        r#"
        function main() -> int { int.parse("12a") }
    "#
    );
    let Err(bex_engine::EngineError::UnhandledThrow { .. }) = &output.result else {
        panic!("expected UnhandledThrow, got: {:?}", output.result);
    };
}

#[tokio::test]
async fn int_parse_alpha_throws() {
    let output = baml_test!(
        r#"
        function main() -> int { int.parse("hello") }
    "#
    );
    let Err(bex_engine::EngineError::UnhandledThrow { .. }) = &output.result else {
        panic!("expected UnhandledThrow, got: {:?}", output.result);
    };
}

#[tokio::test]
async fn int_parse_whitespace_throws() {
    let output = baml_test!(
        r#"
        function main() -> int { int.parse(" 5 ") }
    "#
    );
    let Err(bex_engine::EngineError::UnhandledThrow { .. }) = &output.result else {
        panic!("expected UnhandledThrow, got: {:?}", output.result);
    };
}

#[tokio::test]
async fn int_parse_overflow_throws() {
    // i64::MAX + 1 = 9223372036854775808
    let output = baml_test!(
        r#"
        function main() -> int { int.parse("9223372036854775808") }
    "#
    );
    let Err(bex_engine::EngineError::UnhandledThrow { .. }) = &output.result else {
        panic!("expected UnhandledThrow, got: {:?}", output.result);
    };
}

#[tokio::test]
async fn int_parse_underflow_throws() {
    let output = baml_test!(
        r#"
        function main() -> int { int.parse("-9223372036854775809") }
    "#
    );
    let Err(bex_engine::EngineError::UnhandledThrow { .. }) = &output.result else {
        panic!("expected UnhandledThrow, got: {:?}", output.result);
    };
}

#[tokio::test]
async fn int_parse_just_sign_throws() {
    let output = baml_test!(
        r#"
        function main() -> int { int.parse("-") }
    "#
    );
    let Err(bex_engine::EngineError::UnhandledThrow { .. }) = &output.result else {
        panic!("expected UnhandledThrow, got: {:?}", output.result);
    };
}

#[tokio::test]
async fn int_parse_underscore_throws() {
    // Rust's i64::from_str does NOT accept digit separators — verify.
    let output = baml_test!(
        r#"
        function main() -> int { int.parse("1_000") }
    "#
    );
    let Err(bex_engine::EngineError::UnhandledThrow { .. }) = &output.result else {
        panic!("expected UnhandledThrow, got: {:?}", output.result);
    };
}

#[tokio::test]
async fn int_parse_round_trip_via_to_string() {
    // Once int has stringification this would be fully round-trippable.
    // For now just verify parse → arithmetic works.
    let output = baml_test!(
        r#"
        function main() -> int { int.parse("100") + int.parse("23") }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(123)));
}

// ─── random ───────────────────────────────────────────────────────────────────
//
// `int.random(lower, upper)` returns a uniform value in `[lower, upper)`. We
// can't assert on the exact value, so we sample many times and verify:
//   - every sample is in range
//   - over enough samples both endpoints {lower, upper-1} are observed (this
//     deterministically passes for any reasonable PRNG with N=1000 and a tiny
//     range; for larger ranges we just check that distinct values are produced).

/// Run a BAML program that returns `int[]` and unwrap the elements as i64s.
async fn run_int_array(src: &str) -> Vec<i64> {
    let output = baml_test!(baml: src, entry: "main");
    match output.result {
        Ok(BexExternalValue::Array { items, .. }) => items
            .into_iter()
            .map(|item| match item {
                BexExternalValue::Int(n) => n,
                other => panic!("expected int element, got {other:?}"),
            })
            .collect(),
        other => panic!("expected array, got {other:?}"),
    }
}

#[tokio::test]
async fn int_random_in_range_and_covers_endpoints() {
    // Sample 1000 values from [0, 10) inside one BAML program — we need a
    // tight loop here to avoid recompiling per sample. Over 1000 samples in a
    // range of size 10, the probability of missing either endpoint is
    // (9/10)^1000 ≈ 1.7e-46 — effectively zero.
    let samples = run_int_array(
        r#"
        function main() -> int[] {
            let acc: int[] = [];
            let i = 0;
            while (i < 1000) {
                let _ = acc.push(int.random(0, 10));
                i = i + 1;
            }
            acc
        }
        "#,
    )
    .await;
    assert_eq!(samples.len(), 1000);
    let mut min_seen = i64::MAX;
    let mut max_seen = i64::MIN;
    for n in &samples {
        assert!(
            (0..10).contains(n),
            "int.random(0, 10) returned {n}, out of [0, 10)"
        );
        min_seen = min_seen.min(*n);
        max_seen = max_seen.max(*n);
    }
    assert_eq!(min_seen, 0, "0 should be reachable");
    assert_eq!(max_seen, 9, "9 should be reachable (upper is exclusive)");
}

#[tokio::test]
async fn int_random_negative_range() {
    let samples = run_int_array(
        r#"
        function main() -> int[] {
            let acc: int[] = [];
            let i = 0;
            while (i < 200) {
                let _ = acc.push(int.random(-5, 5));
                i = i + 1;
            }
            acc
        }
        "#,
    )
    .await;
    for n in &samples {
        assert!(
            (-5..5).contains(n),
            "int.random(-5, 5) returned {n}, out of [-5, 5)"
        );
    }
}

#[tokio::test]
async fn int_random_single_element_range() {
    let samples = run_int_array(
        r#"
        function main() -> int[] {
            let acc: int[] = [];
            let i = 0;
            while (i < 50) {
                let _ = acc.push(int.random(0, 1));
                i = i + 1;
            }
            acc
        }
        "#,
    )
    .await;
    for n in &samples {
        assert_eq!(
            *n, 0,
            "single-element range must always return the lower bound"
        );
    }
}

#[tokio::test]
async fn int_random_full_range_returns_some_int() {
    // Spanning the full i64 range exercises the largest `range` (2^64 - 1).
    // Just verify the call succeeds; distribution is covered by smaller-range tests.
    let output = baml_test!(
        r#"
        function main() -> int { int.random(int.min_value(), int.max_value()) }
    "#
    );
    assert!(matches!(output.result, Ok(BexExternalValue::Int(_))));
}

#[tokio::test]
async fn int_random_distinct_values_in_wide_range() {
    // Over 100 samples in a 1000-wide range, fewer than 50 distinct values
    // would indicate the RNG is broken (probability negligible).
    let samples = run_int_array(
        r#"
        function main() -> int[] {
            let acc: int[] = [];
            let i = 0;
            while (i < 100) {
                let _ = acc.push(int.random(0, 1000));
                i = i + 1;
            }
            acc
        }
        "#,
    )
    .await;
    let distinct: std::collections::HashSet<_> = samples.iter().copied().collect();
    assert!(
        distinct.len() >= 50,
        "expected at least 50 distinct values out of 100 samples, got {}",
        distinct.len()
    );
}

#[tokio::test]
async fn int_random_lower_equals_upper_throws() {
    let output = baml_test!(
        r#"
        function main() -> int { int.random(5, 5) }
    "#
    );
    let Err(bex_engine::EngineError::UnhandledThrow { .. }) = &output.result else {
        panic!("expected UnhandledThrow, got: {:?}", output.result);
    };
}

#[tokio::test]
async fn int_random_lower_greater_than_upper_throws() {
    let output = baml_test!(
        r#"
        function main() -> int { int.random(10, 0) }
    "#
    );
    let Err(bex_engine::EngineError::UnhandledThrow { .. }) = &output.result else {
        panic!("expected UnhandledThrow, got: {:?}", output.result);
    };
}

// ─── isqrt ────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn int_isqrt_perfect_square() {
    let output = baml_test!(
        r#"
        function main() -> int { (16).isqrt() }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(4)));
}

#[tokio::test]
async fn int_isqrt_non_square_rounds_down() {
    let output = baml_test!(
        r#"
        function main() -> int { (10).isqrt() }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(3)));
}

#[tokio::test]
async fn int_isqrt_zero() {
    let output = baml_test!(
        r#"
        function main() -> int { (0).isqrt() }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(0)));
}

#[tokio::test]
async fn int_isqrt_one() {
    let output = baml_test!(
        r#"
        function main() -> int { (1).isqrt() }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(1)));
}

#[tokio::test]
async fn int_isqrt_large() {
    // sqrt(1_000_000_000_000) = 1_000_000.
    let output = baml_test!(
        r#"
        function main() -> int { (1000000000000).isqrt() }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(1_000_000)));
}

#[tokio::test]
async fn int_isqrt_max_value() {
    // floor(sqrt(i64::MAX)) = 3037000499.
    let output = baml_test!(
        r#"
        function main() -> int { int.max_value().isqrt() }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(3_037_000_499)));
}

#[tokio::test]
async fn int_isqrt_negative_throws() {
    let output = baml_test!(
        r#"
        function main() -> int { (-1).isqrt() }
    "#
    );
    let Err(bex_engine::EngineError::UnhandledThrow { .. }) = &output.result else {
        panic!("expected UnhandledThrow, got: {:?}", output.result);
    };
}

#[tokio::test]
async fn int_isqrt_min_value_throws() {
    let output = baml_test!(
        r#"
        function main() -> int { int.min_value().isqrt() }
    "#
    );
    let Err(bex_engine::EngineError::UnhandledThrow { .. }) = &output.result else {
        panic!("expected UnhandledThrow, got: {:?}", output.result);
    };
}

// ─── pow ──────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn int_pow_basic() {
    let output = baml_test!(
        r#"
        function main() -> int { (2).pow(10) }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(1024)));
}

#[tokio::test]
async fn int_pow_zero_exponent() {
    let output = baml_test!(
        r#"
        function main() -> int { (5).pow(0) }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(1)));
}

#[tokio::test]
async fn int_pow_zero_to_zero_is_one() {
    let output = baml_test!(
        r#"
        function main() -> int { (0).pow(0) }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(1)));
}

#[tokio::test]
async fn int_pow_zero_to_positive_is_zero() {
    let output = baml_test!(
        r#"
        function main() -> int { (0).pow(5) }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(0)));
}

#[tokio::test]
async fn int_pow_negative_exponent_returns_zero() {
    let output = baml_test!(
        r#"
        function main() -> int { (2).pow(-1) }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(0)));
}

#[tokio::test]
async fn int_pow_negative_base_odd_exp() {
    let output = baml_test!(
        r#"
        function main() -> int { (-2).pow(3) }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(-8)));
}

#[tokio::test]
async fn int_pow_negative_base_even_exp() {
    let output = baml_test!(
        r#"
        function main() -> int { (-2).pow(4) }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(16)));
}

#[tokio::test]
async fn int_pow_one_anything() {
    let output = baml_test!(
        r#"
        function main() -> int { (1).pow(1000000) }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(1)));
}

#[tokio::test]
async fn int_pow_negative_one_even() {
    let output = baml_test!(
        r#"
        function main() -> int { (-1).pow(1000000) }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(1)));
}

#[tokio::test]
async fn int_pow_negative_one_odd() {
    let output = baml_test!(
        r#"
        function main() -> int { (-1).pow(1000001) }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(-1)));
}

#[tokio::test]
async fn int_pow_negative_one_huge_even_exp() {
    // Verify -1 parity is preserved even when exp doesn't fit in u32.
    let output = baml_test!(
        r#"
        function main() -> int { (-1).pow(8589934592) }  // 2^33, even
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(1)));
}

#[tokio::test]
async fn int_pow_overflow_positive_saturates() {
    let output = baml_test!(
        r#"
        function main() -> int { (10).pow(100) }
    "#
    );
    assert_eq!(
        output.result,
        Ok(BexExternalValue::Int(9_223_372_036_854_775_807))
    );
}

#[tokio::test]
async fn int_pow_overflow_negative_odd_saturates_to_min() {
    // (-10)^99 mathematically a huge negative number → saturate to i64::MIN.
    let output = baml_test!(
        r#"
        function main() -> int { (-10).pow(99) }
    "#
    );
    assert_eq!(
        output.result,
        Ok(BexExternalValue::Int(-9_223_372_036_854_775_808))
    );
}

#[tokio::test]
async fn int_pow_overflow_negative_even_saturates_to_max() {
    // (-10)^100 is a huge positive number → saturate to i64::MAX.
    let output = baml_test!(
        r#"
        function main() -> int { (-10).pow(100) }
    "#
    );
    assert_eq!(
        output.result,
        Ok(BexExternalValue::Int(9_223_372_036_854_775_807))
    );
}

// ─── ilog ─────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn int_ilog_base_10() {
    let output = baml_test!(
        r#"
        function main() -> int { (1000).ilog(10) }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(3)));
}

#[tokio::test]
async fn int_ilog_base_2_power_of_two() {
    let output = baml_test!(
        r#"
        function main() -> int { (1024).ilog(2) }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(10)));
}

#[tokio::test]
async fn int_ilog_rounds_down() {
    // 999 in base 10 → 2 (since 10^2 = 100 ≤ 999 < 1000 = 10^3).
    let output = baml_test!(
        r#"
        function main() -> int { (999).ilog(10) }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(2)));
}

#[tokio::test]
async fn int_ilog_one_is_zero() {
    let output = baml_test!(
        r#"
        function main() -> int { (1).ilog(10) }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(0)));
}

#[tokio::test]
async fn int_ilog_self_zero_throws() {
    let output = baml_test!(
        r#"
        function main() -> int { (0).ilog(10) }
    "#
    );
    let Err(bex_engine::EngineError::UnhandledThrow { .. }) = &output.result else {
        panic!("expected UnhandledThrow, got: {:?}", output.result);
    };
}

#[tokio::test]
async fn int_ilog_self_negative_throws() {
    let output = baml_test!(
        r#"
        function main() -> int { (-5).ilog(10) }
    "#
    );
    let Err(bex_engine::EngineError::UnhandledThrow { .. }) = &output.result else {
        panic!("expected UnhandledThrow, got: {:?}", output.result);
    };
}

#[tokio::test]
async fn int_ilog_base_one_throws() {
    let output = baml_test!(
        r#"
        function main() -> int { (10).ilog(1) }
    "#
    );
    let Err(bex_engine::EngineError::UnhandledThrow { .. }) = &output.result else {
        panic!("expected UnhandledThrow, got: {:?}", output.result);
    };
}

#[tokio::test]
async fn int_ilog_base_zero_throws() {
    let output = baml_test!(
        r#"
        function main() -> int { (10).ilog(0) }
    "#
    );
    let Err(bex_engine::EngineError::UnhandledThrow { .. }) = &output.result else {
        panic!("expected UnhandledThrow, got: {:?}", output.result);
    };
}

#[tokio::test]
async fn int_ilog_base_negative_throws() {
    let output = baml_test!(
        r#"
        function main() -> int { (10).ilog(-2) }
    "#
    );
    let Err(bex_engine::EngineError::UnhandledThrow { .. }) = &output.result else {
        panic!("expected UnhandledThrow, got: {:?}", output.result);
    };
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
