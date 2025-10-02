use std::sync::Arc;

use baml_cffi::baml::cffi::CffiFunctionArguments;
use baml_client_rust::{
    client::BamlClient,
    types::{Collector, TypeBuilder},
    BamlContext,
};
use prost::Message;

#[test]
fn encoded_arguments_include_env_and_handles() {
    std::env::set_var("OPENAI_API_KEY", "test-openai-key");
    let _ = std::env::var("OPENAI_API_KEY").expect("OPENAI_API_KEY should be set for tests");

    let mut context = BamlContext::new()
        .set_arg("message", "hello world")
        .expect("arg encoding");

    context = context.set_env_var("OPENAI_API_KEY", "override-key");

    let type_builder = TypeBuilder::new().expect("allocate type builder");
    context = context.with_type_builder(type_builder.clone());

    let collector = Arc::new(Collector::new(None).expect("allocate collector"));
    context = context.with_collector(collector);

    context = context.set_tag("environment", "test");
    context = context.set_tag("version", "1.0.0");

    let encoded = BamlClient::encode_context_for_test(&context).expect("encode context");
    let decoded = CffiFunctionArguments::decode(encoded.as_slice()).expect("decode proto");

    assert!(
        decoded.kwargs.iter().any(|entry| entry.key == "message"),
        "kwargs should include message argument"
    );

    assert!(decoded
        .env
        .iter()
        .any(|env| env.key == "OPENAI_API_KEY" && env.value == "override-key"));

    assert_eq!(
        decoded.collectors.len(),
        1,
        "collector handle should be present"
    );
    assert!(
        decoded.type_builder.is_some(),
        "type builder handle should be present"
    );

    assert_eq!(decoded.tags.len(), 2, "should have 2 tags");
    assert!(
        decoded
            .tags
            .iter()
            .any(|tag| tag.key == "environment"
                && tag.value.as_ref().and_then(|v| v.value.as_ref()).map(|v| match v {
                    baml_cffi::baml::cffi::cffi_value_holder::Value::StringValue(s) => s == "test",
                    _ => false,
                }) == Some(true)),
        "should have environment tag with value 'test'"
    );
    assert!(
        decoded
            .tags
            .iter()
            .any(|tag| tag.key == "version"
                && tag.value.as_ref().and_then(|v| v.value.as_ref()).map(|v| match v {
                    baml_cffi::baml::cffi::cffi_value_holder::Value::StringValue(s) => s == "1.0.0",
                    _ => false,
                }) == Some(true)),
        "should have version tag with value '1.0.0'"
    );
}
