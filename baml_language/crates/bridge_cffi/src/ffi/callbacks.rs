//! Callback registration and invocation.

use bex_project::BexExternalValue;
use bridge_ctypes::external_to_outbound;
use once_cell::sync::OnceCell;
use prost::Message;

/// Callback signature: (call_id, is_error, content, length)
/// is_error=0: content is protobuf-encoded BamlOutboundValue
/// is_error=1: content is UTF-8 error string
pub type CallbackFn = extern "C" fn(call_id: u32, is_error: i32, content: *const i8, length: usize);

static CALLBACK_FN: OnceCell<CallbackFn> = OnceCell::new();

#[unsafe(no_mangle)]
pub extern "C" fn register_callback(callback_fn: CallbackFn) {
    let _ = CALLBACK_FN.set(callback_fn);
}

pub fn send_result_to_callback(id: u32, value: &BexExternalValue) {
    let callback_fn = match CALLBACK_FN.get() {
        Some(f) => f,
        None => {
            eprintln!(
                "BAML internal error: BAML function was called before register_callback was called"
            );
            return;
        }
    };

    let handle_options = bridge_ctypes::CffiHandleTableOptions::for_in_process();
    match external_to_outbound(value, &handle_options) {
        Ok(baml_value) => {
            let buf = baml_value.encode_to_vec();
            tokio::task::block_in_place(|| {
                callback_fn(id, 0, buf.as_ptr() as *const i8, buf.len());
            });
        }
        Err(e) => {
            send_error_to_callback(id, &e.to_string());
        }
    }
}

pub fn send_error_to_callback(id: u32, error: &str) {
    let callback_fn = match CALLBACK_FN.get() {
        Some(f) => f,
        None => {
            eprintln!(
                "BAML internal error: BAML function was called before register_callback was called"
            );
            eprintln!("{error}");
            return;
        }
    };
    tokio::task::block_in_place(|| {
        callback_fn(id, 1, error.as_ptr() as *const i8, error.len());
    });
}
