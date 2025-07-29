use std::{collections::HashMap, ops::Deref, ptr::null, sync::Arc};

use anyhow::Result;
use baml_runtime::{BamlRuntime, FunctionResult};
use baml_types::BamlValue;
use once_cell::sync::Lazy;

use super::*;
use crate::ffi::{callbacks::safe_trigger_callback, utils::handle_ffi_error};

/// cbindgen:ignore
static RUNTIME: Lazy<Arc<tokio::runtime::Runtime>> =
    Lazy::new(|| Arc::new(tokio::runtime::Runtime::new().expect("Failed to create Tokio runtime")));

/// Extern "C" function that returns immediately, scheduling the async call.
/// Once the asynchronous function completes, the provided callback is invoked.
#[no_mangle]
pub extern "C" fn call_function_from_c(
    runtime: *const libc::c_void,
    function_name: *const c_char,
    encoded_args: *const libc::c_char,
    length: usize,
    id: u32,
) -> *const libc::c_void {
    match call_function_from_c_inner(runtime, function_name, encoded_args, length, id) {
        Ok(_) => null(),
        Err(e) => handle_ffi_error(e),
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
    } = BamlFunctionArguments::from_c_buffer(encoded_args, length)?;

    let ctx = runtime.create_ctx_manager(BamlValue::String("cffi".to_string()), None);

    // Spawn an async task to await the future and call the callback when done.
    // Ensure that a Tokio runtime is running in your application.
    let rt = RUNTIME.clone();
    rt.spawn(async move {
        let result = match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| async {
            runtime
                .call_function(
                    func_name,
                    &kwargs,
                    &ctx,
                    None,
                    client_registry.as_ref(),
                    collectors.map(|c| c.iter().map(|c| c.deref().clone()).collect()),
                    env_vars,
                )
                .await
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

                (Err(anyhow::anyhow!(error_msg)), Default::default())
            }
        };

        let (final_result, _) = result;
        safe_trigger_callback(id, true, final_result, runtime);
    });

    Ok(())
}

/// Extern "C" function that returns immediately, scheduling the async call.
/// Once the asynchronous function completes, the provided callback is invoked.
#[no_mangle]
pub extern "C" fn call_function_stream_from_c(
    runtime: *const libc::c_void,
    function_name: *const c_char,
    encoded_args: *const libc::c_char,
    length: usize,
    id: u32,
) -> *const libc::c_void {
    match call_function_stream_from_c_inner(runtime, function_name, encoded_args, length, id) {
        Ok(_) => null(),
        Err(e) => handle_ffi_error(e),
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
    } = BamlFunctionArguments::from_c_buffer(encoded_args, length)?;

    let ctx = runtime.create_ctx_manager(BamlValue::String("cffi".to_string()), None);
    let mut stream = match runtime.stream_function(
        func_name,
        &kwargs,
        &ctx,
        None,
        client_registry.as_ref(),
        collectors.map(|c| c.iter().map(|c| c.deref().clone()).collect()),
        env_vars,
    ) {
        Ok(stream) => stream,
        Err(e) => {
            return Err(anyhow::anyhow!("Failed to stream function: {}", e));
        }
    };

    let ctx = runtime.create_ctx_manager(BamlValue::String("cffi".to_string()), None);

    RUNTIME.spawn(async move {
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| async move {
            stream
                .run(
                    Some(|| on_tick(id)),
                    Some(|r| on_event(id, r, runtime)),
                    &ctx,
                    None,
                    None,
                    HashMap::new(),
                )
                .await
        }));

        let final_result = match result {
            Ok(future) => {
                let (stream_result, _) = future.await;
                stream_result
            }
            Err(panic_info) => {
                // Handle the panic case - create an error result
                let error_msg = if let Some(s) = panic_info.downcast_ref::<&str>() {
                    format!("Stream function panicked: {s}")
                } else if let Some(s) = panic_info.downcast_ref::<String>() {
                    format!("Stream function panicked: {s}")
                } else {
                    "Stream function panicked with unknown error".to_string()
                };

                Err(anyhow::anyhow!(error_msg))
            }
        };

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
