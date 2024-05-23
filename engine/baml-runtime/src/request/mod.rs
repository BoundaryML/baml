#[cfg(not(target_arch = "wasm32"))]
mod no_wasm;
#[cfg(not(target_arch = "wasm32"))]
pub(crate) type RequestError = no_wasm::NoWasmRequestError;
#[cfg(not(target_arch = "wasm32"))]
pub(crate) use no_wasm::{call_request_with_json, response_json, response_text};

#[cfg(target_arch = "wasm32")]
mod wasm;
#[cfg(target_arch = "wasm32")]
pub(crate) type RequestError = wasm::WasmRequestError;
#[cfg(target_arch = "wasm32")]
pub(crate) use wasm::{call_request_with_json, response_json, response_text};
