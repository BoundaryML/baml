//! Function call FFI entry points.

use std::{ffi::CStr, panic::AssertUnwindSafe};

use futures::future::FutureExt;
use prost::Message;

use crate::{
    Buffer,
    baml::cffi::{
        HostFunctionArguments, InvocationResponse, invocation_response::Response as CResponse,
    },
    ctypes::{DecodeFromBuffer, kwargs_to_bex_values},
    engine::{get_engine, get_runtime},
    error::BridgeError,
    ffi::callbacks::{send_error_to_callback, send_result_to_callback},
};

/// Encode a success response (task spawned successfully).
fn encode_success_response() -> Buffer {
    let msg = InvocationResponse { response: None };
    Buffer::from(msg.encode_to_vec())
}

/// Encode an error response (failed to spawn task).
fn encode_error_response(error: &BridgeError) -> Buffer {
    let msg = InvocationResponse {
        response: Some(CResponse::Error(error.to_string())),
    };
    Buffer::from(msg.encode_to_vec())
}

/// Call a BAML function asynchronously.
///
/// Returns immediately after spawning the async task.
/// Result is delivered via the registered callback.
///
/// Note: `_runtime` is unused since we use a global engine.
#[unsafe(no_mangle)]
pub extern "C" fn call_function_from_c(
    _runtime: *const libc::c_void,
    function_name: *const libc::c_char,
    encoded_args: *const libc::c_char,
    length: usize,
    id: u32,
) -> Buffer {
    match call_function_inner(function_name, encoded_args, length, id) {
        Ok(()) => encode_success_response(),
        Err(e) => encode_error_response(&e),
    }
}

fn call_function_inner(
    function_name: *const libc::c_char,
    encoded_args: *const libc::c_char,
    length: usize,
    id: u32,
) -> Result<(), BridgeError> {
    // Get engine (must be initialized)
    let engine = get_engine()?.clone();

    // Check for null function name pointer
    if function_name.is_null() {
        return Err(BridgeError::NullFunctionName);
    }

    // Parse function name
    // SAFETY: We've verified function_name is not null above
    let func_name = unsafe {
        CStr::from_ptr(function_name)
            .to_str()
            .map_err(BridgeError::from)?
            .to_owned()
    };

    // Decode protobuf arguments
    let args = HostFunctionArguments::from_c_buffer(encoded_args as *const u8, length)?;

    // Convert kwargs to BexValue
    let kwargs = kwargs_to_bex_values(args.kwargs)?;

    // Silently ignore collectors and type_builder (not supported)
    // TODO: Support collectors when bex_engine adds support
    // TODO: Support type_builder when bex_engine adds support

    // Look up function parameters to get parameter order
    let params =
        engine
            .function_params(&func_name)
            .ok_or_else(|| BridgeError::FunctionNotFound {
                name: func_name.clone(),
            })?;

    // Reorder kwargs to match function parameter declaration order.
    // This ensures arguments are passed correctly even if the client sends
    // them in a different order than the function expects.
    let bex_args: Vec<bex_external_types::BexExternalValue> = params
        .iter()
        .map(|(param_name, _param_type)| {
            kwargs
                .get(*param_name)
                .cloned()
                .ok_or_else(|| BridgeError::MissingArgument {
                    function: func_name.clone(),
                    parameter: (*param_name).to_string(),
                })
        })
        .collect::<Result<Vec<_>, _>>()?;

    // Spawn async task with panic catching
    let rt = get_runtime().clone();
    rt.spawn(async move {
        // Wrap the async block with catch_unwind to handle panics
        let result = AssertUnwindSafe(async { engine.call_function(&func_name, bex_args).await })
            .catch_unwind()
            .await;

        match result {
            Ok(Ok(value)) => {
                send_result_to_callback(id, true, &value);
            }
            Ok(Err(e)) => {
                send_error_to_callback(id, &format!("{}", e));
            }
            Err(panic_info) => {
                // Extract panic message
                let msg = if let Some(s) = panic_info.downcast_ref::<&str>() {
                    s.to_string()
                } else if let Some(s) = panic_info.downcast_ref::<String>() {
                    s.clone()
                } else {
                    "Unknown panic in async task".to_string()
                };
                send_error_to_callback(id, &format!("Panic: {}", msg));
            }
        }
    });

    Ok(())
}

/// Parse LLM response (call_function_parse).
#[unsafe(no_mangle)]
pub extern "C" fn call_function_parse_from_c(
    _runtime: *const libc::c_void,
    _function_name: *const libc::c_char,
    _encoded_args: *const libc::c_char,
    _length: usize,
    id: u32,
) -> Buffer {
    // TODO: Implement when bex_engine supports parsing
    send_error_to_callback(id, "call_function_parse not implemented in bridge_cffi");
    encode_success_response()
}

/// Stream a function call (placeholder).
#[unsafe(no_mangle)]
pub extern "C" fn call_function_stream_from_c(
    _runtime: *const libc::c_void,
    _function_name: *const libc::c_char,
    _encoded_args: *const libc::c_char,
    _length: usize,
    id: u32,
) -> Buffer {
    // TODO: Implement when bex_engine supports streaming
    send_error_to_callback(id, "Streaming not implemented in bridge_cffi");
    encode_success_response()
}

/// Cancel a function call (placeholder).
#[unsafe(no_mangle)]
pub extern "C" fn cancel_function_call(_id: u32) -> Buffer {
    // TODO: Implement cancellation
    encode_success_response()
}
