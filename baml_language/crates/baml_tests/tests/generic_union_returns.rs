use baml_tests::baml_test;
use bex_external_types::BexExternalValue;

const SOURCE: &str =
    include_str!("../baml_src/ns_generic_union_returns/generic_union_returns.baml");

#[tokio::test]
async fn model_step_flattens_union_type_arg_before_sap_conversion() {
    let output = baml_test!(
        baml: SOURCE,
        entry: "model_step_flattens_union_type_arg"
    );
    assert_eq!(output.result, Ok(BexExternalValue::Bool(true)));
}

#[tokio::test]
async fn sap_parses_null_as_optional_json() {
    let output = baml_test!(baml: SOURCE, entry: "sap_parses_optional_json_null");
    assert_eq!(output.result, Ok(BexExternalValue::Bool(true)));
}

#[tokio::test]
async fn sap_parses_non_null_optional_json_value() {
    let output = baml_test!(baml: SOURCE, entry: "sap_parses_optional_json_value");
    assert_eq!(output.result, Ok(BexExternalValue::Bool(true)));
}

#[tokio::test]
async fn sap_preserves_declared_types_for_nullable_containers() {
    let output = baml_test!(
        baml: SOURCE,
        entry: "sap_nullable_container_types_are_preserved"
    );
    assert_eq!(output.result, Ok(BexExternalValue::Bool(true)));
}
