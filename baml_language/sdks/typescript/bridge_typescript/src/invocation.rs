//! Private host projections injected into the generated invocation facade.
use bridge_ctypes::{CffiHandleTableEntry, HANDLE_TABLE, baml_bridge::cffi::BamlHandleType};
use napi::{
    bindgen_prelude::{Buffer, External, Function},
    threadsafe_function::ThreadsafeFunctionCallMode,
};
use napi_derive::napi;

use crate::{errors::bridge_error_to_napi, handle::BamlHandle};

#[napi(js_name = "_traceSelection")]
pub fn trace_selection(
    handle: &BamlHandle,
    call_id: String,
) -> napi::Result<(Buffer, Option<BamlHandle>)> {
    let id = call_id
        .parse::<u64>()
        .map_err(|_| napi::Error::from_reason("invalid call ID"))?;
    let (wire, reservation) =
        bridge_cffi::control_projection::trace_selection(id, handle.key_u64())
            .map_err(bridge_error_to_napi)?;
    Ok((
        wire.into(),
        reservation.map(|key| BamlHandle::from_parts(key, BamlHandleType::TraceReservation as i32)),
    ))
}

#[napi(js_name = "_invocationContext")]
pub fn invocation_context(handle: Option<&BamlHandle>) -> napi::Result<Buffer> {
    bridge_cffi::control_projection::invocation_context(handle.map_or(0, BamlHandle::key_u64))
        .map(Into::into)
        .map_err(bridge_error_to_napi)
}

#[napi(js_name = "_isInvocationCancelled")]
pub fn is_invocation_cancelled(handle: &BamlHandle) -> napi::Result<bool> {
    let entry = HANDLE_TABLE
        .resolve(handle.key_u64())
        .ok_or_else(|| napi::Error::from_reason("invocation is no longer live"))?;
    let CffiHandleTableEntry::InvocationState(state) = &*entry else {
        return Err(napi::Error::from_reason("expected invocation state"));
    };
    Ok(state.state.is_cancelled())
}

pub struct CancellationWatch(napi::tokio::task::JoinHandle<()>);
impl Drop for CancellationWatch {
    fn drop(&mut self) {
        self.0.abort();
    }
}

/// The JS Invocation owns this watcher. A weak TSFN neither retains its JS
/// owner nor pins the event loop; dropping the owner stops native observation.
#[napi(js_name = "_watchInvocationCancellation", ts_return_type = "object")]
pub fn watch_invocation_cancellation(
    handle: &BamlHandle,
    callback: Function<'_, (), ()>,
) -> napi::Result<External<CancellationWatch>> {
    let entry = HANDLE_TABLE
        .resolve(handle.key_u64())
        .ok_or_else(|| napi::Error::from_reason("invocation is no longer live"))?;
    let CffiHandleTableEntry::InvocationState(state) = &*entry else {
        return Err(napi::Error::from_reason("expected invocation state"));
    };
    let cancellation = state.state.clone();
    let tsfn = callback
        .build_threadsafe_function()
        .callee_handled::<false>()
        .weak::<true>()
        .build()?;
    let task = bridge_cffi::get_tokio_runtime()
        .map_err(bridge_error_to_napi)?
        .spawn(async move {
            cancellation.cancelled().await;
            tsfn.call((), ThreadsafeFunctionCallMode::NonBlocking);
        });
    Ok(External::new(CancellationWatch(task)))
}
