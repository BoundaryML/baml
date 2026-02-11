//! Error types for `bridge_wasm`.

use thiserror::Error;
use wasm_bindgen::JsCast;

/// Errors that can occur during bridge operations.
#[derive(Debug, Error)]
pub enum BridgeError {
    #[error("{0}")]
    Runtime(#[from] bex_factory::RuntimeError),

    #[error("Protobuf decode error: {0}")]
    ProtobufDecode(#[from] prost::DecodeError),

    #[error("Null buffer pointer")]
    NullBuffer,

    #[error("Handle values not supported")]
    HandleNotSupported,

    #[error("Map entry missing key")]
    MapEntryMissingKey,

    #[error("Function not found: {name}")]
    FunctionNotFound { name: String },

    #[error("Missing argument '{parameter}' for function '{function}'")]
    MissingArgument { function: String, parameter: String },

    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("JavaScript error: {0}")]
    JsError(String),
}

impl From<wasm_bindgen::JsValue> for BridgeError {
    fn from(e: wasm_bindgen::JsValue) -> Self {
        let msg = if let Some(s) = e.as_string() {
            s
        } else if let Some(err) = e.dyn_ref::<js_sys::Error>() {
            String::from(err.message())
        } else {
            format!("{e:?}")
        };
        BridgeError::JsError(msg)
    }
}
