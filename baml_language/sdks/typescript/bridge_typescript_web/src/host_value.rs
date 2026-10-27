//! Raw Web host-value bindings.

use wasm_bindgen::{JsCast, prelude::*};

#[wasm_bindgen(js_name = configureWebSysops)]
pub fn configure_web_sysops(fetch_key: u64, read_file_sync_key: u64) -> Result<(), JsError> {
    sys_wasm::configure_web_sysops(fetch_key, read_file_sync_key)
        .map_err(|error| JsError::new(&error))
}

#[wasm_bindgen(js_name = registerWebHostCallable)]
pub fn register_web_host_callable(callable: js_sys::Function) -> u64 {
    sys_wasm::register_host_callable(callable)
}

#[wasm_bindgen(js_name = mintWebHostValueKey)]
pub fn mint_web_host_value_key() -> u64 {
    sys_wasm::mint_host_value_key()
}

#[wasm_bindgen(js_name = registerWebHostValueReleaseCallback)]
pub fn register_web_host_value_release_callback(callback: js_sys::Function) -> bool {
    // Every release the engine drives also forgets the callable's signature;
    // the runtime's own callback runs after. One closure for the page's life.
    let forgetting = Closure::<dyn Fn(JsValue)>::new(move |key: JsValue| {
        if let Some(key) = js_sys::BigInt::new(&key)
            .ok()
            .and_then(|key| u64::try_from(key).ok())
            .and_then(bridge_ctypes::HostKey::new)
        {
            crate::encode::forget_signature(key);
        }
        if let Err(error) = callback.call1(&JsValue::NULL, &key) {
            log::warn!("host-value release callback threw: {error:?}");
        }
    });
    let function: js_sys::Function = forgetting
        .as_ref()
        .unchecked_ref::<js_sys::Function>()
        .clone();
    forgetting.forget();
    sys_wasm::register_host_value_release_callback(function)
}

#[wasm_bindgen(js_name = releaseWebHostCallable)]
pub fn release_web_host_callable(key: u64) {
    sys_wasm::release_host_callable(key);
    if let Some(key) = bridge_ctypes::HostKey::new(key) {
        crate::encode::forget_signature(key);
    }
}

/// Complete an in-flight host call as the host's own exception: the engine
/// builds the `baml.errors.HostCallable` whose `_handle` is `handle`, the
/// JS-side registration of the error object (a decimal host-value key).
#[wasm_bindgen(js_name = completeWebHostCallRaised)]
pub fn complete_web_host_call_raised(
    call_id: u32,
    class_name: String,
    message: String,
    stack: Option<String>,
    handle: &str,
) -> Result<bool, JsValue> {
    let handle = handle
        .parse()
        .ok()
        .and_then(bridge_ctypes::HostKey::new)
        .ok_or_else(|| JsValue::from_str("a raised error must be registered first"))?;
    let completion = bridge_encode::raised(message, class_name, "web", stack, handle);
    Ok(sys_wasm::complete_host_call(
        call_id,
        &prost::Message::encode_to_vec(&completion),
    ))
}

/// Complete an in-flight host call as a failure of the bridge's own, which
/// the engine surfaces as `baml.panics.SdkPanic`.
#[wasm_bindgen(js_name = completeWebHostCallBridgeFailure)]
pub fn complete_web_host_call_bridge_failure(call_id: u32, message: String) -> bool {
    let completion = bridge_encode::bridge_failure(message);
    sys_wasm::complete_host_call(call_id, &prost::Message::encode_to_vec(&completion))
}

#[wasm_bindgen(js_name = _testWebHostCallableCount)]
pub fn test_web_host_callable_count() -> u32 {
    sys_wasm::test_host_callable_count()
}

#[wasm_bindgen(js_name = _testWebInFlightHostCallCount)]
pub fn test_web_in_flight_host_call_count() -> u32 {
    sys_wasm::test_in_flight_host_call_count()
}

#[wasm_bindgen(js_name = _testWebHostReleaseCallbackInstalled)]
pub fn test_web_host_release_callback_installed() -> bool {
    sys_wasm::test_host_release_callback_installed()
}

#[wasm_bindgen(js_name = _testWebFireHostRelease)]
pub fn test_web_fire_host_release(key: u64) {
    sys_wasm::test_fire_host_release(key);
}

#[wasm_bindgen(js_name = _testWebMissingHostCallableError)]
pub async fn test_web_missing_host_callable_error(key: u64) -> String {
    sys_wasm::test_missing_host_callable_error(key).await
}

#[wasm_bindgen(js_name = _testWebSyncPendingHostCallableError)]
pub fn test_web_sync_pending_host_callable_error(key: u64) -> String {
    sys_wasm::test_sync_pending_host_callable_error(key)
}

#[wasm_bindgen(js_name = registerWebHostCancelCallback)]
pub fn register_web_host_cancel_callback(callback: js_sys::Function) -> bool {
    sys_wasm::register_host_cancel_callback(callback)
}
