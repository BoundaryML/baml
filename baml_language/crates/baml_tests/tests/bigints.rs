//! FFI-boundary tests for the `bigint` builtin class (BEP-022).
//!
//! Most `bigint` tests were converted to in-BAML `test` blocks in
//! `baml_src/ns_bigints/bigints.baml`. The tests below stay in Rust because
//! they exercise the host↔VM call boundary: they pass host `BexExternalValue`
//! arguments (`Int`/`Bigint`) directly, and assert the int↔bigint coercion and
//! range-checking that happen only at that boundary. Out-of-range host values
//! (above the i63 range) cannot be written as BAML literals, so these are not
//! expressible from a BAML call site.

use baml_tests::baml_test;
use bex_engine::BexExternalValue;
use num_bigint::BigInt;

// ─── FFI-boundary int↔bigint coercion ────────────────────────────────────────

#[tokio::test]
async fn test_bigint_param_accepts_int_from_host() {
    // Host SDK passes a plain `int(42)` to a function whose declared param
    // is `bigint` and whose body actually exercises bigint arithmetic. The
    // engine's call-boundary coercion widens int→bigint so the body's
    // `+ 1n` doesn't fault with `CannotApplyBinOp(Bigint, Int)`.
    let output = baml_test!(
        baml: r#"
        function PlusOne(x: bigint) -> bigint { x + 1n }
    "#,
        entry: "PlusOne",
        args: { "x" => BexExternalValue::Int(42) },
    );
    assert_eq!(
        output.result,
        Ok(BexExternalValue::Bigint(BigInt::from(43)))
    );
}

#[tokio::test]
async fn test_int_param_accepts_bigint_from_host_in_range() {
    // Symmetric: a Bigint that fits in i64 lands in an `int` slot.
    let output = baml_test!(
        baml: r#"
        function Echo(x: int) -> int { x }
    "#,
        entry: "Echo",
        args: { "x" => BexExternalValue::Bigint(BigInt::from(42)) },
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(42)));
}

#[tokio::test]
async fn test_int_param_rejects_bigint_overflow() {
    // A Bigint that doesn't fit in i64 against an `int` slot is a hard
    // error — there's no silently-truncate option that's safe.
    let huge = BigInt::parse_bytes(b"99999999999999999999", 10).unwrap();
    let output = baml_test!(
        baml: r#"
        function Echo(x: int) -> int { x }
    "#,
        entry: "Echo",
        args: { "x" => BexExternalValue::Bigint(huge) },
    );
    let Err(err) = &output.result else {
        panic!("expected overflow error, got: {:?}", output.result);
    };
    let msg = format!("{err:?}");
    assert!(
        msg.contains("does not fit in i64"),
        "expected i64 overflow error, got: {msg}"
    );
}

#[tokio::test]
async fn test_int_param_rejects_bigint_above_i63() {
    // A bigint that fits in i64 but exceeds the VM's i63 integer range
    // (INT_MAX = 2^62 - 1) must be rejected at the conversion boundary, not
    // silently wrapped to a negative. `2^62` is the smallest such value.
    let above = BigInt::from(4_611_686_018_427_387_904_i64); // 2^62 = INT_MAX + 1
    let output = baml_test!(
        baml: r#"
        function Echo(x: int) -> int { x }
    "#,
        entry: "Echo",
        args: { "x" => BexExternalValue::Bigint(above) },
    );
    let Err(err) = &output.result else {
        panic!("expected out-of-range error, got: {:?}", output.result);
    };
    let msg = format!("{err:?}");
    assert!(
        msg.contains("outside the BAML integer range"),
        "expected i63 range error, got: {msg}"
    );
}

#[tokio::test]
async fn test_int_param_rejects_host_int_above_i63() {
    // A plain host i64 above the i63 range is likewise rejected rather than
    // wrapping (release) or panicking (debug) inside `Value::int`.
    let output = baml_test!(
        baml: r#"
        function Echo(x: int) -> int { x }
    "#,
        entry: "Echo",
        args: { "x" => BexExternalValue::Int(4_611_686_018_427_387_904) }, // 2^62
    );
    let Err(err) = &output.result else {
        panic!("expected out-of-range error, got: {:?}", output.result);
    };
    let msg = format!("{err:?}");
    assert!(
        msg.contains("outside the BAML integer range"),
        "expected i63 range error, got: {msg}"
    );
}

#[tokio::test]
async fn test_int_param_accepts_max_i63() {
    // The largest in-range integer (INT_MAX = 2^62 - 1) round-trips from both
    // a host int and a host bigint.
    let max_i63: i64 = 4_611_686_018_427_387_903; // 2^62 - 1
    let from_int = baml_test!(
        baml: r#"function Echo(x: int) -> int { x }"#,
        entry: "Echo",
        args: { "x" => BexExternalValue::Int(max_i63) },
    );
    assert_eq!(from_int.result, Ok(BexExternalValue::Int(max_i63)));

    let from_bigint = baml_test!(
        baml: r#"function Echo(x: int) -> int { x }"#,
        entry: "Echo",
        args: { "x" => BexExternalValue::Bigint(BigInt::from(max_i63)) },
    );
    assert_eq!(from_bigint.result, Ok(BexExternalValue::Int(max_i63)));
}
