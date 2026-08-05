//! WASM host for the same protobuf tooling.v1 endpoint as napi.

use std::cell::RefCell;

use baml_tooling_api::ToolingProtocol;
use wasm_bindgen::prelude::*;

thread_local! {
    static PROTOCOL: RefCell<ToolingProtocol> = RefCell::new(ToolingProtocol::default());
}

/// Dispatch an encoded `baml.tooling.v1.ToolingRequest`.
///
/// Browser callers provide the virtual filesystem by including complete file
/// snapshots in `ProjectOpen` and `ProjectUpdate`; the compiler never reads or
/// parses source in TypeScript.
#[wasm_bindgen(js_name = toolingRequest)]
pub fn tooling_request(request: &[u8]) -> Vec<u8> {
    PROTOCOL.with(|protocol| protocol.borrow_mut().dispatch(request))
}
