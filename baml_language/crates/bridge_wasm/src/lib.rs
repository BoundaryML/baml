//! `bridge_wasm` - WASM bindings for BAML using `bex_project`.
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
mod handle;
mod registry;
mod send_wrapper;
mod wasm_env;
mod wasm_fs;
mod wasm_http;
mod wasm_io;
mod wasm_io_fs;
mod wasm_io_glob;
mod wasm_lsp;
mod wasm_playground;
mod wasm_sys;

pub use bridge_ctypes::{HANDLE_TABLE, baml_core, external_to_outbound, kwargs_to_bex_values};
pub use error::BridgeError;
use js_sys::Function;
use prost::Message;
use wasm_bindgen::prelude::*;
pub use wasm_lsp::LspNotification;

static LOGGER_INIT: std::sync::Once = std::sync::Once::new();

/// Initialize the WASM module with panic hook (auto-called by wasm-bindgen).
#[wasm_bindgen(start)]
pub fn start() {
    #[cfg(feature = "console_error_panic")]
    console_error_panic_hook::set_once();
    LOGGER_INIT.call_once(|| {
        let level = if cfg!(debug_assertions) {
            log::Level::Debug
        } else {
            log::Level::Info
        };
        wasm_logger::init(wasm_logger::Config::new(level));
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

export type WasmInputCallback = (callId: number, prompt: string | undefined) => Promise<string> | string;

export type WasmSendNotificationCallback = (notification: LspNotification) => void;
export type WasmSendResponseCallback = (response: LspResponse) => void;
export type WasmMakeRequestCallback = (request: LspRequest) => void;
export type WasmPlaygroundNotificationCallback = (notification: PlaygroundNotification) => void;

export type WasmExecCallback = (
  program: string,
  args: string[] | undefined,
  optionsJson: string | undefined,
) => Promise<{ stdout: string; stderr: string; exit_code: number;
               stdout_bytes: Uint8Array; stderr_bytes: Uint8Array }>;

export type WasmShellCallback = (
  command: string,
  optionsJson: string | undefined,
) => Promise<{ stdout: string; stderr: string; exit_code: number;
               stdout_bytes: Uint8Array; stderr_bytes: Uint8Array }>;
"#;

#[wasm_bindgen]
extern "C" {
    /// Callback bundle passed to [`BamlWasmRuntime::create`].
    ///
    /// From JS, pass a plain object: `{ fetch: ..., env: ... }`.
    #[wasm_bindgen(typescript_type = r#"{
        fetch: WasmFetchCallback;
        env: WasmEnvVarsCallback;
        input: WasmInputCallback;
        exec: WasmExecCallback;
        shell: WasmShellCallback;
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

    #[wasm_bindgen(method, getter, structural, js_name = "input")]
    fn input(this: &WasmCallbacks) -> Function;

    #[wasm_bindgen(method, getter, structural, js_name = "exec")]
    fn exec(this: &WasmCallbacks) -> Function;

    #[wasm_bindgen(method, getter, structural, js_name = "shell")]
    fn shell_fn(this: &WasmCallbacks) -> Function;

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
        let input_fn = callbacks.input();
        let exec_fn = callbacks.exec();
        let shell_fn = callbacks.shell_fn();
        let send_notification_fn = callbacks.send_notification();
        let send_response_fn = callbacks.send_response();
        let make_request_fn = callbacks.make_request();
        let playground_send_notification_fn = callbacks.playground_send_notification();

        // Wrap wasm_vfs in Arc so it can be shared across the VFS filesystem,
        // the fs IO namespace, and the glob IO namespace without cloning the
        // underlying JS value.
        #[allow(clippy::arc_with_non_send_sync)]
        let wasm_vfs_arc = std::sync::Arc::new(wasm_vfs);

        let sys_ops = sys_ops::SysOpsBuilder::new()
            .with_http_instance(std::sync::Arc::new(wasm_http::WasmHttp::new(fetch_fn)))
            .with_env_instance(std::sync::Arc::new(wasm_env::WasmEnv::new(env_vars_fn)))
            .with_io_instance(std::sync::Arc::new(wasm_io::WasmIo::new(input_fn)))
            .with_sys_instance(std::sync::Arc::new(wasm_sys::WasmSys::new(
                exec_fn, shell_fn,
            )))
            .with_fs_instance(std::sync::Arc::new(wasm_io_fs::WasmIoFs::new(
                std::sync::Arc::clone(&wasm_vfs_arc),
            )))
            .with_glob_instance(std::sync::Arc::new(wasm_io_glob::WasmIoGlob::new(
                std::sync::Arc::clone(&wasm_vfs_arc),
            )))
            .build();
        let sys_ops = std::sync::Arc::new(sys_ops);
        let sys_op_factory = std::sync::Arc::new(move |_path: &vfs::VfsPath| sys_ops.clone());

        let lsp = wasm_lsp::WasmLsp::new(send_notification_fn, send_response_fn, make_request_fn);
        let playground =
            wasm_playground::WasmPlaygroundSender::new(playground_send_notification_fn.clone());
        let event_sink = wasm_playground::WasmEventSink::new(playground_send_notification_fn);

        let vfs = wasm_fs::WasmFs::new(wasm_vfs_arc);
        let vfs = std::sync::Arc::new(vfs);

        let bex = bex_project::new_lsp(
            sys_op_factory,
            std::sync::Arc::new(lsp),
            std::sync::Arc::new(playground),
            bex_project::BamlVFS::new(vfs),
            Some(std::sync::Arc::new(event_sink)),
            bex_project::BackgroundSpawner::new(),
        );

        Ok(BamlWasmRuntime { bex: Box::new(bex) })
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
        let args = baml_core::cffi::CallFunctionArgs::decode(args_proto)
            .map_err(|e| JsError::new(&format!("Failed to decode arguments: {e}")))?;

        let kwargs = kwargs_to_bex_values(args.kwargs, &HANDLE_TABLE)
            .map_err(|e| JsError::new(&format!("Failed to convert arguments: {e}")))?;

        let call_id = sys_types::CallId(u64::from(id));
        let fs_path = bex_project::FsPath::from_str(project);

        let function_call_ctx = bex_project::FunctionCallContextBuilder::new(call_id);

        // Call the function.
        let bex = self
            .bex
            .get_bex_for_project(&fs_path)
            .map_err(|e| JsError::new(&format!("Failed to get Bex for project: {e}")))?;
        let result = bex
            .call_function(name, kwargs.into(), function_call_ctx.build())
            .await;

        // Handle cancellation error.
        let result = result.map_err(|e| -> JsValue {
            if bex_project::is_cancelled_runtime_error(&e) {
                let err = js_sys::Error::new("Operation cancelled");
                err.set_name("BamlCancelledError");
                err.into()
            } else {
                JsError::new(&format!("Function call failed: {e}")).into()
            }
        })?;

        let handle_options = bridge_ctypes::CffiHandleTableOptions::for_wire();
        let baml_value = external_to_outbound(&result, &handle_options)
            .map_err(|e| JsError::new(&format!("Failed to encode result: {e}")))?;

        Ok(baml_value.encode_to_vec())
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
    pub fn cancel_call(&self, project: String, call_id: u32) -> Result<(), JsError> {
        let fs_path = bex_project::FsPath::from_str(project);
        let bex = self
            .bex
            .get_bex_for_project(&fs_path)
            .map_err(|e| JsError::new(&format!("Failed to get Bex for project: {e}")))?;
        bex.cancel_function_call(sys_types::CallId(u64::from(call_id)))
            .map_err(|e| JsError::new(&format!("Failed to cancel function call: {e}")))?;
        Ok(())
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

    /// Request the control flow graph for a function.
    ///
    /// Triggers a `playground_send_notification` callback with a
    /// `ControlFlowGraphResult` notification containing the serialized graph.
    #[wasm_bindgen(js_name = requestControlFlowGraph)]
    pub fn request_control_flow_graph(&self, _project: String, function_name: &str) {
        self.bex.request_control_flow_graph(function_name);
    }

    /// Handle a cursor position change from the editor.
    ///
    /// Computes cursor context (which function/workflow the cursor is in) and
    /// sends it via a `CursorContext` playground notification.
    #[wasm_bindgen(js_name = handleCursorPosition)]
    pub fn handle_cursor_position(&self, file: &str, line: u32, column: u32) {
        self.bex.request_cursor_context(file, line, column);
    }

    /// Resolve a file ID to its file path.
    ///
    /// Used by the playground to navigate to source locations when clicking on
    /// log events. Returns the file path if the ID is valid, or undefined if not found.
    #[wasm_bindgen(js_name = resolveFileId)]
    pub fn resolve_file_id(&self, file_id: u32) -> Option<String> {
        self.bex.resolve_file_id(file_id)
    }

    /// Request test collection for a project.
    ///
    /// Triggers async test collection for the given project root path and sends
    /// a `TestCollectionResult` playground notification with the serialized test tree.
    #[wasm_bindgen(js_name = "requestCollectTests")]
    pub fn request_collect_tests(&self, project: &str) {
        self.bex.request_collect_tests(project);
    }

    /// Run a specific test by name. Request-response — returns proto-encoded `TestReport`,
    /// using the same path as `callFunction`.
    ///
    /// # Arguments
    ///
    /// * `id` - Unique call identifier (used for cancellation via `cancelCall`)
    /// * `project` - Project root path (e.g. `"/workspace/baml_src"`)
    /// * `generation` - The test-state generation captured when the test list was collected
    /// * `test_name` - The test name to run
    ///
    /// # Returns
    ///
    /// Protobuf-encoded `CffiValueHolder` containing the `TestReport` result.
    #[wasm_bindgen(js_name = "callTestFunction")]
    pub async fn call_test_function(
        &self,
        id: u32,
        project: &str,
        generation: u32,
        test_name: &str,
    ) -> Result<Vec<u8>, JsValue> {
        let call_id = sys_types::CallId(u64::from(id));
        let function_call_ctx = bex_project::FunctionCallContextBuilder::new(call_id);

        let result = self
            .bex
            .call_test_function(
                project,
                u64::from(generation),
                test_name,
                function_call_ctx.build(),
            )
            .await;

        match result {
            Ok(result) => {
                let handle_options = bridge_ctypes::CffiHandleTableOptions::for_wire();
                let baml_value = external_to_outbound(&result, &handle_options)
                    .map_err(|e| JsError::new(&format!("Failed to encode result: {e}")))?;
                Ok(baml_value.encode_to_vec())
            }
            Err(e) if bex_project::is_cancelled_engine_error(&e) => {
                let error = js_sys::Error::new("Function call was cancelled");
                error.set_name("BamlCancelledError");
                Err(error.into())
            }
            Err(e) => Err(JsError::new(&e.to_string()).into()),
        }
    }

    /// Expand a lazy test set by name. Fire-and-forget — result comes via a
    /// `TestCollectionResult` playground notification with the full serialized tree.
    ///
    /// # Arguments
    ///
    /// * `project` - Project root path (e.g. `"/workspace/baml_src"`)
    /// * `generation` - The test-state generation captured when the test list was collected
    /// * `testset_name` - The lazy test set name to expand
    #[wasm_bindgen(js_name = "expandTestSet")]
    pub fn expand_test_set(&self, project: &str, generation: u32, testset_name: &str) {
        self.bex
            .expand_test_set(project, u64::from(generation), testset_name);
    }
}
