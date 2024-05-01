use std::path::PathBuf;

use baml_runtime::BamlRuntime;
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::JsFuture;
use web_sys::{Request, RequestInit, RequestMode, Response};

use baml_runtime::InternalRuntimeInterface;

#[wasm_bindgen]
pub struct WasmRuntime {
    runtime: BamlRuntime,
}

#[wasm_bindgen]
pub fn create_runtime(path: String) -> Result<WasmRuntime, String> {
    let path = PathBuf::from(path);
    match baml_runtime::BamlRuntime::from_directory(&path) {
        Ok(runtime) => Ok(WasmRuntime { runtime }),
        Err(e) => Err(e.to_string()),
    }
}

#[wasm_bindgen(getter_with_clone)]
pub struct BamlFunction {
    pub name: String,
}

#[wasm_bindgen]
pub fn list_functions(runtime: &WasmRuntime) -> Vec<BamlFunction> {
    runtime
        .runtime
        .internal()
        .ir()
        .walk_functions()
        .map(|f| BamlFunction {
            name: f.name().to_string(),
        })
        .collect()
}
