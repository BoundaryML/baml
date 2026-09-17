//! Typed JSON decoding tests for bigint literal types.
//!
//! These use embedded BAML because the runtime supports bigint literal types,
//! while the standalone BAML formatter does not yet accept them in type positions.

use baml_tests::baml_test;
use bex_engine::BexExternalValue;
use num_bigint::BigInt;

#[tokio::test]
async fn from_string_decodes_bigint_literal() {
    let output = baml_test!(
        baml: r#"
        function Decode() -> 42n {
            baml.json.from_string<42n>("\"42\"")
        }
    "#,
        entry: "Decode",
        args: {},
    );

    assert_eq!(
        output.result,
        Ok(BexExternalValue::Bigint(BigInt::from(42)))
    );
}

#[tokio::test]
async fn from_json_decodes_bigint_literal() {
    let output = baml_test!(
        baml: r#"
        function Decode() -> 42n {
            baml.json.from_json<42n>(baml.json.to_json(42n))
        }
    "#,
        entry: "Decode",
        args: {},
    );

    assert_eq!(
        output.result,
        Ok(BexExternalValue::Bigint(BigInt::from(42)))
    );
}

#[tokio::test]
async fn from_string_rejects_mismatched_bigint_literal() {
    let output = baml_test!(
        baml: r#"
        function Decode() -> 42n {
            baml.json.from_string<42n>("\"43\"")
        }
    "#,
        entry: "Decode",
        args: {},
    );

    let Err(error) = output.result else {
        panic!("expected mismatched bigint literal to fail")
    };
    assert!(
        format!("{error:?}").contains("literal bigint mismatch"),
        "unexpected error: {error:?}"
    );
}
