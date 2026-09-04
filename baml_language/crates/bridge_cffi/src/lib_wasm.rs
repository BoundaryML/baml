//! Wasm runtime implementation for `bridge_cffi`.

use std::sync::Arc;

use bex_project::{Bex, BexArgs};
use bridge_ctypes::{HANDLE_TABLE, kwargs_to_bex_values};
use prost::Message;

use crate::{
    BridgeError, call_and_encode, call_handle_and_encode, error_to_outbound,
    function_call_context_builder,
};

struct DecodedCall {
    runtime: Arc<dyn Bex>,
    target: bridge_ctypes::baml_bridge::cffi::call_function_args::CallTarget,
    args: BexArgs,
    context: bex_project::FunctionCallContext,
}

fn decode_call(runtime_key: Option<u64>, encoded_args: &[u8]) -> Result<DecodedCall, BridgeError> {
    use bridge_ctypes::baml_bridge::cffi::{CallFunctionArgs, call_function_args::CallTarget};

    let call = CallFunctionArgs::decode(encoded_args).map_err(bridge_ctypes::CtypesError::from)?;
    if call.call_id == 0 {
        return Err(BridgeError::InvalidCallId);
    }
    let runtime = crate::runtime_for_call(runtime_key, &call)?;
    let target = call.call_target.ok_or(BridgeError::MissingCallTarget)?;
    if matches!(target, CallTarget::FunctionHandle(_)) && !call.type_args.is_empty() {
        return Err(BridgeError::FunctionHandleTypeArgs);
    }
    let type_args = bridge_ctypes::proto_ty_args_to_named(&call.type_args)?;
    let kwargs = kwargs_to_bex_values(call.kwargs, &HANDLE_TABLE)?;
    let context = function_call_context_builder(bex_project::CallId(call.call_id))
        .with_type_args(type_args.type_args)
        .with_type_defs(type_args.type_defs)
        .build();
    Ok(DecodedCall {
        runtime,
        target,
        args: kwargs.into(),
        context,
    })
}

pub async fn call_function_in_wasm(encoded_args: &[u8]) -> Vec<u8> {
    call_function_in_wasm_for_runtime(None, encoded_args).await
}

pub async fn call_function_in_wasm_for_runtime(key: Option<u64>, encoded_args: &[u8]) -> Vec<u8> {
    use bridge_ctypes::baml_bridge::cffi::call_function_args::CallTarget;

    let call = match decode_call(key, encoded_args) {
        Ok(call) => call,
        Err(error) => return error_to_outbound(error),
    };
    match call.target {
        CallTarget::FunctionName(function_name) => {
            call_and_encode(call.runtime, function_name, call.args, call.context).await
        }
        CallTarget::FunctionHandle(handle_key) => {
            call_handle_and_encode(call.runtime, handle_key, call.args, call.context).await
        }
    }
}

pub fn call_function_in_wasm_sync(encoded_args: &[u8]) -> Vec<u8> {
    futures::executor::block_on(call_function_in_wasm(encoded_args))
}
