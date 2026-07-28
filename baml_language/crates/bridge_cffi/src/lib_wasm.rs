//! Wasm runtime implementation for `bridge_cffi`.

use std::{cell::RefCell, sync::Arc};

use bex_project::{Bex, BexArgs};
use bridge_ctypes::{HANDLE_TABLE, kwargs_to_bex_values};
use prost::Message;

use crate::{BridgeError, call_and_encode, error_to_outbound, function_call_context_builder};

thread_local! {
    static RUNTIME: RefCell<Option<Arc<dyn Bex>>> = RefCell::new(None);
}

pub(crate) fn replace_runtime(runtime: Arc<dyn Bex>) -> Result<(), BridgeError> {
    RUNTIME.with(|slot| {
        let previous = slot.borrow_mut().replace(runtime);
        if let Some(previous) = previous {
            wasm_bindgen_futures::spawn_local(previous.shutdown());
        }
    });
    Ok(())
}

pub(crate) fn take_runtime() -> Result<Option<Arc<dyn Bex>>, BridgeError> {
    Ok(RUNTIME.with(|slot| slot.borrow_mut().take()))
}

pub(crate) fn get_runtime() -> Result<Arc<dyn Bex>, BridgeError> {
    RUNTIME.with(|slot| slot.borrow().clone().ok_or(BridgeError::NotInitialized))
}

struct DecodedCall {
    runtime: Arc<dyn Bex>,
    function_name: String,
    args: BexArgs,
    context: bex_project::FunctionCallContext,
}

fn decode_call(function_name: &str, encoded_args: &[u8]) -> Result<DecodedCall, BridgeError> {
    use bridge_ctypes::baml_bridge::cffi::CallFunctionArgs;

    let call = CallFunctionArgs::decode(encoded_args).map_err(bridge_ctypes::CtypesError::from)?;
    if call.call_id == 0 {
        return Err(BridgeError::InvalidCallId);
    }
    let type_args = bridge_ctypes::proto_ty_args_to_named(&call.type_args)?;
    let kwargs = kwargs_to_bex_values(call.kwargs, &HANDLE_TABLE)?;
    let context = function_call_context_builder(bex_project::CallId(call.call_id))
        .with_type_args(type_args)
        .build();
    Ok(DecodedCall {
        runtime: crate::get_runtime()?,
        function_name: function_name.to_string(),
        args: kwargs.into(),
        context,
    })
}

pub async fn call_function_in_wasm(function_name: &str, encoded_args: &[u8]) -> Vec<u8> {
    let call = match decode_call(function_name, encoded_args) {
        Ok(call) => call,
        Err(error) => return error_to_outbound(error),
    };
    call_and_encode(call.runtime, call.function_name, call.args, call.context).await
}

pub fn call_function_in_wasm_sync(function_name: &str, encoded_args: &[u8]) -> Vec<u8> {
    futures::executor::block_on(call_function_in_wasm(function_name, encoded_args))
}

pub async fn call_handle_in_wasm(handle_key: u64, encoded_args: &[u8]) -> Vec<u8> {
    use bridge_ctypes::baml_bridge::cffi::CallFunctionArgs;

    let call =
        match CallFunctionArgs::decode(encoded_args).map_err(bridge_ctypes::CtypesError::from) {
            Ok(call) => call,
            Err(error) => return error_to_outbound(error.into()),
        };
    if call.call_id == 0 {
        return error_to_outbound(BridgeError::InvalidCallId);
    }
    let type_args = match bridge_ctypes::proto_ty_args_to_named(&call.type_args) {
        Ok(type_args) => type_args,
        Err(error) => return error_to_outbound(error.into()),
    };
    let args = match kwargs_to_bex_values(call.kwargs, &HANDLE_TABLE) {
        Ok(args) => args,
        Err(error) => return error_to_outbound(error.into()),
    };
    let context = function_call_context_builder(bex_project::CallId(call.call_id))
        .with_type_args(type_args)
        .build();
    let runtime = match crate::get_runtime() {
        Ok(runtime) => runtime,
        Err(error) => return error_to_outbound(error),
    };
    crate::call_handle_and_encode(runtime, handle_key, args.into(), context).await
}

pub fn call_handle_in_wasm_sync(handle_key: u64, encoded_args: &[u8]) -> Vec<u8> {
    futures::executor::block_on(call_handle_in_wasm(handle_key, encoded_args))
}
