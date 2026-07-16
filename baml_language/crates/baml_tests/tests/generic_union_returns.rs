use baml_tests::baml_test;
use bex_external_types::BexExternalValue;

const SOURCE: &str =
    include_str!("../baml_src/ns_generic_union_returns/generic_union_returns.baml");

async fn assert_true(entry: &str) {
    let output = baml_tests::engine::run_test(
        SOURCE,
        entry,
        baml_tests::engine::IndexMap::new(),
        baml_tests::engine::OptLevel::One,
    )
    .await;
    assert_eq!(output.result, Ok(BexExternalValue::Bool(true)));
}

#[tokio::test]
async fn explicit_concrete_branch_through_erased_interface() {
    let output = baml_test!(
        baml: SOURCE,
        entry: "generic_union_explicit_return_through_interface"
    );
    assert_eq!(output.result, Ok(BexExternalValue::Bool(true)));
}

#[tokio::test]
async fn tail_concrete_branch_through_erased_interface() {
    assert_true("generic_union_tail_return_through_interface").await;
}

#[tokio::test]
async fn generic_branch_through_erased_interface() {
    assert_true("generic_union_generic_branch_through_interface").await;
}

#[tokio::test]
async fn concrete_union_branch_after_sap_parsed_json_array() {
    assert_true("generic_union_sap_parsed_json_array_branch").await;
}

#[tokio::test]
async fn model_step_flattens_union_type_arg_before_sap_conversion() {
    assert_true("model_step_flattens_union_type_arg").await;
}

#[tokio::test]
async fn sap_parses_null_as_optional_json() {
    assert_true("sap_parses_optional_json_null").await;
}

#[tokio::test]
async fn sap_parses_non_null_optional_json_value() {
    assert_true("sap_parses_optional_json_value").await;
}

#[tokio::test]
async fn sap_parses_optional_json_class_field() {
    assert_true("sap_parses_optional_json_class_field").await;
}

#[tokio::test]
async fn sap_preserves_declared_types_for_nullable_containers() {
    assert_true("sap_nullable_container_types_are_preserved").await;
}
