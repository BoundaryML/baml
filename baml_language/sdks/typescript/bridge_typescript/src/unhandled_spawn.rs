use std::sync::{Arc, OnceLock};

use bridge_cffi::OwnedUnhandledSpawnError;
use bridge_ctypes::{HANDLE_TABLE, TransferSession};
use napi::{Status, bindgen_prelude::Function};
use napi_derive::napi;

use crate::{
    dispatch_queue::{DispatchQueue, NotificationArgs},
    encoded_result::BamlEncodedResult,
};

static CALLBACK: OnceLock<Arc<DispatchQueue>> = OnceLock::new();

#[napi(ts_args_type = "callback: (error: BamlEncodedResult, cancelled: boolean) => void")]
pub fn register_unhandled_spawn_error_callback(
    callback: Function<'_, NotificationArgs, ()>,
) -> napi::Result<()> {
    let queue = DispatchQueue::notification(callback)?;
    if CALLBACK.set(Arc::new(queue)).is_ok() {
        bridge_cffi::register_owned_unhandled_spawn_error_callback(deliver);
    }
    Ok(())
}

fn deliver(error: OwnedUnhandledSpawnError) {
    let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| -> napi::Result<()> {
        let Some(callback) = CALLBACK.get() else {
            return Ok(());
        };
        let result = BamlEncodedResult::with_invocation_session(
            error.content,
            TransferSession::new(&HANDLE_TABLE),
            error.runtime,
            error.transfers,
        )?;
        let status = callback.notify(result, error.cancelled);
        if status != Status::Ok && status != Status::Closing {
            return Err(napi::Error::from_status(status));
        }
        Ok(())
    }));
    match outcome {
        Ok(Ok(())) => {}
        Ok(Err(error)) => log::error!("Node unhandled-spawn delivery failed: {error}"),
        Err(_) => log::error!("Node unhandled-spawn delivery panicked"),
    }
}
