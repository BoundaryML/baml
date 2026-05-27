//! Callback registration and invocation.

use once_cell::sync::OnceCell;

/// Callback signature: (call_id, content, length).
///
/// `content` is always a protobuf-encoded `BamlOutboundResult` envelope —
/// carrying the ok value, a thrown error, a panic, or a synthesized pre-call
/// host-boundary failure. There is no separate error channel.
pub type CallbackFn = extern "C" fn(call_id: u32, content: *const i8, length: usize);

static CALLBACK_FN: OnceCell<CallbackFn> = OnceCell::new();

#[unsafe(no_mangle)]
pub extern "C" fn register_callback(callback_fn: CallbackFn) {
    let _ = CALLBACK_FN.set(callback_fn);
}

/// Deliver an already-encoded `BamlOutboundResult` envelope (the bytes from
/// [`crate::call_and_encode`]) to the registered callback as a non-error
/// payload — the envelope itself carries any thrown error/panic.
pub fn send_outbound_result_to_callback(id: u32, buf: &[u8]) {
    let callback_fn = match CALLBACK_FN.get() {
        Some(f) => f,
        None => {
            eprintln!(
                "BAML internal error: BAML function was called before register_callback was called"
            );
            return;
        }
    };
    tokio::task::block_in_place(|| {
        callback_fn(id, buf.as_ptr() as *const i8, buf.len());
    });
}
