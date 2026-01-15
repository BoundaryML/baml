use std::{collections::HashMap, ops::Deref, sync::Arc};

use anyhow::Result;
use baml_runtime::{BamlRuntime, FunctionResult};
use baml_types::BamlValue;
use once_cell::sync::Lazy;
use prost::Message;

use super::*;
use crate::{
    baml::cffi::{invocation_response::Response as CResponse, InvocationResponse},
    ffi::callbacks::{safe_trigger_callback, send_error_to_callback, send_result_to_callback},
};

/// Encode a success response (task spawned successfully, no return value)
fn encode_success_response() -> Buffer {
    // Empty response means success - task was spawned
    let msg = InvocationResponse { response: None };
    Buffer::from(msg.encode_to_vec())
}

/// Encode an error response (failed to spawn task)
fn encode_error_response(error: anyhow::Error) -> Buffer {
    let msg = InvocationResponse {
        response: Some(CResponse::Error(error.to_string())),
    };
    Buffer::from(msg.encode_to_vec())
}

/// cbindgen:ignore
static RUNTIME: Lazy<Arc<tokio::runtime::Runtime>> =
    Lazy::new(|| Arc::new(tokio::runtime::Runtime::new().expect("Failed to create Tokio runtime")));

/// Extern "C" function that returns immediately, scheduling the async call.
/// Once the asynchronous function completes, the provided callback is invoked.
/// Returns Buffer with InvocationResponse (empty on success, error message on failure).
/// Caller must free with free_buffer().
#[no_mangle]
pub extern "C" fn call_function_from_c(
    runtime: *const libc::c_void,
    function_name: *const c_char,
    encoded_args: *const libc::c_char,
    length: usize,
    id: u32,
) -> Buffer {
    match call_function_from_c_inner(runtime, function_name, encoded_args, length, id) {
        Ok(_) => encode_success_response(),
        Err(e) => encode_error_response(e),
    }
}

/// Returns Buffer with InvocationResponse (empty on success, error message on failure).
/// Caller must free with free_buffer().
#[no_mangle]
pub extern "C" fn call_function_parse_from_c(
    runtime: *const libc::c_void,
    function_name: *const c_char,
    encoded_args: *const libc::c_char,
    length: usize,
    id: u32,
) -> Buffer {
    match call_function_parse_from_c_inner(runtime, function_name, encoded_args, length, id) {
        Ok(_) => encode_success_response(),
        Err(e) => encode_error_response(e),
    }
}

fn call_function_from_c_inner(
    runtime: *const libc::c_void,
    function_name: *const c_char,
    encoded_args: *const libc::c_char,
    length: usize,
    id: u32,
) -> Result<()> {
    // Safety: assume that the pointers provided are valid.
    let runtime = unsafe { &*(runtime as *const BamlRuntime) };

    // Convert the function name.
    let func_name = match unsafe { CStr::from_ptr(function_name) }.to_str() {
        Ok(s) => s.to_owned(),
        Err(_) => {
            return Err(anyhow::anyhow!("Failed to convert function name to string"));
        }
    };

    // Convert keyword arguments.
    let BamlFunctionArguments {
        kwargs,
        client_registry,
        env_vars,
        collectors,
        type_builder,
        tags,
    } = BamlFunctionArguments::from_c_buffer(encoded_args, length)?;

    let ctx = runtime.create_ctx_manager(BamlValue::String("cffi".to_string()), None);
    let tripwire = trip_wire::make_trip_wire(id);

    // Spawn an async task to await the future and call the callback when done.
    // Ensure that a Tokio runtime is running in your application.
    let rt = RUNTIME.clone();
    rt.spawn(async move {
        // Create a future for the call_function
        // TODO: There's a race condition bug here. Technically we should COPY the type builder, not just clone it.
        let type_builder = type_builder.map(|t| t.type_builder.as_ref().clone());
        let result = runtime
            .call_function(
                func_name,
                &kwargs,
                &ctx,
                type_builder.as_ref(),
                client_registry.as_ref(),
                collectors.map(|c| c.iter().map(|c| c.deref().clone()).collect()),
                env_vars,
                Some(&tags),
                tripwire.clone(),
            )
            .await;

        let (final_result, _) = result;
        // This drop seems to be required due to timing issues accross ffi-boundaries
        // We want to ensure we drop BEFORE any callbacks are made.
        // If we don't do this explicitly, it will also auto-drop eventually.
        drop(tripwire);

        safe_trigger_callback(id, true, final_result, runtime);
    });

    Ok(())
}

fn call_function_parse_from_c_inner(
    runtime: *const libc::c_void,
    function_name: *const c_char,
    encoded_args: *const libc::c_char,
    length: usize,
    id: u32,
) -> Result<()> {
    // Safety: assume that the pointers provided are valid.
    let runtime = unsafe { &*(runtime as *const BamlRuntime) };

    // Convert the function name.
    let func_name = match unsafe { CStr::from_ptr(function_name) }.to_str() {
        Ok(s) => s.to_owned(),
        Err(_) => {
            return Err(anyhow::anyhow!("Failed to convert function name to string"));
        }
    };

    // Convert keyword arguments.
    let BamlFunctionArguments {
        kwargs,
        client_registry,
        env_vars,
        collectors: _,
        type_builder,
        tags: _,
    } = BamlFunctionArguments::from_c_buffer(encoded_args, length)?;

    let ctx = runtime.create_ctx_manager(BamlValue::String("cffi".to_string()), None);
    let text = match kwargs.get("text") {
        Some(t) => match t.as_str() {
            Some(s) => s.to_string(),
            None => {
                return Err(anyhow::anyhow!("text is not a string"));
            }
        },
        None => {
            return Err(anyhow::anyhow!("text is required"));
        }
    };
    let allow_stream_types = match kwargs.get("stream") {
        Some(s) => match s.as_bool() {
            Some(b) => b,
            None => {
                return Err(anyhow::anyhow!("stream is not a boolean"));
            }
        },
        None => false,
    };

    // Spawn an async task to await the future and call the callback when done.
    // Ensure that a Tokio runtime is running in your application.
    let rt = RUNTIME.clone();
    rt.spawn(async move {
        let result = match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| async {
            // TODO: There's a race condition bug here. Technically we should COPY the type builder, not just clone it.
            let type_builder = type_builder.map(|t| t.type_builder.as_ref().clone());
            runtime.parse_llm_response(
                func_name,
                text,
                allow_stream_types,
                &ctx,
                type_builder.as_ref(),
                client_registry.as_ref(),
                env_vars,
            )
        })) {
            Ok(future) => future.await,
            Err(panic_info) => {
                // Handle the panic case - create an error result
                let error_msg = if let Some(s) = panic_info.downcast_ref::<&str>() {
                    format!("Function panicked: {s}")
                } else if let Some(s) = panic_info.downcast_ref::<String>() {
                    format!("Function panicked: {s}")
                } else {
                    "Function panicked with unknown error".to_string()
                };

                Err(anyhow::anyhow!(error_msg))
            }
        };

        match result {
            Ok(result) => send_result_to_callback(id, !allow_stream_types, &result, runtime),
            Err(e) => {
                send_error_to_callback(id, &e);
            }
        };
    });

    Ok(())
}

/// Extern "C" function that returns immediately, scheduling the async call.
/// Once the asynchronous function completes, the provided callback is invoked.
/// Returns Buffer with InvocationResponse (empty on success, error message on failure).
/// Caller must free with free_buffer().
#[no_mangle]
pub extern "C" fn call_function_stream_from_c(
    runtime: *const libc::c_void,
    function_name: *const c_char,
    encoded_args: *const libc::c_char,
    length: usize,
    id: u32,
) -> Buffer {
    match call_function_stream_from_c_inner(runtime, function_name, encoded_args, length, id) {
        Ok(_) => encode_success_response(),
        Err(e) => encode_error_response(e),
    }
}

fn call_function_stream_from_c_inner(
    runtime: *const libc::c_void,
    function_name: *const c_char,
    encoded_args: *const libc::c_char,
    length: usize,
    id: u32,
) -> Result<()> {
    // Safety: assume that the pointers provided are valid.
    let runtime = unsafe { &*(runtime as *const BamlRuntime) };

    // Convert the function name.
    let func_name = match unsafe { CStr::from_ptr(function_name) }.to_str() {
        Ok(s) => s.to_owned(),
        Err(_) => {
            return Err(anyhow::anyhow!("Failed to convert function name to string"));
        }
    };

    // Convert keyword arguments.
    let BamlFunctionArguments {
        kwargs,
        client_registry,
        env_vars,
        collectors,
        type_builder,
        tags,
    } = BamlFunctionArguments::from_c_buffer(encoded_args, length)?;

    let tripwire = trip_wire::make_trip_wire(id);
    let ctx = runtime.create_ctx_manager(BamlValue::String("cffi".to_string()), None);
    // TODO: There's a race condition bug here. Technically we should COPY the type builder, not just clone it.
    let type_builder = type_builder.map(|t| t.type_builder.as_ref().clone());
    let mut stream = match runtime.stream_function(
        func_name,
        &kwargs,
        &ctx,
        type_builder.as_ref(),
        client_registry.as_ref(),
        collectors.map(|c| c.iter().map(|c| c.deref().clone()).collect()),
        env_vars,
        tripwire,
        Some(&tags),
    ) {
        Ok(stream) => stream,
        Err(e) => {
            return Err(anyhow::anyhow!("Failed to stream function: {}", e));
        }
    };

    let ctx = runtime.create_ctx_manager(BamlValue::String("cffi".to_string()), None);

    RUNTIME.spawn(async move {
        // Create the stream.run future
        let (final_result, _) = stream
            .run(
                Some(|| on_tick(id)),
                Some(|r| on_event(id, r, runtime)),
                &ctx,
                None,
                None,
                HashMap::new(),
            )
            .await;

        // We should explicitly destruct the stream object
        // BEFORE we send any data to the runtime.
        drop(stream);

        safe_trigger_callback(id, true, final_result, runtime);
    });

    Ok(())
}

fn on_tick(id: u32) {
    use crate::ffi::callbacks::trigger_on_tick_callback;
    trigger_on_tick_callback(id);
}

fn on_event(id: u32, result: FunctionResult, runtime: &BamlRuntime) {
    safe_trigger_callback(id, false, Ok(result), runtime);
}

/// Cancel a function call by its ID
/// Returns Buffer with InvocationResponse (empty = success).
/// Caller must free with free_buffer().
#[no_mangle]
pub extern "C" fn cancel_function_call(id: u32) -> Buffer {
    trip_wire::cancel(id);
    encode_success_response()
}
