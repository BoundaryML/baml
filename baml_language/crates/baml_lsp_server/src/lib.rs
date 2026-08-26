//! `baml_lsp_server` — the native (stdio) host for the BAML language server.
//!
//! The protocol lives in [`baml_lsp`]; this crate supplies what a process
//! needs around it:
//!
//! - **Transport**: bounded stdio framing in (`read_lsp_message`) and a
//!   budgeted writer thread out (`OutboundFrame`/`OutboundBudget`).
//! - **Admission**: the [`lsp_ingress`] scheduler (per-session lifecycle FSM,
//!   bounded queues, `$/cancelRequest` control path, response ownership).
//! - **The owner thread**: [`lsp_runtime::LspRuntime`] moves the
//!   [`baml_lsp::GlobalState`] into `baml-lsp-owner`, which is the only
//!   thread that ever touches it, and blocks in one `select!` over the ingress
//!   wake, the owner's event queue, and an armed-only timer.
//! - **Executor**: a fixed [`baml_lsp::executor::ThreadPool`] for reads.
//!
//! ```text
//!  stdin ──▶ read_lsp_message ──▶ LspRuntime::submit ──▶ IngressScheduler
//!                                                             │ wake
//!                                                             ▼
//!                                   baml-lsp-owner: GlobalState::dispatch_*
//!                                     │ snapshot reads on the ThreadPool
//!                                     ▼ OwnerEvent::RequestDone
//!  stdout ◀── writer thread ◀── OutboundBudget ◀── responder / ClientSender
//! ```

mod deadlock_watchdog;
pub mod engine;
pub mod lsp_ingress;
pub mod lsp_runtime;
pub mod native_lsp_sender;
pub mod playground_env;
pub mod playground_http;
pub mod playground_io;
pub mod playground_notify;
pub mod playground_runs;
pub mod playground_seam;
pub mod playground_sender;
pub mod playground_server;
pub mod playground_session;
pub mod playground_telemetry;
pub mod playground_ws;

use std::{
    io::{BufRead, Write},
    path::PathBuf,
    sync::Arc,
    time::Duration,
};

use anyhow::Context as _;
use baml_lsp::{GlobalState, OwnerEvent, SessionKey, discovery::NativeFs, executor::Executors};

use crate::{
    lsp_runtime::{LspRuntime, SubmitResult},
    playground_env::{PlaygroundEnv, PlaygroundEnvState},
    playground_http::{PlaygroundHttp, PlaygroundHttpState},
    playground_io::{PlaygroundIo, PlaygroundIoState},
    playground_seam::PlaygroundSeam,
    playground_session::PlaygroundSessionStore,
    playground_ws::WsOutMessage,
};

// ---------------------------------------------------------------------------
// Bounded outbound frames: no transport hides an unbounded writer queue
// ---------------------------------------------------------------------------

const OUTBOUND_QUEUE_BYTES: usize = 64 * 1024 * 1024;
pub(crate) const MAX_OUTBOUND_FRAME_BYTES: usize = 4 * 1024 * 1024;

/// One serialized outbound JSON-RPC frame.
///
/// The body is serialized exactly once — with the `jsonrpc` member included,
/// so the same bytes are valid for stdio Content-Length framing and for any
/// other byte transport — into shared `Arc<[u8]>` storage carrying one budget
/// charge. Clones share the allocation *and* the charge, so the budget
/// accounts real memory exactly.
#[derive(Debug, Clone)]
pub struct OutboundFrame {
    bytes: Arc<[u8]>,
    is_response: bool,
    _charge: Arc<OutboundCharge>,
}

impl OutboundFrame {
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    /// Whether the frame is a JSON-RPC response. Response routing is owned by
    /// the per-session runtime path; broadcast consumers must skip these.
    pub fn is_response(&self) -> bool {
        self.is_response
    }
}

#[derive(Debug)]
pub struct OutboundBudget {
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
pub enum OutboundReserveError {
    Serialization,
    Oversized,
    Saturated,
}

impl OutboundBudget {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            used: std::sync::atomic::AtomicUsize::new(0),
            limit: OUTBOUND_QUEUE_BYTES,
            max_frame: MAX_OUTBOUND_FRAME_BYTES,
        })
    }

    pub fn try_message(
        self: &Arc<Self>,
        message: &lsp_server::Message,
    ) -> Result<OutboundFrame, OutboundReserveError> {
        let is_response = matches!(message, lsp_server::Message::Response(_));
        let bytes =
            serialize_jsonrpc_message(message).map_err(|_| OutboundReserveError::Serialization)?;
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

// ---------------------------------------------------------------------------
// Bootstrap
// ---------------------------------------------------------------------------

/// Where the materialized stdlib stubs live, so goto-definition into the
/// stdlib can open a real file: `BAML_STDLIB_DIR`, else `<exe dir>/../stdlib`
/// (the toolchain copy `baml ide install` writes), else the in-repo
/// `baml_std/` checkout the binary was built from (a compile-time path, so it
/// exists only on the build machine — the dev fallback), else none (the
/// protocol layer then declines stdlib navigation targets). Runs before
/// `initialize`, so a client-supplied
/// `initializationOptions.bamlClient.stdlibDir` — if the protocol layer
/// consumes one — is its concern, not the host's.
fn resolve_stdlib_dir() -> Option<PathBuf> {
    if let Some(dir) = std::env::var_os("BAML_STDLIB_DIR") {
        let dir = PathBuf::from(dir);
        if dir.is_dir() {
            return Some(std::fs::canonicalize(&dir).unwrap_or(dir));
        }
        tracing::warn!(
            path = %dir.display(),
            "BAML_STDLIB_DIR is not a directory; ignoring it"
        );
    }
    let toolchain = std::env::current_exe()
        .ok()
        .and_then(|exe| Some(exe.parent()?.join("..").join("stdlib")))
        .filter(|candidate| candidate.is_dir());
    if let Some(candidate) = toolchain {
        return Some(std::fs::canonicalize(&candidate).unwrap_or(candidate));
    }
    let checkout = PathBuf::from(baml_builtins2::BAML_STD_DIR);
    checkout
        .is_dir()
        .then(|| std::fs::canonicalize(&checkout).unwrap_or(checkout))
}

/// Build the `SysOps` every playground engine runs on: the native platform,
/// with exactly three namespaces intercepted so fetch logs, env prompts and
/// `baml.io` reads reach the webview.
///
/// The base is `native()`, not an empty builder. A playground run is an
/// ordinary program run — it must have the whole platform, and the set of
/// operations it needs is not enumerable here. Building up from
/// `SysOpsBuilder::new()` left everything unlisted throwing `Unsupported`,
/// which broke `baml.time.Instant.now` and so every test run, since
/// `testing.run_test` times each one.
fn build_playground_sys_ops(
    broadcast_tx: &tokio::sync::broadcast::Sender<WsOutMessage>,
    run_store: &Arc<bex_events::run::InMemoryRunStore>,
    env_state: &Arc<PlaygroundEnvState>,
    io_state: &Arc<PlaygroundIoState>,
) -> sys_ops::SysOps {
    use sys_native::SysOpsExt as _;

    let http_state = Arc::new(PlaygroundHttpState::new(
        broadcast_tx.clone(),
        run_store.clone(),
    ));
    sys_ops::SysOpsBuilder::from_ops(sys_ops::SysOps::native())
        .with_http_instance(Arc::new(PlaygroundHttp(http_state)))
        .with_env_instance(Arc::new(PlaygroundEnv(env_state.clone())))
        .with_io_instance(Arc::new(PlaygroundIo(io_state.clone())))
        .build()
}

/// Who the playground is opened for.
#[derive(Clone, Copy, PartialEq, Eq)]
enum PlaygroundOpenTarget {
    /// `baml lsp`: an editor drives the process over stdio and the playground
    /// runs alongside it, for the extension's webview.
    LspClient,
    /// `baml playground`: no editor client — the browser is the only front
    /// end, and the HTTP server is the process's foreground work.
    Browser,
}

/// Options for [`run_playground_server`] (browser mode only).
#[derive(Default)]
pub struct PlaygroundServerOptions {
    /// Bind exactly this port; error if unavailable. `None` scans from 4265.
    pub port: Option<u16>,
    /// Open the local browser once the server is up.
    pub open_browser: bool,
    /// Called once with the bound port, before anything is served. The
    /// process's terminal output belongs to the caller — a library that owns
    /// stdout cannot also be the stdio LSP host — so this is how `baml
    /// playground` prints its banner.
    pub on_listening: Option<Box<dyn FnOnce(u16) + Send>>,
}

impl std::fmt::Debug for PlaygroundServerOptions {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PlaygroundServerOptions")
            .field("port", &self.port)
            .field("open_browser", &self.open_browser)
            .field("on_listening", &self.on_listening.is_some())
            .finish()
    }
}

/// Run the native BAML LSP server over stdio until the client exits.
///
/// Returns `Err` when the client sent `exit` before completing `shutdown`
/// (the LSP lifecycle's abnormal termination), so the process exits nonzero.
pub fn run_server(workspace_roots: Vec<PathBuf>) -> anyhow::Result<()> {
    run_server_inner(
        PlaygroundOpenTarget::LspClient,
        workspace_roots,
        None,
        PlaygroundServerOptions::default(),
    )
}

/// Run the standalone browser playground: the same server, with the HTTP host
/// in the foreground and no stdio client.
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

fn run_server_inner(
    open_target: PlaygroundOpenTarget,
    workspace_roots: Vec<PathBuf>,
    playground_dir_override: Option<PathBuf>,
    options: PlaygroundServerOptions,
) -> anyhow::Result<()> {
    let workspace_roots = absolutize_workspace_roots(workspace_roots)?;

    // Tracing → stderr so vscode-languageclient captures it in the "BAML
    // Language Server" output channel. stdout is the protocol channel.
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info,salsa=warn")),
        )
        .with_ansi(false)
        .init();

    // Panics carry their location and backtrace only at panic time; the
    // `catch_unwind` boundaries downstream (the pool guard, the owner-step
    // guard) see just the payload. Log the full picture here so a field
    // report names the panicking query. Salsa cancellations unwind via
    // `resume_unwind` and never reach the hook, so this fires for real
    // defects only. The default hook is dropped: tracing already writes to
    // stderr.
    std::panic::set_hook(Box::new(|info| {
        let backtrace = std::backtrace::Backtrace::force_capture();
        tracing::error!(
            panic = %info,
            thread = std::thread::current().name().unwrap_or("<unnamed>"),
            %backtrace,
            "panic"
        );
    }));

    apply_single_workspace_cwd(&workspace_roots)?;

    tracing::info!("baml-lsp v{} starting", version());
    deadlock_watchdog::spawn();

    let stdlib_dir = resolve_stdlib_dir();
    if let Some(dir) = &stdlib_dir {
        tracing::info!(path = %dir.display(), "stdlib stubs directory");
    }
    let state = GlobalState::with_fs(Executors::native_default(), stdlib_dir, Arc::new(NativeFs));
    let runtime = LspRuntime::new(state)?;
    let tokio_runtime = tokio::runtime::Runtime::new()?;

    // Outbound LSP frames: bounded and charged against one process budget;
    // there is no unbounded writer queue. In stdio mode a writer thread
    // drains them to stdout; in browser mode a bridge thread forwards them
    // to `/api/lsp` instead.
    let (writer_tx, writer_rx) = crossbeam_channel::bounded::<OutboundFrame>(512);
    let writer_tx = Arc::new(writer_tx);
    let writer_budget = OutboundBudget::new();
    let lsp_sender = Arc::new(native_lsp_sender::NativeLspSender::new(
        &writer_tx,
        &writer_budget,
    ));

    // ── Playground host ──────────────────────────────────────────────────
    let (broadcast_tx, _) = tokio::sync::broadcast::channel::<WsOutMessage>(64);
    // Terminal runs beyond the cap are evicted from memory; the playground
    // rehydrates them on demand from the disk-backed history store.
    let run_store = Arc::new(bex_events::run::InMemoryRunStore::new(
        bex_events::run::RunRetentionPolicy {
            max_terminal_runs: Some(100),
            ..Default::default()
        },
    ));
    let env_state = Arc::new(PlaygroundEnvState::new(
        broadcast_tx.clone(),
        run_store.clone(),
        Arc::new(PlaygroundSessionStore::default()),
    ));
    let io_state = Arc::new(PlaygroundIoState::new(
        broadcast_tx.clone(),
        run_store.clone(),
    ));
    let sys_ops = Arc::new(build_playground_sys_ops(
        &broadcast_tx,
        &run_store,
        &env_state,
        &io_state,
    ));

    // Bind the playground port before anything advertises it. Browser mode is
    // useless without the server; stdio mode keeps serving the editor.
    let (playground_listener, playground_port) = match tokio_runtime.block_on(async {
        match options.port {
            Some(port) => playground_server::bind_exact_port(port)
                .await
                .map(|listener| (listener, port)),
            None => playground_server::pick_port(4265, 100).await,
        }
    }) {
        Ok((listener, port)) => (Some(listener), port),
        Err(error) => {
            if open_target == PlaygroundOpenTarget::Browser {
                return Err(error);
            }
            tracing::error!("Could not find a playground port: {error}");
            (None, 0)
        }
    };
    if let Some(announce) = options.on_listening {
        announce(playground_port);
    }

    // Where the most recent `OpenPlayground` pointed, so a page that connects
    // afterwards (a fresh window, or a reconnect) can be navigated there.
    let current_open_target: playground_sender::SharedOpenTarget =
        Arc::new(std::sync::Mutex::new(None));
    let playground_sender = Arc::new(playground_sender::NativePlaygroundSender::new(
        broadcast_tx.clone(),
        lsp_sender.clone(),
        playground_port,
        open_target == PlaygroundOpenTarget::Browser,
        current_open_target.clone(),
    ));

    let runtimes = Arc::new(engine::RuntimeRegistry::default());
    let seam = PlaygroundSeam::new(
        runtime.clone(),
        runtimes,
        playground_sender.clone(),
        env_state.clone(),
        sys_ops,
    );
    {
        // The pipeline task and its debounce timer need the tokio context.
        let _enter = tokio_runtime.enter();
        seam.spawn_source_pipeline();
    }

    // How this host runs `baml.openBamlPanel`: the sender decides whether
    // that means navigating a browser page or telling the editor to open its
    // webview. Installed before any client connects, because `initialize`
    // reads it to decide whether to advertise code lenses at all.
    let panel_sender = playground_sender.clone();
    runtime
        .owner()
        .post(OwnerEvent::Call(Box::new(move |state| {
            state.set_open_panel_handler(Arc::new(move |request: &baml_lsp::OpenPanelRequest| {
                panel_sender.send_playground_notification(
                    &playground_notify::PlaygroundNotification::OpenPlayground {
                        project: request.project.to_string_lossy().into_owned(),
                        function_name: request.function_name.clone(),
                        test_name: request.test_name.clone(),
                        testset_name: request.testset_name.clone(),
                    },
                );
            }));
        })));

    // Mirror of what the browser editor has per file: written by the
    // `/api/lsp` bridge on didOpen/didChange, read by the disk watcher so the
    // browser's own write-throughs are not echoed back as external changes.
    let doc_mirror: playground_server::DocMirror =
        Arc::new(std::sync::Mutex::new(std::collections::HashMap::new()));
    // Broadcast LSP output destined for `/api/lsp`: root notifications and
    // disk-change pushes. Responses never travel here — the ingress runtime
    // routes those per session.
    let (lsp_out_tx, _lsp_out_rx) = tokio::sync::broadcast::channel::<OutboundFrame>(256);

    if open_target == PlaygroundOpenTarget::Browser {
        // No editor client to announce workspace folders, so discover the CLI
        // roots directly.
        let roots = workspace_roots.clone();
        runtime
            .owner()
            .post(OwnerEvent::Call(Box::new(move |state| {
                for root in roots {
                    tracing::info!(path = %root.display(), "discovering playground root");
                    state.spawn_discovery(baml_lsp::paths::canonical_physical_path(&root));
                }
            })));
    }

    let Some(listener) = playground_listener else {
        anyhow::ensure!(
            open_target == PlaygroundOpenTarget::LspClient,
            "could not start the playground server"
        );
        return run_stdio_loop(
            &runtime,
            &writer_tx,
            &writer_budget,
            writer_rx,
            &lsp_sender,
            &workspace_roots,
        );
    };

    if open_target == PlaygroundOpenTarget::Browser {
        // Nothing drains `writer_rx` in browser mode (no stdout writer), so
        // the bridge owns it and fans LSP output out to `/api/lsp`.
        let lsp_out_tx_bridge = lsp_out_tx.clone();
        std::thread::Builder::new()
            .name("lsp-ws-bridge".into())
            .spawn(move || {
                while let Ok(frame) = writer_rx.recv() {
                    let _ = lsp_out_tx_bridge.send(frame);
                }
            })?;

        if options.open_browser
            && let Some(project) = workspace_roots.first()
        {
            playground_sender.send_playground_notification(
                &playground_notify::PlaygroundNotification::OpenPlayground {
                    project: project.to_string_lossy().into_owned(),
                    function_name: None,
                    test_name: None,
                    testset_name: None,
                },
            );
        }

        // Watch for external edits (another editor, a formatter) for the
        // session's lifetime: they reach the database *and* the browser model.
        let _disk_watcher = playground_server::spawn_disk_watcher(
            &workspace_roots,
            lsp_out_tx.clone(),
            writer_budget,
            doc_mirror.clone(),
            runtime.owner().clone(),
        );

        return tokio_runtime.block_on(playground_server::run(
            listener,
            seam,
            broadcast_tx,
            env_state,
            io_state,
            run_store,
            playground_dir_override,
            lsp_out_tx,
            runtime,
            doc_mirror,
            workspace_roots,
            current_open_target,
        ));
    }

    tokio_runtime.spawn(playground_host_task(
        listener,
        seam,
        broadcast_tx,
        env_state,
        io_state,
        run_store,
        playground_dir_override,
        lsp_out_tx,
        runtime.clone(),
        doc_mirror,
        workspace_roots.clone(),
        current_open_target,
    ));

    run_stdio_loop(
        &runtime,
        &writer_tx,
        &writer_budget,
        writer_rx,
        &lsp_sender,
        &workspace_roots,
    )
}

/// The playground host in editor mode: a background task, whose exit is only
/// worth an error line when it is not "the host was never configured".
#[allow(clippy::too_many_arguments)]
async fn playground_host_task(
    listener: tokio::net::TcpListener,
    seam: Arc<PlaygroundSeam>,
    broadcast_tx: tokio::sync::broadcast::Sender<WsOutMessage>,
    env_state: Arc<PlaygroundEnvState>,
    io_state: Arc<PlaygroundIoState>,
    run_store: Arc<bex_events::run::InMemoryRunStore>,
    playground_dir_override: Option<PathBuf>,
    lsp_out_tx: tokio::sync::broadcast::Sender<OutboundFrame>,
    runtime: Arc<LspRuntime>,
    doc_mirror: playground_server::DocMirror,
    workspace_roots: Vec<PathBuf>,
    current_open_target: playground_sender::SharedOpenTarget,
) {
    let Err(error) = playground_server::run(
        listener,
        seam,
        broadcast_tx,
        env_state,
        io_state,
        run_store,
        playground_dir_override,
        lsp_out_tx,
        runtime,
        doc_mirror,
        workspace_roots,
        current_open_target,
    )
    .await
    else {
        return;
    };
    if error.is::<playground_server::PlaygroundNotConfigured>() {
        tracing::info!("Playground assets are not configured; running without it: {error}");
    } else {
        tracing::error!("Playground server exited: {error}");
    }
}

/// The stdio session and its read loop: the editor drives the process here.
fn run_stdio_loop(
    runtime: &Arc<LspRuntime>,
    writer_tx: &Arc<crossbeam_channel::Sender<OutboundFrame>>,
    writer_budget: &Arc<OutboundBudget>,
    writer_rx: crossbeam_channel::Receiver<OutboundFrame>,
    lsp_sender: &Arc<native_lsp_sender::NativeLspSender>,
    workspace_roots: &[PathBuf],
) -> anyhow::Result<()> {
    // The stdio session: a bounded sink into the writer channel. Saturation
    // is backpressure (the response stays reserved and is retried), never
    // silent loss; a disconnected writer closes the session.
    let stdio_closed = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let stdio_sender = lsp_sender.clone();
    let stdio_sink: lsp_runtime::Sink = Arc::new(move |message| stdio_sender.deliver(&message));
    let stdio_closed_for_endpoint = stdio_closed.clone();
    let stdio_close: lsp_runtime::Close = Arc::new(move || {
        stdio_closed_for_endpoint.store(true, std::sync::atomic::Ordering::Release);
    });
    let after_notification = (!workspace_roots.is_empty()).then(|| {
        let roots = workspace_roots.to_vec();
        let hook: lsp_runtime::NotificationHook = Arc::new(move |state, session, notification| {
            if notification.method == "initialized" {
                apply_cli_workspace_roots(state, session, &roots);
            }
        });
        hook
    });
    let stdio_session = runtime
        .open_session(
            lsp_ingress::TransportKind::Stdio,
            stdio_sink,
            stdio_close,
            after_notification,
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

    // Main thread: bounded framing (capped headers, per-message body
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
            match runtime.submit(stdio_session, msg.clone()) {
                SubmitResult::Accepted | SubmitResult::Dropped => break,
                SubmitResult::Backpressure => {
                    // Reads are rejected under overload; only mutation/
                    // lifecycle reserve pressure stalls the reader briefly.
                    std::thread::sleep(Duration::from_millis(2));
                }
                SubmitResult::Exited { normal } => {
                    abnormal_exit = !normal;
                    terminate = true;
                    break;
                }
                SubmitResult::Closed => {
                    terminate = true;
                    break;
                }
            }
        }
        if terminate || stdio_closed.load(std::sync::atomic::Ordering::Acquire) {
            break;
        }
    }

    runtime.close_session(stdio_session);

    if abnormal_exit {
        // Lifecycle rule: `exit` before a completed shutdown is an abnormal
        // termination (nonzero for stdio).
        anyhow::bail!("LSP client sent exit before completing shutdown");
    }

    tracing::info!("LSP server shutting down");
    Ok(())
}

/// `--workspace` roots given on the command line join the stdio session's
/// workspace folders once the client has finished `initialize`/`initialized`
/// (so the client's own folders, applied by `initialize`, are not clobbered)
/// and are discovered exactly like folders the client announced. A root the
/// client already announced is left alone: `initialized` discovers it.
fn apply_cli_workspace_roots(state: &mut GlobalState, session: SessionKey, roots: &[PathBuf]) {
    let session_state = match state.session_mut(session) {
        Ok(session_state) => session_state,
        Err(error) => {
            tracing::warn!(%error, "could not add --workspace roots to the stdio session");
            return;
        }
    };
    let mut added = Vec::new();
    for root in roots {
        let folder = baml_lsp::paths::canonical_physical_path(root);
        if session_state.workspace_folders.contains(&folder) || added.contains(&folder) {
            continue;
        }
        session_state.workspace_folders.push(folder.clone());
        added.push(folder);
    }
    for folder in added {
        tracing::info!(path = %folder.display(), "discovering --workspace root");
        state.spawn_discovery(folder);
    }
}

/// Run playground/LSP work from inside the project when exactly one root was
/// named on the command line. BAML's own filesystem operations resolve
/// relative paths against the process's current directory, so a run started
/// from the playground must see the project as its working directory — not
/// wherever the terminal (or the editor's spawn) happened to be.
fn apply_single_workspace_cwd(workspace_roots: &[PathBuf]) -> anyhow::Result<()> {
    let [root] = workspace_roots else {
        return Ok(());
    };
    let cwd = if root.is_file() {
        root.parent().unwrap_or(root)
    } else {
        root
    };
    std::env::set_current_dir(cwd)
        .with_context(|| format!("Failed to set current directory to {}", cwd.display()))?;
    tracing::info!(path = %cwd.display(), "working directory");
    Ok(())
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

#[cfg(test)]
mod tests {
    use super::*;

    /// The playground's platform is the native one. Regression for a table
    /// built up from an empty `SysOpsBuilder`, which left everything the host
    /// did not name throwing `Unsupported` — `baml.time.Instant.now` among
    /// them, and `testing.run_test` times every test, so no test could ever
    /// pass in the playground.
    #[tokio::test]
    async fn playground_sys_ops_provide_the_whole_native_platform() {
        let (broadcast_tx, _rx) = tokio::sync::broadcast::channel(8);
        let run_store = Arc::new(bex_events::run::InMemoryRunStore::new(
            bex_events::run::RunRetentionPolicy::default(),
        ));
        let env_state = Arc::new(PlaygroundEnvState::new(
            broadcast_tx.clone(),
            run_store.clone(),
            Arc::new(PlaygroundSessionStore::default()),
        ));
        let io_state = Arc::new(PlaygroundIoState::new(
            broadcast_tx.clone(),
            run_store.clone(),
        ));
        let sys_ops = Arc::new(build_playground_sys_ops(
            &broadcast_tx,
            &run_store,
            &env_state,
            &io_state,
        ));

        let mut db = baml_db::ProjectDatabase::new();
        db.ensure_stdlib_sources();
        let root = db
            .add_source_root(baml_db::SourceRootSpec {
                path: PathBuf::from("/sysops-test"),
                package: baml_db::Name::new(baml_type::RESERVED_USER_PACKAGE),
                kind: baml_db::SourceRootKind::Workspace,
            })
            .unwrap_or_else(|e| unreachable!("fresh database accepts one workspace root: {e}"));
        db.add_or_update_file_in(
            root,
            std::path::Path::new("/sysops-test/main.baml"),
            "function stamp() -> bool throws never {\n    \
             let start = baml.time.Instant.now();\n    \
             start.elapsed().to_milliseconds() >= 0n\n}\n",
        );
        let program = db
            .get_bytecode_unchecked()
            .unwrap_or_else(|e| unreachable!("the fixture compiles: {e}"));

        let engine = Arc::new(
            engine::construct_engine_candidate(program, sys_ops, baml_lsp::SourceRevision(1))
                .unwrap_or_else(|e| unreachable!("the engine constructs: {e}"))
                .into_engine(),
        );
        let context =
            bex_project::FunctionCallContextBuilder::new(sys_types::CallId::next()).build();
        let value = engine
            .call_function("stamp", Vec::new(), context, true)
            .await
            .expect("a timed function runs on the playground platform");
        assert_eq!(value, bex_project::BexExternalValue::Bool(true));
        bex_project::Bex::shutdown(engine).await;
    }

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
            std::fmt::Write::write_fmt(&mut headers, format_args!("X-Filler-{index}: value\r\n"))
                .unwrap();
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
            .try_message(&lsp_server::Message::Notification(
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
            budget.try_message(&oversized).unwrap_err(),
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
    fn absolutize_workspace_roots_makes_relative_paths_absolute() {
        let cwd = std::env::current_dir().expect("cwd should be available");
        let absolute = cwd.join("already-absolute");

        let roots =
            absolutize_workspace_roots(vec![PathBuf::from("relative-workspace"), absolute.clone()])
                .expect("workspace roots should absolutize");

        assert_eq!(roots, vec![cwd.join("relative-workspace"), absolute]);
    }
}
