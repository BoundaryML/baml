//! Raw runtime bindings.

use std::collections::HashMap;

use wasm_bindgen::prelude::*;

/// Stage a runtime from precompiled BAML bytecode: a raw artifact
/// (`Uint8Array`), or the encoded string generated SDKs embed (decoded
/// natively, never in JavaScript).
#[wasm_bindgen(js_name = stageRuntimeBytecode)]
#[allow(clippy::needless_pass_by_value)] // wasm-bindgen requires an owned optional string at the JS boundary.
pub fn stage_runtime_bytecode(
    bytecode: JsValue,
    embedded_baml_toml: Option<String>,
) -> Result<(), JsValue> {
    let bytecode = match bytecode.as_string() {
        Some(encoded) => encoded.into_bytes(),
        None => js_sys::Uint8Array::new(&bytecode).to_vec(),
    };
    let sys_ops = sys_wasm::build()
        .map_err(|error| crate::errors::setup_error(crate::errors::CLIENT, error))?;
    bridge_cffi::stage_runtime_from_blob_with_sys_ops(
        &bytecode,
        embedded_baml_toml.as_deref(),
        sys_ops,
    )
    .map_err(|error| crate::errors::bridge_error(&error))
}

#[wasm_bindgen(js_name = stageRuntimeSources)]
pub fn stage_runtime_sources(root_path: &str, files: JsValue) -> Result<(), JsValue> {
    let files: HashMap<String, String> =
        serde_wasm_bindgen::from_value(files).map_err(|error| {
            crate::errors::setup_error(
                crate::errors::INVALID_ARGUMENT,
                format!("source files must be a record of string contents: {error}"),
            )
        })?;
    let sys_ops = sys_wasm::build()
        .map_err(|error| crate::errors::setup_error(crate::errors::CLIENT, error))?;
    bridge_cffi::stage_runtime_from_files_with_sys_ops(root_path, files, sys_ops)
        .map_err(|error| crate::errors::bridge_error(&error))
}

/// Finalize the SDK lifetime: drain in-flight calls and spawned work, then
/// release the runtime. Repeated calls are harmless; BAML calls made
/// afterwards fail.
#[wasm_bindgen(js_name = shutdownRuntime)]
pub async fn shutdown_runtime() -> Result<(), JsValue> {
    bridge_cffi::shutdown_runtime(None)
        .await
        .map_err(|error| crate::errors::bridge_error(&error))
}

/// Call a BAML function synchronously. `encoded_args` carries the call's
/// control part with no `kwargs`; `kwargs` are the arguments by declared
/// parameter name as JavaScript values, encoded here against the target's
/// declared types, and `cancel` the `$baml.cancel` token. A value that does
/// not fit raises before anything is sent.
#[wasm_bindgen(js_name = callFunctionSync)]
pub fn call_function_sync(
    encoded_args: &[u8],
    kwargs: &JsValue,
    cancel: &JsValue,
) -> Result<Vec<u8>, JsValue> {
    let encoded_args = crate::encode::encode_request(encoded_args, kwargs, cancel, true)?;
    Ok(sys_wasm::with_web_sync_mode(|| {
        bridge_cffi::call_function_in_wasm_sync(&encoded_args)
    }))
}

/// Call a BAML function asynchronously. Arguments as for
/// [`call_function_sync`]; they are encoded while the caller's frame is still
/// running, before the promise exists, so a value the caller mutates after
/// the call is made is never what crosses (an `async fn` export would run
/// its body in a microtask, after the caller's next statements). A value that
/// does not fit throws synchronously, as on Node.
#[wasm_bindgen(js_name = callFunction)]
pub fn call_function(
    encoded_args: Vec<u8>,
    kwargs: JsValue,
    cancel: JsValue,
) -> Result<js_sys::Promise, JsValue> {
    let encoded_args = crate::encode::encode_request(&encoded_args, &kwargs, &cancel, false)?;
    Ok(wasm_bindgen_futures::future_to_promise(async move {
        let bytes = bridge_cffi::call_function_in_wasm(&encoded_args).await;
        Ok(js_sys::Uint8Array::from(bytes.as_slice()).into())
    }))
}
