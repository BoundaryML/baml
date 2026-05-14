//! Global Bex runtime management for bridge_cffi.
//!
//! Holds an `Arc<dyn BexLsp>` (not a raw `Arc<dyn Bex>`) so the same host
//! process can serve the playground webview from inside bridge_python (or any
//! other CFFI consumer) using the same protocol the VS Code-shipped
//! `baml-cli lsp` serves. `get_runtime()` routes through
//! `BexLsp::get_bex_for_project(root_path)` for the project registered at
//! `initialize_runtime` time.

use std::{
    collections::HashMap,
    sync::{Arc, RwLock},
};

use base64::Engine as _;
use baml_lsp_server::{
    no_op_lsp_sender::NoOpLspSender,
    playground_sender::NativePlaygroundSender,
    playground_server,
    playground_setup::PlaygroundWiring,
    playground_ws::WsOutMessage,
};
use bex_events::EventSink;
use bex_project::{
    BamlVFS, Bex, BexExternalValue, BexLsp, FsPath, InMemoryFs, LspClientSenderTrait,
    RuntimeError, is_cancelled_runtime_error, new_lsp_with_initial_project,
};
use once_cell::sync::OnceCell;
use prost::Message as _;
use tokio::{net::TcpListener, runtime::Runtime, sync::broadcast};

use crate::error::BridgeError;

static RUNTIME_INSTANCE: RwLock<Option<Arc<dyn BexLsp>>> = RwLock::new(None);
static ROOT_PATH: RwLock<Option<FsPath>> = RwLock::new(None);
// Event sink and broadcast channel are replaced on every `initialize_runtime`
// call (the host process re-initializes between tests). A `OnceCell` would
// leave `flush_event_sink` pointing at the first sink, so later tests with a
// fresh `BAML_TRACE_FILE` would drain the wrong file.
static EVENT_SINK: RwLock<Option<Arc<dyn EventSink>>> = RwLock::new(None);
static BROADCAST_TX: RwLock<Option<broadcast::Sender<WsOutMessage>>> = RwLock::new(None);
static TOKIO_RUNTIME: OnceCell<Arc<Runtime>> = OnceCell::new();

/// Initialize the global Tokio runtime.
pub fn get_tokio_runtime() -> Result<Arc<Runtime>, BridgeError> {
    TOKIO_RUNTIME
        .get_or_try_init(|| {
            Runtime::new()
                .map_err(|e| BridgeError::Internal(format!("Failed to create Tokio runtime: {e}")))
                .map(Arc::new)
        })
        .cloned()
}

/// Initialize the global LSP-backed runtime from in-memory BAML source files.
///
/// On success the playground HTTP/WS server is bound to the lowest free port
/// in [3700, 3800). What the webview serves depends on env vars:
/// `BAML_PLAYGROUND_DIR` (static assets) takes precedence, then
/// `BAML_PLAYGROUND_DEV_PORT` (Vite dev-proxy); we default the latter to 4000
/// (matching `.vscode/launch.json`) when neither is set so a developer running
/// `pnpm dev` in `typescript2/app-vscode-webview` automatically drives the
/// bridge_python-hosted playground.
pub fn initialize_runtime(
    root_path: &str,
    src_files: HashMap<String, String>,
) -> Result<Arc<dyn Bex>, BridgeError> {
    // Default the playground to dev-proxy mode against localhost:4000 — matches
    // .vscode/launch.json. Setting either env var means the host wants
    // something else (custom Vite port, prebuilt assets), so leave it alone.
    //
    // SAFETY: `set_var` is documented as unsafe under POSIX-thread semantics.
    // We mutate before any other thread reads these vars — `initialize_runtime`
    // is the first thing bridge_python's `__init__.py` does, before the tokio
    // runtime spawns any background tasks.
    if std::env::var_os("BAML_PLAYGROUND_DEV_PORT").is_none()
        && std::env::var_os("BAML_PLAYGROUND_DIR").is_none()
    {
        // SAFETY: see comment above.
        unsafe {
            std::env::set_var("BAML_PLAYGROUND_DEV_PORT", "4000");
        }
    }

    let tokio_runtime = get_tokio_runtime()?;
    let wiring = PlaygroundWiring::build();

    // Path normalization: VfsPath normalizes "." / "/" / "" all to the empty
    // string, and strips leading slashes from absolute joins. Build the VfsPath
    // once and derive every other key (project lookup, per-file paths) from it
    // so they match the keys BexMultiProject uses internally
    // (`FsPath::from_vfs(&root_vfs_path)`).
    //
    // Build a placeholder VFS first to compute the canonical paths; the real
    // VFS gets constructed below once we know each file's canonical key.
    let placeholder_vfs = BamlVFS::new(Arc::new(Box::new(InMemoryFs::new(HashMap::new()))));
    let root_vfs_path = vfs::VfsPath::from(placeholder_vfs)
        .join(root_path.trim_start_matches('/'))
        .map_err(|e| BridgeError::Internal(format!("Failed to construct VFS root: {e}")))?;
    let project_key = FsPath::from_vfs(&root_vfs_path);

    let mut files_for_vfs: HashMap<String, Vec<u8>> = HashMap::with_capacity(src_files.len());
    let mut sources_for_lsp: HashMap<FsPath, String> = HashMap::with_capacity(src_files.len());
    for (rel, contents) in src_files {
        let joined = root_vfs_path
            .join(rel.trim_start_matches('/'))
            .map_err(|e| BridgeError::Internal(format!("Failed to join source path {rel}: {e}")))?;
        let abs = joined.as_str().to_string();
        files_for_vfs.insert(abs, contents.as_bytes().to_vec());
        sources_for_lsp.insert(FsPath::from_vfs(&joined), contents);
    }

    let in_memory_fs = InMemoryFs::new(files_for_vfs);
    let baml_vfs = BamlVFS::new(Arc::new(Box::new(in_memory_fs)));
    // Re-root the project VfsPath on the real VFS now that it exists. The
    // .as_str() form is identical to the placeholder version above, so
    // project_key stays consistent.
    let root_vfs_path = vfs::VfsPath::from(baml_vfs.clone())
        .join(root_path.trim_start_matches('/'))
        .map_err(|e| BridgeError::Internal(format!("Failed to construct VFS root: {e}")))?;

    let (playground_listener, playground_port): (Option<TcpListener>, u16) =
        match tokio_runtime.block_on(playground_server::pick_port(3700, 100)) {
            Ok((listener, port)) => (Some(listener), port),
            Err(e) => {
                log::error!("bridge_cffi: could not bind playground port: {e}");
                (None, 0)
            }
        };

    if playground_port != 0 {
        let upstream = std::env::var("BAML_PLAYGROUND_DIR")
            .ok()
            .map(|d| format!(" (assets: {d})"))
            .or_else(|| {
                std::env::var("BAML_PLAYGROUND_DEV_PORT")
                    .ok()
                    .map(|p| format!(" (dev-proxy -> http://localhost:{p})"))
            })
            .unwrap_or_else(|| " (api only)".to_string());
        eprintln!("BAML playground: http://localhost:{playground_port}{upstream}");
    }

    let lsp_sender: Arc<dyn LspClientSenderTrait + Send + Sync> = Arc::new(NoOpLspSender);
    let playground_sender = Arc::new(NativePlaygroundSender::new(
        wiring.broadcast_tx.clone(),
        lsp_sender.clone(),
        playground_port,
        /* playground_via_browser = */ true,
    ));

    let event_sink = wiring.event_sink.clone();

    let bex = new_lsp_with_initial_project(
        wiring.sys_op_factory.clone(),
        lsp_sender,
        playground_sender,
        baml_vfs,
        Some(event_sink.clone()),
        bex_project::BackgroundSpawner::with_handle(tokio_runtime.handle().clone()),
        root_vfs_path,
        sources_for_lsp,
    )
    .map_err(|e| BridgeError::Internal(format!("Failed to register initial project: {e}")))?;
    let bex: Arc<dyn BexLsp> = Arc::new(bex);

    if let Some(listener) = playground_listener {
        let bex_for_playground = bex.clone();
        let btx = wiring.broadcast_tx.clone();
        let es = wiring.env_state.clone();
        let ios = wiring.io_state.clone();
        tokio_runtime.spawn(async move {
            if let Err(e) = playground_server::run(listener, bex_for_playground, btx, es, ios).await
            {
                log::error!("bridge_cffi: playground server exited: {e}");
            }
        });
    }

    {
        let mut guard = RUNTIME_INSTANCE
            .write()
            .map_err(|_| BridgeError::LockPoisoned)?;
        *guard = Some(bex.clone());
    }
    {
        let mut guard = ROOT_PATH.write().map_err(|_| BridgeError::LockPoisoned)?;
        *guard = Some(project_key.clone());
    }
    {
        let mut guard = EVENT_SINK.write().map_err(|_| BridgeError::LockPoisoned)?;
        *guard = Some(event_sink);
    }
    {
        let mut guard = BROADCAST_TX.write().map_err(|_| BridgeError::LockPoisoned)?;
        *guard = Some(wiring.broadcast_tx);
    }

    bex.get_bex_for_project(&project_key)
        .map_err(BridgeError::from)
}

/// Resolve the `Bex` for the project initialized by the most recent
/// `initialize_runtime` call.
pub fn get_runtime() -> Result<Arc<dyn Bex>, BridgeError> {
    let lsp = {
        let guard = RUNTIME_INSTANCE
            .read()
            .map_err(|_| BridgeError::LockPoisoned)?;
        guard.as_ref().ok_or(BridgeError::NotInitialized)?.clone()
    };
    let project_key = {
        let guard = ROOT_PATH.read().map_err(|_| BridgeError::LockPoisoned)?;
        guard.as_ref().ok_or(BridgeError::NotInitialized)?.clone()
    };
    lsp.get_bex_for_project(&project_key)
        .map_err(BridgeError::from)
}

/// Access the underlying `BexLsp`. Returned for future bridge_python use
/// (e.g. exposing a playground URL accessor) and for tests.
pub fn get_lsp() -> Result<Arc<dyn BexLsp>, BridgeError> {
    RUNTIME_INSTANCE
        .read()
        .map_err(|_| BridgeError::LockPoisoned)?
        .clone()
        .ok_or(BridgeError::NotInitialized)
}

/// Access the playground broadcast channel. Used by the FFI call_function
/// entry point to emit `WsOutMessage::CallFunction` / `CallFunctionResult`
/// so Python-triggered calls bracket-match the WS-triggered ones the
/// playground UI already buckets on.
pub fn get_broadcast_tx() -> Result<broadcast::Sender<WsOutMessage>, BridgeError> {
    BROADCAST_TX
        .read()
        .map_err(|_| BridgeError::LockPoisoned)?
        .clone()
        .ok_or(BridgeError::NotInitialized)
}

/// Project key set during `initialize_runtime` — the canonical
/// `FsPath::from_vfs(&root_vfs_path)` form used as the LSP project lookup key.
pub fn get_project_key() -> Result<FsPath, BridgeError> {
    ROOT_PATH
        .read()
        .map_err(|_| BridgeError::LockPoisoned)?
        .clone()
        .ok_or(BridgeError::NotInitialized)
}

/// Emit `WsOutMessage::CallFunction` on the playground broadcast channel before
/// a `bex.call_function(...)` await. Mirrors the start half of the bracket the
/// LSP WS handler emits at `playground_server.rs`. Returns Err only when the
/// runtime isn't initialized — broadcast send errors (zero subscribers) are
/// dropped silently, matching the rest of the WS emit sites.
pub fn broadcast_call_function_start(
    id: u64,
    function_name: &str,
    args_proto: &[u8],
) -> Result<(), BridgeError> {
    let tx = get_broadcast_tx()?;
    let project = get_project_key()?;
    let args_b64 = base64::engine::general_purpose::STANDARD.encode(args_proto);
    let _ = tx.send(WsOutMessage::CallFunction {
        id,
        project: project.as_path().to_string_lossy().into_owned(),
        name: function_name.to_string(),
        args_proto: args_b64,
    });
    Ok(())
}

/// Emit `CallFunctionResult` (Ok) or `CallFunctionError` (Err) after a
/// `bex.call_function(...)` await. Re-encodes Ok values with `for_wire()` —
/// the WS client lives in a browser and can't share in-process handle table
/// entries with the host (Python / Go / etc.), so media bytes and prompt AST
/// have to be inlined on the wire. Best-effort: no broadcast channel means
/// the playground server didn't start, which is fine in unit tests.
pub fn broadcast_call_function_end(
    id: u64,
    result: Result<&BexExternalValue, &RuntimeError>,
) {
    let Ok(tx) = get_broadcast_tx() else { return };
    let msg = match result {
        Ok(value) => {
            let opts = bridge_ctypes::CffiHandleTableOptions::for_wire();
            match bridge_ctypes::external_to_baml_value(value, &opts) {
                Ok(baml_val) => WsOutMessage::CallFunctionResult {
                    id,
                    result: base64::engine::general_purpose::STANDARD
                        .encode(baml_val.encode_to_vec()),
                },
                Err(e) => WsOutMessage::CallFunctionError {
                    id,
                    error: format!("Failed to encode result for wire: {e}"),
                    cancelled: None,
                },
            }
        }
        Err(e) => WsOutMessage::CallFunctionError {
            id,
            error: format!("{e}"),
            cancelled: if is_cancelled_runtime_error(e) {
                Some(true)
            } else {
                None
            },
        },
    };
    let _ = tx.send(msg);
}

/// Flush the registered event sink. Called by `bridge_python::flush_events()`.
pub fn flush_event_sink() {
    if let Ok(guard) = EVENT_SINK.read()
        && let Some(sink) = guard.as_ref()
    {
        sink.flush();
    }
}

/// Get the registered event sink (for passing to `HostSpanManager`).
pub fn get_event_sink() -> Option<Arc<dyn EventSink>> {
    EVENT_SINK.read().ok().and_then(|g| g.clone())
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use bex_project::{BexArgs, FunctionCallContextBuilder};

    use super::*;

    /// End-to-end smoke: a function call with `BAML_TRACE_FILE` set writes
    /// events to the file once `flush_event_sink` runs, *and* the FFI
    /// `call_function` entry point broadcasts both `CallFunction` and
    /// `CallFunctionResult` on the playground channel.
    ///
    /// Lives in bridge_cffi because that's where the global engine + file
    /// sink + broadcast wiring lives. One unified test instead of two
    /// because bridge_cffi's runtime statics are process-global; two tests
    /// both calling `initialize_runtime` would race on the locks.
    #[test]
    fn flush_event_sink_after_call() {
        let dir = tempfile::tempdir().expect("tempdir");
        let trace = dir.path().join("trace.jsonl");
        // SAFETY: bridge_cffi tests run in one binary; the other tests
        // (host_spans::*) don't read BAML_TRACE_FILE or the engine globals.
        unsafe {
            std::env::set_var("BAML_TRACE_FILE", &trace);
        }

        let mut src = HashMap::new();
        src.insert(
            "main.baml".to_string(),
            "function greet() -> string { \"hi\" }".to_string(),
        );

        let bex = initialize_runtime("baml_src", src).expect("initialize_runtime");

        let rt = get_tokio_runtime().expect("tokio runtime");

        // Subscribe before driving the FFI call — broadcast channels
        // only deliver messages sent *after* a subscriber attaches.
        let mut rx = get_broadcast_tx()
            .expect("broadcast tx available after init")
            .subscribe();

        // (1) Bex-trait path: exercises the event sink so the trace file
        // gets populated. Does NOT go through the FFI emit code.
        let result = rt.block_on(async {
            bex.clone()
                .call_function(
                    "greet",
                    BexArgs(HashMap::new()),
                    FunctionCallContextBuilder::new(sys_types::CallId(1)).build(),
                )
                .await
        });
        result.expect("call_function ok");

        // (2) FFI path: exercises the new `WsOutMessage::CallFunction` /
        // `CallFunctionResult` emits added by 03b. send_result_to_callback
        // will log "callback not registered" and return early, but the
        // broadcast emits happen in functions.rs::call_function_inner
        // around that call, not inside it — so they still fire.
        const FFI_ID: u32 = 2;
        crate::ffi::functions::call_function(
            c"greet".as_ptr().cast(),
            std::ptr::null(),
            0,
            FFI_ID,
        );

        // (3) Helper-bracket path: mirrors what bridge_python's
        // `BamlRuntime::call_function{,_sync}` does — wraps a direct
        // `bex.call_function(...)` await with the public broadcast helpers.
        // Uses `CallId::next()` instead of a hand-picked id so the assertion
        // is on whatever the runtime allocates, matching pyo3's call_id flow.
        let pyo3_id = sys_types::CallId::next();
        broadcast_call_function_start(pyo3_id.0, "greet", &[])
            .expect("start helper");
        let pyo3_result = rt.block_on(async {
            bex.clone()
                .call_function(
                    "greet",
                    BexArgs(HashMap::new()),
                    FunctionCallContextBuilder::new(pyo3_id).build(),
                )
                .await
        });
        broadcast_call_function_end(pyo3_id.0, pyo3_result.as_ref());

        let (saw_call, saw_result, saw_pyo3_call, saw_pyo3_result) = rt.block_on(async {
            let mut saw_call = false;
            let mut saw_result = false;
            let mut saw_pyo3_call = false;
            let mut saw_pyo3_result = false;
            let deadline =
                tokio::time::Instant::now() + std::time::Duration::from_secs(5);
            while tokio::time::Instant::now() < deadline
                && !(saw_call && saw_result && saw_pyo3_call && saw_pyo3_result)
            {
                match tokio::time::timeout(
                    std::time::Duration::from_millis(200),
                    rx.recv(),
                )
                .await
                {
                    Ok(Ok(
                        baml_lsp_server::playground_ws::WsOutMessage::CallFunction {
                            id, ..
                        },
                    )) => {
                        if id == u64::from(FFI_ID) {
                            saw_call = true;
                        } else if id == pyo3_id.0 {
                            saw_pyo3_call = true;
                        }
                    }
                    Ok(Ok(
                        baml_lsp_server::playground_ws::WsOutMessage::CallFunctionResult {
                            id, ..
                        },
                    )) => {
                        if id == u64::from(FFI_ID) {
                            saw_result = true;
                        } else if id == pyo3_id.0 {
                            saw_pyo3_result = true;
                        }
                    }
                    _ => {}
                }
            }
            (saw_call, saw_result, saw_pyo3_call, saw_pyo3_result)
        });

        flush_event_sink();

        let bytes = std::fs::metadata(&trace)
            .unwrap_or_else(|e| panic!("trace file {trace:?} missing: {e}"))
            .len();
        assert!(
            bytes > 0,
            "trace file {trace:?} should be non-empty after flush"
        );
        assert!(saw_call, "FFI: WsOutMessage::CallFunction was not broadcast");
        assert!(
            saw_result,
            "FFI: WsOutMessage::CallFunctionResult was not broadcast"
        );
        assert!(
            saw_pyo3_call,
            "pyo3-shape: WsOutMessage::CallFunction was not broadcast"
        );
        assert!(
            saw_pyo3_result,
            "pyo3-shape: WsOutMessage::CallFunctionResult was not broadcast"
        );

        // Cleanup: drop the BAML_TRACE_FILE env var so a follow-on `cargo test`
        // run in the same shell doesn't inherit it.
        // SAFETY: same justification as the set_var above.
        unsafe {
            std::env::remove_var("BAML_TRACE_FILE");
        }
    }
}
