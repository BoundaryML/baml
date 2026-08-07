//! Wasm runtime implementation for `bridge_cffi`.

use std::{cell::RefCell, sync::Arc};

use bex_project::{Bex, BexArgs};
use bridge_ctypes::{HANDLE_TABLE, kwargs_to_bex_values};
use prost::Message;

use crate::{
    BridgeError, call_and_encode, call_handle_and_encode, error_to_outbound,
    function_call_context_builder,
};

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
    target: bridge_ctypes::baml_bridge::cffi::call_function_args::CallTarget,
    args: BexArgs,
    context: bex_project::FunctionCallContext,
}

fn decode_call(encoded_args: &[u8]) -> Result<DecodedCall, BridgeError> {
    use bridge_ctypes::baml_bridge::cffi::{CallFunctionArgs, call_function_args::CallTarget};

    let call = CallFunctionArgs::decode(encoded_args).map_err(bridge_ctypes::CtypesError::from)?;
    if call.call_id == 0 {
        return Err(BridgeError::InvalidCallId);
    }
    let target = call.call_target.ok_or(BridgeError::MissingCallTarget)?;
    if matches!(target, CallTarget::FunctionHandle(_)) && !call.type_args.is_empty() {
        return Err(BridgeError::FunctionHandleTypeArgs);
    }
    let type_args = bridge_ctypes::proto_ty_args_to_named(&call.type_args)?;
    let kwargs = kwargs_to_bex_values(call.kwargs, &HANDLE_TABLE)?;
    let context = function_call_context_builder(bex_project::CallId(call.call_id))
        .with_type_args(type_args)
        .build();
    Ok(DecodedCall {
        runtime: crate::get_runtime()?,
        target,
        args: kwargs.into(),
        context,
    })
}

pub async fn call_function_in_wasm(encoded_args: &[u8]) -> Vec<u8> {
    use bridge_ctypes::baml_bridge::cffi::call_function_args::CallTarget;

    let call = match decode_call(encoded_args) {
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
