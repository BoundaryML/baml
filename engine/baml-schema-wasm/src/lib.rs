pub mod runtime_wasm;
use std::env;
use wasm_bindgen::prelude::*;

#[wasm_bindgen]
pub fn version() -> String {
    // register_panic_hook();
    env!("CARGO_PKG_VERSION").to_string()
}
