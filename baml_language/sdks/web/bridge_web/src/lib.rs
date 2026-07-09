//! Browser bindings around a WASM-linked `bridge_cffi`.
//!
//! Runtime execution intentionally stops at the CFFI boundary in this first
//! scaffold. This lets browser SDK tests exercise the correct core artifact
//! before browser `SysOps` are implemented.

#[cfg(target_arch = "wasm32")]
use wasm_bindgen::prelude::*;

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen(js_name = stageRuntimeBytecode)]
pub fn stage_runtime_bytecode(bytecode: &[u8]) -> Result<(), JsError> {
    bridge_cffi::stage_runtime_bytecode(bytecode).map_err(JsError::new)
}

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen(js_name = callFunction)]
pub fn call_function(function_name: &str, encoded_args: &[u8]) -> Result<Vec<u8>, JsError> {
    bridge_cffi::call_function_in_wasm(function_name, encoded_args).map_err(JsError::new)
}

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen(js_name = newFunctionCall)]
pub fn new_function_call() -> u64 {
    bridge_cffi::new_function_call_id()
}
