//! Error types for `bridge_wasm`.

use thiserror::Error;
use wasm_bindgen::JsCast;

pub(crate) fn js_error_message(error: &wasm_bindgen::JsValue) -> String {
    error
        .as_string()
        .or_else(|| {
            error
                .dyn_ref::<js_sys::Error>()
                .map(|error| String::from(error.message()))
        })
        .unwrap_or_else(|| format!("{error:?}"))
}

/// Errors that can occur during bridge operations.
#[derive(Debug, Error)]
pub enum BridgeError {
    #[error("{0}")]
    Runtime(#[from] bex_project::RuntimeError),

    #[error("Protobuf decode error: {0}")]
    ProtobufDecode(#[from] prost::DecodeError),

    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("JavaScript error: {0}")]
    JsError(String),
}

impl From<wasm_bindgen::JsValue> for BridgeError {
    fn from(error: wasm_bindgen::JsValue) -> Self {
        BridgeError::JsError(js_error_message(&error))
    }
}
