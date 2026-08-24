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
pub mod lsp_ingress;
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

// ---------------------------------------------------------------------------
// Bounded outbound frames: no transport hides an unbounded writer queue
// ---------------------------------------------------------------------------

const OUTBOUND_QUEUE_BYTES: usize = 64 * 1024 * 1024;
pub(crate) const MAX_OUTBOUND_FRAME_BYTES: usize = 4 * 1024 * 1024;

/// One serialized outbound JSON-RPC frame.
///
/// The body is serialized exactly once — with the `jsonrpc` member included,
/// so the same bytes are valid for stdio Content-Length framing and for a
/// WebSocket text frame — into shared `Arc<[u8]>` storage carrying one budget
/// charge. Clones, including the per-receiver clones made by
/// `tokio::sync::broadcast`, share the allocation *and* the charge, so the
/// budget accounts real memory exactly instead of deep-cloning the payload
/// per receiver while charging once.
#[derive(Debug, Clone)]
pub(crate) struct OutboundFrame {
    bytes: Arc<[u8]>,
    is_response: bool,
    _charge: Arc<OutboundCharge>,
}

impl OutboundFrame {
    pub(crate) fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    /// Whether the frame is a JSON-RPC response. Response routing is owned by
    /// the per-session runtime path; broadcast consumers must skip these.
    pub(crate) fn is_response(&self) -> bool {
        self.is_response
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

impl Drop for OutboundCharge {
    fn drop(&mut self) {
        self.budget
            .used
            .fetch_sub(self.bytes, std::sync::atomic::Ordering::AcqRel);
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum OutboundReserveError {
    Serialization,
    Oversized,
    Saturated,
}

impl OutboundBudget {
    pub(crate) fn new() -> Arc<Self> {
        Arc::new(Self {
            used: std::sync::atomic::AtomicUsize::new(0),
            limit: OUTBOUND_QUEUE_BYTES,
            max_frame: MAX_OUTBOUND_FRAME_BYTES,
        })
    }

    pub(crate) fn try_message(
        self: &Arc<Self>,
        message: lsp_server::Message,
    ) -> Result<OutboundFrame, OutboundReserveError> {
        let is_response = matches!(&message, lsp_server::Message::Response(_));
        let bytes =
            serialize_jsonrpc_message(&message).map_err(|_| OutboundReserveError::Serialization)?;
        self.try_reserve(bytes, is_response)
    }

    /// Raw pre-built JSON (transport-level null-ID protocol errors).
    fn try_raw(self: &Arc<Self>, value: &serde_json::Value) -> Option<OutboundFrame> {
        let bytes = serde_json::to_vec(value).ok()?;
        self.try_reserve(bytes, true).ok()
    }

    fn try_reserve(
        self: &Arc<Self>,
        bytes: Vec<u8>,
        is_response: bool,
    ) -> Result<OutboundFrame, OutboundReserveError> {
        let len = bytes.len();
        if len > self.max_frame {
            return Err(OutboundReserveError::Oversized);
        }
        let mut used = self.used.load(std::sync::atomic::Ordering::Acquire);
        loop {
            if used.saturating_add(len) > self.limit {
                return Err(OutboundReserveError::Saturated);
            }
            match self.used.compare_exchange_weak(
                used,
                used + len,
                std::sync::atomic::Ordering::AcqRel,
                std::sync::atomic::Ordering::Acquire,
            ) {
                Ok(_) => break,
                Err(observed) => used = observed,
            }
        }
        Ok(OutboundFrame {
            bytes: bytes.into(),
            is_response,
            _charge: Arc::new(OutboundCharge {
                budget: self.clone(),
                bytes: len,
            }),
        })
    }
}

/// Serialize with the `jsonrpc` member (plain `serde_json::to_vec` of an
/// `lsp_server::Message` omits it; only the crate's stdio writer adds it).
fn serialize_jsonrpc_message(message: &lsp_server::Message) -> serde_json::Result<Vec<u8>> {
    let mut value = serde_json::to_value(message)?;
    if let Some(object) = value.as_object_mut() {
        object.insert(
            "jsonrpc".to_string(),
            serde_json::Value::String("2.0".to_string()),
        );
    }
    serde_json::to_vec(&value)
}

fn write_frame(output: &mut impl Write, frame: &OutboundFrame) -> std::io::Result<()> {
    write!(output, "Content-Length: {}\r\n\r\n", frame.bytes().len())?;
    output.write_all(frame.bytes())?;
    output.flush()
}

// ---------------------------------------------------------------------------
// Bounded stdio framing: the transport adapter only parses and frames
// ---------------------------------------------------------------------------

/// Per-message body cap. Larger than every ingress class budget, so any frame
/// that passes here is judged (and, if needed, rejected per-message) by the
/// scheduler's admission rather than by the transport.
const MAX_STDIO_FRAME_BYTES: usize = 8 * 1024 * 1024;

/// Total header-block cap. Headers are read through a `Take` so a peer that
/// never sends CRLF cannot grow an unbounded header line in memory.
const MAX_STDIO_HEADER_BYTES: u64 = 16 * 1024;

#[derive(Debug, PartialEq, Eq)]
enum StdioReadError {
    Parse(String),
    InvalidRequest(String),
    Framing(String),
    /// `Content-Length` exceeded [`MAX_STDIO_FRAME_BYTES`]. The body has
    /// already been consumed and discarded in bounded chunks, so the stream
    /// stays in sync and the session stays alive (recoverable per-message).
    OversizedBody {
        content_length: usize,
    },
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
    let mut read_any_header = false;
    let mut remaining_header_bytes = MAX_STDIO_HEADER_BYTES;
    loop {
        header.clear();
        let read = std::io::Read::take(&mut *input, remaining_header_bytes)
            .read_line(&mut header)
            .map_err(|error| StdioReadError::Framing(error.to_string()))?;
        remaining_header_bytes -= read as u64;
        if read == 0 {
            return if read_any_header {
                Err(StdioReadError::Framing(if remaining_header_bytes == 0 {
                    format!("LSP header block exceeds {MAX_STDIO_HEADER_BYTES} bytes")
                } else {
                    "unexpected EOF in LSP headers".to_string()
                }))
            } else {
                Ok(None)
            };
        }
        read_any_header = true;
        let Some(line) = header.strip_suffix("\r\n") else {
            // Either malformed framing or the header cap truncated mid-line.
            return Err(StdioReadError::Framing(if remaining_header_bytes == 0 {
                format!("LSP header block exceeds {MAX_STDIO_HEADER_BYTES} bytes")
            } else {
                "LSP header must end in CRLF".to_string()
            }));
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
            content_length = Some(parsed);
        }
    }
    let content_length = content_length
        .ok_or_else(|| StdioReadError::Framing("missing Content-Length".to_string()))?;
    if content_length > MAX_STDIO_FRAME_BYTES {
        // Consume the body in bounded chunks so the next frame parses from a
        // clean boundary; the caller answers with a per-message error and the
        // session survives.
        discard_exact(input, content_length)?;
        return Err(StdioReadError::OversizedBody { content_length });
    }
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

fn discard_exact(input: &mut impl BufRead, mut remaining: usize) -> Result<(), StdioReadError> {
    let mut chunk = [0u8; 64 * 1024];
    while remaining > 0 {
        let take = remaining.min(chunk.len());
        input
            .read_exact(&mut chunk[..take])
            .map_err(|error| StdioReadError::Framing(error.to_string()))?;
        remaining -= take;
    }
    Ok(())
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PlaygroundExitSeverity {
    Info,
    Error,
}

fn playground_exit_severity(error: &anyhow::Error) -> PlaygroundExitSeverity {
    if error
        .downcast_ref::<playground_server::PlaygroundNotConfigured>()
        .is_some()
    {
        PlaygroundExitSeverity::Info
    } else {
        PlaygroundExitSeverity::Error
    }
}

fn log_playground_exit(error: &anyhow::Error) {
    match playground_exit_severity(error) {
        PlaygroundExitSeverity::Info => {
            tracing::info!(
                "Playground not configured; running without playground support: {error}"
            );
        }
        PlaygroundExitSeverity::Error => {
            tracing::error!("Playground server exited: {error}");
        }
    }
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

    // Stdio sender (LSP client sender): bounded frames charged against one
    // process outbound budget; there is no unbounded writer queue.
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

    // Process-owned ingress runtime: one bounded scheduler + one dispatch
    // worker shared by the stdio loop and every `/api/lsp` browser session.
    let lsp_runtime = lsp_runtime::LspRuntime::new()?;

    // Mirror of the content the browser editor currently has per file. Shared
    // between the `/api/lsp` bridge (writes it on didOpen/didChange) and the disk
    // watcher (reads it to avoid echoing the browser's own write-throughs back as
    // external changes).
    let doc_mirror: playground_server::DocMirror =
        Arc::new(std::sync::Mutex::new(std::collections::HashMap::new()));

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
        print_playground_banner(playground_port, &workspace_roots);
    }

    // Tracks the target of the most recent OpenPlayground so browser-mode pages
    // that connect after the request can be navigated to it (see the sender and
    // the WS `RequestState` handler). Shared between the sender and the server.
    let current_open_target: playground_sender::SharedOpenTarget =
        Arc::new(std::sync::Mutex::new(None));

    // Playground sender (needs port + lsp_sender for OpenPlayground)
    let playground_sender: Arc<dyn bex_project::PlaygroundSender> =
        Arc::new(playground_sender::NativePlaygroundSender::new(
            broadcast_tx.clone(),
            lsp_sender.clone(),
            playground_port,
            matches!(playground_open_target, PlaygroundOpenTarget::Browser),
            current_open_target.clone(),
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
                current_open_target.clone(),
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
                current_open_target.clone(),
            )
            .await
            {
                log_playground_exit(&e);
            }
        });
    } else if matches!(playground_open_target, PlaygroundOpenTarget::Browser) {
        anyhow::bail!("Could not start playground server");
    }

    // The stdio session: a bounded sink into the writer channel. Saturation
    // is backpressure (the response stays reserved and is retried), never
    // silent loss; a disconnected writer closes the session.
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
    let stdio_close: lsp_runtime::Close = Arc::new(move || {
        stdio_closed_for_endpoint.store(true, std::sync::atomic::Ordering::Release);
    });
    let stdio_session = lsp_runtime
        .open_session(
            lsp_ingress::TransportKind::Stdio,
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
                if write_frame(&mut stdout, &frame).is_err() {
                    break;
                }
            }
        })?;

    // Main event loop: bounded framing (capped headers, per-message body
    // rejection) feeding the shared ingress runtime. Lifecycle — including
    // shutdown/exit — is owned by the scheduler; no transport shortcuts.
    let stdin = std::io::stdin();
    let mut stdin = stdin.lock();

    let mut abnormal_exit = false;
    loop {
        let msg = match read_lsp_message(&mut stdin) {
            Ok(Some(msg)) => msg,
            Ok(None) => break,
            Err(error) => {
                let (code, message, recoverable) = match error {
                    StdioReadError::Parse(message) => (-32700, message, true),
                    StdioReadError::InvalidRequest(message) => (-32600, message, true),
                    // The body was discarded without buffering, so the id is
                    // unknown: a null-ID error is the best per-message answer
                    // available. Frames within MAX_STDIO_FRAME_BYTES but over
                    // an ingress class budget do carry their id and get a
                    // typed per-request rejection from admission instead.
                    StdioReadError::OversizedBody { content_length } => (
                        -32803,
                        format!(
                            "dropped LSP frame of {content_length} bytes \
                             (limit {MAX_STDIO_FRAME_BYTES}); session stays open"
                        ),
                        true,
                    ),
                    StdioReadError::Framing(message) => (-32700, message, false),
                };
                let response = serde_json::json!({
                    "jsonrpc": "2.0",
                    "id": serde_json::Value::Null,
                    "error": { "code": code, "message": message },
                });
                let queued = writer_budget
                    .try_raw(&response)
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
                    // Reads are rejected under overload; only mutation/
                    // lifecycle reserve pressure stalls the reader briefly.
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
        // Lifecycle rule: `exit` before a completed shutdown is an abnormal
        // termination (nonzero for stdio).
        anyhow::bail!("LSP client sent exit before completing shutdown");
    }

    tracing::info!("LSP server shutting down");
    Ok(())
}

#[allow(clippy::print_stdout)] // user-facing banner; browser mode has no stdio LSP client
fn print_playground_banner(port: u16, roots: &[PathBuf]) {
    println!("{}", format_playground_banner(port, roots));
}

fn format_playground_banner(port: u16, roots: &[PathBuf]) -> String {
    let root = roots
        .first()
        .map(|r| r.display().to_string())
        .unwrap_or_else(|| "(no workspace roots)".to_string());
    format!(
        "\n  Playground:  http://localhost:{port}/\n  Project:     {root}\n\n  \
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
    fn missing_playground_configuration_is_not_an_error_exit() {
        let unconfigured = anyhow::Error::new(playground_server::PlaygroundNotConfigured);
        assert_eq!(
            playground_exit_severity(&unconfigured),
            PlaygroundExitSeverity::Info
        );

        let real_failure = anyhow::anyhow!("playground listener failed");
        assert_eq!(
            playground_exit_severity(&real_failure),
            PlaygroundExitSeverity::Error
        );
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

    /// Defect containment: an unbounded header line (no CRLF ever) must fail
    /// at the header cap instead of growing an unbounded String.
    #[test]
    fn stdio_header_block_is_capped() {
        let mut endless = std::io::Cursor::new(vec![b'X'; 64 * 1024]);
        let error = read_lsp_message(&mut endless).unwrap_err();
        let StdioReadError::Framing(message) = error else {
            panic!("expected a framing error, got {error:?}");
        };
        assert!(message.contains("header block exceeds"), "{message}");

        // Many small headers also hit the cap.
        let mut headers = String::new();
        for index in 0..2000 {
            headers.push_str(&format!("X-Filler-{index}: value\r\n"));
        }
        headers.push_str("\r\n");
        let mut many = std::io::Cursor::new(headers.into_bytes());
        assert!(matches!(
            read_lsp_message(&mut many),
            Err(StdioReadError::Framing(_))
        ));
    }

    /// Defect containment: an oversized body is discarded in bounded chunks
    /// and reading continues with the next frame — the session stays alive.
    #[test]
    fn oversized_stdio_body_is_recoverable_per_message() {
        let huge_length = MAX_STDIO_FRAME_BYTES + 1;
        let mut stream = format!("Content-Length: {huge_length}\r\n\r\n").into_bytes();
        stream.extend(std::iter::repeat_n(b'x', huge_length));
        stream.extend(framed(
            r#"{"jsonrpc":"2.0","id":7,"method":"shutdown","params":null}"#,
        ));
        let mut input = std::io::Cursor::new(stream);

        assert!(matches!(
            read_lsp_message(&mut input),
            Err(StdioReadError::OversizedBody { content_length }) if content_length == huge_length
        ));
        // The stream is still in sync: the next frame parses normally.
        let Ok(Some(lsp_server::Message::Request(request))) = read_lsp_message(&mut input) else {
            panic!("the frame after an oversized body must parse");
        };
        assert_eq!(request.method, "shutdown");
    }

    #[test]
    fn outbound_frames_share_bytes_and_one_budget_charge() {
        let budget = OutboundBudget::new();
        let frame = budget
            .try_message(lsp_server::Message::Notification(
                lsp_server::Notification::new(
                    "window/logMessage".to_string(),
                    serde_json::json!({ "type": 3, "message": "hello" }),
                ),
            ))
            .unwrap();
        let used_with_one = budget.used.load(std::sync::atomic::Ordering::Acquire);
        assert_eq!(used_with_one, frame.bytes().len());
        assert!(!frame.is_response());
        // Serialized once, with the jsonrpc member for both transports.
        let value: serde_json::Value = serde_json::from_slice(frame.bytes()).unwrap();
        assert_eq!(value["jsonrpc"], "2.0");

        // Broadcast-style clones share the allocation and the charge.
        let clone = frame.clone();
        assert_eq!(
            budget.used.load(std::sync::atomic::Ordering::Acquire),
            used_with_one
        );
        drop(frame);
        assert_eq!(
            budget.used.load(std::sync::atomic::Ordering::Acquire),
            used_with_one
        );
        drop(clone);
        assert_eq!(budget.used.load(std::sync::atomic::Ordering::Acquire), 0);
    }

    #[test]
    fn oversized_outbound_frame_is_rejected_by_the_budget() {
        let budget = OutboundBudget::new();
        let oversized = lsp_server::Message::Notification(lsp_server::Notification::new(
            "test/oversized".to_string(),
            serde_json::Value::String("x".repeat(MAX_OUTBOUND_FRAME_BYTES + 1)),
        ));
        assert_eq!(
            budget.try_message(oversized).unwrap_err(),
            OutboundReserveError::Oversized
        );
        assert_eq!(budget.used.load(std::sync::atomic::Ordering::Acquire), 0);
    }

    #[test]
    fn raw_stdio_error_uses_null_id_and_content_length() {
        let value = serde_json::json!({
            "jsonrpc": "2.0",
            "id": serde_json::Value::Null,
            "error": { "code": -32700, "message": "Parse error" },
        });
        let budget = OutboundBudget::new();
        let frame = budget.try_raw(&value).unwrap();
        let mut output = Vec::new();
        write_frame(&mut output, &frame).unwrap();
        let output = String::from_utf8(output).unwrap();
        assert!(output.starts_with("Content-Length: "));
        assert!(output.contains("\r\n\r\n"));
        assert!(output.contains(r#""id":null"#));
        assert!(output.contains(r#""code":-32700"#));
    }

    #[test]
    fn playground_banner_shows_url_project_root_and_ssh_hint() {
        let banner = format_playground_banner(4265, &[PathBuf::from("/home/dev/my-app")]);
        assert!(banner.contains("http://localhost:4265/"), "{banner}");
        assert!(banner.contains("/home/dev/my-app"), "{banner}");
        assert!(
            banner.contains("ssh -L 4265:localhost:4265 <user@host>"),
            "{banner}"
        );
        assert!(banner.contains("Press Ctrl-C to stop."), "{banner}");

        let no_roots = format_playground_banner(4270, &[]);
        assert!(no_roots.contains("(no workspace roots)"), "{no_roots}");
        assert!(no_roots.contains("http://localhost:4270/"), "{no_roots}");
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
            ".baml/profiles-v1/runs/run.meta"
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
