//! `bridge_wasm` - WASM bindings for BAML using `bex_engine`.
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
//!     JSON.stringify({ 'OPENAI_API_KEY': 'sk-...' }),
//!     async (method, url, headers, body) => {
//!         const response = await fetch(url, { method, headers: JSON.parse(headers), body });
//!         return {
//!             status: response.status,
//!             headers: JSON.stringify(Object.fromEntries(response.headers)),
//!             url: response.url,
//!             body: await response.text(),
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

use bex_factory::BexFactory;
pub use bridge_ctypes::{baml, external_to_cffi_value, kwargs_to_bex_values};
pub use error::BridgeError;
use js_sys::Function;
use prost::Message;
use sys_types::SysOpsBuilder;
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

/// A BAML runtime for WASM environments.
///
/// Each instance compiles BAML source files and can execute functions.
/// HTTP requests are performed via a JS callback provided at creation time.
#[wasm_bindgen]
pub struct BamlWasmRuntime {
    factory: BexFactory,
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
    /// * `env_vars_json` - JSON object of environment variables
    ///   e.g., `{"OPENAI_API_KEY": "sk-..."}`
    /// * `fetch_fn` - JS function for HTTP requests with signature:
    ///   `(method: string, url: string, headersJson: string, body: string)
    ///    => Promise<{status: number, headers: string, url: string, body: string}>`
    #[wasm_bindgen]
    pub fn create(
        root_path: &str,
        src_files_json: &str,
        env_vars_json: &str,
        fetch_fn: Function,
    ) -> Result<BamlWasmRuntime, JsError> {
        // Initialize HTTP provider
        wasm_http::init_http_provider(fetch_fn)
            .map_err(|e| JsError::new(&format!("Failed to init HTTP provider: {e}")))?;

        // Parse source files
        let src_files: HashMap<String, String> = serde_json::from_str(src_files_json)
            .map_err(|e| JsError::new(&format!("Failed to parse src_files_json: {e}")))?;

        // Parse environment variables
        let env_vars: HashMap<String, String> = serde_json::from_str(env_vars_json)
            .map_err(|e| JsError::new(&format!("Failed to parse env_vars_json: {e}")))?;

        // Build SysOps with WASM HTTP implementation
        let sys_ops = SysOpsBuilder::new()
            .with_http::<wasm_http::WasmHttp>()
            .build();

        // Create the factory
        let factory = BexFactory::new(root_path, &src_files, env_vars, sys_ops)
            .map_err(|e| JsError::new(&format!("Failed to create runtime: {e}")))?;

        Ok(BamlWasmRuntime { factory })
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

        // Look up function parameters to get parameter order
        let params = self
            .factory
            .function_params(name)
            .ok_or_else(|| JsError::new(&format!("Function not found: {name}")))?;

        // Reorder kwargs to match function parameter declaration order
        let bex_args: Vec<bex_factory::BexExternalValue> = params
            .iter()
            .map(|(param_name, _param_type)| {
                kwargs.get(*param_name).cloned().ok_or_else(|| {
                    JsError::new(&format!(
                        "Missing argument '{param_name}' for function '{name}'"
                    ))
                })
            })
            .collect::<Result<Vec<_>, _>>()?;

        // Call the function
        let result = self
            .factory
            .call_function(name, bex_args)
            .await
            .map_err(|e| JsError::new(&format!("Function call failed: {e}")))?;

        // Encode result as protobuf
        let cffi_value = external_to_cffi_value(&result)
            .map_err(|e| JsError::new(&format!("Failed to encode result: {e}")))?;

        Ok(cffi_value.encode_to_vec())
    }

    /// Get the parameter names for a function (for introspection).
    ///
    /// # Arguments
    ///
    /// * `name` - The function name
    ///
    /// # Returns
    ///
    /// JSON array of parameter names, or `null` if function not found.
    #[wasm_bindgen(js_name = functionParams)]
    pub fn function_params(&self, name: &str) -> Option<String> {
        let params = self.factory.function_params(name)?;
        let names: Vec<&str> = params.iter().map(|(name, _)| *name).collect();
        serde_json::to_string(&names).ok()
    }
}
