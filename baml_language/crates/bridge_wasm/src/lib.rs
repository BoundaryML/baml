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
//! // Create a runtime with source files and callbacks object
//! const runtime = BamlWasmRuntime.create(
//!     '/project',
//!     JSON.stringify({ 'main.baml': 'function Greet(name: string) -> string { ... }' }),
//!     {
//!         fetch: async (method, url, headers, body) => {
//!             const response = await fetch(url, { method, headers: JSON.parse(headers), body });
//!             return {
//!                 status: response.status,
//!                 headersJson: JSON.stringify(Object.fromEntries(response.headers)),
//!                 url: response.url,
//!                 bodyPromise: response.text(),  // body is read when .text() is called in BAML
//!             };
//!         },
//!         env: (variable) => process.env[variable],  // or (variable) => undefined to disable env lookups
//!     }
//! );
//!
//! // Call a function (protobuf in/out)
//! const result = await runtime.callFunction('Greet', argsProtoBytes);
//!
//! // Call a function with cancellation support
//! const callId = 42; // caller-provided unique ID
//! const resultPromise = runtime.callFunction('Greet', argsProtoBytes, callId);
//! // Cancel from another microtask:
//! runtime.cancelCall(callId);
//! ```

mod error;
mod registry;
mod send_wrapper;
mod wasm_env;
mod wasm_http;

use std::{cell::RefCell, collections::HashMap};

use bex_factory::{BexIncremental, CancellationToken};
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

export type WasmEnvVarsCallback = (variable: string) => string | undefined;
"#;

#[wasm_bindgen]
extern "C" {
    /// Callback bundle passed to [`BamlWasmRuntime::create`].
    ///
    /// From JS, pass a plain object: `{ fetch: ..., env: ... }`.
    #[wasm_bindgen(typescript_type = "{ fetch: WasmFetchCallback; env: WasmEnvVarsCallback }")]
    pub type WasmCallbacks;

    #[wasm_bindgen(method, getter, structural)]
    fn fetch(this: &WasmCallbacks) -> Function;

    #[wasm_bindgen(method, getter, structural, js_name = "env")]
    fn env(this: &WasmCallbacks) -> Function;
}

/// A BAML runtime for WASM environments.
///
/// Each instance compiles BAML source files and can execute functions.
/// HTTP requests are performed via a JS callback provided at creation time.
#[wasm_bindgen]
pub struct BamlWasmRuntime {
    bex: Box<dyn BexIncremental>,
    /// Active calls keyed by caller-provided ID. WASM is single-threaded so `RefCell` suffices.
    active_calls: RefCell<HashMap<u32, CancellationToken>>,
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
    /// * `callbacks` - Object containing callback functions (see `WasmCallbacks` interface).
    pub fn create(
        root_path: &str,
        src_files_json: &str,
        callbacks: &WasmCallbacks,
    ) -> Result<BamlWasmRuntime, JsError> {
        let fetch_fn = callbacks.fetch();
        let env_vars_fn = callbacks.env();

        // Parse source files
        let src_files: HashMap<String, String> = serde_json::from_str(src_files_json)
            .map_err(|e| JsError::new(&format!("Failed to parse src_files_json: {e}")))?;

        // Build SysOps with WASM HTTP and env implementations
        let sys_ops = sys_types::SysOpsBuilder::new()
            .with_http_instance(std::sync::Arc::new(wasm_http::WasmHttp::new(fetch_fn)))
            .with_env_instance(std::sync::Arc::new(wasm_env::WasmEnv::new(env_vars_fn)))
            .build();

        // Create the engine via factory
        let bex = bex_factory::new_incremental(root_path, &src_files, sys_ops);

        Ok(BamlWasmRuntime {
            bex,
            active_calls: RefCell::new(HashMap::new()),
        })
    }

    /// Call a BAML function.
    ///
    /// # Arguments
    ///
    /// * `name` - The function name to call
    /// * `args_proto` - Protobuf-encoded `HostFunctionArguments`
    /// * `call_id` - Optional caller-provided ID for cancellation. If provided,
    ///   the call can be cancelled via `cancelCall(callId)`.
    ///
    /// # Returns
    ///
    /// Protobuf-encoded `CffiValueHolder` containing the result.
    #[wasm_bindgen(js_name = callFunction)]
    pub async fn call_function(
        &self,
        name: &str,
        args_proto: &[u8],
        call_id: Option<u32>,
    ) -> Result<Vec<u8>, JsValue> {
        // Decode protobuf arguments
        let args = baml::cffi::HostFunctionArguments::decode(args_proto)
            .map_err(|e| JsError::new(&format!("Failed to decode arguments: {e}")))?;

        // Convert kwargs to BexExternalValue
        let kwargs = kwargs_to_bex_values(args.kwargs)
            .map_err(|e| JsError::new(&format!("Failed to convert arguments: {e}")))?;

        // Create cancellation token and register if call_id is provided.
        let cancel = CancellationToken::new();
        if let Some(id) = call_id {
            let mut calls = self.active_calls.borrow_mut();
            if calls.contains_key(&id) {
                return Err(JsError::new(&format!(
                    "call_id {id} is already in use by an active call"
                ))
                .into());
            }
            calls.insert(id, cancel.clone());
        }

        // Call the function (Bex trait)
        let result = self.bex.call_function(name, kwargs.into(), cancel).await;

        // Unregister from active calls.
        if let Some(id) = call_id {
            self.active_calls.borrow_mut().remove(&id);
        }

        let result = result.map_err(|e| -> JsValue {
            if matches!(
                e,
                bex_factory::RuntimeError::Engine(bex_factory::EngineError::Cancelled)
            ) {
                let err = js_sys::Error::new("Operation cancelled");
                err.set_name("BamlCancelledError");
                err.into()
            } else {
                JsError::new(&format!("Function call failed: {e}")).into()
            }
        })?;

        // Encode result as protobuf
        let cffi_value = external_to_cffi_value(&result)
            .map_err(|e| JsError::new(&format!("Failed to encode result: {e}")))?;

        Ok(cffi_value.encode_to_vec())
    }

    /// Cancel an in-flight function call by its ID.
    ///
    /// If the call is still running, it will be interrupted at the next
    /// cancellation check point. If the call has already completed or the ID
    /// is unknown, this is a no-op.
    #[wasm_bindgen(js_name = cancelCall)]
    pub fn cancel_call(&self, call_id: u32) {
        if let Some(token) = self.active_calls.borrow_mut().remove(&call_id) {
            token.cancel();
        }
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
