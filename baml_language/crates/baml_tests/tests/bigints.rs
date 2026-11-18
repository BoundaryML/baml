//! FFI-boundary tests for the `bigint` builtin class (BEP-022).
//!
//! Most `bigint` tests were converted to in-BAML `test` blocks in
//! `baml_src/ns_bigints/bigints.baml`. The tests below stay in Rust because
//! they exercise the host↔VM call boundary: they pass host `BexExternalValue`
//! arguments (`Int`/`Bigint`) directly. The engine converts nothing there: a
//! host `int` is no `bigint` and a host `bigint` is no `int`, whatever its
//! size, and every host `int` (any `i64`) is a BAML `int`.

use baml_tests::baml_test;
use bex_engine::BexExternalValue;
use num_bigint::BigInt;

/// The diagnostic of a refused call.
fn refusal(result: &Result<BexExternalValue, impl std::fmt::Debug>) -> String {
    match result {
        Err(err) => format!("{err:?}"),
        Ok(value) => panic!("expected the call to be refused, got: {value:?}"),
    }
}

#[tokio::test]
async fn test_bigint_param_accepts_a_host_bigint_of_any_size() {
    // A value above the `i64` range cannot be written as a BAML literal, so
    // this is a boundary test.
    let huge = BigInt::parse_bytes(b"99999999999999999999", 10).unwrap();
    let output = baml_test!(
        baml: r#"
        function PlusOne(x: bigint) -> bigint { x + 1n }
    "#,
        entry: "PlusOne",
        args: { "x" => BexExternalValue::Bigint(huge.clone()) },
    );
    assert_eq!(output.result, Ok(BexExternalValue::Bigint(huge + 1)));
}

#[tokio::test]
async fn test_bigint_param_refuses_a_host_int() {
    let output = baml_test!(
        baml: r#"
        function PlusOne(x: bigint) -> bigint { x + 1n }
    "#,
        entry: "PlusOne",
        args: { "x" => BexExternalValue::Int(42) },
    );
    let msg = refusal(&output.result);
    assert!(
        msg.contains("does not match the declared type `bigint`"),
        "expected a bigint mismatch, got: {msg}"
    );
}

#[tokio::test]
async fn test_int_param_refuses_a_host_bigint_of_any_size() {
    let huge = BigInt::parse_bytes(b"99999999999999999999", 10).unwrap();
    for bigint in [BigInt::from(42), huge] {
        let output = baml_test!(
            baml: r#"
            function Echo(x: int) -> int { x }
        "#,
            entry: "Echo",
            args: { "x" => BexExternalValue::Bigint(bigint.clone()) },
        );
        let msg = refusal(&output.result);
        assert!(
            msg.contains("does not match the declared type `int`"),
            "for {bigint}: expected an int mismatch, got: {msg}"
        );
    }
}

#[tokio::test]
async fn test_int_param_accepts_every_host_i64() {
    // `int` is 64-bit: a host int of magnitude 2^62 or more (heap-boxed in
    // the VM) round-trips like any other, both ends of the range included.
    for int in [1_i64 << 62, -(1_i64 << 62) - 1, i64::MAX, i64::MIN] {
        let output = baml_test!(
            baml: r#"function Echo(x: int) -> int { x }"#,
            entry: "Echo",
            args: { "x" => BexExternalValue::Int(int) },
        );
        assert_eq!(output.result, Ok(BexExternalValue::Int(int)), "for {int}");
    }
}

#[tokio::test]
async fn test_int_param_arithmetic_on_host_ints_past_the_inline_range() {
    let output = baml_test!(
        baml: r#"function Next(x: int) -> int { x + 1 }"#,
        entry: "Next",
        args: { "x" => BexExternalValue::Int((1_i64 << 62) - 1) },
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(1_i64 << 62)));
}
