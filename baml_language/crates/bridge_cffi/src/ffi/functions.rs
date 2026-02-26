//! Function call FFI entry points.

use std::{ffi::CStr, panic::AssertUnwindSafe};

use bridge_ctypes::{DecodeFromBuffer, HANDLE_TABLE, kwargs_to_bex_values};
use futures::future::FutureExt;
use prost::Message;

use crate::{
    Buffer,
    baml::cffi::{CallAck, CallFunctionArgs, call_ack::Response as CResponse},
    engine::{get_runtime, get_tokio_runtime},
    error::BridgeError,
    ffi::callbacks::{send_error_to_callback, send_result_to_callback},
};

/// Encode a success response (task spawned successfully).
fn encode_success_response() -> Buffer {
    let msg = CallAck { response: None };
    Buffer::from(msg.encode_to_vec())
}

/// Encode an error response (failed to spawn task).
fn encode_error_response(error: &BridgeError) -> Buffer {
    let msg = CallAck {
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
    // Get runtime (must be initialized)
    let runtime = get_runtime()?;

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
    let args = unsafe { CallFunctionArgs::from_c_buffer(encoded_args as *const u8, length) }?;

    // Convert kwargs to BexValue
    let kwargs = kwargs_to_bex_values(args.kwargs, &HANDLE_TABLE)?;

    // Silently ignore collectors and type_builder (not supported)
    let call_ctx = bex_project::FunctionCallContextBuilder::new(sys_types::CallId(id.into()));

    // Spawn async task with panic catching
    get_tokio_runtime()?.spawn(async move {
        // Wrap the async block with catch_unwind to handle panics
        let result = AssertUnwindSafe(async {
            runtime
                .call_function(&func_name, kwargs.into(), call_ctx.build())
                .await
        })
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

/// Cancel an in-flight function call.
///
/// Fires the `CancellationToken` for the given call ID, which causes:
/// 1. The engine's Await handler to exit immediately with `EngineError::Cancelled`
/// 2. All in-flight async tasks (HTTP requests, sleeps) to be aborted
///
/// If the call has already completed or the ID is unknown, this returns an error.
#[unsafe(no_mangle)]
pub extern "C" fn cancel_function_call(id: u32) -> Buffer {
    match get_runtime() {
        Ok(runtime) => match runtime.cancel_function_call(sys_types::CallId(id.into())) {
            Ok(()) => encode_success_response(),
            Err(e) => encode_error_response(&BridgeError::Runtime(e)),
        },
        Err(e) => encode_error_response(&e),
    }
}
