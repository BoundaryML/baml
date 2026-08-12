//! Tests for the `float` builtin class (BEP-043).
//!
//! These are FFI-boundary tests: they pass host `BexExternalValue` args,
//! which is not expressible from a BAML call site.

use baml_tests::baml_test;
use bex_engine::BexExternalValue;

// ─── FFI-boundary int → float coercion ────────────────────────────────────────

/// Unwrap the `Union { value, .. }` envelope a union-typed return carries.
fn union_payload<E: std::fmt::Debug>(result: &Result<BexExternalValue, E>) -> &BexExternalValue {
    match result {
        Ok(BexExternalValue::Union { value, .. }) => value,
        other => panic!("expected Ok(Union), got: {other:?}"),
    }
}

#[tokio::test]
async fn test_int_to_float_coercion() {
    // Host encoders are value-shaped, not schema-shaped (Python `7` and JS
    // integral `Number`s arrive as `Int`), so a declared `float` slot must
    // widen a host `Int` at the call boundary. The type system itself does
    // not relate `int` and `float`, so this is not expressible from a BAML
    // call site — it stays a Rust test like the bigint boundary tests.
    const BAML: &str = r#"
        function DoubleOrEcho(x: float | string) -> float | string {
            if (x is float) { x * 2.0 } else { x }
        }
    "#;

    // A host int lands on the union's float member and arrives as a genuine
    // VM float — the body's `* 2.0` would fault on an int operand.
    let from_int = baml_test!(
        baml: BAML,
        entry: "DoubleOrEcho",
        args: { "x" => BexExternalValue::Int(7) },
    );
    assert_eq!(
        union_payload(&from_int.result),
        &BexExternalValue::Float(14.0)
    );

    // The string member is untouched by the numeric routing and echoes back.
    let from_string = baml_test!(
        baml: BAML,
        entry: "DoubleOrEcho",
        args: { "x" => BexExternalValue::String("hi".into()) },
    );
    assert_eq!(
        union_payload(&from_string.result),
        &BexExternalValue::String("hi".into())
    );
}

// ─── Numeric-union member selection for a host int ────────────────────────────
//
// A host int arriving at a numeric union picks the best member, not the first
// declared one: an exact `int` member wins outright, and between `bigint` and
// `float` the lossless `bigint` (exact for every i64) beats the lossy `float`
// (rounds above 2^53). Each entry function reports the member the value
// actually landed on by matching on the member types.

/// Build a `WhichMember(x: <union>) -> string` snippet that reports which
/// union member `x` inhabits at runtime, with one match arm per member.
fn which_member_src(union: &str) -> String {
    let arms: String = union
        .split('|')
        .map(str::trim)
        .map(|member| format!("                let v: {member} => \"{member}\",\n"))
        .collect();
    format!(
        r#"
        function WhichMember(x: {union}) -> string {{
            match (x) {{
{arms}            }}
        }}
    "#
    )
}

/// Call `WhichMember` with a host `Int(7)` and return the reported member name.
async fn member_for_host_int(union: &str) -> String {
    let output = baml_test!(
        baml: &which_member_src(union),
        entry: "WhichMember",
        args: { "x" => BexExternalValue::Int(7) },
    );
    match output.result {
        Ok(BexExternalValue::String(s)) => s.as_str().to_owned(),
        other => panic!("expected Ok(String), got: {other:?}"),
    }
}

#[tokio::test]
async fn test_int_into_int_float_bigint_union_prefers_int() {
    assert_eq!(member_for_host_int("int | float | bigint").await, "int");
}

#[tokio::test]
async fn test_int_into_float_bigint_int_union_prefers_int_regardless_of_order() {
    assert_eq!(member_for_host_int("float | bigint | int").await, "int");
}

#[tokio::test]
async fn test_int_into_bigint_float_union_prefers_bigint() {
    assert_eq!(member_for_host_int("bigint | float").await, "bigint");
}

#[tokio::test]
async fn test_int_into_float_bigint_union_prefers_bigint_regardless_of_order() {
    // `float` is declared first — the lossless `bigint` must still win.
    assert_eq!(member_for_host_int("float | bigint").await, "bigint");
}
