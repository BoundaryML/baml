//! Raw runtime bindings.

use std::collections::HashMap;

use wasm_bindgen::prelude::*;

#[wasm_bindgen(js_name = stageRuntimeBytecode)]
#[allow(clippy::needless_pass_by_value)] // wasm-bindgen requires an owned optional string at the JS boundary.
pub fn stage_runtime_bytecode(
    bytecode: &[u8],
    embedded_baml_toml: Option<String>,
    key: Option<u64>,
) -> Result<u64, JsValue> {
    let sys_ops = sys_wasm::build()
        .map_err(|error| crate::errors::setup_error(crate::errors::CLIENT, error))?;
    let result = match key {
        Some(key) => bridge_cffi::register_runtime_from_bytecode_with_sys_ops(
            key,
            bytecode,
            embedded_baml_toml.as_deref(),
            sys_ops,
        ),
        None => bridge_cffi::initialize_runtime_from_bytecode_with_sys_ops(
            bytecode,
            embedded_baml_toml.as_deref(),
            sys_ops,
        ),
    };
    result
        .and_then(|runtime| bridge_cffi::runtime_key(&runtime))
        .map_err(|error| crate::errors::bridge_error(&error))
}

#[wasm_bindgen(js_name = stageRuntimeSources)]
pub fn stage_runtime_sources(root_path: &str, files: JsValue) -> Result<u64, JsValue> {
    let files: HashMap<String, String> =
        serde_wasm_bindgen::from_value(files).map_err(|error| {
            crate::errors::setup_error(
                crate::errors::INVALID_ARGUMENT,
                format!("source files must be a record of string contents: {error}"),
            )
        })?;
    let sys_ops = sys_wasm::build()
        .map_err(|error| crate::errors::setup_error(crate::errors::CLIENT, error))?;
    bridge_cffi::initialize_runtime_from_files_with_sys_ops(root_path, files, sys_ops)
        .and_then(|runtime| bridge_cffi::runtime_key(&runtime))
        .map_err(|error| crate::errors::bridge_error(&error))
}

#[wasm_bindgen(js_name = callFunctionSync)]
pub fn call_function_sync(key: u64, encoded_args: &[u8]) -> Vec<u8> {
    sys_wasm::with_web_sync_mode(|| {
        futures::executor::block_on(bridge_cffi::call_function_in_wasm_for_runtime(
            Some(key),
            encoded_args,
        ))
    })
}

#[wasm_bindgen(js_name = callFunction)]
pub async fn call_function(key: u64, encoded_args: &[u8]) -> Vec<u8> {
    bridge_cffi::call_function_in_wasm_for_runtime(Some(key), encoded_args).await
}

#[wasm_bindgen(js_name = unregisterRuntime)]
pub fn unregister_runtime(key: u64) -> Result<(), JsValue> {
    bridge_cffi::unregister_runtime(key)
        .map(|_| ())
        .map_err(|e| crate::errors::bridge_error(&e))
}
