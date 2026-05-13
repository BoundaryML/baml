//! Tests for the `bigint` builtin class (BEP-022).
//!
//! The VM returns bigint values across the external API via
//! `BexExternalValue::Bigint`, wrapping a `num_bigint::BigInt`.

use baml_tests::baml_test;
use bex_engine::BexExternalValue;
use num_bigint::BigInt;

// ─── int → bigint widening (Phase 5) ─────────────────────────────────────────

#[tokio::test]
async fn test_int_to_bigint_assign() {
    let output = baml_test!(
        r#"
        function main() -> bigint {
            let x: bigint = 42;
            x
        }
    "#
    );
    assert_eq!(
        output.result,
        Ok(BexExternalValue::Bigint(BigInt::from(42)))
    );
}

#[tokio::test]
async fn test_int_to_bigint_arg() {
    let output = baml_test!(
        baml: r#"
        function Identity(x: bigint) -> bigint { x }
        function Caller() -> bigint { Identity(42) }
    "#,
        entry: "Caller",
    );
    assert_eq!(
        output.result,
        Ok(BexExternalValue::Bigint(BigInt::from(42)))
    );
}

// ─── literal syntax (Phase 3) ─────────────────────────────────────────────────

#[tokio::test]
async fn bigint_literal_small() {
    let output = baml_test!(
        r#"
        function main() -> bigint { 42n }
    "#
    );
    assert_eq!(
        output.result,
        Ok(BexExternalValue::Bigint(BigInt::from(42)))
    );
}

#[tokio::test]
async fn bigint_literal_large() {
    let output = baml_test!(
        r#"
        function main() -> bigint { 99999999999999999999n }
    "#
    );
    assert_eq!(
        output.result,
        Ok(BexExternalValue::Bigint(
            BigInt::parse_bytes(b"99999999999999999999", 10).unwrap()
        ))
    );
}

#[tokio::test]
async fn bigint_literal_negative() {
    let output = baml_test!(
        r#"
        function main() -> bigint { -7n }
    "#
    );
    assert_eq!(
        output.result,
        Ok(BexExternalValue::Bigint(BigInt::from(-7)))
    );
}

#[tokio::test]
async fn bigint_literal_zero() {
    let output = baml_test!(
        r#"
        function main() -> bigint { 0n }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Bigint(BigInt::from(0))));
}

#[tokio::test]
async fn bigint_literal_let_binding() {
    let output = baml_test!(
        r#"
        function main() -> bigint {
            let x = 42n;
            x
        }
    "#
    );
    assert_eq!(
        output.result,
        Ok(BexExternalValue::Bigint(BigInt::from(42)))
    );
}

// ─── parse ────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn bigint_parse_small() {
    let output = baml_test!(
        r#"
        function main() -> bigint { bigint.parse("12345") }
    "#
    );
    assert_eq!(
        output.result,
        Ok(BexExternalValue::Bigint(BigInt::from(12345)))
    );
}

#[tokio::test]
async fn bigint_parse_large() {
    let output = baml_test!(
        r#"
        function main() -> bigint { bigint.parse("99999999999999999999") }
    "#
    );
    assert_eq!(
        output.result,
        Ok(BexExternalValue::Bigint(
            BigInt::parse_bytes(b"99999999999999999999", 10).unwrap()
        ))
    );
}

#[tokio::test]
async fn bigint_parse_negative() {
    let output = baml_test!(
        r#"
        function main() -> bigint { bigint.parse("-7") }
    "#
    );
    assert_eq!(
        output.result,
        Ok(BexExternalValue::Bigint(BigInt::from(-7)))
    );
}

#[tokio::test]
async fn bigint_parse_zero() {
    let output = baml_test!(
        r#"
        function main() -> bigint { bigint.parse("0") }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Bigint(BigInt::from(0))));
}

#[tokio::test]
async fn bigint_parse_invalid_throws() {
    let output = baml_test!(
        r#"
        function main() -> bigint { bigint.parse("not-a-number") }
    "#
    );
    let Err(bex_engine::EngineError::UnhandledThrow { .. }) = &output.result else {
        panic!("expected UnhandledThrow, got: {:?}", output.result);
    };
}

#[tokio::test]
async fn bigint_parse_empty_throws() {
    let output = baml_test!(
        r#"
        function main() -> bigint { bigint.parse("") }
    "#
    );
    let Err(bex_engine::EngineError::UnhandledThrow { .. }) = &output.result else {
        panic!("expected UnhandledThrow, got: {:?}", output.result);
    };
}

// ─── abs ──────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn bigint_abs_positive() {
    let output = baml_test!(
        r#"
        function main() -> bigint { (3n).abs() }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Bigint(BigInt::from(3))));
}

#[tokio::test]
async fn bigint_abs_negative() {
    let output = baml_test!(
        r#"
        function main() -> bigint { (-7n).abs() }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Bigint(BigInt::from(7))));
}

#[tokio::test]
async fn bigint_abs_zero() {
    let output = baml_test!(
        r#"
        function main() -> bigint { (0n).abs() }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Bigint(BigInt::from(0))));
}

#[tokio::test]
async fn bigint_abs_large_negative() {
    // Absolute value of a number larger than i64::MAX — only possible with bigint.
    let output = baml_test!(
        r#"
        function main() -> bigint { (-99999999999999999999n).abs() }
    "#
    );
    assert_eq!(
        output.result,
        Ok(BexExternalValue::Bigint(
            BigInt::parse_bytes(b"99999999999999999999", 10).unwrap()
        ))
    );
}

// ─── min / max ────────────────────────────────────────────────────────────────

#[tokio::test]
async fn bigint_min_self_smaller() {
    let output = baml_test!(
        r#"
        function main() -> bigint { (3n).min(5n) }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Bigint(BigInt::from(3))));
}

#[tokio::test]
async fn bigint_min_other_smaller() {
    let output = baml_test!(
        r#"
        function main() -> bigint { (5n).min(3n) }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Bigint(BigInt::from(3))));
}

#[tokio::test]
async fn bigint_min_equal() {
    let output = baml_test!(
        r#"
        function main() -> bigint { (3n).min(3n) }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Bigint(BigInt::from(3))));
}

#[tokio::test]
async fn bigint_min_negative() {
    let output = baml_test!(
        r#"
        function main() -> bigint { (-2n).min(0n) }
    "#
    );
    assert_eq!(
        output.result,
        Ok(BexExternalValue::Bigint(BigInt::from(-2)))
    );
}

#[tokio::test]
async fn bigint_max_self_larger() {
    let output = baml_test!(
        r#"
        function main() -> bigint { (5n).max(3n) }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Bigint(BigInt::from(5))));
}

#[tokio::test]
async fn bigint_max_other_larger() {
    let output = baml_test!(
        r#"
        function main() -> bigint { (3n).max(5n) }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Bigint(BigInt::from(5))));
}

#[tokio::test]
async fn bigint_max_equal() {
    let output = baml_test!(
        r#"
        function main() -> bigint { (3n).max(3n) }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Bigint(BigInt::from(3))));
}

// ─── clamp ────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn bigint_clamp_in_range() {
    let output = baml_test!(
        r#"
        function main() -> bigint { (5n).clamp(0n, 10n) }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Bigint(BigInt::from(5))));
}

#[tokio::test]
async fn bigint_clamp_below_min() {
    let output = baml_test!(
        r#"
        function main() -> bigint { (-3n).clamp(0n, 10n) }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Bigint(BigInt::from(0))));
}

#[tokio::test]
async fn bigint_clamp_above_max() {
    let output = baml_test!(
        r#"
        function main() -> bigint { (15n).clamp(0n, 10n) }
    "#
    );
    assert_eq!(
        output.result,
        Ok(BexExternalValue::Bigint(BigInt::from(10)))
    );
}

#[tokio::test]
async fn bigint_clamp_at_min() {
    let output = baml_test!(
        r#"
        function main() -> bigint { (0n).clamp(0n, 10n) }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Bigint(BigInt::from(0))));
}

#[tokio::test]
async fn bigint_clamp_at_max() {
    let output = baml_test!(
        r#"
        function main() -> bigint { (10n).clamp(0n, 10n) }
    "#
    );
    assert_eq!(
        output.result,
        Ok(BexExternalValue::Bigint(BigInt::from(10)))
    );
}

// ─── isqrt ────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn bigint_isqrt_perfect_square() {
    let output = baml_test!(
        r#"
        function main() -> bigint { (16n).isqrt() }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Bigint(BigInt::from(4))));
}

#[tokio::test]
async fn bigint_isqrt_non_perfect_square() {
    let output = baml_test!(
        r#"
        function main() -> bigint { (10n).isqrt() }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Bigint(BigInt::from(3))));
}

#[tokio::test]
async fn bigint_isqrt_zero() {
    let output = baml_test!(
        r#"
        function main() -> bigint { (0n).isqrt() }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Bigint(BigInt::from(0))));
}

#[tokio::test]
async fn bigint_isqrt_one() {
    let output = baml_test!(
        r#"
        function main() -> bigint { (1n).isqrt() }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Bigint(BigInt::from(1))));
}

#[tokio::test]
async fn bigint_isqrt_negative_throws() {
    let output = baml_test!(
        r#"
        function main() -> bigint { (-1n).isqrt() }
    "#
    );
    let Err(bex_engine::EngineError::UnhandledThrow { .. }) = &output.result else {
        panic!("expected UnhandledThrow, got: {:?}", output.result);
    };
}

// ─── pow ──────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn bigint_pow_basic() {
    let output = baml_test!(
        r#"
        function main() -> bigint { (2n).pow(10n) }
    "#
    );
    assert_eq!(
        output.result,
        Ok(BexExternalValue::Bigint(BigInt::from(1024)))
    );
}

#[tokio::test]
async fn bigint_pow_zero_exp() {
    let output = baml_test!(
        r#"
        function main() -> bigint { (2n).pow(0n) }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Bigint(BigInt::from(1))));
}

#[tokio::test]
async fn bigint_pow_zero_base_zero_exp() {
    // 0^0 == 1 by convention
    let output = baml_test!(
        r#"
        function main() -> bigint { (0n).pow(0n) }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Bigint(BigInt::from(1))));
}

#[tokio::test]
async fn bigint_pow_negative_base() {
    let output = baml_test!(
        r#"
        function main() -> bigint { (-2n).pow(3n) }
    "#
    );
    assert_eq!(
        output.result,
        Ok(BexExternalValue::Bigint(BigInt::from(-8)))
    );
}

#[tokio::test]
async fn bigint_pow_negative_exp_returns_zero() {
    let output = baml_test!(
        r#"
        function main() -> bigint { (2n).pow(-1n) }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Bigint(BigInt::from(0))));
}

#[tokio::test]
async fn bigint_pow_large() {
    // 2^256 is a 78-digit number; verify the result length and value prefix.
    // The exact value starts with 115792089237316195... (decimal).
    let output = baml_test!(
        r#"
        function main() -> bigint { (2n).pow(256n) }
    "#
    );
    let Ok(BexExternalValue::Bigint(bi)) = &output.result else {
        panic!("expected Bigint result, got: {:?}", output.result);
    };
    let s = bi.to_string();
    // 2^256 has exactly 78 decimal digits.
    assert_eq!(s.len(), 78, "2^256 should be a 78-digit number, got: {s}");
    assert!(
        s.starts_with("115792089237316195"),
        "unexpected prefix: {s}"
    );
}

// ─── ilog ─────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn bigint_ilog_base10() {
    let output = baml_test!(
        r#"
        function main() -> bigint { (1000n).ilog(10n) }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Bigint(BigInt::from(3))));
}

#[tokio::test]
async fn bigint_ilog_base2() {
    let output = baml_test!(
        r#"
        function main() -> bigint { (1024n).ilog(2n) }
    "#
    );
    assert_eq!(
        output.result,
        Ok(BexExternalValue::Bigint(BigInt::from(10)))
    );
}

#[tokio::test]
async fn bigint_ilog_one_returns_zero() {
    let output = baml_test!(
        r#"
        function main() -> bigint { (1n).ilog(10n) }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Bigint(BigInt::from(0))));
}

#[tokio::test]
async fn bigint_ilog_zero_throws() {
    let output = baml_test!(
        r#"
        function main() -> bigint { (0n).ilog(10n) }
    "#
    );
    let Err(bex_engine::EngineError::UnhandledThrow { .. }) = &output.result else {
        panic!("expected UnhandledThrow, got: {:?}", output.result);
    };
}

#[tokio::test]
async fn bigint_ilog_negative_throws() {
    let output = baml_test!(
        r#"
        function main() -> bigint { (-5n).ilog(10n) }
    "#
    );
    let Err(bex_engine::EngineError::UnhandledThrow { .. }) = &output.result else {
        panic!("expected UnhandledThrow, got: {:?}", output.result);
    };
}

#[tokio::test]
async fn bigint_ilog_base_one_throws() {
    let output = baml_test!(
        r#"
        function main() -> bigint { (10n).ilog(1n) }
    "#
    );
    let Err(bex_engine::EngineError::UnhandledThrow { .. }) = &output.result else {
        panic!("expected UnhandledThrow, got: {:?}", output.result);
    };
}

// ─── random ───────────────────────────────────────────────────────────────────

#[tokio::test]
async fn bigint_random_single_element_range() {
    // [0, 1) contains exactly one value: 0
    let output = baml_test!(
        r#"
        function main() -> bigint { bigint.random(0n, 1n) }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Bigint(BigInt::from(0))));
}

#[tokio::test]
async fn bigint_random_in_range() {
    // Sample from [10, 20); result must be in that range.
    let output = baml_test!(
        r#"
        function main() -> bigint { bigint.random(10n, 20n) }
    "#
    );
    let Ok(BexExternalValue::Bigint(bi)) = &output.result else {
        panic!("expected Bigint result, got: {:?}", output.result);
    };
    assert!(
        *bi >= BigInt::from(10) && *bi < BigInt::from(20),
        "random result {bi} not in [10, 20)"
    );
}

#[tokio::test]
async fn bigint_random_empty_range_throws() {
    let output = baml_test!(
        r#"
        function main() -> bigint { bigint.random(5n, 5n) }
    "#
    );
    let Err(bex_engine::EngineError::UnhandledThrow { .. }) = &output.result else {
        panic!("expected UnhandledThrow, got: {:?}", output.result);
    };
}

#[tokio::test]
async fn bigint_random_lower_greater_throws() {
    let output = baml_test!(
        r#"
        function main() -> bigint { bigint.random(10n, 0n) }
    "#
    );
    let Err(bex_engine::EngineError::UnhandledThrow { .. }) = &output.result else {
        panic!("expected UnhandledThrow, got: {:?}", output.result);
    };
}

// ─── equality and ordering ─────────────────────────────────────────

#[tokio::test]
async fn test_bigint_eq_same_alloc() {
    let output = baml_test!(
        r#"
        function main() -> bool { return 42n == 42n; }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Bool(true)));
}

#[tokio::test]
async fn test_bigint_eq_distinct_allocs() {
    let output = baml_test!(
        r#"
        function main() -> bool {
            let a: bigint = 42n;
            let b: bigint = bigint.parse("42");
            return a == b;
        }
        "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Bool(true)));
}

#[tokio::test]
async fn test_bigint_ordering_lt() {
    let output = baml_test!(
        r#"
        function main() -> bool { return 3n < 5n; }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Bool(true)));
}

#[tokio::test]
async fn test_bigint_ordering_gt_eq() {
    let output = baml_test!(
        r#"
        function main() -> bool { return 5n >= 5n; }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Bool(true)));
}

#[tokio::test]
async fn test_bigint_ordering_ne() {
    let output = baml_test!(
        r#"
        function main() -> bool { return 42n != 43n; }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Bool(true)));
}

#[tokio::test]
async fn test_bigint_ordering_large() {
    // Compare values larger than i64::MAX to verify BigInt arithmetic, not i64.
    let output = baml_test!(
        r#"
        function main() -> bool { return 99999999999999999999n > 99999999999999999998n; }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Bool(true)));
}

// ─── typed arithmetic and bitwise (Phase 7) ─────────────────────────────────

#[tokio::test]
async fn test_bigint_arithmetic_add() {
    let output = baml_test!(
        r#"
        function main() -> bool { return 2n + 3n == 5n; }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Bool(true)));
}

#[tokio::test]
async fn test_bigint_arithmetic_sub() {
    let output = baml_test!(
        r#"
        function main() -> bool { return 5n - 3n == 2n; }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Bool(true)));
}

#[tokio::test]
async fn test_bigint_arithmetic_mul() {
    let output = baml_test!(
        r#"
        function main() -> bool { return 2n * 3n == 6n; }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Bool(true)));
}

#[tokio::test]
async fn test_bigint_arithmetic_div() {
    let output = baml_test!(
        r#"
        function main() -> bool { return 7n / 2n == 3n; }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Bool(true)));
}

#[tokio::test]
async fn test_bigint_arithmetic_mod() {
    let output = baml_test!(
        r#"
        function main() -> bool { return 7n % 2n == 1n; }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Bool(true)));
}

#[tokio::test]
async fn test_bigint_bitwise_and() {
    let output = baml_test!(
        r#"
        function main() -> bool { return (15n & 240n) == 0n; }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Bool(true)));
}

#[tokio::test]
async fn test_bigint_bitwise_or() {
    let output = baml_test!(
        r#"
        function main() -> bool { return (15n | 240n) == 255n; }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Bool(true)));
}

#[tokio::test]
async fn test_bigint_bitwise_xor() {
    let output = baml_test!(
        r#"
        function main() -> bool { return (170n ^ 255n) == 85n; }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Bool(true)));
}

#[tokio::test]
async fn test_bigint_bitwise_shl() {
    let output = baml_test!(
        r#"
        function main() -> bool { return (1n << 4n) == 16n; }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Bool(true)));
}

#[tokio::test]
async fn test_bigint_bitwise_shr() {
    let output = baml_test!(
        r#"
        function main() -> bool { return (256n >> 4n) == 16n; }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Bool(true)));
}

#[tokio::test]
async fn test_bigint_bitwise_negative() {
    // num-bigint represents negative bigints in two's-complement for bitwise
    // ops: -1 & 0xFF == 0xFF (255).
    let output = baml_test!(
        r#"
        function main() -> bool { return ((-1n) & 255n) == 255n; }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Bool(true)));
}

#[tokio::test]
async fn test_bigint_div_by_zero() {
    let output = baml_test!(
        r#"
        function main() -> bigint { 1n / 0n }
    "#
    );
    let Err(bex_engine::EngineError::UnhandledThrow { .. }) = &output.result else {
        panic!("expected UnhandledThrow, got: {:?}", output.result);
    };
}

#[tokio::test]
async fn test_bigint_mod_by_zero() {
    let output = baml_test!(
        r#"
        function main() -> bigint { 1n % 0n }
    "#
    );
    let Err(bex_engine::EngineError::UnhandledThrow { .. }) = &output.result else {
        panic!("expected UnhandledThrow, got: {:?}", output.result);
    };
}

#[tokio::test]
async fn test_bigint_arithmetic_huge() {
    // (2n).pow(64n) + 1n produces 2^64 + 1 = 18446744073709551617,
    // which does not fit in an i64.
    let output = baml_test!(
        r#"
        function main() -> bigint { (2n).pow(64n) + 1n }
    "#
    );
    assert_eq!(
        output.result,
        Ok(BexExternalValue::Bigint(
            BigInt::parse_bytes(b"18446744073709551617", 10).unwrap()
        ))
    );
}

// ─── mixed `int OP bigint` arithmetic ──────────────────────────────

#[tokio::test]
async fn test_bigint_int_mixed_add_left() {
    let output = baml_test!(
        r#"
        function main() -> bool { return 2n + 3 == 5n; }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Bool(true)));
}

#[tokio::test]
async fn test_bigint_int_mixed_add_right() {
    let output = baml_test!(
        r#"
        function main() -> bool { return 3 + 2n == 5n; }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Bool(true)));
}

#[tokio::test]
async fn test_bigint_int_mixed_mul() {
    let output = baml_test!(
        r#"
        function main() -> bool { return 2n * 3 == 6n; }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Bool(true)));
}

#[tokio::test]
async fn test_bigint_int_mixed_shl() {
    let output = baml_test!(
        r#"
        function main() -> bool { return (1n << 4) == 16n; }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Bool(true)));
}

#[tokio::test]
async fn test_bigint_int_mixed_bitand_negative() {
    let output = baml_test!(
        r#"
        function main() -> bool { return ((-1n) & 255) == 255n; }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Bool(true)));
}

#[tokio::test]
async fn test_bigint_int_mixed_comparison_gt() {
    let output = baml_test!(
        r#"
        function main() -> bool { return 5n > 3; }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Bool(true)));
}

#[tokio::test]
async fn test_bigint_int_mixed_comparison_eq() {
    let output = baml_test!(
        r#"
        function main() -> bool { return 5 == 5n; }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Bool(true)));
}

// ─── AllocFailure guards ────────────────────────────────────────

/// Asserts that the engine result is an unhandled throw of `baml.panics.AllocFailure`.
fn assert_alloc_failure(result: &Result<BexExternalValue, bex_engine::EngineError>) {
    let Err(bex_engine::EngineError::UnhandledThrow { value, .. }) = result else {
        panic!("expected UnhandledThrow with AllocFailure, got: {result:?}");
    };
    let BexExternalValue::Instance { class_name, .. } = value.as_ref() else {
        panic!("expected Instance, got: {value:?}");
    };
    assert_eq!(
        class_name, "baml.panics.AllocFailure",
        "expected AllocFailure panic, got: {class_name}"
    );
}

#[tokio::test]
async fn test_bigint_pow_alloc_failure() {
    // (2n).pow(1_000_000_000n) would allocate ~125 MB; we raise AllocFailure
    // before any allocation is attempted.
    let output = baml_test!(
        r#"
        function main() -> bigint { return (2n).pow(1000000000n); }
    "#
    );
    assert_alloc_failure(&output.result);
}

#[tokio::test]
async fn test_bigint_shl_alloc_failure() {
    // 1n << 1_000_000_000n would also far exceed MAX_BIGINT_BITS.
    let output = baml_test!(
        r#"
        function main() -> bigint { return 1n << 1000000000n; }
    "#
    );
    assert_alloc_failure(&output.result);
}

#[tokio::test]
async fn test_bigint_shl_negative_shift() {
    // A negative shift count should raise AllocFailure with a clear message,
    // not the generic "does not fit in usize" path.
    let output = baml_test!(
        r#"
        function main() -> bigint { return 1n << -1n; }
    "#
    );
    assert_alloc_failure(&output.result);
}

#[tokio::test]
async fn test_bigint_pow_normal_works() {
    // (2n).pow(256n) is a 78-digit number — well within MAX_BIGINT_BITS.
    let output = baml_test!(
        r#"
        function main() -> bigint { return (2n).pow(256n); }
    "#
    );
    let Ok(BexExternalValue::Bigint(bi)) = &output.result else {
        panic!("expected Bigint result, got: {:?}", output.result);
    };
    let s = bi.to_string();
    assert_eq!(s.len(), 78, "2^256 should be a 78-digit number, got: {s}");
    assert!(
        s.starts_with("115792089237316195"),
        "unexpected prefix: {s}"
    );
}
