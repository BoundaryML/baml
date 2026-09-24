//! `prepare_call` pins a handle target while the SDK's key is still live, so
//! the SDK releasing that key between an adapter returning and its executor
//! running the call cannot invalidate the call.

use std::collections::HashMap;

use bridge_cffi::baml_bridge::cffi::{
    BamlOutboundResult, CallFunctionArgs, InboundMapEntry, InboundValue, baml_outbound_result,
    baml_outbound_value, call_function_args::CallTarget, inbound_map_entry::Key,
    inbound_value::Value as InboundValueVariant,
};
use bridge_ctypes::HANDLE_TABLE;
use prost::Message;

fn call_args(target: CallTarget, arg: (&str, i64)) -> Vec<u8> {
    CallFunctionArgs {
        call_id: bridge_cffi::new_function_call_id(),
        call_target: Some(target),
        kwargs: vec![InboundMapEntry {
            key: Some(Key::StringKey(arg.0.to_string())),
            value: Some(InboundValue {
                value_type: None,
                value: Some(InboundValueVariant::IntValue(arg.1)),
            }),
        }],
        ..Default::default()
    }
    .encode_to_vec()
}

fn ok_value(bytes: &[u8]) -> baml_outbound_value::Value {
    let envelope = BamlOutboundResult::decode(bytes).unwrap();
    let Some(baml_outbound_result::Result::Ok(value)) = envelope.result else {
        panic!("expected an ok envelope, got {envelope:?}");
    };
    value.value.expect("ok envelope carries a value")
}

#[tokio::test]
async fn prepared_handle_call_survives_release_of_its_key() {
    bridge_cffi::initialize_runtime(
        ".",
        HashMap::from([(
            "main.baml".to_string(),
            r#"
                function make_adder(base: int) -> (int) -> int throws never {
                    (x: int) -> { base + x }
                }
            "#
            .to_string(),
        )]),
    )
    .unwrap();
    let runtime = bridge_cffi::get_runtime().unwrap();

    // The callable reaches the host as a handle-table key the SDK now owns.
    let prepared = bridge_cffi::prepare_call(&call_args(
        CallTarget::FunctionName("make_adder".to_string()),
        ("base", 40),
    ))
    .unwrap();
    let bytes = bridge_cffi::invoke_prepared(runtime.clone(), prepared).await;
    let baml_outbound_value::Value::HandleValue(handle) = ok_value(&bytes) else {
        panic!("expected the callable as a handle");
    };

    // Prepare the call through that key, then release it before invoking:
    // the window between `call_function` returning and its task running.
    // A closure's parameters are unnamed on the wire, so they go by position.
    let prepared = bridge_cffi::prepare_call(&call_args(
        CallTarget::FunctionHandle(handle.key),
        ("arg0", 2),
    ))
    .unwrap();
    assert!(HANDLE_TABLE.release(handle.key));
    assert!(HANDLE_TABLE.resolve(handle.key).is_none());

    let bytes = bridge_cffi::invoke_prepared(runtime, prepared).await;
    assert_eq!(ok_value(&bytes), baml_outbound_value::Value::IntValue(42));

    bridge_cffi::shutdown_runtime().await.unwrap();
}
