use std::sync::{Arc, OnceLock};

use napi::{
    Status,
    bindgen_prelude::{Buffer, FnArgs, Function},
    threadsafe_function::{ThreadsafeFunction, ThreadsafeFunctionCallMode},
};
use napi_derive::napi;

type CallbackArgs = FnArgs<(Buffer, bool)>;

/// Weak (does not pin the libuv loop) and unbounded: `deliver` runs on an
/// engine thread that may be inside a `callFunctionSync` `block_on` — where
/// the JS thread is parked and can never drain a full queue — so the call
/// must be `NonBlocking` and the queue must never be full.
type Callback = ThreadsafeFunction<CallbackArgs, (), CallbackArgs, Status, false, true>;

static CALLBACK: OnceLock<Arc<Callback>> = OnceLock::new();

#[napi(ts_args_type = "callback: (errorBytes: Buffer, cancelled: boolean) => void")]
pub fn register_unhandled_spawn_error_callback(
    callback: Function<'_, CallbackArgs, ()>,
) -> napi::Result<()> {
    let tsfn: Callback = callback
        .build_threadsafe_function()
        .callee_handled::<false>()
        .weak::<true>()
        .build()?;
    if CALLBACK.set(Arc::new(tsfn)).is_ok() {
        bridge_cffi::register_unhandled_spawn_error_callback(deliver);
    }
    Ok(())
}

extern "C" fn deliver(content: *const i8, length: usize, cancelled: i32) {
    let Some(callback) = CALLBACK.get() else {
        return;
    };
    let bytes = if content.is_null() || length == 0 {
        Vec::new()
    } else {
        // SAFETY: bridge_cffi keeps the borrowed callback buffer valid until return.
        unsafe { std::slice::from_raw_parts(content.cast(), length) }.to_vec()
    };
    let status = callback.call(
        FnArgs::from((Buffer::from(bytes), cancelled != 0)),
        ThreadsafeFunctionCallMode::NonBlocking,
    );
    // `Closing` is env teardown: the report would have nowhere to go anyway.
    if status != Status::Ok && status != Status::Closing {
        log::warn!("unhandled-spawn error delivery to Node failed with status {status:?}");
    }
}
