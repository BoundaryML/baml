//! FFI-boundary tests for the `float` builtin class (BEP-043).
//!
//! These pass host `BexExternalValue` arguments, which is not expressible
//! from a BAML call site. The engine converts nothing at the boundary: a host
//! value inhabits the declared type as it is, or the call is refused.

use baml_tests::baml_test;
use bex_engine::BexExternalValue;

/// Unwrap the `Union { value, .. }` envelope a union-typed return carries.
fn union_payload<E: std::fmt::Debug>(result: &Result<BexExternalValue, E>) -> &BexExternalValue {
    match result {
        Ok(BexExternalValue::Union { value, .. }) => value,
        other => panic!("expected Ok(Union), got: {other:?}"),
    }
}

#[tokio::test]
async fn test_host_int_is_not_a_float() {
    // The type system does not relate `int` and `float`, and neither does
    // the call boundary: what a host means by `7` where a `float` is
    // declared is the host's bridge to say before the value arrives.
    const BAML: &str = r#"
        function DoubleOrEcho(x: float | string) -> float | string {
            if (x is float) { x * 2.0 } else { x }
        }
    "#;

    let from_int = baml_test!(
        baml: BAML,
        entry: "DoubleOrEcho",
        args: { "x" => BexExternalValue::Int(7) },
    );
    let Err(err) = &from_int.result else {
        panic!("expected a type mismatch, got: {:?}", from_int.result);
    };
    let msg = format!("{err:?}");
    assert!(
        msg.contains("Value of type 'int' does not match the declared type `float | string`"),
        "expected a union mismatch, got: {msg}"
    );

    let from_float = baml_test!(
        baml: BAML,
        entry: "DoubleOrEcho",
        args: { "x" => BexExternalValue::Float(7.0) },
    );
    assert_eq!(
        union_payload(&from_float.result),
        &BexExternalValue::Float(14.0)
    );

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
// A host int arriving at a numeric union lands on the `int` member wherever
// it is declared, and nowhere where there is none. Each entry function reports
// the member the value landed on by matching on the member types.

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

/// Call `WhichMember` with a host `Int(7)`.
async fn member_for_host_int(union: &str) -> Result<BexExternalValue, String> {
    let output = baml_test!(
        baml: &which_member_src(union),
        entry: "WhichMember",
        args: { "x" => BexExternalValue::Int(7) },
    );
    output.result.map_err(|err| format!("{err:?}"))
}

#[tokio::test]
async fn test_host_int_lands_on_the_int_member_wherever_it_is_declared() {
    for union in ["int | float | bigint", "float | bigint | int"] {
        assert_eq!(
            member_for_host_int(union).await,
            Ok(BexExternalValue::String("int".into())),
            "in `{union}`"
        );
    }
}

#[tokio::test]
async fn test_host_int_is_refused_by_a_numeric_union_without_int() {
    for union in ["bigint | float", "float | bigint"] {
        let err = member_for_host_int(union)
            .await
            .expect_err("a host int is neither a bigint nor a float");
        assert!(
            err.contains("Value of type 'int' does not match the declared type"),
            "in `{union}`: {err}"
        );
    }
}
