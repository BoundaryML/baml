#[cfg(not(target = "wasm32"))]
mod no_wasm;
#[cfg(not(target = "wasm32"))]
pub(crate) type RequestError = no_wasm::NoWasmRequestError;
#[cfg(not(target = "wasm32"))]
pub(crate) use no_wasm::{call_request_with_json, response_json, response_text};

#[cfg(target = "wasm32")]
mod wasm;
#[cfg(target = "wasm32")]
pub(crate) type RequestError = wasm::WasmRequestError;
#[cfg(target = "wasm32")]
pub(crate) use wasm::{call_request_with_json, response_json, response_text};
