//! Raw runtime bindings.

use std::collections::HashMap;

use wasm_bindgen::prelude::*;

#[wasm_bindgen(js_name = stageRuntimeBytecode)]
pub fn stage_runtime_bytecode(bytecode: &[u8], runtime_key: Option<u32>) -> Result<u32, JsValue> {
    let sys_ops = sys_wasm::build()
        .map_err(|error| crate::errors::setup_error(crate::errors::CLIENT, error))?;
    bridge_cffi::initialize_runtime_from_bytecode_with_sys_ops(bytecode, sys_ops, runtime_key)
        .map_err(|error| crate::errors::bridge_error(&error))
}

#[wasm_bindgen(js_name = stageRuntimeSources)]
pub fn stage_runtime_sources(
    root_path: &str,
    files: JsValue,
    runtime_key: Option<u32>,
) -> Result<u32, JsValue> {
    let files: HashMap<String, String> =
        serde_wasm_bindgen::from_value(files).map_err(|error| {
            crate::errors::setup_error(
                crate::errors::INVALID_ARGUMENT,
                format!("source files must be a record of string contents: {error}"),
            )
        })?;
    let sys_ops = sys_wasm::build()
        .map_err(|error| crate::errors::setup_error(crate::errors::CLIENT, error))?;
    bridge_cffi::initialize_runtime_from_files_with_sys_ops(root_path, files, sys_ops, runtime_key)
        .map_err(|error| crate::errors::bridge_error(&error))
}

#[wasm_bindgen(js_name = callFunctionSync)]
pub fn call_function_sync(runtime_key: u32, function_name: &str, encoded_args: &[u8]) -> Vec<u8> {
    sys_wasm::with_web_sync_mode(|| {
        bridge_cffi::call_function_in_wasm_sync(runtime_key, function_name, encoded_args)
    })
}

#[wasm_bindgen(js_name = callFunction)]
pub async fn call_function(runtime_key: u32, function_name: &str, encoded_args: &[u8]) -> Vec<u8> {
    bridge_cffi::call_function_in_wasm(runtime_key, function_name, encoded_args).await
}
