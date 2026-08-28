use std::collections::HashMap;

use bex_project::{BexArgs, FunctionCallContextBuilder, FunctionOperation};
use bridge_cffi::baml_bridge::cffi::{
    BamlHandleType, BamlOutboundResult, baml_outbound_result, baml_outbound_value,
};
use prost::Message;

#[tokio::test]
async fn spec_operation_uses_the_authored_name_across_the_wire() {
    bridge_cffi::initialize_runtime(
        ".",
        HashMap::from([(
            "main.baml".to_string(),
            r#"
                function Dollar$Ask(question: string) -> string {
                    client: "openai/gpt-4o-mini"
                    prompt: `${question}`
                }
            "#
            .to_string(),
        )]),
    )
    .expect("initialize bridge runtime");
    let runtime = bridge_cffi::get_runtime().expect("bridge runtime");

    let encoded = bridge_cffi::call_operation_and_encode(
        runtime,
        "Dollar$Ask".to_string(),
        FunctionOperation::Spec,
        BexArgs {
            required: indexmap::IndexMap::from([(
                "question".to_string(),
                "private exact companion".into(),
            )]),
            optional: indexmap::IndexMap::new(),
        },
        FunctionCallContextBuilder::new(sys_types::CallId::next()).build(),
    )
    .await;
    bridge_cffi::shutdown_runtime()
        .await
        .expect("shutdown bridge runtime");

    let envelope = BamlOutboundResult::decode(encoded.as_slice()).expect("decode result");
    let Some(baml_outbound_result::Result::Ok(value)) = envelope.result else {
        panic!("expected a successful spec operation");
    };
    let Some(baml_outbound_value::Value::HandleValue(handle)) = value.value else {
        panic!("spec operation must return a FunctionSpec handle");
    };
    assert_eq!(handle.handle_type, BamlHandleType::AdtFunctionSpec as i32);
}
