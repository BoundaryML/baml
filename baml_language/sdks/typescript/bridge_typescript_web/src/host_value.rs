//! Raw Web host-value bindings.

use wasm_bindgen::prelude::*;

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
    sys_wasm::register_host_value_release_callback(callback)
}

#[wasm_bindgen(js_name = releaseWebHostCallable)]
pub fn release_web_host_callable(key: u64) {
    sys_wasm::release_host_callable(key);
}

#[wasm_bindgen(js_name = completeWebHostCall)]
pub fn complete_web_host_call(call_id: u32, is_error: i32, content: &[u8]) -> bool {
    sys_wasm::complete_host_call(call_id, is_error, content)
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
