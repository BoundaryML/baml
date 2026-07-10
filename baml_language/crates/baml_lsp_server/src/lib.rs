//! `baml_lsp_server2` — Native LSP server for BAML using `bex_project`.
//!
//! This crate provides a native (stdio) LSP server that delegates all
//! LSP logic to `bex_project::BexLsp`. It acts as the native counterpart
//! to `bridge_wasm`, providing:
//!
//! - Stdio transport for LSP messages
//! - Native filesystem (VFS) for project file access
//! - Playground HTTP/WS server for webview communication
//! - Fetch log interception for the playground
//! - Env var resolution via the playground webview
//!
//! # Architecture
//!
//! ```text
//!  ┌────────────┐   stdio    ┌──────────────────┐
//!  │  LSP Client│ <--------> │  baml_lsp_server2 │
//!  │  (VS Code) │            │                    │
//!  └────────────┘            │  ┌──────────────┐  │
//!                            │  │  bex_project  │  │
//!  ┌────────────┐   ws      │  │  (BexLsp)     │  │
//!  │  Playground│ <--------> │  └──────────────┘  │
//!  │  Webview   │            │                    │
//!  └────────────┘            └──────────────────────┘
//! ```
//!
//! `bex_project` handles all LSP protocol logic. This crate only provides:
//! - Transport (stdio reader/writer, WS server)
//! - Native implementations of `SysOps` (with playground interception)
//! - `LspClientSenderTrait` and `PlaygroundSender` implementations
//!
//! **TLS:** Enable exactly one of `native-tls` or `rustls`. CI may build with
//! `--all-features` (both enabled); prefer one when building the LSP binary.

mod deadlock_watchdog;
mod lsp_runtime;
mod native_lsp_sender;
mod native_vfs;
pub mod playground_env;
pub mod playground_http;
pub mod playground_io;
pub mod playground_runs;
pub mod playground_sender;
pub mod playground_server;
pub mod playground_session;
pub mod playground_ws;

const OUTBOUND_QUEUE_BYTES: usize = 64 * 1024 * 1024;
const MAX_OUTBOUND_FRAME_BYTES: usize = 4 * 1024 * 1024;

#[derive(Debug, Clone)]
enum OutboundPayload {
    Message(lsp_server::Message),
    RawJson(serde_json::Value),
}

#[derive(Debug, Clone)]
pub(crate) struct OutboundFrame {
    payload: OutboundPayload,
    _charge: Arc<OutboundCharge>,
}

impl OutboundFrame {
    pub(crate) fn message(&self) -> Option<&lsp_server::Message> {
        match &self.payload {
            OutboundPayload::Message(message) => Some(message),
            OutboundPayload::RawJson(_) => None,
        }
    }
}

#[derive(Debug)]
pub(crate) struct OutboundBudget {
    used: std::sync::atomic::AtomicUsize,
    limit: usize,
    max_frame: usize,
}

#[derive(Debug)]
struct OutboundCharge {
    budget: Arc<OutboundBudget>,
    bytes: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum OutboundReserveError {
    Serialization,
    Oversized,
    Saturated,
}

impl Drop for OutboundCharge {
    fn drop(&mut self) {
        self.budget
            .used
            .fetch_sub(self.bytes, std::sync::atomic::Ordering::AcqRel);
    }
}

impl OutboundBudget {
    fn new() -> Arc<Self> {
        Arc::new(Self {
            used: std::sync::atomic::AtomicUsize::new(0),
            limit: OUTBOUND_QUEUE_BYTES,
            max_frame: MAX_OUTBOUND_FRAME_BYTES,
        })
    }

    fn try_reserve(
        self: &Arc<Self>,
        payload: OutboundPayload,
    ) -> Result<OutboundFrame, OutboundReserveError> {
        let bytes = match &payload {
            OutboundPayload::Message(message) => serde_json::to_vec(message)
                .map_err(|_| OutboundReserveError::Serialization)?
                .len(),
            OutboundPayload::RawJson(value) => serde_json::to_vec(value)
                .map_err(|_| OutboundReserveError::Serialization)?
                .len(),
        };
        if bytes > self.max_frame {
            return Err(OutboundReserveError::Oversized);
        }
        let mut used = self.used.load(std::sync::atomic::Ordering::Acquire);
        loop {
            if used.saturating_add(bytes) > self.limit {
                return Err(OutboundReserveError::Saturated);
            }
            match self.used.compare_exchange_weak(
                used,
                used + bytes,
                std::sync::atomic::Ordering::AcqRel,
                std::sync::atomic::Ordering::Acquire,
            ) {
                Ok(_) => break,
                Err(observed) => used = observed,
            }
        }
        Ok(OutboundFrame {
            payload,
            _charge: Arc::new(OutboundCharge {
                budget: self.clone(),
                bytes,
            }),
        })
    }

    pub(crate) fn try_message(
        self: &Arc<Self>,
        message: lsp_server::Message,
    ) -> Result<OutboundFrame, OutboundReserveError> {
        self.try_reserve(OutboundPayload::Message(message))
    }

    fn try_raw(self: &Arc<Self>, value: serde_json::Value) -> Option<OutboundFrame> {
        self.try_reserve(OutboundPayload::RawJson(value)).ok()
    }
}

use std::{
    collections::BTreeMap,
    fs,
    io::{BufRead, Write},
    path::{Path, PathBuf},
    sync::Arc,
    time::{Duration, UNIX_EPOCH},
};

use anyhow::Context as _;
use playground_env::{PlaygroundEnv, PlaygroundEnvState};
use playground_http::{PlaygroundHttp, PlaygroundHttpState};
use playground_io::{PlaygroundIo, PlaygroundIoState};
use playground_session::PlaygroundSessionStore;
use playground_ws::WsOutMessage;
use tokio::net::TcpListener;

const MAX_STDIO_FRAME_BYTES: usize = 8 * 1024 * 1024;

#[derive(Debug, PartialEq, Eq)]
enum StdioReadError {
    Parse(String),
    InvalidRequest(String),
    Framing(String),
}

fn decode_lsp_message(value: serde_json::Value) -> Result<lsp_server::Message, String> {
    let Some(object) = value.as_object() else {
        return Err("JSON-RPC envelope must be an object".to_string());
    };
    if object.get("jsonrpc").and_then(serde_json::Value::as_str) != Some("2.0") {
        return Err("JSON-RPC envelope must contain jsonrpc: \"2.0\"".to_string());
    }
    let has_method = object
        .get("method")
        .is_some_and(serde_json::Value::is_string);
    let has_result = object.contains_key("result");
    let has_error = object.contains_key("error");
    if has_method {
        if has_result || has_error {
            return Err("JSON-RPC request/notification cannot contain result or error".to_string());
        }
        if object
            .get("id")
            .is_some_and(|id| !(id.is_string() || id.as_i64().is_some() || id.as_u64().is_some()))
        {
            return Err("JSON-RPC request id must be a string or integer".to_string());
        }
    } else {
        if has_result == has_error {
            return Err(
                "JSON-RPC response must contain exactly one of result or error".to_string(),
            );
        }
        let Some(id) = object.get("id") else {
            return Err("JSON-RPC response is missing id".to_string());
        };
        if !(id.is_string() || id.as_i64().is_some() || id.as_u64().is_some()) {
            return Err("JSON-RPC response id must be a string or integer".to_string());
        }
    }
    serde_json::from_value(value).map_err(|error| format!("Invalid request: {error}"))
}

fn read_lsp_message(
    input: &mut impl BufRead,
) -> Result<Option<lsp_server::Message>, StdioReadError> {
    let mut content_length = None;
    let mut header = String::new();
    loop {
        header.clear();
        let read = input
            .read_line(&mut header)
            .map_err(|error| StdioReadError::Framing(error.to_string()))?;
        if read == 0 {
            return if content_length.is_none() {
                Ok(None)
            } else {
                Err(StdioReadError::Framing(
                    "unexpected EOF in LSP headers".to_string(),
                ))
            };
        }
        let Some(line) = header.strip_suffix("\r\n") else {
            return Err(StdioReadError::Framing(
                "LSP header must end in CRLF".to_string(),
            ));
        };
        if line.is_empty() {
            break;
        }
        let Some((name, value)) = line.split_once(": ") else {
            return Err(StdioReadError::Framing(format!(
                "malformed LSP header: {line}"
            )));
        };
        if name.eq_ignore_ascii_case("Content-Length") {
            let parsed = value
                .parse::<usize>()
                .map_err(|_| StdioReadError::Framing("invalid Content-Length".to_string()))?;
            if parsed > MAX_STDIO_FRAME_BYTES {
                return Err(StdioReadError::Framing(format!(
                    "LSP frame exceeds {MAX_STDIO_FRAME_BYTES} bytes"
                )));
            }
            content_length = Some(parsed);
        }
    }
    let content_length = content_length
        .ok_or_else(|| StdioReadError::Framing("missing Content-Length".to_string()))?;
    let mut body = vec![0; content_length];
    input
        .read_exact(&mut body)
        .map_err(|error| StdioReadError::Framing(error.to_string()))?;
    let value: serde_json::Value = serde_json::from_slice(&body)
        .map_err(|error| StdioReadError::Parse(format!("Parse error: {error}")))?;
    decode_lsp_message(value)
        .map(Some)
        .map_err(StdioReadError::InvalidRequest)
}

fn write_raw_lsp_json(output: &mut impl Write, value: &serde_json::Value) -> std::io::Result<()> {
    let body = serde_json::to_vec(value)?;
    write!(output, "Content-Length: {}\r\n\r\n", body.len())?;
    output.write_all(&body)?;
    output.flush()
}

pub fn version() -> &'static str {
    baml_version::CANONICAL_VERSION
}

/// Build `SysOps` for a playground-connected project.
///
/// Uses native FS/sys/net but intercepts HTTP (for fetch logs) and env
/// (for webview-resolved env vars).
fn build_playground_sys_ops(
    broadcast_tx: &tokio::sync::broadcast::Sender<WsOutMessage>,
    run_store: &Arc<bex_events::run::InMemoryRunStore>,
    env_state: &Arc<PlaygroundEnvState>,
    io_state: &Arc<PlaygroundIoState>,
) -> sys_ops::SysOps {
    let http_state = Arc::new(PlaygroundHttpState::new(
        broadcast_tx.clone(),
        run_store.clone(),
    ));
    sys_ops::SysOpsBuilder::new()
        .with_fs::<sys_native::NativeSysOps>()
        .with_sys::<sys_native::NativeSysOps>()
        .with_net::<sys_native::NativeSysOps>()
        .with_http_instance(Arc::new(PlaygroundHttp(http_state)))
        .with_env_instance(Arc::new(PlaygroundEnv(env_state.clone())))
        .with_io_instance(Arc::new(PlaygroundIo(io_state.clone())))
        .build()
}

/// Run the native BAML LSP server.
///
/// This is the main entry point. It:
/// 1. Creates the tokio runtime and broadcast channel
/// 2. Sets up native VFS and playground-intercepting SysOps
/// 3. Creates `bex_project::BexLsp` via `bex_project::new_lsp`
/// 4. Starts the playground HTTP/WS server
/// 5. Runs the stdio LSP event loop
pub fn run_server(workspace_roots: Vec<PathBuf>) -> anyhow::Result<()> {
    run_server_inner(
        PlaygroundOpenTarget::LspClient,
        workspace_roots,
        None,
        PlaygroundServerOptions::default(),
    )
}

/// Options for `run_playground_server` (browser mode only; LSP mode ignores them).
#[derive(Debug, Clone)]
pub struct PlaygroundServerOptions {
    /// Bind exactly this port; error if unavailable. `None` = scan from 4265.
    pub port: Option<u16>,
    /// Open the local browser once the server is up.
    pub open_browser: bool,
}

impl Default for PlaygroundServerOptions {
    fn default() -> Self {
        Self {
            port: None,
            open_browser: true,
        }
    }
}

pub fn run_playground_server(
    workspace_roots: Vec<PathBuf>,
    playground_dir_override: Option<PathBuf>,
    options: PlaygroundServerOptions,
) -> anyhow::Result<()> {
    run_server_inner(
        PlaygroundOpenTarget::Browser,
        workspace_roots,
        playground_dir_override,
        options,
    )
}

#[derive(Clone, Copy)]
enum PlaygroundOpenTarget {
    LspClient,
    Browser,
}

fn run_server_inner(
    playground_open_target: PlaygroundOpenTarget,
    workspace_roots: Vec<PathBuf>,
    playground_dir_override: Option<PathBuf>,
    options: PlaygroundServerOptions,
) -> anyhow::Result<()> {
    let workspace_roots = absolutize_workspace_roots(workspace_roots)?;

    // Set up tracing → stderr so vscode-languageclient captures it
    // in the "BAML Language Server" output channel.
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info,salsa=warn")),
        )
        .with_ansi(false)
        .init();

    apply_single_workspace_cwd(&workspace_roots)?;

    tracing::info!("baml-lsp v{} starting", version());
    deadlock_watchdog::spawn();

    let tokio_runtime = tokio::runtime::Runtime::new()?;

    // Broadcast channel for playground WS messages (fetch logs, env requests, etc.)
    let (broadcast_tx, _) = tokio::sync::broadcast::channel::<WsOutMessage>(64);
    // Terminal runs beyond the cap are evicted from memory; the playground
    // rehydrates them on demand from the disk-backed history store.
    let run_store = Arc::new(bex_events::run::InMemoryRunStore::new(
        bex_events::run::RunRetentionPolicy {
            max_terminal_runs: Some(100),
            ..Default::default()
        },
    ));
    let _profile_observer = bex_events::run::register_profile_observer(Arc::new(
        playground_runs::RunStoreProfileObserver::new(run_store.clone(), broadcast_tx.clone()),
    ));
    let session_store = Arc::new(PlaygroundSessionStore::default());
    let env_state = Arc::new(PlaygroundEnvState::new(
        broadcast_tx.clone(),
        run_store.clone(),
        session_store,
    ));
    let io_state = Arc::new(PlaygroundIoState::new(
        broadcast_tx.clone(),
        run_store.clone(),
    ));

    // Build SysOps with playground interception.
    // The factory creates the same ops for every project.
    let broadcast_tx_for_factory = broadcast_tx.clone();
    let run_store_for_factory = run_store.clone();
    let env_state_for_factory = env_state.clone();
    let io_state_for_factory = io_state.clone();
    #[allow(clippy::type_complexity)]
    let sys_op_factory: Arc<dyn Fn(&vfs::VfsPath) -> Arc<sys_ops::SysOps> + Send + Sync> =
        Arc::new(move |_path: &vfs::VfsPath| {
            Arc::new(build_playground_sys_ops(
                &broadcast_tx_for_factory,
                &run_store_for_factory,
                &env_state_for_factory,
                &io_state_for_factory,
            ))
        });

    // Native VFS
    let vfs: Arc<Box<dyn bex_project::BulkReadFileSystem>> =
        Arc::new(Box::new(native_vfs::NativeVfs::new()));
    let baml_vfs = bex_project::BamlVFS::new(vfs);

    // Stdio sender (LSP client sender)
    let (writer_tx, writer_rx) = crossbeam_channel::bounded::<OutboundFrame>(512);
    let writer_tx = Arc::new(writer_tx);
    let writer_budget = OutboundBudget::new();
    let lsp_sender: Arc<dyn bex_project::LspClientSenderTrait + Send + Sync> = Arc::new(
        native_lsp_sender::NativeLspSender::new(&writer_tx, &writer_budget),
    );

    // Browser-mode LSP transport: the standalone playground has no stdio LSP
    // client, so in browser mode we forward all LSP output (responses +
    // publishDiagnostics) to the `/api/lsp` WebSocket instead of stdout. A
    // bridge thread (spawned only in browser mode) drains `writer_rx` into
    // this broadcast channel.
    let (lsp_out_tx, _lsp_out_rx) = tokio::sync::broadcast::channel::<OutboundFrame>(256);
    let lsp_runtime = lsp_runtime::LspRuntime::new()?;

    // Mirror of the content the browser editor currently has per file. Shared
    // between the `/api/lsp` bridge (writes it on didOpen/didChange) and the disk
    // watcher (reads it to avoid echoing the browser's own write-throughs back as
    // external changes).
    let doc_mirror: playground_server::DocMirror =
        Arc::new(std::sync::Mutex::new(std::collections::HashMap::new()));

    // Per-session bearer token for /api, browser mode only. In LSP/editor
    // mode no token is minted and the guard behaves exactly as before (D6).
    let session_token: Option<Arc<str>> =
        matches!(playground_open_target, PlaygroundOpenTarget::Browser)
            .then(|| uuid::Uuid::new_v4().simple().to_string().into());

    // Pick the playground port early so we can pass it to the sender.
    let (playground_listener, playground_port): (Option<TcpListener>, u16) = {
        let bind_result = match options.port {
            Some(port) => tokio_runtime
                .block_on(playground_server::bind_exact_port(port))
                .map(|listener| (listener, port)),
            None => tokio_runtime.block_on(playground_server::pick_port(4265, 100)),
        };
        match bind_result {
            Ok((listener, port)) => (Some(listener), port),
            Err(e) => {
                if matches!(playground_open_target, PlaygroundOpenTarget::Browser) {
                    return Err(e); // browser mode is useless without the server
                }
                tracing::error!("Could not find playground port: {e}");
                (None, 0) // LSP mode continues serving stdio
            }
        }
    };

    if matches!(playground_open_target, PlaygroundOpenTarget::Browser) {
        print_playground_banner(playground_port, &workspace_roots, session_token.as_deref());
    }

    // Playground sender (needs port + lsp_sender for OpenPlayground)
    let playground_sender: Arc<dyn bex_project::PlaygroundSender> =
        Arc::new(playground_sender::NativePlaygroundSender::new(
            broadcast_tx.clone(),
            playground_port,
            matches!(playground_open_target, PlaygroundOpenTarget::Browser),
            session_token.clone(),
        ));

    // Create the BexLsp (multi-project LSP)
    let spawner = bex_project::BackgroundSpawner::with_handle(tokio_runtime.handle().clone());
    let bex = bex_project::new_lsp(
        sys_op_factory,
        lsp_sender,
        playground_sender.clone(),
        baml_vfs,
        spawner,
    );
    let bex: Arc<dyn bex_project::BexLsp> = Arc::new(bex);
    run_store.set_graph_runtime_overlay_span_provider(Arc::new(
        playground_runs::ProjectGraphRuntimeOverlaySpanProvider::new(bex.clone()),
    ));

    let has_explicit_workspace_roots = !workspace_roots.is_empty();
    let explicit_projects = if has_explicit_workspace_roots {
        bex.initialize_workspace_roots(workspace_roots.clone())?
    } else {
        Vec::new()
    };
    if matches!(playground_open_target, PlaygroundOpenTarget::Browser)
        && has_explicit_workspace_roots
    {
        spawn_standalone_workspace_poller(bex.clone(), workspace_roots.clone())?;
    }

    if matches!(playground_open_target, PlaygroundOpenTarget::Browser) && playground_port != 0 {
        if let Some(project) = explicit_projects.first() {
            if options.open_browser {
                playground_sender.send_playground_notification(
                    bex_project::PlaygroundNotification::OpenPlayground {
                        project: project.clone(),
                        function_name: None,
                        test_name: None,
                        testset_name: None,
                    },
                );
            }
        } else if has_explicit_workspace_roots {
            tracing::warn!("No BAML projects discovered for explicit workspace roots");
        }
    }

    // Start playground HTTP/WS server. In editor/LSP mode it runs in the
    // background while stdio drives the process. In browser mode it is the
    // foreground task; otherwise a terminal stdin EOF would shut down the
    // playground immediately.
    if let Some(listener) = playground_listener {
        let bex_for_playground = bex.clone();
        let btx = broadcast_tx.clone();
        let es = env_state.clone();
        let ios = io_state.clone();
        let runs = run_store.clone();
        let playground_dir = playground_dir_override.clone();

        if matches!(playground_open_target, PlaygroundOpenTarget::Browser) {
            // No stdio LSP client in browser mode; the stdout writer thread is
            // never spawned, so `writer_rx` is free for us to drain into the
            // `/api/lsp` broadcast channel.
            let lsp_out_tx_bridge = lsp_out_tx.clone();
            std::thread::Builder::new()
                .name("lsp-ws-bridge".into())
                .spawn(move || {
                    while let Ok(frame) = writer_rx.recv() {
                        let _ = lsp_out_tx_bridge.send(frame);
                    }
                })?;

            // Watch the workspace for external edits (e.g. from another editor)
            // and push them to the browser editor over `/api/lsp`. Held until the
            // server task returns so watching continues for the session lifetime.
            let _disk_watcher = playground_server::spawn_disk_watcher(
                &workspace_roots,
                lsp_out_tx.clone(),
                writer_budget.clone(),
                doc_mirror.clone(),
            );

            return tokio_runtime.block_on(playground_server::run(
                listener,
                bex_for_playground,
                btx,
                es,
                ios,
                runs,
                playground_dir,
                lsp_out_tx,
                lsp_runtime.clone(),
                doc_mirror,
                workspace_roots.clone(),
                session_token,
            ));
        }

        let workspace_roots_for_server = workspace_roots.clone();
        let lsp_runtime_for_server = lsp_runtime.clone();
        tokio_runtime.spawn(async move {
            if let Err(e) = playground_server::run(
                listener,
                bex_for_playground,
                btx,
                es,
                ios,
                runs,
                playground_dir,
                lsp_out_tx,
                lsp_runtime_for_server,
                doc_mirror,
                workspace_roots_for_server,
                session_token,
            )
            .await
            {
                tracing::error!("Playground server exited: {e}");
            }
        });
    } else if matches!(playground_open_target, PlaygroundOpenTarget::Browser) {
        anyhow::bail!("Could not start playground server");
    }

    let stdio_closed = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let stdio_sink_tx = writer_tx.clone();
    let stdio_sink_budget = writer_budget.clone();
    let stdio_sink: lsp_runtime::Sink = Arc::new(move |message| {
        let frame = match stdio_sink_budget.try_message(message) {
            Ok(frame) => frame,
            Err(OutboundReserveError::Saturated) => {
                return lsp_runtime::SinkDelivery::Saturated;
            }
            Err(OutboundReserveError::Oversized | OutboundReserveError::Serialization) => {
                return lsp_runtime::SinkDelivery::Oversized;
            }
        };
        match stdio_sink_tx.try_send(frame) {
            Ok(()) => lsp_runtime::SinkDelivery::Sent,
            Err(crossbeam_channel::TrySendError::Full(_)) => lsp_runtime::SinkDelivery::Saturated,
            Err(crossbeam_channel::TrySendError::Disconnected(_)) => {
                lsp_runtime::SinkDelivery::Closed
            }
        }
    });
    let stdio_closed_for_endpoint = stdio_closed.clone();
    let stdio_close: Arc<dyn Fn() + Send + Sync> = Arc::new(move || {
        stdio_closed_for_endpoint.store(true, std::sync::atomic::Ordering::Release);
    });
    let stdio_session = lsp_runtime
        .open_session(
            bex_project::lsp_ingress::TransportKind::Stdio,
            bex.clone(),
            stdio_sink,
            stdio_close,
            None,
        )
        .session_id;

    // Spawn the stdout writer thread.
    std::thread::Builder::new()
        .name("lsp-stdout-writer".into())
        .spawn(move || {
            let stdout = std::io::stdout();
            let mut stdout = stdout.lock();
            while let Ok(frame) = writer_rx.recv() {
                let result = match &frame.payload {
                    OutboundPayload::Message(message) => message.write(&mut stdout),
                    OutboundPayload::RawJson(value) => write_raw_lsp_json(&mut stdout, value),
                };
                if result.is_err() {
                    break;
                }
            }
        })?;

    // Main event loop: read from stdin, dispatch to bex_project.
    let stdin = std::io::stdin();
    let mut stdin = stdin.lock();

    // Main event loop — forward all messages to bex_project.
    // The `initialize` handshake is handled by `bex_project` via `handle_request`.
    let mut abnormal_exit = false;
    loop {
        let msg = match read_lsp_message(&mut stdin) {
            Ok(Some(msg)) => msg,
            Ok(None) => break,
            Err(error) => {
                let (code, message, recoverable) = match error {
                    StdioReadError::Parse(message) => (-32700, message, true),
                    StdioReadError::InvalidRequest(message) => (-32600, message, true),
                    StdioReadError::Framing(message) => (-32700, message, false),
                };
                let response = serde_json::json!({
                    "jsonrpc": "2.0",
                    "id": serde_json::Value::Null,
                    "error": { "code": code, "message": message },
                });
                let queued = writer_budget
                    .try_raw(response)
                    .is_some_and(|frame| writer_tx.try_send(frame).is_ok());
                if !queued || !recoverable {
                    break;
                }
                continue;
            }
        };

        let mut terminate = false;
        loop {
            match lsp_runtime.submit(stdio_session, msg.clone()) {
                lsp_runtime::SubmitResult::Accepted | lsp_runtime::SubmitResult::Dropped => break,
                lsp_runtime::SubmitResult::Backpressure => {
                    std::thread::sleep(Duration::from_millis(2));
                }
                lsp_runtime::SubmitResult::Exited { normal } => {
                    abnormal_exit = !normal;
                    terminate = true;
                    break;
                }
                lsp_runtime::SubmitResult::Closed => {
                    terminate = true;
                    break;
                }
            }
        }
        if terminate || stdio_closed.load(std::sync::atomic::Ordering::Acquire) {
            break;
        }
    }

    lsp_runtime.close_session(stdio_session);

    if abnormal_exit {
        anyhow::bail!("LSP client sent exit before completing shutdown");
    }

    tracing::info!("LSP server shutting down");
    Ok(())
}

#[allow(clippy::print_stdout)] // user-facing banner; browser mode has no stdio LSP client
fn print_playground_banner(port: u16, roots: &[PathBuf], token: Option<&str>) {
    println!("{}", format_playground_banner(port, roots, token));
}

fn format_playground_banner(port: u16, roots: &[PathBuf], token: Option<&str>) -> String {
    let root = roots
        .first()
        .map(|r| r.display().to_string())
        .unwrap_or_else(|| "(no workspace roots)".to_string());
    // The token rides the URL, not the tunnel, so the ssh hint doesn't carry it.
    let url = match token {
        Some(token) => format!("http://localhost:{port}/?token={token}"),
        None => format!("http://localhost:{port}/"),
    };
    format!(
        "\n  Playground:  {url}\n  Project:     {root}\n\n  \
         Remote machine? Forward the port, then open the URL locally:\n    \
         ssh -L {port}:localhost:{port} <user@host>\n\n  Press Ctrl-C to stop.\n"
    )
}

fn absolutize_workspace_roots(workspace_roots: Vec<PathBuf>) -> anyhow::Result<Vec<PathBuf>> {
    if workspace_roots.iter().all(|root| root.is_absolute()) {
        return Ok(workspace_roots);
    }

    let cwd = std::env::current_dir().context("Failed to read current directory")?;
    Ok(workspace_roots
        .into_iter()
        .map(|root| {
            if root.is_absolute() {
                root
            } else {
                cwd.join(root)
            }
        })
        .collect())
}

fn apply_single_workspace_cwd(workspace_roots: &[PathBuf]) -> anyhow::Result<()> {
    let [root] = workspace_roots else {
        return Ok(());
    };

    let cwd = workspace_cwd(root);
    std::env::set_current_dir(&cwd)
        .with_context(|| format!("Failed to set current directory to {}", cwd.display()))?;
    tracing::info!(
        "Using {} as standalone LSP current directory",
        cwd.display()
    );
    Ok(())
}

fn workspace_cwd(root: &Path) -> PathBuf {
    if root.is_file() {
        root.parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| root.to_path_buf())
    } else {
        root.to_path_buf()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct WorkspaceSignature {
    files: BTreeMap<PathBuf, FileSignature>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct FileSignature {
    len: u64,
    modified_ns: Option<u128>,
}

fn spawn_standalone_workspace_poller(
    bex: Arc<dyn bex_project::BexLsp>,
    workspace_roots: Vec<PathBuf>,
) -> anyhow::Result<()> {
    std::thread::Builder::new()
        .name("playground-workspace-poller".to_string())
        .spawn(move || {
            let mut last_signature = workspace_signature(&workspace_roots);
            loop {
                std::thread::sleep(Duration::from_secs(1));
                let next_signature = workspace_signature(&workspace_roots);
                if next_signature == last_signature {
                    continue;
                }
                last_signature = next_signature;
                tracing::info!("Detected standalone workspace file change; refreshing playground");
                if let Err(err) = bex.initialize_workspace_roots(workspace_roots.clone()) {
                    tracing::warn!("Failed to refresh standalone playground workspace: {err}");
                }
            }
        })?;
    Ok(())
}

fn workspace_signature(workspace_roots: &[PathBuf]) -> WorkspaceSignature {
    let mut files = BTreeMap::new();
    for root in workspace_roots {
        collect_workspace_signature(root, &mut files);
    }
    WorkspaceSignature { files }
}

fn collect_workspace_signature(path: &Path, files: &mut BTreeMap<PathBuf, FileSignature>) {
    let Ok(metadata) = fs::symlink_metadata(path) else {
        return;
    };
    if metadata.file_type().is_symlink() {
        return;
    }
    if metadata.is_file() {
        if watches_standalone_workspace_file(path) {
            files.insert(path.to_path_buf(), file_signature(&metadata));
        }
        return;
    }
    if !metadata.is_dir() || should_skip_poll_dir(path) {
        return;
    }
    let Ok(entries) = fs::read_dir(path) else {
        return;
    };
    for entry in entries.flatten() {
        collect_workspace_signature(&entry.path(), files);
    }
}

fn watches_standalone_workspace_file(path: &Path) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name == "baml.toml")
        || path
            .extension()
            .and_then(|extension| extension.to_str())
            .is_some_and(|extension| extension == "baml")
}

fn should_skip_poll_dir(path: &Path) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| {
            matches!(
                name,
                ".baml" | ".git" | ".next" | ".turbo" | "dist" | "node_modules" | "target"
            )
        })
}

fn file_signature(metadata: &fs::Metadata) -> FileSignature {
    FileSignature {
        len: metadata.len(),
        modified_ns: metadata
            .modified()
            .ok()
            .and_then(|modified| modified.duration_since(UNIX_EPOCH).ok())
            .map(|duration| duration.as_nanos()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn framed(body: &str) -> Vec<u8> {
        format!("Content-Length: {}\r\n\r\n{body}", body.len()).into_bytes()
    }

    #[test]
    fn stdio_parser_distinguishes_parse_and_invalid_request() {
        let mut malformed = std::io::Cursor::new(framed("{"));
        assert!(matches!(
            read_lsp_message(&mut malformed),
            Err(StdioReadError::Parse(_))
        ));

        let mut invalid = std::io::Cursor::new(framed(r#"{"jsonrpc":"2.0","wat":true}"#));
        assert!(matches!(
            read_lsp_message(&mut invalid),
            Err(StdioReadError::InvalidRequest(_))
        ));

        let mut request = std::io::Cursor::new(framed(
            r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}"#,
        ));
        assert!(matches!(
            read_lsp_message(&mut request),
            Ok(Some(lsp_server::Message::Request(_)))
        ));
    }

    #[test]
    fn raw_stdio_error_uses_null_id_and_content_length() {
        let value = serde_json::json!({
            "jsonrpc": "2.0",
            "id": serde_json::Value::Null,
            "error": { "code": -32700, "message": "Parse error" },
        });
        let mut output = Vec::new();
        write_raw_lsp_json(&mut output, &value).unwrap();
        let output = String::from_utf8(output).unwrap();
        assert!(output.starts_with("Content-Length: "));
        assert!(output.contains("\r\n\r\n"));
        assert!(output.contains(r#""id":null"#));
        assert!(output.contains(r#""code":-32700"#));
    }

    #[test]
    fn playground_banner_shows_url_project_root_and_ssh_hint() {
        let banner = format_playground_banner(4265, &[PathBuf::from("/home/dev/my-app")], None);
        assert!(banner.contains("http://localhost:4265/"), "{banner}");
        assert!(!banner.contains("?token="), "{banner}");
        assert!(banner.contains("/home/dev/my-app"), "{banner}");
        assert!(
            banner.contains("ssh -L 4265:localhost:4265 <user@host>"),
            "{banner}"
        );
        assert!(banner.contains("Press Ctrl-C to stop."), "{banner}");

        let no_roots = format_playground_banner(4270, &[], None);
        assert!(no_roots.contains("(no workspace roots)"), "{no_roots}");
        assert!(no_roots.contains("http://localhost:4270/"), "{no_roots}");
    }

    #[test]
    fn playground_banner_carries_the_session_token_on_the_url_only() {
        let banner =
            format_playground_banner(4265, &[PathBuf::from("/home/dev/my-app")], Some("abc123"));
        assert!(
            banner.contains("http://localhost:4265/?token=abc123"),
            "{banner}"
        );
        // The token rides the URL, not the tunnel: the ssh hint stays bare.
        assert!(
            banner.contains("ssh -L 4265:localhost:4265 <user@host>"),
            "{banner}"
        );
        assert_eq!(banner.matches("token").count(), 1, "{banner}");
    }

    #[test]
    fn absolutize_workspace_roots_makes_relative_paths_absolute() {
        let cwd = std::env::current_dir().expect("cwd should be available");
        let absolute = cwd.join("already-absolute");

        let roots =
            absolutize_workspace_roots(vec![PathBuf::from("relative-workspace"), absolute.clone()])
                .expect("workspace roots should absolutize");

        assert_eq!(roots, vec![cwd.join("relative-workspace"), absolute]);
    }

    #[test]
    fn workspace_cwd_uses_file_parent_and_keeps_directories() {
        let dir = std::env::temp_dir().join(format!(
            "baml-lsp-workspace-cwd-test-{}",
            std::process::id()
        ));
        let file = dir.join("project.baml");

        std::fs::create_dir_all(&dir).expect("temp dir should be created");
        std::fs::write(&file, "function Test() -> int { 1 }\n")
            .expect("temp file should be created");

        assert_eq!(workspace_cwd(&dir), dir);
        assert_eq!(workspace_cwd(&file), file.parent().unwrap());

        let _ = std::fs::remove_file(&file);
        let _ = std::fs::remove_dir(&dir);
    }

    #[test]
    fn standalone_workspace_poller_watches_sources_and_skips_generated_dirs() {
        assert!(watches_standalone_workspace_file(Path::new("baml.toml")));
        assert!(watches_standalone_workspace_file(Path::new(
            "baml_src/main.baml"
        )));
        assert!(!watches_standalone_workspace_file(Path::new(
            ".baml/profiles/run.bamlprof"
        )));
        assert!(should_skip_poll_dir(Path::new(".baml")));
        assert!(should_skip_poll_dir(Path::new("target")));
        assert!(!should_skip_poll_dir(Path::new("baml_src")));
    }

    #[cfg(unix)]
    #[test]
    fn workspace_signature_skips_symlink_cycles() {
        let root = std::env::temp_dir().join(format!(
            "baml-lsp-signature-symlink-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let nested = root.join("nested");
        std::fs::create_dir_all(&nested).expect("temp dir should be created");
        std::fs::write(root.join("main.baml"), "function Test() -> int { 1 }\n")
            .expect("temp file should be created");
        std::os::unix::fs::symlink(&root, nested.join("loop"))
            .expect("symlink loop should be created");

        let signature = workspace_signature(std::slice::from_ref(&root));
        assert_eq!(signature.files.len(), 1);
        assert!(signature.files.contains_key(&root.join("main.baml")));

        let _ = std::fs::remove_file(nested.join("loop"));
        let _ = std::fs::remove_dir_all(&root);
    }
}
