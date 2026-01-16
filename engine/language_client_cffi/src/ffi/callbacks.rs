use anyhow::Result;
use baml_runtime::{
    errors::ExposedError, internal::llm_client::ResponseBamlValue, BamlRuntime, FunctionResult,
};
use once_cell::sync::OnceCell;

use crate::ctypes::{EncodeMeta, EncodeToBuffer};

pub type CallbackFn = extern "C" fn(call_id: u32, is_done: i32, content: *const i8, length: usize);
pub type OnTickCallbackFn = extern "C" fn(call_id: u32);

/// cbindgen:ignore
static RESULT_CALLBACK_FN: OnceCell<CallbackFn> = OnceCell::new();

/// cbindgen:ignore
static ERROR_CALLBACK_FN: OnceCell<CallbackFn> = OnceCell::new();

/// cbindgen:ignore
static ON_TICK_CALLBACK_FN: OnceCell<OnTickCallbackFn> = OnceCell::new();

#[no_mangle]
pub extern "C" fn register_callbacks(
    callback_fn: CallbackFn,
    error_callback_fn: CallbackFn,
    on_tick_callback_fn: OnTickCallbackFn,
) {
    let log_setup = baml_log::init();
    if let Err(e) = log_setup {
        eprintln!("Error setting up BAML_LOG logging: {e}");
    }
    let env = env_logger::Env::new().filter("BAML_INTERNAL_LOG");
    let log_setup = env_logger::try_init_from_env(env);
    if let Err(e) = log_setup {
        eprintln!("Error setting up BAML_INTERNAL_LOG logging: {e}");
    }

    // Create a global runtime or pass it along as needed.
    let _ = RESULT_CALLBACK_FN.set(callback_fn);
    let _ = ERROR_CALLBACK_FN.set(error_callback_fn);
    let _ = ON_TICK_CALLBACK_FN.set(on_tick_callback_fn);
}

pub fn send_result_to_callback(
    id: u32,
    is_done: bool,
    content: &ResponseBamlValue,
    runtime: &BamlRuntime,
) {
    let callback_fn = RESULT_CALLBACK_FN
        .get()
        .expect("expected callback function to be set. Did you call register_callbacks?");

    let error_callback_fn = ERROR_CALLBACK_FN
        .get()
        .expect("expected error callback function to be set. Did you call register_callbacks?");

    let buf_result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        if is_done {
            let meta = content.0.map_meta(|f| EncodeMeta {
                field_type: f.3.to_non_streaming_type(runtime.ir.as_ref()),
                checks: &f.1,
            });

            meta.encode_to_c_buffer(runtime.ir.as_ref(), baml_types::StreamingMode::NonStreaming)
        } else {
            // Top level types in streaming always have `not_null` set to true.
            let mut content = content.0.clone();
            content.meta_mut().3.meta_mut().streaming_behavior.needed = true;
            let meta = content.map_meta(|f| EncodeMeta {
                field_type: f.3.to_streaming_type(runtime.ir.as_ref()),
                checks: &f.1,
            });
            meta.encode_to_c_buffer(runtime.ir.as_ref(), baml_types::StreamingMode::Streaming)
        }
    }));

    match buf_result {
        Ok(buf) => {
            let is_done_int = if is_done { 1 } else { 0 };
            // Use block_in_place to tell Tokio this is a blocking operation.
            // This allows Tokio to move other async tasks to different worker threads,
            // preventing deadlock when the callback performs blocking FFI calls.
            tokio::task::block_in_place(|| {
                callback_fn(id, is_done_int, buf.as_ptr() as *const i8, buf.len());
            });
        }
        Err(panic_info) => {
            let error_msg = if let Some(s) = panic_info.downcast_ref::<&str>() {
                format!("Buffer encoding panicked: {s}")
            } else if let Some(s) = panic_info.downcast_ref::<String>() {
                format!("Buffer encoding panicked: {s}")
            } else {
                "Buffer encoding panicked with unknown error".to_string()
            };

            if is_done {
                // For final results, send error via callback
                // Use block_in_place to tell Tokio this is a blocking operation.
                tokio::task::block_in_place(|| {
                    error_callback_fn(id, 1, error_msg.as_ptr() as *const i8, error_msg.len());
                });
            } else {
                // For streaming events, just log and drop the event
                baml_log::error!("Encoding error: {}", error_msg);
            }
        }
    }
}

pub fn send_error_to_callback(id: u32, error: &anyhow::Error) {
    let error_callback_fn = ERROR_CALLBACK_FN
        .get()
        .expect("expected error callback function to be set. Did you call register_callbacks?");
    let message = error.to_string();
    // Use block_in_place to tell Tokio this is a blocking operation.
    // This allows Tokio to move other async tasks to different worker threads,
    // preventing deadlock when the callback performs blocking FFI calls.
    tokio::task::block_in_place(|| {
        error_callback_fn(id, 1, message.as_ptr() as *const i8, message.len());
    });
}

pub fn safe_trigger_callback(
    id: u32,
    is_done: bool,
    result: Result<FunctionResult>,
    runtime: &BamlRuntime,
) {
    match result {
        Ok(result) => match result.result_with_constraints_content() {
            Ok(content) => {
                send_result_to_callback(id, is_done, content, runtime);
            }
            Err(e) => {
                // IF YOU EVER CHANGE THIS THINK CAREFULLY.
                // Almost definitely you should update ExposedError in engine/baml-runtime/src/errors.rs
                // and then propagate that error.
                match e.downcast_ref::<ExposedError>() {
                    Some(exposed_error) => {
                        send_error_to_callback(id, &exposed_error.to_anyhow_with_details())
                    }
                    None => send_error_to_callback(id, &e),
                }
            }
        },
        Err(e) => {
            send_error_to_callback(id, &e);
        }
    }
}

pub fn trigger_on_tick_callback(id: u32) {
    let on_tick_fn = ON_TICK_CALLBACK_FN
        .get()
        .expect("expected on tick callback function to be set. Did you call register_callbacks?");
    // Use block_in_place to tell Tokio this is a blocking operation.
    // This allows Tokio to move other async tasks to different worker threads,
    // preventing deadlock when the callback performs blocking FFI calls.
    tokio::task::block_in_place(|| {
        on_tick_fn(id);
    });
}
