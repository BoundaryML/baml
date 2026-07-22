use std::sync::{Arc, OnceLock};

use napi::{
    Status,
    bindgen_prelude::{Buffer, FnArgs, Function},
    threadsafe_function::{ThreadsafeFunction, ThreadsafeFunctionCallMode},
};
use napi_derive::napi;

type CallbackArgs = FnArgs<(Buffer, bool)>;
type Callback = ThreadsafeFunction<CallbackArgs, (), CallbackArgs, Status, false, true, 1024>;

static CALLBACK: OnceLock<Arc<Callback>> = OnceLock::new();

#[napi(ts_args_type = "callback: (errorBytes: Buffer, cancelled: boolean) => void")]
pub fn register_unhandled_spawn_error_callback(
    callback: Function<'_, CallbackArgs, ()>,
) -> napi::Result<()> {
    let tsfn: Callback = callback
        .build_threadsafe_function()
        .callee_handled::<false>()
        .weak::<true>()
        .max_queue_size::<1024>()
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
    let _ = callback.call(
        FnArgs::from((Buffer::from(bytes), cancelled != 0)),
        ThreadsafeFunctionCallMode::Blocking,
    );
}
