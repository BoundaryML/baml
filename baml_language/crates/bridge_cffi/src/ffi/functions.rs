//! Function call FFI entry points.

use std::{ffi::CStr, panic::AssertUnwindSafe};

use bridge_ctypes::{DecodeFromBuffer, HANDLE_TABLE, kwargs_to_bex_values};
use futures::future::FutureExt;

use crate::{
    engine::{get_runtime, get_tokio_runtime},
    error::BridgeError,
    ffi::callbacks::{send_error_to_callback, send_result_to_callback},
};

/// Format a BridgeError into the prefixed string protocol that Go's
/// ParseBamlError expects.
fn bridge_error_to_string(err: &BridgeError) -> String {
    match err {
        BridgeError::Ctypes(e) => format!("BamlError: BamlInvalidArgumentError: {e}"),
        BridgeError::NotInitialized => {
            "BamlError: BamlInvalidArgumentError: Engine not initialized".to_string()
        }
        BridgeError::NullFunctionName => {
            "BamlError: BamlInvalidArgumentError: Function name is null".to_string()
        }
        BridgeError::InvalidFunctionName(e) => {
            format!("BamlError: BamlInvalidArgumentError: Invalid function name: {e}")
        }
        BridgeError::FunctionNotFound { name } => {
            format!("BamlError: BamlInvalidArgumentError: Function not found: {name}")
        }
        BridgeError::MissingArgument {
            function,
            parameter,
        } => {
            format!(
                "BamlError: BamlInvalidArgumentError: Missing argument '{parameter}' for function '{function}'"
            )
        }
        BridgeError::NotImplemented(msg) => {
            format!("BamlError: BamlInvalidArgumentError: Not implemented: {msg}")
        }
        BridgeError::DuplicateCallId(id) => {
            format!("BamlError: BamlInvalidArgumentError: Duplicate call ID: {id}")
        }
        BridgeError::ProjectNotInitialized => {
            "BamlError: BamlClientError: Project not initialized".to_string()
        }
        BridgeError::LockPoisoned => "BamlError: BamlClientError: Lock poisoned".to_string(),
        BridgeError::Internal(msg) => format!("BamlError: BamlClientError: {msg}"),
        BridgeError::Runtime(re) => runtime_error_to_string(re),
    }
}

fn runtime_error_to_string(err: &bex_project::RuntimeError) -> String {
    use bex_project::RuntimeError;
    match err {
        RuntimeError::InvalidArgument { .. } => {
            format!("BamlError: BamlInvalidArgumentError: {err}")
        }
        RuntimeError::Engine(engine_err) => {
            use bex_project::EngineError;
            match engine_err {
                EngineError::FunctionNotFound { .. } => {
                    format!("BamlError: BamlInvalidArgumentError: {engine_err}")
                }
                e if bex_project::is_cancelled_engine_error(e) => {
                    format!("BamlError: BamlCancelledError: {engine_err}")
                }
                _ => format!("BamlError: BamlClientError: {engine_err}"),
            }
        }
        _ => format!("BamlError: BamlClientError: {err}"),
    }
}

/// Call a BAML function asynchronously.
///
/// Returns immediately after spawning the async task.
/// Result/error is delivered via the registered callback.
#[unsafe(no_mangle)]
pub extern "C" fn call_function(
    function_name: *const libc::c_char,
    encoded_args: *const u8,
    length: usize,
    id: u32,
) {
    if let Err(e) = call_function_inner(function_name, encoded_args, length, id) {
        send_error_to_callback(id, &bridge_error_to_string(&e));
    }
}

fn call_function_inner(
    function_name: *const libc::c_char,
    encoded_args: *const u8,
    length: usize,
    id: u32,
) -> Result<(), BridgeError> {
    use bridge_ctypes::baml_core::cffi::CallFunctionArgs;

    let runtime = get_runtime()?;

    if function_name.is_null() {
        return Err(BridgeError::NullFunctionName);
    }
    let func_name = unsafe { CStr::from_ptr(function_name) }
        .to_str()
        .map_err(BridgeError::from)?
        .to_owned();

    let args = if encoded_args.is_null() || length == 0 {
        CallFunctionArgs::default()
    } else {
        unsafe { CallFunctionArgs::from_c_buffer(encoded_args, length) }?
    };
    let kwargs = kwargs_to_bex_values(args.kwargs, &HANDLE_TABLE)?;

    let call_ctx = bex_project::FunctionCallContextBuilder::new(sys_types::CallId(id.into()));

    get_tokio_runtime()?.spawn(async move {
        let result = AssertUnwindSafe(async {
            runtime
                .call_function(&func_name, kwargs.into(), call_ctx.build())
                .await
        })
        .catch_unwind()
        .await;

        match result {
            Ok(Ok(value)) => {
                send_result_to_callback(id, &value);
            }
            Ok(Err(e)) => {
                let bridge_err = BridgeError::Runtime(e);
                send_error_to_callback(id, &bridge_error_to_string(&bridge_err));
            }
            Err(panic_info) => {
                let msg = if let Some(s) = panic_info.downcast_ref::<&str>() {
                    s.to_string()
                } else if let Some(s) = panic_info.downcast_ref::<String>() {
                    s.clone()
                } else {
                    "Unknown panic".to_string()
                };
                send_error_to_callback(id, &format!("Panic: {msg}"));
            }
        }
    });

    Ok(())
}

/// Cancel an in-flight function call.
///
/// Returns 0 on success, 1 if the call ID is unknown or already completed.
#[unsafe(no_mangle)]
pub extern "C" fn cancel_function_call(id: u32) -> i32 {
    let runtime = match get_runtime() {
        Ok(rt) => rt,
        Err(_) => return 1,
    };
    match runtime.cancel_function_call(sys_types::CallId(id.into())) {
        Ok(()) => 0,
        Err(_) => 1,
    }
}
