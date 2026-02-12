//! Callback registration and invocation.

use bex_factory::BexExternalValue;
use bridge_ctypes::external_to_cffi_value;
use once_cell::sync::OnceCell;
use prost::Message;

pub type CallbackFn = extern "C" fn(call_id: u32, is_done: i32, content: *const i8, length: usize);
pub type OnTickCallbackFn = extern "C" fn(call_id: u32);

/// Result callback (success).
static RESULT_CALLBACK_FN: OnceCell<CallbackFn> = OnceCell::new();

/// Error callback.
static ERROR_CALLBACK_FN: OnceCell<CallbackFn> = OnceCell::new();

/// Tick callback (streaming progress).
static ON_TICK_CALLBACK_FN: OnceCell<OnTickCallbackFn> = OnceCell::new();

/// Register callbacks for async result delivery.
#[unsafe(no_mangle)]
pub extern "C" fn register_callbacks(
    callback_fn: CallbackFn,
    error_callback_fn: CallbackFn,
    on_tick_callback_fn: OnTickCallbackFn,
) {
    let _ = RESULT_CALLBACK_FN.set(callback_fn);
    let _ = ERROR_CALLBACK_FN.set(error_callback_fn);
    let _ = ON_TICK_CALLBACK_FN.set(on_tick_callback_fn);
}

/// Send a successful result via callback.
pub fn send_result_to_callback(id: u32, is_done: bool, value: &BexExternalValue) {
    let callback_fn = match RESULT_CALLBACK_FN.get() {
        Some(f) => f,
        None => {
            eprintln!("Result callback not registered");
            return;
        }
    };

    match external_to_cffi_value(value) {
        Ok(cffi_value) => {
            let buf = cffi_value.encode_to_vec();
            let is_done_int = if is_done { 1 } else { 0 };
            tokio::task::block_in_place(|| {
                callback_fn(id, is_done_int, buf.as_ptr() as *const i8, buf.len());
            });
        }
        Err(e) => {
            send_error_to_callback(id, &e.to_string());
        }
    }
}

/// Send an error via callback.
pub fn send_error_to_callback(id: u32, error: &str) {
    let error_callback_fn = match ERROR_CALLBACK_FN.get() {
        Some(f) => f,
        None => {
            eprintln!("Error callback not registered: {}", error);
            return;
        }
    };
    tokio::task::block_in_place(|| {
        error_callback_fn(id, 1, error.as_ptr() as *const i8, error.len());
    });
}

/// Trigger the on-tick callback for streaming progress.
#[allow(dead_code)] // Will be used when streaming is implemented
pub fn trigger_on_tick_callback(id: u32) {
    if let Some(on_tick_fn) = ON_TICK_CALLBACK_FN.get() {
        tokio::task::block_in_place(|| {
            on_tick_fn(id);
        });
    }
}
