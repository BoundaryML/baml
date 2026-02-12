//! `bridge_wasm` - WASM bindings for BAML using `bex_engine`.
//!
//! This crate only supports the `wasm32-unknown-unknown` target. Use
//! `--target wasm32-unknown-unknown` when building.
//!
//! This crate provides WebAssembly bindings for BAML, allowing it to run in
//! browsers and Node.js. It uses the same protobuf protocol as `bridge_cffi`
//! for function arguments and results.
//!
//! # Usage
//!
//! ```javascript
//! import init, { BamlWasmRuntime } from 'bridge_wasm';
//!
//! // Initialize the WASM module
//! await init();
//!
//! // Create a runtime with source files and HTTP callback
//! const runtime = BamlWasmRuntime.create(
//!     '/project',
//!     JSON.stringify({ 'main.baml': 'function Greet(name: string) -> string { ... }' }),
//!     async (method, url, headers, body) => {
//!         const response = await fetch(url, { method, headers: JSON.parse(headers), body });
//!         return {
//!             status: response.status,
//!             headersJson: JSON.stringify(Object.fromEntries(response.headers)),
//!             url: response.url,
//!             bodyPromise: response.text(),  // body is read when .text() is called in BAML
//!         };
//!     }
//! );
//!
//! // Call a function (protobuf in/out)
//! const result = await runtime.callFunction('Greet', argsProtoBytes);
//! ```

mod error;
mod registry;
mod send_wrapper;
mod wasm_http;

use std::collections::HashMap;

use bex_factory::BexIncremental;
pub use bridge_ctypes::{baml, external_to_cffi_value, kwargs_to_bex_values};
pub use error::BridgeError;
use js_sys::Function;
use prost::Message;
use wasm_bindgen::prelude::*;

/// Initialize the WASM module with panic hook (auto-called by wasm-bindgen).
#[wasm_bindgen(start)]
pub fn start() {
    #[cfg(feature = "console_error_panic")]
    console_error_panic_hook::set_once();
}

/// Get the version of the `bridge_wasm` crate.
#[wasm_bindgen]
pub fn version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

/// Returns a test string for hot-reload testing (see app-vscode-webview hot-reload.hmr.test.ts).
#[wasm_bindgen(js_name = hotReloadTestString)]
pub fn hot_reload_test_string() -> String {
    // BEGIN_VITE_HOT_RELOAD_TEST
    "injected for hot reload test, see hot-reload.hmr.test.ts".to_string()
    // END_VITE_HOT_RELOAD_TEST
}

// ============================================================================
// TypeScript type declarations (injected into the generated .d.ts)
// ============================================================================

#[wasm_bindgen(typescript_custom_section)]
const TS_FETCH_TYPES: &str = r#"
export type WasmFetchCallback = (
  method: string,
  url: string,
  headersJson: string,
  body: string,
) => Promise<{ status: number; headersJson: string; url: string; bodyPromise: Promise<string> }>;
"#;

// Typed `create` declaration (the auto-generated one is suppressed via skip_typescript).
#[wasm_bindgen(typescript_custom_section)]
const TS_CREATE_METHOD: &str = r#"
export namespace BamlWasmRuntime {
  function create(root_path: string, src_files_json: string, fetch_fn: WasmFetchCallback): BamlWasmRuntime;
}
"#;

/// A BAML runtime for WASM environments.
///
/// Each instance compiles BAML source files and can execute functions.
/// HTTP requests are performed via a JS callback provided at creation time.
#[wasm_bindgen]
pub struct BamlWasmRuntime {
    bex: Box<dyn BexIncremental>,
}

#[wasm_bindgen]
impl BamlWasmRuntime {
    /// Create a new BAML runtime.
    ///
    /// # Arguments
    ///
    /// * `root_path` - Root path for BAML files (e.g., "/project")
    /// * `src_files_json` - JSON object mapping filenames to content
    ///   e.g., `{"main.baml": "function Greet(name: string) -> string { ... }"}`
    /// * `fetch_fn` - JS function for HTTP requests (see `WasmFetchCallback` type).
    #[wasm_bindgen(skip_typescript)]
    pub fn create(
        root_path: &str,
        src_files_json: &str,
        fetch_fn: Function,
    ) -> Result<BamlWasmRuntime, JsError> {
        // Parse source files
        let src_files: HashMap<String, String> = serde_json::from_str(src_files_json)
            .map_err(|e| JsError::new(&format!("Failed to parse src_files_json: {e}")))?;

        // Build SysOps with WASM HTTP implementation (each runtime gets its own WasmHttp holding fetch_fn)
        let sys_ops = sys_types::SysOpsBuilder::new()
            .with_http_instance(std::sync::Arc::new(wasm_http::WasmHttp::new(fetch_fn)))
            .build();

        // Create the engine via factory
        let bex = bex_factory::new_incremental(root_path, &src_files, sys_ops);

        Ok(BamlWasmRuntime { bex })
    }

    /// Call a BAML function.
    ///
    /// # Arguments
    ///
    /// * `name` - The function name to call
    /// * `args_proto` - Protobuf-encoded `HostFunctionArguments`
    ///
    /// # Returns
    ///
    /// Protobuf-encoded `CffiValueHolder` containing the result.
    #[wasm_bindgen(js_name = callFunction)]
    pub async fn call_function(&self, name: &str, args_proto: &[u8]) -> Result<Vec<u8>, JsError> {
        // Decode protobuf arguments
        let args = baml::cffi::HostFunctionArguments::decode(args_proto)
            .map_err(|e| JsError::new(&format!("Failed to decode arguments: {e}")))?;

        // Convert kwargs to BexExternalValue
        let kwargs = kwargs_to_bex_values(args.kwargs)
            .map_err(|e| JsError::new(&format!("Failed to convert arguments: {e}")))?;

        // Call the function (Bex trait)
        let result: bex_factory::BexExternalValue = self
            .bex
            .call_function(name, kwargs.into())
            .await
            .map_err(|e| JsError::new(&format!("Function call failed: {e}")))?;

        // Encode result as protobuf
        let cffi_value = external_to_cffi_value(&result)
            .map_err(|e| JsError::new(&format!("Failed to encode result: {e}")))?;

        Ok(cffi_value.encode_to_vec())
    }

    /// Add a source file to the runtime.
    ///
    /// # Arguments
    ///
    /// * `path` - The path to the source file
    /// * `content` - The content of the source file
    #[wasm_bindgen(js_name = addSource)]
    pub fn add_source(&mut self, path: &str, content: &str) -> Result<(), JsError> {
        let result = self.bex.add_source(path, content);
        if result.engine_updated {
            Ok(())
        } else {
            Err(JsError::new(&result.diagnostics))
        }
    }

    /// Set the main file content (convenience for single-file). Equivalent to `addSource("main.baml", content)`.
    #[wasm_bindgen(js_name = setSource)]
    pub fn set_source(&mut self, content: &str) -> Result<(), JsError> {
        self.add_source("main.baml", content)
    }

    /// Return the names of all functions defined in the current project.
    #[wasm_bindgen(js_name = functionNames)]
    pub fn function_names(&self) -> Vec<String> {
        self.bex.function_names()
    }
}
