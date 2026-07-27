//! Wasm runtime implementation for `bridge_cffi`.

use std::{cell::RefCell, collections::HashMap, sync::Arc};

use bex_project::{Bex, BexArgs};
use bridge_ctypes::{HANDLE_TABLE, kwargs_to_bex_values};
use prost::Message;

use crate::{BridgeError, call_and_encode, error_to_outbound, function_call_context_builder};

thread_local! {
    static RUNTIMES: RefCell<RuntimeRegistry> = RefCell::new(RuntimeRegistry::default());
}

#[derive(Default)]
struct RuntimeRegistry {
    instances: HashMap<u32, Arc<dyn Bex>>,
    next_key: u32,
}

pub(crate) fn insert_runtime(
    requested_key: Option<u32>,
    runtime: Arc<dyn Bex>,
) -> Result<u32, BridgeError> {
    Ok(RUNTIMES.with(|slot| {
        let mut registry = slot.borrow_mut();
        if registry.next_key == 0 {
            registry.next_key = 1;
        }
        let runtime_key = match requested_key {
            Some(runtime_key) => runtime_key,
            None => loop {
                let candidate = registry.next_key;
                registry.next_key = registry.next_key.wrapping_add(1);
                if registry.next_key == 0 {
                    registry.next_key = 1;
                }
                if candidate != u32::MAX && !registry.instances.contains_key(&candidate) {
                    break candidate;
                }
            },
        };
        let previous = registry.instances.insert(runtime_key, runtime);
        if let Some(previous) = previous {
            wasm_bindgen_futures::spawn_local(previous.shutdown());
        }
        runtime_key
    }))
}

pub(crate) fn take_all_runtimes() -> Result<Vec<Arc<dyn Bex>>, BridgeError> {
    Ok(RUNTIMES.with(|slot| {
        slot.borrow_mut()
            .instances
            .drain()
            .map(|(_, runtime)| runtime)
            .collect()
    }))
}

pub(crate) fn get_runtime(runtime_key: u32) -> Result<Arc<dyn Bex>, BridgeError> {
    RUNTIMES.with(|slot| {
        slot.borrow()
            .instances
            .get(&runtime_key)
            .cloned()
            .ok_or(BridgeError::RuntimeNotFound(runtime_key))
    })
}

struct DecodedCall {
    runtime: Arc<dyn Bex>,
    function_name: String,
    args: BexArgs,
    context: bex_project::FunctionCallContext,
}

fn decode_call(
    runtime_key: u32,
    function_name: &str,
    encoded_args: &[u8],
) -> Result<DecodedCall, BridgeError> {
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
        runtime: crate::get_runtime(runtime_key)?,
        function_name: function_name.to_string(),
        args: kwargs.into(),
        context,
    })
}

pub async fn call_function_in_wasm(
    runtime_key: u32,
    function_name: &str,
    encoded_args: &[u8],
) -> Vec<u8> {
    let call = match decode_call(runtime_key, function_name, encoded_args) {
        Ok(call) => call,
        Err(error) => return error_to_outbound(error),
    };
    call_and_encode(call.runtime, call.function_name, call.args, call.context).await
}

pub fn call_function_in_wasm_sync(
    runtime_key: u32,
    function_name: &str,
    encoded_args: &[u8],
) -> Vec<u8> {
    futures::executor::block_on(call_function_in_wasm(
        runtime_key,
        function_name,
        encoded_args,
    ))
}
