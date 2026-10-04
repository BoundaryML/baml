//! `decode_invocation_request` pins a handle target while the SDK's key is still live, so
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
        invocation: Some(Default::default()),
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
    bridge_cffi::stage_runtime(
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

    // A request cancelled after bridge preparation must reach the same
    // admission gate, without executing the target.
    let request = bridge_cffi::decode_invocation_request(&call_args(
        CallTarget::FunctionName("make_adder".to_string()),
        ("base", 40),
    ))
    .unwrap();
    assert!(bridge_cffi::cancel_function_call_by_id(
        request.host_call_id()
    ));
    let bytes = bridge_cffi::execute_invocation(request).await;
    let envelope = BamlOutboundResult::decode(bytes.as_slice()).unwrap();
    let Some(baml_outbound_result::Result::Panic(panic)) = envelope.result else {
        panic!("expected cancellation before admission");
    };
    let Some(baml_outbound_value::Value::ClassValue(class)) =
        panic.value.and_then(|value| value.value)
    else {
        panic!("expected the structured cancellation panic");
    };
    assert_eq!(class.name, "baml.panics.Cancelled");

    // The callable reaches the host as a handle-table key the SDK now owns.
    let request = bridge_cffi::decode_invocation_request(&call_args(
        CallTarget::FunctionName("make_adder".to_string()),
        ("base", 40),
    ))
    .unwrap();
    let bytes = bridge_cffi::execute_invocation(request).await;
    let baml_outbound_value::Value::HandleValue(handle) = ok_value(&bytes) else {
        panic!("expected the callable as a handle");
    };

    // Prepare the call through that key, then release it before invoking:
    // the window between `call_function` returning and its task running.
    // A closure's parameters are unnamed on the wire, so they go by position.
    let request = bridge_cffi::decode_invocation_request(&call_args(
        CallTarget::FunctionHandle(handle.key),
        ("arg0", 2),
    ))
    .unwrap();
    assert!(HANDLE_TABLE.release(handle.key));
    assert!(HANDLE_TABLE.resolve(handle.key).is_none());

    let bytes = bridge_cffi::execute_invocation(request).await;
    assert_eq!(ok_value(&bytes), baml_outbound_value::Value::IntValue(42));

    bridge_cffi::shutdown_runtime(None).await.unwrap();
}
