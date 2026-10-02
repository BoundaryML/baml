//! Private control projections shared with the native SDK adapters.
use bridge_ctypes::{CffiHandleTableEntry, HANDLE_TABLE, baml_bridge::cffi::BamlOutboundValue};
use futures::future::{AbortHandle, Abortable};
use prost::Message;
use wasm_bindgen::prelude::*;

#[wasm_bindgen(js_name = traceSelection)]
pub fn trace_selection(call_id: u64, key: u64) -> Result<js_sys::Array, JsValue> {
    let (wire, owner) = bridge_cffi::control_projection::trace_selection(call_id, key)
        .map_err(|error| crate::errors::bridge_error(&error))?;
    let result = js_sys::Array::new();
    result.push(&js_sys::Uint8Array::from(wire.as_slice()));
    result.push(&owner.map_or(JsValue::NULL, |key| js_sys::BigInt::from(key).into()));
    Ok(result)
}

#[wasm_bindgen(js_name = invocationContext)]
pub fn invocation_context(key: u64) -> Result<Vec<u8>, JsValue> {
    bridge_cffi::control_projection::invocation_context(key)
        .map_err(|error| crate::errors::bridge_error(&error))
}

#[wasm_bindgen(js_name = cloneOutboundValue)]
pub fn clone_outbound_value(wire: &[u8]) -> Result<Vec<u8>, JsValue> {
    let value =
        BamlOutboundValue::decode(wire).map_err(|error| JsValue::from_str(&error.to_string()))?;
    bridge_cffi::control_projection::clone_outbound(&value)
        .map(|value| value.encode_to_vec())
        .map_err(|error| crate::errors::bridge_error(&error))
}

#[wasm_bindgen]
pub struct CancellationWatch(AbortHandle);
impl Drop for CancellationWatch {
    fn drop(&mut self) {
        self.0.abort();
    }
}

#[wasm_bindgen(js_name = watchInvocationCancellation)]
pub fn watch_invocation_cancellation(
    key: u64,
    callback: js_sys::Function,
) -> Result<CancellationWatch, JsValue> {
    let entry = HANDLE_TABLE
        .resolve(key)
        .ok_or_else(|| JsValue::from_str("invocation is no longer live"))?;
    let CffiHandleTableEntry::InvocationState(state) = &*entry else {
        return Err(JsValue::from_str("expected invocation state"));
    };
    let state = state.state.clone();
    let (abort, registration) = AbortHandle::new_pair();
    wasm_bindgen_futures::spawn_local(async move {
        let _ = Abortable::new(
            async move {
                state.cancelled().await;
                let _ = callback.call0(&JsValue::UNDEFINED);
            },
            registration,
        )
        .await;
    });
    Ok(CancellationWatch(abort))
}

#[wasm_bindgen(js_name = isInvocationCancelled)]
pub fn is_invocation_cancelled(key: u64) -> Result<bool, JsValue> {
    let entry = HANDLE_TABLE
        .resolve(key)
        .ok_or_else(|| JsValue::from_str("invocation is no longer live"))?;
    let CffiHandleTableEntry::InvocationState(state) = &*entry else {
        return Err(JsValue::from_str("expected invocation state"));
    };
    Ok(state.state.is_cancelled())
}
