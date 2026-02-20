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
//!         env: (variable) => process.env[variable],  // may return Promise<string | undefined> for async lookups
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
mod wasm_fs;
mod wasm_http;
mod wasm_lsp;
mod wasm_playground;

use std::{cell::RefCell, collections::HashMap};

pub use bridge_ctypes::{baml, external_to_cffi_value, kwargs_to_bex_values};
pub use error::BridgeError;
use js_sys::Function;
use prost::Message;
use sys_types::CancellationToken;
use wasm_bindgen::prelude::*;

static LOGGER_INIT: std::sync::Once = std::sync::Once::new();

/// Initialize the WASM module with panic hook (auto-called by wasm-bindgen).
#[wasm_bindgen(start)]
pub fn start() {
    #[cfg(feature = "console_error_panic")]
    console_error_panic_hook::set_once();
    LOGGER_INIT.call_once(|| {
        wasm_logger::init(wasm_logger::Config::new(log::Level::Info));
    });
}

/// Get the version of the `bridge_wasm` crate.
#[wasm_bindgen]
pub fn version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

/// Returns the build timestamp (unix seconds) for hot-reload / build-identity checks.
#[wasm_bindgen(js_name = getBuildTime)]
pub fn get_build_time() -> String {
    env!("BRIDGE_WASM_BUILD_TS").to_string()
}

// ============================================================================
// TypeScript type declarations (injected into the generated .d.ts)
// ============================================================================

#[wasm_bindgen(typescript_custom_section)]
const TS_FETCH_TYPES: &str = r#"
export type WasmFetchCallback = (
  callId: number,
  method: string,
  url: string,
  headersJson: string,
  body: string,
) => Promise<{ status: number; headersJson: string; url: string; bodyPromise: Promise<string> }>;

export type WasmEnvVarsCallback = (variable: string) => Promise<string | undefined> | string | undefined;

export type WasmSendNotificationCallback = (notification: LspNotification) => void;
export type WasmSendResponseCallback = (response: LspResponse) => void;
export type WasmMakeRequestCallback = (request: LspRequest) => void;
export type WasmPlaygroundNotificationCallback = (notification: PlaygroundNotification) => void;
"#;

#[wasm_bindgen]
extern "C" {
    /// Callback bundle passed to [`BamlWasmRuntime::create`].
    ///
    /// From JS, pass a plain object: `{ fetch: ..., env: ... }`.
    #[wasm_bindgen(typescript_type = r#"{
        fetch: WasmFetchCallback;
        env: WasmEnvVarsCallback;
        lsp_send_notification: WasmSendNotificationCallback;
        lsp_send_response: WasmSendResponseCallback;
        lsp_make_request: WasmMakeRequestCallback;
        playground_send_notification: WasmPlaygroundNotificationCallback
}"#)]
    pub type WasmCallbacks;

    #[wasm_bindgen(method, getter, structural)]
    fn fetch(this: &WasmCallbacks) -> Function;

    #[wasm_bindgen(method, getter, structural, js_name = "env")]
    fn env(this: &WasmCallbacks) -> Function;

    #[wasm_bindgen(method, getter, structural, js_name = "lsp_send_notification")]
    fn send_notification(this: &WasmCallbacks) -> Function;

    #[wasm_bindgen(method, getter, structural, js_name = "lsp_send_response")]
    fn send_response(this: &WasmCallbacks) -> Function;

    #[wasm_bindgen(method, getter, structural, js_name = "lsp_make_request")]
    fn make_request(this: &WasmCallbacks) -> Function;

    #[wasm_bindgen(method, getter, structural, js_name = "playground_send_notification")]
    fn playground_send_notification(this: &WasmCallbacks) -> Function;
}

/// A BAML runtime for WASM environments.
///
/// Each instance compiles BAML source files and can execute functions.
/// HTTP requests are performed via a JS callback provided at creation time.
#[wasm_bindgen]
pub struct BamlWasmRuntime {
    bex: Box<dyn bex_project::BexLsp>,
    /// Active calls keyed by caller-provided ID. WASM is single-threaded so `RefCell` suffices.
    active_calls: RefCell<HashMap<u32, CancellationToken>>,
}

// SAFETY: wasm32-unknown-unknown is single-threaded, so unwind safety is
// trivially satisfied — there is no concurrent observer of partially-unwound state.
impl std::panic::UnwindSafe for BamlWasmRuntime {}
impl std::panic::RefUnwindSafe for BamlWasmRuntime {}

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
        callbacks: &WasmCallbacks,
        wasm_vfs: wasm_fs::WasmVfs,
    ) -> Result<BamlWasmRuntime, JsError> {
        let fetch_fn = callbacks.fetch();
        let env_vars_fn = callbacks.env();
        let send_notification_fn = callbacks.send_notification();
        let send_response_fn = callbacks.send_response();
        let make_request_fn = callbacks.make_request();
        let playground_send_notification_fn = callbacks.playground_send_notification();

        let sys_ops = sys_types::SysOpsBuilder::new()
            .with_http_instance(std::sync::Arc::new(wasm_http::WasmHttp::new(fetch_fn)))
            .with_env_instance(std::sync::Arc::new(wasm_env::WasmEnv::new(env_vars_fn)))
            .build();
        let sys_ops = std::sync::Arc::new(sys_ops);
        let sys_op_factory = std::sync::Arc::new(move |_path: &vfs::VfsPath| sys_ops.clone());

        let lsp = wasm_lsp::WasmLsp::new(send_notification_fn, send_response_fn, make_request_fn);
        let playground =
            wasm_playground::WasmPlaygroundSender::new(playground_send_notification_fn);

        let vfs = wasm_fs::WasmFs::new(wasm_vfs);
        let vfs = std::sync::Arc::new(vfs);

        let bex = bex_project::new_lsp(
            sys_op_factory,
            std::sync::Arc::new(lsp),
            std::sync::Arc::new(playground),
            bex_project::BamlVFS::new(vfs),
        );

        Ok(BamlWasmRuntime {
            bex: Box::new(bex),
            active_calls: RefCell::new(HashMap::new()),
        })
    }

    /// Call a BAML function for a specific project.
    ///
    /// # Arguments
    ///
    /// * `id` - Unique call identifier
    /// * `project` - Project root path (e.g. `"/workspace/baml_src"`)
    /// * `name` - The function name to call
    /// * `args_proto` - Protobuf-encoded `HostFunctionArguments`
    ///
    /// # Returns
    ///
    /// Protobuf-encoded `CffiValueHolder` containing the result.
    #[wasm_bindgen(js_name = callFunction)]
    pub async fn call_function(
        &self,
        id: u32,
        project: String,
        name: &str,
        args_proto: &[u8],
    ) -> Result<Vec<u8>, JsValue> {
        // Decode protobuf arguments
        let args = baml::cffi::HostFunctionArguments::decode(args_proto)
            .map_err(|e| JsError::new(&format!("Failed to decode arguments: {e}")))?;

        let kwargs = kwargs_to_bex_values(args.kwargs)
            .map_err(|e| JsError::new(&format!("Failed to convert arguments: {e}")))?;

        let call_id = sys_types::CallId(u64::from(id));
        let fs_path = bex_project::FsPath::from_str(project);

        // Create cancellation token and register.
        let cancel = CancellationToken::new();
        if self.active_calls.borrow().contains_key(&id) {
            return Err(JsError::new("Call ID already in use").into());
        }
        self.active_calls.borrow_mut().insert(id, cancel.clone());

        // Call the function.
        let result = self
            .bex
            .call_function_for_project(&fs_path, name, kwargs.into(), call_id, cancel)
            .await;

        // Unregister from active calls.
        self.active_calls.borrow_mut().remove(&id);

        // Handle cancellation error.
        let result = result.map_err(|e| -> JsValue {
            if matches!(
                e,
                bex_project::RuntimeError::Engine(bex_engine::EngineError::Cancelled)
            ) {
                let err = js_sys::Error::new("Operation cancelled");
                err.set_name("BamlCancelledError");
                err.into()
            } else {
                JsError::new(&format!("Function call failed: {e}")).into()
            }
        })?;

        let cffi_value = external_to_cffi_value(&result)
            .map_err(|e| JsError::new(&format!("Failed to encode result: {e}")))?;

        Ok(cffi_value.encode_to_vec())
    }

    /// Handle an LSP notification.
    #[wasm_bindgen(js_name = handleLspNotification)]
    pub fn handle_notification(&self, notification: wasm_lsp::LspNotification) {
        self.bex.handle_notification(notification.into());
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

    /// Handle an LSP request.
    #[wasm_bindgen(js_name = handleLspRequest)]
    pub fn handle_request(&self, request: wasm_lsp::LspRequest) {
        self.bex.handle_request(request.into());
    }

    /// Request the current playground state.
    ///
    /// Triggers `playground_send_notification` callbacks with the current
    /// list of projects and each project's state.
    #[wasm_bindgen(js_name = requestPlaygroundState)]
    pub fn request_playground_state(&self) {
        self.bex.request_playground_state();
    }
}
