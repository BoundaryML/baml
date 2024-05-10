#[cfg(not(feature = "wasm"))]
mod no_wasm;
#[cfg(feature = "wasm")]
mod wasm;

#[cfg(feature = "wasm")]
pub(crate) type RequestError = wasm::WasmRequestError;
#[cfg(feature = "wasm")]
pub(crate) use wasm::{call_request_with_json, response_json, response_text};

#[cfg(not(feature = "wasm"))]
pub(crate) type RequestError = no_wasm::NoWasmRequestError;
#[cfg(not(feature = "wasm"))]
pub(crate) use no_wasm::{call_request_with_json, response_json, response_text};
