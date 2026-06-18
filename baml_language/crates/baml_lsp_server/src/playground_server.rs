//! Lightweight HTTP server for the BAML Playground.
//!
//! Two modes controlled by environment variables:
//!
//! **Dev mode** (`BAML_PLAYGROUND_DEV_PORT` is set):
//!   Reverse-proxies all non-API requests to a local Vite dev server.
//!
//! **Prod mode** (`BAML_PLAYGROUND_DIR` is set):
//!   Serves pre-built static assets with SPA fallback.

use std::{
    net::SocketAddr,
    path::{Path, PathBuf},
    sync::Arc,
};

use axum::{
    Router,
    body::Body,
    extract::{
        FromRequestParts, Path as AxumPath, Query, State,
        ws::{Message as AxumWsMsg, WebSocket, WebSocketUpgrade},
    },
    http::{Method, Request, StatusCode, header},
    middleware::{self, Next},
    response::Response,
    routing::{get, get_service},
};
use base64::Engine as _;
use bex_events::run::{
    AttachRootTraceResult, CancellationState, ExecutionRequest, HostCallId, InMemoryRunStore,
    ProjectGeneration, ProjectId, RequestId, RunCursor, RunCursorExpiredReason, RunError,
    RunErrorClass, RunFilter, RunId, RunKind, RunOutcome, RunResult, RunSubscription, RunTarget,
    RunVisibilityFilter,
};
use bex_project::{is_cancelled_engine_error, is_cancelled_runtime_error};
use futures::{SinkExt, stream::StreamExt};
use prost::Message;
use tokio::{net::TcpListener, sync::broadcast};

use crate::{
    playground_env::PlaygroundEnvState,
    playground_io::PlaygroundIoState,
    playground_runs::{patch_to_wire, run_summary_to_wire, run_to_wire},
    playground_ws::{RunListFilter, RunListKind, RunListVisibility, WsInMessage, WsOutMessage},
};

fn to_ws_text(msg: &WsOutMessage) -> Option<AxumWsMsg> {
    match serde_json::to_string(msg) {
        Ok(json) => Some(AxumWsMsg::Text(json.into())),
        Err(e) => {
            tracing::error!("Playground WS: failed to serialize message: {e}");
            None
        }
    }
}

fn epoch_ms() -> u64 {
    let millis = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    u64::try_from(millis).unwrap_or(u64::MAX)
}

fn broadcast_run_started(
    broadcast_tx: &broadcast::Sender<WsOutMessage>,
    run_store: &InMemoryRunStore,
    run_id: bex_events::run::RunId,
    request_id: Option<u64>,
) {
    if let Some(run) = run_store.snapshot(run_id) {
        let _ = broadcast_tx.send(WsOutMessage::RunStarted {
            request_id,
            run: run_to_wire(&run),
        });
    }
}

fn broadcast_run_patch(
    broadcast_tx: &broadcast::Sender<WsOutMessage>,
    patch: &bex_events::run::RunPatch,
) {
    let _ = broadcast_tx.send(WsOutMessage::RunPatch {
        patch: patch_to_wire(patch),
    });
}

async fn send_ws(sink: &mut futures::stream::SplitSink<WebSocket, AxumWsMsg>, msg: &WsOutMessage) {
    if let Some(ws_msg) = to_ws_text(msg) {
        let _ = sink.send(ws_msg).await;
    }
}

#[derive(Clone, Copy)]
struct FunctionRunClient {
    request_id: u64,
}

impl FunctionRunClient {
    fn request_id(self) -> u64 {
        self.request_id
    }

    fn run_started_request_id(self) -> Option<u64> {
        Some(self.request_id)
    }

    fn error(self, code: &'static str, message: String) -> WsOutMessage {
        WsOutMessage::CommandError {
            request_id: self.request_id,
            code: code.to_string(),
            message,
        }
    }
}

#[derive(Clone, Debug)]
struct FunctionRunTarget {
    call_function_name: String,
    run_target: RunTarget,
}

impl FunctionRunTarget {
    fn function(function_name: String) -> Self {
        Self {
            call_function_name: function_name.clone(),
            run_target: RunTarget::Function { function_name },
        }
    }

    fn preview(parent_function_name: String, helper: String, function_name: String) -> Self {
        Self {
            call_function_name: function_name,
            run_target: RunTarget::Preview {
                parent_function_name,
                helper,
            },
        }
    }
}

#[allow(clippy::result_large_err)]
fn parse_run_id_for_request(request_id: u64, run_id: &str) -> Result<RunId, WsOutMessage> {
    RunId::from_wire_str(run_id).ok_or_else(|| WsOutMessage::CommandError {
        request_id,
        code: "invalidRunId".to_string(),
        message: format!("Invalid runId: {run_id}"),
    })
}

fn cursor_expired_reason_to_wire(reason: RunCursorExpiredReason) -> &'static str {
    match reason {
        RunCursorExpiredReason::Expired => "expired",
        RunCursorExpiredReason::Compacted => "compacted",
        RunCursorExpiredReason::Unknown => "unknown",
        RunCursorExpiredReason::Future => "future",
        RunCursorExpiredReason::Unavailable => "unavailable",
    }
}

fn run_filter_from_wire(filter: Option<RunListFilter>) -> RunFilter {
    let Some(filter) = filter else {
        return RunFilter::default();
    };
    RunFilter {
        project_id: filter.project_id.map(ProjectId),
        project_generation: filter.project_generation.map(ProjectGeneration),
        kinds: filter
            .kinds
            .unwrap_or_default()
            .into_iter()
            .map(run_kind_from_wire)
            .collect(),
        statuses: Vec::new(),
        call_tree_contains_function: filter.call_tree_contains_function,
        visibility: filter
            .visibility
            .map(run_visibility_from_wire)
            .unwrap_or_default(),
    }
}

fn run_kind_from_wire(kind: RunListKind) -> RunKind {
    match kind {
        RunListKind::Function => RunKind::Function,
        RunListKind::Test => RunKind::Test,
        RunListKind::Preview => RunKind::Preview,
        RunListKind::Companion => RunKind::Companion,
        RunListKind::Internal => RunKind::Internal,
    }
}

fn run_visibility_from_wire(visibility: RunListVisibility) -> RunVisibilityFilter {
    match visibility {
        RunListVisibility::HistoryOnly => RunVisibilityFilter::HistoryOnly,
        RunListVisibility::IncludeHidden => RunVisibilityFilter::IncludeHidden,
        RunListVisibility::AllForDebug => RunVisibilityFilter::AllForDebug,
    }
}

fn runtime_error_class(err: &bex_project::RuntimeError) -> RunErrorClass {
    match err {
        bex_project::RuntimeError::InvalidArgument { .. } => RunErrorClass::Validation,
        bex_project::RuntimeError::Access(_) => RunErrorClass::Host,
        bex_project::RuntimeError::Other(_) | bex_project::RuntimeError::Compilation { .. } => {
            RunErrorClass::Host
        }
        bex_project::RuntimeError::Engine(_) => RunErrorClass::Runtime,
    }
}

fn runtime_error_outcome(err: &bex_project::RuntimeError) -> RunOutcome {
    if is_cancelled_runtime_error(err) {
        let now = epoch_ms();
        return RunOutcome::Cancelled(CancellationState {
            requested_at_ms: now,
            completed_at_ms: Some(now),
            reason: Some(format!("{err}")),
        });
    }
    RunOutcome::Failed(RunError {
        class: runtime_error_class(err),
        message: format!("{err}"),
        details: None,
    })
}

fn host_error_outcome(message: String) -> RunOutcome {
    RunOutcome::Failed(RunError {
        class: RunErrorClass::Host,
        message,
        details: None,
    })
}

/// Find an available TCP port starting from `base_port`.
pub async fn pick_port(base_port: u16, max_attempts: u16) -> anyhow::Result<(TcpListener, u16)> {
    for offset in 0..max_attempts {
        let port = base_port + offset;
        let addr = SocketAddr::from(([127, 0, 0, 1], port));
        match TcpListener::bind(addr).await {
            Ok(listener) => return Ok((listener, port)),
            Err(_) => continue,
        }
    }
    anyhow::bail!(
        "Could not find an available port in range {}..{}",
        base_port,
        base_port + max_attempts
    )
}

// ---------------------------------------------------------------------------
// Shared state for Axum handlers
// ---------------------------------------------------------------------------

#[derive(Clone)]
struct WsState {
    bex: Arc<dyn bex_project::BexLsp>,
    broadcast_tx: broadcast::Sender<WsOutMessage>,
    env_state: Arc<PlaygroundEnvState>,
    io_state: Arc<PlaygroundIoState>,
    run_store: Arc<InMemoryRunStore>,
    /// LSP output (responses + publishDiagnostics) destined for `/api/lsp`.
    lsp_out_tx: broadcast::Sender<lsp_server::Message>,
}

#[derive(Debug, serde::Deserialize)]
struct SourceFilesQuery {
    project: String,
}

#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct SourceFilesResponse {
    project: String,
    files: Vec<bex_project::PlaygroundSourceFile>,
}

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct UpdateSourceFileRequest {
    project: String,
    path: String,
    content: String,
}

#[derive(Debug, serde::Serialize)]
struct UpdateSourceFileResponse {
    ok: bool,
}

/// Start the playground server on the given listener.
#[allow(clippy::too_many_arguments)]
pub async fn run(
    listener: TcpListener,
    bex: Arc<dyn bex_project::BexLsp>,
    broadcast_tx: broadcast::Sender<WsOutMessage>,
    env_state: Arc<PlaygroundEnvState>,
    io_state: Arc<PlaygroundIoState>,
    run_store: Arc<InMemoryRunStore>,
    playground_dir_override: Option<PathBuf>,
    lsp_out_tx: broadcast::Sender<lsp_server::Message>,
) -> anyhow::Result<()> {
    let app = build_router(
        bex,
        broadcast_tx,
        env_state,
        io_state,
        run_store,
        playground_dir_override,
        lsp_out_tx,
    )?;

    tracing::info!(
        "Playground: http://localhost:{}",
        listener.local_addr()?.port()
    );

    axum::serve(listener, app)
        .await
        .map_err(|e| anyhow::anyhow!("Playground server error: {e}"))
}

#[allow(clippy::too_many_arguments)]
fn build_router(
    bex: Arc<dyn bex_project::BexLsp>,
    broadcast_tx: broadcast::Sender<WsOutMessage>,
    env_state: Arc<PlaygroundEnvState>,
    io_state: Arc<PlaygroundIoState>,
    run_store: Arc<InMemoryRunStore>,
    playground_dir_override: Option<PathBuf>,
    lsp_out_tx: broadcast::Sender<lsp_server::Message>,
) -> anyhow::Result<Router> {
    let ws_state = WsState {
        bex,
        broadcast_tx,
        env_state,
        io_state,
        run_store,
        lsp_out_tx,
    };

    let api = Router::new()
        .route("/api/ws", get(playground_ws_handler))
        .route("/api/lsp", get(lsp_ws_handler))
        .route(
            "/api/source-files",
            get(source_files_handler).put(update_source_file_handler),
        )
        .with_state(ws_state);

    let fallback = if let Some(dir) = playground_dir_override {
        tracing::info!("Playground: serving static files from {}", dir.display());
        static_router(dir.to_string_lossy().into_owned())
    } else if let Ok(dev_port) = std::env::var("BAML_PLAYGROUND_DEV_PORT") {
        let dev_port: u16 = dev_port
            .parse()
            .map_err(|e| anyhow::anyhow!("Invalid BAML_PLAYGROUND_DEV_PORT: {e}"))?;
        tracing::info!("Playground: dev proxy -> http://localhost:{dev_port}");
        dev_proxy_router(format!("http://localhost:{dev_port}"))
    } else if let Ok(dir) = std::env::var("BAML_PLAYGROUND_DIR") {
        tracing::info!("Playground: serving static files from {dir}");
        static_router(dir)
    } else {
        anyhow::bail!(
            "Playground server requires either BAML_PLAYGROUND_DEV_PORT or BAML_PLAYGROUND_DIR"
        )
    };

    Ok(api
        .fallback_service(fallback)
        .layer(middleware::from_fn(cors_middleware)))
}

// ---------------------------------------------------------------------------
// WebSocket handler
// ---------------------------------------------------------------------------

async fn playground_ws_handler(State(state): State<WsState>, ws: WebSocketUpgrade) -> Response {
    tracing::info!("Playground: /api/ws upgrade request received");
    ws.on_upgrade(move |socket| playground_ws_session(socket, state))
}

// ---------------------------------------------------------------------------
// LSP-over-WebSocket bridge (`/api/lsp`)
//
// A browser Monaco language client speaks plain JSON-RPC (one message per WS
// text frame, no Content-Length framing). We parse each frame into an
// `lsp_server::Message`, dispatch it to `BexLsp`, and forward the server's
// output (responses + publishDiagnostics) back over the socket.
//
// The browser is a normal LSP client and therefore "owns" the file on disk;
// since it has no real disk access, this bridge writes edits through to disk
// on `didSave`, capturing the latest full text from didOpen/didChange.
// ---------------------------------------------------------------------------

async fn lsp_ws_handler(State(state): State<WsState>, ws: WebSocketUpgrade) -> Response {
    tracing::info!("Playground: /api/lsp upgrade request received");
    ws.on_upgrade(move |socket| lsp_ws_session(socket, state))
}

/// Serialize an `lsp_server::Message` as a JSON-RPC text frame (adds the
/// `jsonrpc` field that `lsp_server` only writes via its stdio framing).
fn lsp_message_to_ws_text(msg: &lsp_server::Message) -> Option<AxumWsMsg> {
    let mut value = match serde_json::to_value(msg) {
        Ok(v) => v,
        Err(e) => {
            tracing::error!("LSP WS: failed to serialize message: {e}");
            return None;
        }
    };
    if let Some(obj) = value.as_object_mut() {
        obj.insert(
            "jsonrpc".to_string(),
            serde_json::Value::String("2.0".to_string()),
        );
    }
    match serde_json::to_string(&value) {
        Ok(text) => Some(AxumWsMsg::Text(text.into())),
        Err(e) => {
            tracing::error!("LSP WS: failed to stringify message: {e}");
            None
        }
    }
}

async fn lsp_ws_session(socket: WebSocket, state: WsState) {
    tracing::info!("Playground: LSP WS session started");
    let (mut sink, mut stream) = socket.split();
    let mut lsp_rx = state.lsp_out_tx.subscribe();

    // Dispatch `bex` LSP work (which can recompile the project on every keystroke
    // via didChange) on a DEDICATED thread instead of the async WS task. Calling
    // the synchronous, CPU-heavy `bex.handle_*` inline would block this task's
    // select! — so a save (and its disk write-through) would queue behind the
    // backlog of didChange recompiles and only land once the user stops typing.
    // The thread processes messages in order; the WS loop stays responsive.
    let (dispatch_tx, dispatch_rx) = std::sync::mpsc::channel::<lsp_server::Message>();
    let bex = state.bex.clone();
    let _dispatch_thread = std::thread::Builder::new()
        .name("lsp-ws-dispatch".into())
        .spawn(move || {
            while let Ok(msg) = dispatch_rx.recv() {
                match msg {
                    lsp_server::Message::Request(req) => bex.handle_request(req),
                    lsp_server::Message::Notification(notif) => bex.handle_notification(notif),
                    lsp_server::Message::Response(_) => {}
                }
            }
        });

    // Debounced write-through so edits persist WITHOUT a manual save: didChange
    // streams the full document on every keystroke; we coalesce and write the
    // latest text to disk after a short quiet period. (didSave still writes
    // immediately, below.)
    let (write_tx, mut write_rx) =
        tokio::sync::mpsc::unbounded_channel::<(String, String)>();
    tokio::spawn(async move {
        let debounce = std::time::Duration::from_millis(700);
        let mut latest: std::collections::HashMap<String, String> =
            std::collections::HashMap::new();
        loop {
            // Wait for the first pending change (or exit when the session ends).
            match write_rx.recv().await {
                Some((uri, text)) => {
                    latest.insert(uri, text);
                }
                None => break,
            }
            // Coalesce further changes until the edit stream goes quiet.
            let flush = loop {
                match tokio::time::timeout(debounce, write_rx.recv()).await {
                    Ok(Some((uri, text))) => {
                        latest.insert(uri, text);
                    }
                    Ok(None) => break true,  // channel closed: flush then exit
                    Err(_) => break false,   // quiet period elapsed: flush
                }
            };
            for (uri, text) in latest.drain() {
                write_lsp_document_to_disk(&uri, &text);
            }
            if flush {
                break;
            }
        }
    });

    // Latest full text per document URI, captured from didOpen/didChange so we
    // can write it through to disk on didSave.
    let mut pending_text: std::collections::HashMap<String, String> =
        std::collections::HashMap::new();

    loop {
        tokio::select! {
            client_msg = stream.next() => {
                match client_msg {
                    Some(Ok(AxumWsMsg::Text(text))) => {
                        let text_str: &str = &text;
                        handle_lsp_client_text(text_str, &dispatch_tx, &write_tx, &mut sink, &mut pending_text).await;
                    }
                    Some(Ok(AxumWsMsg::Close(_))) | None => break,
                    _ => {}
                }
            }
            out_msg = lsp_rx.recv() => {
                match out_msg {
                    Ok(msg) => {
                        if let Some(ws_msg) = lsp_message_to_ws_text(&msg)
                            && sink.send(ws_msg).await.is_err()
                        {
                            break;
                        }
                    }
                    Err(broadcast::error::RecvError::Lagged(n)) => {
                        tracing::warn!("LSP WS: broadcast lagged by {n} messages");
                    }
                    Err(broadcast::error::RecvError::Closed) => break,
                }
            }
        }
    }

    // Dropping `dispatch_tx` ends the dispatch thread (its recv() returns Err).
    drop(dispatch_tx);
    tracing::debug!("LSP WS session ended");
}

async fn handle_lsp_client_text(
    text: &str,
    dispatch_tx: &std::sync::mpsc::Sender<lsp_server::Message>,
    write_tx: &tokio::sync::mpsc::UnboundedSender<(String, String)>,
    sink: &mut futures::stream::SplitSink<WebSocket, AxumWsMsg>,
    pending_text: &mut std::collections::HashMap<String, String>,
) {
    let msg = match serde_json::from_str::<lsp_server::Message>(text) {
        Ok(m) => m,
        Err(e) => {
            tracing::warn!("LSP WS: invalid JSON-RPC message: {e}");
            return;
        }
    };

    match msg {
        lsp_server::Message::Request(req) => {
            // Mirror the stdio loop: answer `shutdown` directly without
            // dispatching it into BexLsp.
            if req.method == "shutdown" {
                let response = lsp_server::Response {
                    id: req.id,
                    result: Some(serde_json::Value::Null),
                    error: None,
                };
                if let Some(ws_msg) = lsp_message_to_ws_text(&lsp_server::Message::Response(response))
                {
                    let _ = sink.send(ws_msg).await;
                }
                return;
            }
            let _ = dispatch_tx.send(lsp_server::Message::Request(req));
        }
        lsp_server::Message::Notification(notif) => {
            if notif.method == "exit" {
                return;
            }
            // Capture text and persist to disk RIGHT HERE on the WS read loop —
            // not on the dispatch thread — so persistence is immediate and
            // independent of the recompile backlog. `pending_text` already holds
            // the latest didChange text because messages are read in order.
            // didSave writes immediately; didChange schedules a debounced write.
            // BexLsp still sees the notification (via the dispatch thread).
            track_and_persist_lsp_notification(&notif, pending_text, write_tx);
            let _ = dispatch_tx.send(lsp_server::Message::Notification(notif));
        }
        lsp_server::Message::Response(_) => {
            // Responses to server-initiated requests; the stdio loop ignores
            // these as well.
        }
    }
}

/// Track the latest document text and, on save, write it through to disk.
fn track_and_persist_lsp_notification(
    notif: &lsp_server::Notification,
    pending_text: &mut std::collections::HashMap<String, String>,
    write_tx: &tokio::sync::mpsc::UnboundedSender<(String, String)>,
) {
    let uri = notif
        .params
        .pointer("/textDocument/uri")
        .and_then(serde_json::Value::as_str);

    match notif.method.as_str() {
        "textDocument/didOpen" => {
            if let (Some(uri), Some(text)) = (
                uri,
                notif
                    .params
                    .pointer("/textDocument/text")
                    .and_then(serde_json::Value::as_str),
            ) {
                pending_text.insert(uri.to_string(), text.to_string());
            }
        }
        "textDocument/didChange" => {
            // FULL sync mode: a single change event carries the whole document.
            if let (Some(uri), Some(text)) = (
                uri,
                notif
                    .params
                    .pointer("/contentChanges/0/text")
                    .and_then(serde_json::Value::as_str),
            ) {
                pending_text.insert(uri.to_string(), text.to_string());
                // Schedule a debounced write so the edit persists without a save.
                let _ = write_tx.send((uri.to_string(), text.to_string()));
            }
        }
        "textDocument/didSave" => {
            if let Some(uri) = uri {
                let text = notif
                    .params
                    .pointer("/text")
                    .and_then(serde_json::Value::as_str)
                    .map(str::to_string)
                    .or_else(|| pending_text.get(uri).cloned());
                if let Some(text) = text {
                    write_lsp_document_to_disk(uri, &text);
                }
            }
        }
        "textDocument/didClose" => {
            if let Some(uri) = uri {
                pending_text.remove(uri);
            }
        }
        _ => {}
    }
}

fn write_lsp_document_to_disk(uri: &str, text: &str) {
    let Some(path) = lsp_types::Url::parse(uri)
        .ok()
        .and_then(|u| u.to_file_path().ok())
    else {
        tracing::warn!("LSP WS: cannot resolve file path for uri {uri}");
        return;
    };
    match std::fs::write(&path, text) {
        Ok(()) => tracing::debug!("LSP WS: wrote {} ({} bytes)", path.display(), text.len()),
        Err(e) => tracing::warn!("LSP WS: failed to write {}: {e}", path.display()),
    }
}

async fn source_files_handler(
    State(state): State<WsState>,
    Query(query): Query<SourceFilesQuery>,
) -> Response {
    match state.bex.playground_source_files(&query.project) {
        Ok(files) => json_response(
            StatusCode::OK,
            &SourceFilesResponse {
                project: query.project,
                files,
            },
        ),
        Err(error) => text_response(StatusCode::INTERNAL_SERVER_ERROR, error.to_string()),
    }
}

async fn update_source_file_handler(
    State(state): State<WsState>,
    request: Request<Body>,
) -> Response {
    let body = match axum::body::to_bytes(request.into_body(), 1024 * 1024).await {
        Ok(body) => body,
        Err(error) => {
            return text_response(
                StatusCode::BAD_REQUEST,
                format!("Failed to read request body: {error}"),
            );
        }
    };
    let request = match serde_json::from_slice::<UpdateSourceFileRequest>(&body) {
        Ok(request) => request,
        Err(error) => {
            return text_response(
                StatusCode::BAD_REQUEST,
                format!("Invalid source file update body: {error}"),
            );
        }
    };

    match state
        .bex
        .playground_update_source_file(&request.project, &request.path, request.content)
    {
        Ok(()) => {
            state.bex.request_playground_state();
            json_response(StatusCode::OK, &UpdateSourceFileResponse { ok: true })
        }
        Err(error) => text_response(StatusCode::INTERNAL_SERVER_ERROR, error.to_string()),
    }
}

fn json_response<T: serde::Serialize>(status: StatusCode, value: &T) -> Response {
    match serde_json::to_vec(value) {
        Ok(body) => Response::builder()
            .status(status)
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(body))
            .unwrap_or_else(|_| Response::new(Body::empty())),
        Err(error) => text_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to serialize response: {error}"),
        ),
    }
}

fn text_response(status: StatusCode, message: String) -> Response {
    Response::builder()
        .status(status)
        .header(header::CONTENT_TYPE, "text/plain; charset=utf-8")
        .body(Body::from(message))
        .unwrap_or_else(|_| Response::new(Body::empty()))
}

async fn playground_ws_session(socket: WebSocket, state: WsState) {
    tracing::info!("Playground: WS session started");
    let (mut sink, mut stream) = socket.split();

    if let Some(hello) = to_ws_text(&WsOutMessage::Hello {
        toolchain_version: baml_version::CANONICAL_VERSION.to_string(),
        playground_protocol: 1,
        min_client_playground_protocol: 1,
        capabilities: vec![
            "playgroundWebSocket.v1".to_string(),
            "callFunction.v1".to_string(),
            "collectTests.v1".to_string(),
            "sourceFiles.v1".to_string(),
        ],
    }) {
        if sink.send(hello).await.is_err() {
            return;
        }
    } else {
        return;
    }

    if let Some(ready) = to_ws_text(&WsOutMessage::Ready) {
        if sink.send(ready).await.is_err() {
            return;
        }
    } else {
        return;
    }

    let mut broadcast_rx = state.broadcast_tx.subscribe();

    // Send all process env vars so the UI can display them immediately.
    {
        let vars: std::collections::HashMap<String, String> = std::env::vars().collect();
        if let Some(msg) = to_ws_text(&WsOutMessage::ProcessEnvVars { vars })
            && sink.send(msg).await.is_err()
        {
            return;
        }
    }

    // Send env var names referenced in BAML source code.
    {
        let names = state.bex.all_env_var_names();
        if let Some(msg) = to_ws_text(&WsOutMessage::KnownEnvVarNames { names })
            && sink.send(msg).await.is_err()
        {
            return;
        }
    }

    // Send current playground state.
    state.bex.request_playground_state();

    loop {
        tokio::select! {
            client_msg = stream.next() => {
                match client_msg {
                    Some(Ok(AxumWsMsg::Text(text))) => {
                        let text_str: &str = &text;
                        match serde_json::from_str::<WsInMessage>(text_str) {
                            Ok(msg) => {
                                handle_ws_in_message(msg, &state, &mut sink).await;
                            }
                            Err(e) => {
                                tracing::warn!("Playground WS: invalid message: {e}");
                            }
                        }
                    }
                    Some(Ok(AxumWsMsg::Close(_))) | None => break,
                    _ => {}
                }
            }
            broadcast_msg = broadcast_rx.recv() => {
                match broadcast_msg {
                    Ok(msg) => {
                        if let Some(ws_msg) = to_ws_text(&msg)
                            && sink.send(ws_msg).await.is_err()
                        {
                            break;
                        }
                    }
                    Err(broadcast::error::RecvError::Lagged(n)) => {
                        tracing::warn!("Playground WS: broadcast lagged by {n} messages");
                    }
                    Err(broadcast::error::RecvError::Closed) => break,
                }
            }
        }
    }

    tracing::debug!("Playground WS session ended");
}

async fn handle_function_run(
    client: FunctionRunClient,
    project: String,
    target: FunctionRunTarget,
    args_bytes: String,
    call_id: sys_types::CallId,
    state: &WsState,
    sink: &mut futures::stream::SplitSink<WebSocket, AxumWsMsg>,
) {
    let decoded = match base64::engine::general_purpose::STANDARD.decode(&args_bytes) {
        Ok(d) => d,
        Err(e) => {
            send_ws(
                sink,
                &client.error("invalidBase64", format!("Invalid base64: {e}")),
            )
            .await;
            return;
        }
    };

    let kwargs = match bridge_ctypes::playground_run_args_to_bex_values(
        decoded.as_slice(),
        &bridge_ctypes::HANDLE_TABLE,
    ) {
        Ok(k) => k,
        Err(e) => {
            send_ws(
                sink,
                &client.error(
                    "invalidArguments",
                    format!("Failed to convert arguments: {e}"),
                ),
            )
            .await;
            return;
        }
    };

    let broadcast_tx = state.broadcast_tx.clone();
    let project_generation = state.bex.project_generation(&project).unwrap_or(0);
    let fs_path = bex_project::FsPath::from_str(project);
    let function_call_ctx = bex_project::FunctionCallContextBuilder::new(call_id);

    let bex = match state.bex.get_bex_for_project(&fs_path) {
        Ok(bex) => bex,
        Err(e) => {
            send_ws(
                sink,
                &client.error(
                    "projectMissing",
                    format!("Failed to get Bex for project: {e}"),
                ),
            )
            .await;
            return;
        }
    };

    let run_store = state.run_store.clone();
    let start = run_store.create_run(
        ExecutionRequest {
            project_id: ProjectId(fs_path.as_path().to_string_lossy().to_string()),
            project_generation: ProjectGeneration(project_generation),
            target: target.run_target.clone(),
            args_summary: None,
            options_summary: None,
        },
        RequestId(client.request_id()),
    );
    broadcast_run_started(
        &broadcast_tx,
        &run_store,
        start.run_id,
        client.run_started_request_id(),
    );
    if let Some(patch) = run_store.attach_host_call(start.run_id, HostCallId::Native(call_id)) {
        broadcast_run_patch(&broadcast_tx, &patch);
    }

    tokio::spawn(async move {
        let handle_options = bridge_ctypes::CffiHandleTableOptions::for_wire();
        let function_name = target.call_function_name;
        match bex
            .call_function_with_trace(&function_name, kwargs.into(), function_call_ctx.build())
            .await
        {
            Ok(traced) => {
                match run_store.attach_root_trace(start.run_id, traced.entry_call_ref) {
                    AttachRootTraceResult::Attached { patches } => {
                        for patch in patches {
                            broadcast_run_patch(&broadcast_tx, &patch);
                        }
                    }
                    AttachRootTraceResult::AlreadyAttached => {}
                    AttachRootTraceResult::RunMissing => {
                        tracing::warn!("RunStore missing run {}", start.run_id.to_wire_string());
                    }
                    AttachRootTraceResult::Conflict { existing } => {
                        tracing::warn!(
                            "RunStore root trace conflict for {}: existing {}",
                            start.run_id.to_wire_string(),
                            existing.encode()
                        );
                    }
                }

                match traced.value {
                    Ok(result) => {
                        match bridge_ctypes::external_to_outbound(&result, &handle_options) {
                            Ok(baml_val) => {
                                let b64 = base64::engine::general_purpose::STANDARD
                                    .encode(baml_val.encode_to_vec());
                                if let Some(patch) = run_store.complete_run(
                                    start.run_id,
                                    RunOutcome::Succeeded(RunResult {
                                        value: Some(b64.clone()),
                                        renderer_hint: Some("baml.outbound.base64".to_string()),
                                        supporting_payload_ids: Vec::new(),
                                    }),
                                    epoch_ms(),
                                ) {
                                    broadcast_run_patch(&broadcast_tx, &patch);
                                }
                            }
                            Err(e) => {
                                let message = format!("Failed to encode result: {e}");
                                if let Some(patch) = run_store.complete_run(
                                    start.run_id,
                                    host_error_outcome(message.clone()),
                                    epoch_ms(),
                                ) {
                                    broadcast_run_patch(&broadcast_tx, &patch);
                                }
                            }
                        }
                    }
                    Err(e) => {
                        if let Some(patch) = run_store.complete_run(
                            start.run_id,
                            runtime_error_outcome(&e),
                            epoch_ms(),
                        ) {
                            broadcast_run_patch(&broadcast_tx, &patch);
                        }
                    }
                }
            }
            Err(e) => {
                if let Some(patch) =
                    run_store.complete_run(start.run_id, runtime_error_outcome(&e), epoch_ms())
                {
                    broadcast_run_patch(&broadcast_tx, &patch);
                }
            }
        };
    });
}

fn handle_test_run(
    client: FunctionRunClient,
    project: String,
    generation: u64,
    test_name: String,
    call_id: sys_types::CallId,
    state: &WsState,
) {
    let ctx = bex_project::FunctionCallContextBuilder::new(call_id).build();
    let broadcast_tx = state.broadcast_tx.clone();
    let bex = state.bex.clone();
    let run_store = state.run_store.clone();
    let start = run_store.create_run(
        ExecutionRequest {
            project_id: ProjectId(project.clone()),
            project_generation: ProjectGeneration(generation),
            target: RunTarget::Test {
                generation: ProjectGeneration(generation),
                test_name: test_name.clone(),
            },
            args_summary: None,
            options_summary: None,
        },
        RequestId(client.request_id()),
    );
    broadcast_run_started(
        &broadcast_tx,
        &run_store,
        start.run_id,
        client.run_started_request_id(),
    );
    if let Some(patch) = run_store.attach_host_call(start.run_id, HostCallId::Native(call_id)) {
        broadcast_run_patch(&broadcast_tx, &patch);
    }

    tokio::spawn(async move {
        match bex
            .call_test_function_with_trace(&project, generation, &test_name, ctx)
            .await
        {
            Ok(traced) => {
                match run_store.attach_root_trace(start.run_id, traced.entry_call_ref) {
                    AttachRootTraceResult::Attached { patches } => {
                        for patch in patches {
                            broadcast_run_patch(&broadcast_tx, &patch);
                        }
                    }
                    AttachRootTraceResult::AlreadyAttached => {}
                    AttachRootTraceResult::RunMissing => {
                        tracing::warn!(
                            "RunStore missing test run {}",
                            start.run_id.to_wire_string()
                        );
                    }
                    AttachRootTraceResult::Conflict { existing } => {
                        tracing::warn!(
                            "RunStore test root trace conflict for {}: existing {}",
                            start.run_id.to_wire_string(),
                            existing.encode()
                        );
                    }
                }

                match traced.value {
                    Ok(result) => {
                        let handle_options = bridge_ctypes::CffiHandleTableOptions::for_wire();
                        match bridge_ctypes::external_to_outbound(&result, &handle_options) {
                            Ok(baml_val) => {
                                let b64 = base64::engine::general_purpose::STANDARD
                                    .encode(baml_val.encode_to_vec());
                                if let Some(patch) = run_store.complete_run(
                                    start.run_id,
                                    RunOutcome::Succeeded(RunResult {
                                        value: Some(b64.clone()),
                                        renderer_hint: Some("testReport".to_string()),
                                        supporting_payload_ids: Vec::new(),
                                    }),
                                    epoch_ms(),
                                ) {
                                    broadcast_run_patch(&broadcast_tx, &patch);
                                }
                            }
                            Err(e) => {
                                let message = format!("Failed to encode result: {e}");
                                if let Some(patch) = run_store.complete_run(
                                    start.run_id,
                                    host_error_outcome(message.clone()),
                                    epoch_ms(),
                                ) {
                                    broadcast_run_patch(&broadcast_tx, &patch);
                                }
                            }
                        }
                    }
                    Err(e) => {
                        let is_cancelled = is_cancelled_engine_error(&e);
                        let outcome = if is_cancelled {
                            let now = epoch_ms();
                            RunOutcome::Cancelled(CancellationState {
                                requested_at_ms: now,
                                completed_at_ms: Some(now),
                                reason: Some(format!("{e}")),
                            })
                        } else {
                            RunOutcome::Failed(RunError {
                                class: RunErrorClass::Runtime,
                                message: format!("{e}"),
                                details: None,
                            })
                        };
                        if let Some(patch) =
                            run_store.complete_run(start.run_id, outcome, epoch_ms())
                        {
                            broadcast_run_patch(&broadcast_tx, &patch);
                        }
                    }
                }
            }
            Err(e) => {
                let is_cancelled = is_cancelled_engine_error(&e);
                let outcome = if is_cancelled {
                    let now = epoch_ms();
                    RunOutcome::Cancelled(CancellationState {
                        requested_at_ms: now,
                        completed_at_ms: Some(now),
                        reason: Some(format!("{e}")),
                    })
                } else {
                    RunOutcome::Failed(RunError {
                        class: RunErrorClass::Runtime,
                        message: format!("{e}"),
                        details: None,
                    })
                };
                if let Some(patch) = run_store.complete_run(start.run_id, outcome, epoch_ms()) {
                    broadcast_run_patch(&broadcast_tx, &patch);
                }
            }
        };
    });
}

async fn handle_ws_in_message(
    msg: WsInMessage,
    state: &WsState,
    sink: &mut futures::stream::SplitSink<WebSocket, AxumWsMsg>,
) {
    match msg {
        WsInMessage::StartRun {
            request_id,
            project,
            function_name,
            args_bytes,
        } => {
            handle_function_run(
                FunctionRunClient { request_id },
                project,
                FunctionRunTarget::function(function_name),
                args_bytes,
                sys_types::CallId::next(),
                state,
                sink,
            )
            .await;
        }

        WsInMessage::StartPreviewRun {
            request_id,
            project,
            parent_function_name,
            helper,
            function_name,
            args_bytes,
        } => {
            handle_function_run(
                FunctionRunClient { request_id },
                project,
                FunctionRunTarget::preview(parent_function_name, helper, function_name),
                args_bytes,
                sys_types::CallId::next(),
                state,
                sink,
            )
            .await;
        }

        WsInMessage::CancelRun { request_id, run_id } => {
            let run_id = match parse_run_id_for_request(request_id, &run_id) {
                Ok(run_id) => run_id,
                Err(msg) => {
                    send_ws(sink, &msg).await;
                    return;
                }
            };
            let project_id = state
                .run_store
                .snapshot(run_id)
                .map(|run| run.request.project_id.0);

            let response = match state.run_store.cancel_run(run_id, epoch_ms(), None) {
                bex_events::run::CancelRunEffect::CancelHostCall {
                    host_call_id,
                    patch,
                } => {
                    broadcast_run_patch(&state.broadcast_tx, &patch);
                    state.io_state.cancel_for_host_call(&host_call_id);
                    state.env_state.cancel_for_host_call(&host_call_id);
                    match (host_call_id, project_id) {
                        (HostCallId::Native(call_id), Some(project_id)) => {
                            let fs_path = bex_project::FsPath::from_str(project_id);
                            match state.bex.get_bex_for_project(&fs_path) {
                                Ok(bex) => match bex.cancel_function_call(call_id) {
                                    Ok(()) => WsOutMessage::CommandAck {
                                        request_id,
                                        outcome: "accepted".to_string(),
                                    },
                                    Err(e) => WsOutMessage::CommandError {
                                        request_id,
                                        code: "hostCancelFailed".to_string(),
                                        message: format!("{e}"),
                                    },
                                },
                                Err(e) => WsOutMessage::CommandError {
                                    request_id,
                                    code: "projectMissing".to_string(),
                                    message: format!("{e}"),
                                },
                            }
                        }
                        (other, _) => WsOutMessage::CommandError {
                            request_id,
                            code: "unsupportedHostCallId".to_string(),
                            message: format!(
                                "cancelRun resolved to unsupported host id: {other:?}"
                            ),
                        },
                    }
                }
                bex_events::run::CancelRunEffect::CancelledBeforeHost { patch } => {
                    broadcast_run_patch(&state.broadcast_tx, &patch);
                    WsOutMessage::CommandAck {
                        request_id,
                        outcome: "accepted".to_string(),
                    }
                }
                bex_events::run::CancelRunEffect::AlreadyTerminal => WsOutMessage::CommandAck {
                    request_id,
                    outcome: "alreadyTerminal".to_string(),
                },
                bex_events::run::CancelRunEffect::RunMissing => WsOutMessage::CommandError {
                    request_id,
                    code: "runMissing".to_string(),
                    message: "Run not found".to_string(),
                },
            };
            send_ws(sink, &response).await;
        }

        WsInMessage::ListRuns { request_id, filter } => {
            let runs = state
                .run_store
                .list_runs(&run_filter_from_wire(filter))
                .iter()
                .map(run_summary_to_wire)
                .collect();
            send_ws(sink, &WsOutMessage::RunList { request_id, runs }).await;
        }

        WsInMessage::Snapshot { request_id, run_id } => {
            let run_id = match parse_run_id_for_request(request_id, &run_id) {
                Ok(run_id) => run_id,
                Err(msg) => {
                    send_ws(sink, &msg).await;
                    return;
                }
            };
            match state.run_store.snapshot(run_id) {
                Some(snapshot) => {
                    send_ws(
                        sink,
                        &WsOutMessage::RunSnapshot {
                            request_id: Some(request_id),
                            run_id: run_id.to_wire_string(),
                            snapshot: run_to_wire(&snapshot),
                        },
                    )
                    .await;
                }
                None => {
                    send_ws(
                        sink,
                        &WsOutMessage::CommandError {
                            request_id,
                            code: "runMissing".to_string(),
                            message: "Run not found".to_string(),
                        },
                    )
                    .await;
                }
            }
        }

        WsInMessage::Subscribe {
            request_id,
            subscription_id,
            run_id,
            after_cursor,
        } => {
            let run_id = match parse_run_id_for_request(request_id, &run_id) {
                Ok(run_id) => run_id,
                Err(msg) => {
                    send_ws(sink, &msg).await;
                    return;
                }
            };
            match state
                .run_store
                .subscribe(run_id, after_cursor.map(RunCursor))
            {
                RunSubscription::Snapshot { snapshot, patches } => {
                    send_ws(
                        sink,
                        &WsOutMessage::RunSnapshot {
                            request_id: Some(request_id),
                            run_id: run_id.to_wire_string(),
                            snapshot: run_to_wire(&snapshot),
                        },
                    )
                    .await;
                    for patch in patches {
                        send_ws(
                            sink,
                            &WsOutMessage::RunPatch {
                                patch: patch_to_wire(&patch),
                            },
                        )
                        .await;
                    }
                }
                RunSubscription::CursorExpired { reason, .. } => {
                    send_ws(
                        sink,
                        &WsOutMessage::RunCursorExpired {
                            request_id: Some(request_id),
                            subscription_id: Some(subscription_id),
                            run_id: run_id.to_wire_string(),
                            reason: cursor_expired_reason_to_wire(reason).to_string(),
                        },
                    )
                    .await;
                }
                RunSubscription::Missing { .. } => {
                    send_ws(
                        sink,
                        &WsOutMessage::CommandError {
                            request_id,
                            code: "runMissing".to_string(),
                            message: "Run not found".to_string(),
                        },
                    )
                    .await;
                }
            }
        }

        WsInMessage::Unsubscribe {
            request_id,
            subscription_id: _,
        } => {
            send_ws(
                sink,
                &WsOutMessage::CommandAck {
                    request_id,
                    outcome: "accepted".to_string(),
                },
            )
            .await;
        }

        WsInMessage::StartTestRun {
            request_id,
            project,
            generation,
            test_name,
        } => {
            handle_test_run(
                FunctionRunClient { request_id },
                project,
                generation,
                test_name,
                sys_types::CallId::next(),
                state,
            );
        }

        WsInMessage::ExpandTestSet {
            project,
            generation,
            testset_name,
        } => {
            state
                .bex
                .expand_test_set(&project, generation, &testset_name);
        }

        WsInMessage::RespondToInput {
            request_id,
            run_id,
            input_request_id,
            value,
        } => {
            let run_id = match parse_run_id_for_request(request_id, &run_id) {
                Ok(run_id) => run_id,
                Err(msg) => {
                    send_ws(sink, &msg).await;
                    return;
                }
            };
            let input_request_id = match input_request_id.parse::<u64>() {
                Ok(id) => id,
                Err(_) => {
                    send_ws(
                        sink,
                        &WsOutMessage::CommandError {
                            request_id,
                            code: "invalidInputRequestId".to_string(),
                            message: format!("Invalid inputRequestId: {input_request_id}"),
                        },
                    )
                    .await;
                    return;
                }
            };
            let outcome = state
                .io_state
                .resolve_for_run(run_id, input_request_id, value);
            send_ws(
                sink,
                &WsOutMessage::CommandAck {
                    request_id,
                    outcome: outcome.to_string(),
                },
            )
            .await;
        }

        WsInMessage::RespondToEnv {
            request_id,
            run_id,
            env_request_id,
            value,
        } => {
            let run_id = match parse_run_id_for_request(request_id, &run_id) {
                Ok(run_id) => run_id,
                Err(msg) => {
                    send_ws(sink, &msg).await;
                    return;
                }
            };
            let env_request_id = match env_request_id.parse::<u64>() {
                Ok(id) => id,
                Err(_) => {
                    send_ws(
                        sink,
                        &WsOutMessage::CommandError {
                            request_id,
                            code: "invalidEnvRequestId".to_string(),
                            message: format!("Invalid envRequestId: {env_request_id}"),
                        },
                    )
                    .await;
                    return;
                }
            };
            let outcome = state
                .env_state
                .resolve_for_run(run_id, env_request_id, value);
            send_ws(
                sink,
                &WsOutMessage::CommandAck {
                    request_id,
                    outcome: outcome.to_string(),
                },
            )
            .await;
        }

        WsInMessage::EnvVarResponse { id, value, .. } => {
            state.env_state.resolve(id, value);
        }

        WsInMessage::InputResponse { id, value, call_id } => {
            state.io_state.resolve(id, call_id, value);
        }

        WsInMessage::RequestState => {
            state.bex.request_playground_state();
        }

        WsInMessage::RequestCollectTests { project } => {
            state.bex.request_collect_tests(&project);
        }

        WsInMessage::RequestControlFlowGraph {
            project: _,
            function_name,
        } => {
            let graph = state.bex.ast_control_flow_graph(&function_name);
            let graph = graph.map(|g| {
                baml_compiler2_visualization::control_flow::prepare_control_flow_graph_for_visualization(&g)
            });
            let graph_json = graph.as_ref().and_then(|g| serde_json::to_value(g).ok());
            let msg = WsOutMessage::ControlFlowGraphResult {
                function_name,
                graph: graph_json,
            };
            if let Some(ws_msg) = to_ws_text(&msg)
                && sink.send(ws_msg).await.is_err()
            {
                tracing::warn!("Failed to send control flow graph result");
            }
        }

        WsInMessage::CursorPosition { file, line, column } => {
            let ctx = state.bex.playground_cursor_context(&file, line, column);
            let ctx_json = serde_json::to_value(&ctx).unwrap_or(serde_json::Value::Null);
            let msg = WsOutMessage::CursorContext { context: ctx_json };
            if let Some(ws_msg) = to_ws_text(&msg)
                && sink.send(ws_msg).await.is_err()
            {
                tracing::warn!("Failed to send cursor context");
            }
        }

        WsInMessage::SetEnvVar { key, value } => {
            state.env_state.set_override(key, value);
        }

        WsInMessage::DeleteEnvVar { key } => {
            state.env_state.remove_override(&key);
        }
    }
}

// ---------------------------------------------------------------------------
// CORS middleware
// ---------------------------------------------------------------------------

async fn cors_middleware(req: Request<Body>, next: Next) -> Response {
    if req.method() == Method::OPTIONS {
        return Response::builder()
            .status(StatusCode::NO_CONTENT)
            .header(header::ACCESS_CONTROL_ALLOW_ORIGIN, "*")
            .header(
                header::ACCESS_CONTROL_ALLOW_METHODS,
                "GET, PUT, POST, OPTIONS",
            )
            .header(header::ACCESS_CONTROL_ALLOW_HEADERS, "*")
            .header(header::CACHE_CONTROL, "no-store, no-cache, must-revalidate")
            .header(header::PRAGMA, "no-cache")
            .header(header::EXPIRES, "0")
            .body(Body::empty())
            .unwrap();
    }
    let mut resp = next.run(req).await;
    resp.headers_mut()
        .insert(header::ACCESS_CONTROL_ALLOW_ORIGIN, "*".parse().unwrap());
    resp.headers_mut().insert(
        header::CACHE_CONTROL,
        "no-store, no-cache, must-revalidate".parse().unwrap(),
    );
    resp.headers_mut()
        .insert(header::PRAGMA, "no-cache".parse().unwrap());
    resp.headers_mut()
        .insert(header::EXPIRES, "0".parse().unwrap());
    resp
}

// ---------------------------------------------------------------------------
// Dev proxy mode — reverse-proxy to a local Vite dev server
// ---------------------------------------------------------------------------

fn dev_proxy_router(upstream: String) -> Router {
    Router::new().fallback(move |req: Request<Body>| {
        let upstream = upstream.clone();
        async move { proxy_request(upstream, req).await }
    })
}

async fn proxy_request(upstream: String, req: Request<Body>) -> Response {
    use axum::body::to_bytes;

    let is_ws_upgrade = req
        .headers()
        .get(header::UPGRADE)
        .and_then(|v| v.to_str().ok())
        .map(|v| v.eq_ignore_ascii_case("websocket"))
        .unwrap_or(false);

    if is_ws_upgrade {
        return proxy_ws(upstream, req).await;
    }

    let method = req.method().clone();
    let uri_path_and_query = req
        .uri()
        .path_and_query()
        .map(axum::http::uri::PathAndQuery::as_str)
        .unwrap_or("/");
    let target_url = format!("{upstream}{uri_path_and_query}");

    ensure_rustls_crypto_provider();
    let mut fwd = reqwest::Client::new().request(method, &target_url);
    for (name, value) in req.headers() {
        if name == header::HOST {
            continue;
        }
        fwd = fwd.header(name.clone(), value.clone());
    }

    let body_bytes = match to_bytes(req.into_body(), 10 * 1024 * 1024).await {
        Ok(b) => b,
        Err(e) => {
            tracing::warn!("Dev proxy: failed to read request body: {e}");
            return Response::builder()
                .status(StatusCode::BAD_GATEWAY)
                .body(Body::from("proxy error"))
                .unwrap();
        }
    };
    if !body_bytes.is_empty() {
        fwd = fwd.body(body_bytes);
    }

    let upstream_resp = match fwd.send().await {
        Ok(r) => r,
        Err(e) => {
            tracing::warn!("Dev proxy: upstream error: {e}");
            return Response::builder()
                .status(StatusCode::BAD_GATEWAY)
                .body(Body::from(format!("upstream error: {e}")))
                .unwrap();
        }
    };

    let mut builder = Response::builder().status(upstream_resp.status());
    for (name, value) in upstream_resp.headers() {
        builder = builder.header(name.clone(), value.clone());
    }

    let resp_bytes = upstream_resp.bytes().await.unwrap_or_default();
    builder.body(Body::from(resp_bytes)).unwrap()
}

#[cfg(feature = "ring-crypto")]
fn ensure_rustls_crypto_provider() {
    let _ = rustls::crypto::ring::default_provider().install_default();
}

#[cfg(all(not(feature = "ring-crypto"), feature = "aws-crypto"))]
fn ensure_rustls_crypto_provider() {
    let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();
}

#[cfg(all(not(feature = "ring-crypto"), not(feature = "aws-crypto")))]
fn ensure_rustls_crypto_provider() {}

/// Proxy a WebSocket upgrade request (e.g. Vite HMR) to the upstream dev server.
async fn proxy_ws(upstream: String, req: Request<Body>) -> Response {
    let uri_path_and_query = req
        .uri()
        .path_and_query()
        .map(axum::http::uri::PathAndQuery::as_str)
        .unwrap_or("/");
    let ws_url = format!(
        "ws://{}",
        upstream
            .strip_prefix("http://")
            .unwrap_or(upstream.strip_prefix("https://").unwrap_or(&upstream))
    ) + uri_path_and_query;

    let (mut parts, _body) = req.into_parts();
    let ws_upgrade = match WebSocketUpgrade::from_request_parts(&mut parts, &()).await {
        Ok(upgrade) => upgrade,
        Err(e) => {
            tracing::warn!("Dev proxy: WS upgrade extraction failed: {e}");
            return Response::builder()
                .status(StatusCode::BAD_REQUEST)
                .body(Body::from("ws upgrade error"))
                .unwrap();
        }
    };

    ws_upgrade.on_upgrade(move |client_socket| async move {
        let upstream_ws = match tokio_tungstenite::connect_async(&ws_url).await {
            Ok((stream, _)) => stream,
            Err(e) => {
                tracing::warn!("Dev proxy: failed to connect to upstream WS {ws_url}: {e}");
                return;
            }
        };

        let (mut client_sink, mut client_stream) = client_socket.split();
        let (mut upstream_sink, mut upstream_stream) = upstream_ws.split();

        use tokio_tungstenite::tungstenite::Message as TungMsg;

        let client_to_upstream = async {
            while let Some(Ok(msg)) = client_stream.next().await {
                let tung_msg = match msg {
                    AxumWsMsg::Text(t) => TungMsg::Text(t.to_string().into()),
                    AxumWsMsg::Binary(b) => TungMsg::Binary(b.to_vec().into()),
                    AxumWsMsg::Ping(p) => TungMsg::Ping(p.to_vec().into()),
                    AxumWsMsg::Pong(p) => TungMsg::Pong(p.to_vec().into()),
                    AxumWsMsg::Close(_) => {
                        let _ = upstream_sink.send(TungMsg::Close(None)).await;
                        break;
                    }
                };
                if upstream_sink.send(tung_msg).await.is_err() {
                    break;
                }
            }
        };

        let upstream_to_client = async {
            while let Some(Ok(msg)) = upstream_stream.next().await {
                let axum_msg = match msg {
                    TungMsg::Text(t) => AxumWsMsg::Text(t.to_string().into()),
                    TungMsg::Binary(b) => AxumWsMsg::Binary(b.to_vec().into()),
                    TungMsg::Ping(p) => AxumWsMsg::Ping(p.to_vec().into()),
                    TungMsg::Pong(p) => AxumWsMsg::Pong(p.to_vec().into()),
                    TungMsg::Close(_) => {
                        let _ = client_sink.send(AxumWsMsg::Close(None)).await;
                        break;
                    }
                    _ => continue,
                };
                if client_sink.send(axum_msg).await.is_err() {
                    break;
                }
            }
        };

        tokio::select! {
            _ = client_to_upstream => {}
            _ = upstream_to_client => {}
        }
    })
}

// ---------------------------------------------------------------------------
// Prod static-file mode
// ---------------------------------------------------------------------------

fn static_router(dir: String) -> Router {
    use tower_http::services::ServeDir;
    let dir = PathBuf::from(dir);
    Router::new()
        .route("/", get(static_index_handler))
        .route("/index.html", get(static_index_handler))
        .route("/{*path}", get(static_path_handler))
        .fallback_service(get_service(ServeDir::new(dir.clone())))
        .with_state(dir)
}

async fn static_index_handler(State(dir): State<PathBuf>) -> Response {
    serve_static_index(&dir).await
}

async fn static_path_handler(
    State(dir): State<PathBuf>,
    AxumPath(path): AxumPath<String>,
) -> Response {
    let request_path = path.trim_start_matches('/');
    let file_path = dir.join(request_path);
    if is_existing_file_within_dir(&dir, &file_path) {
        return match tokio::fs::read(&file_path).await {
            Ok(bytes) => {
                let mut builder = Response::builder().status(StatusCode::OK);
                if let Some(content_type) = content_type_for_path(&file_path) {
                    builder = builder.header(header::CONTENT_TYPE, content_type);
                }
                builder
                    .body(Body::from(bytes))
                    .unwrap_or_else(|_| Response::new(Body::empty()))
            }
            Err(error) => {
                return text_response(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    format!("Failed to read static asset: {error}"),
                );
            }
        };
    }
    serve_static_index(&dir).await
}

async fn serve_static_index(dir: &Path) -> Response {
    let index_path = dir.join("index.html");
    let version = tokio::fs::metadata(&index_path)
        .await
        .ok()
        .and_then(|metadata| metadata.modified().ok())
        .and_then(|modified| modified.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|duration| duration.as_millis().to_string())
        .unwrap_or_else(|| epoch_ms().to_string());

    match tokio::fs::read_to_string(&index_path).await {
        Ok(html) => {
            let html = cache_bust_static_asset_urls(&html, &version);
            Response::builder()
                .status(StatusCode::OK)
                .header(header::CONTENT_TYPE, "text/html; charset=utf-8")
                .body(Body::from(html))
                .unwrap_or_else(|_| Response::new(Body::empty()))
        }
        Err(error) => text_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to read playground index: {error}"),
        ),
    }
}

fn cache_bust_static_asset_urls(html: &str, version: &str) -> String {
    // NOTE: deliberately do NOT cache-bust /assets/index.js. It is an ES module
    // entry that code-split chunks import via a bare relative path
    // (`import ... from "./index.js"`). Rewriting the HTML's reference to
    // `index.js?v=…` makes the browser load the entry under TWO different URLs
    // (`index.js?v=…` from the HTML and `index.js` from the chunk imports), so
    // it evaluates the module twice — producing two instances of every
    // singleton it holds (e.g. two React copies → "invalid hook call" the
    // moment a lazy-loaded view mounts). The `no-store, no-cache` headers set
    // by `cors_middleware` already prevent stale caching, so the entry doesn't
    // need a query buster. The CSS link is only referenced from the HTML (never
    // re-imported by a chunk), so busting it is safe.
    html.replace(
        "/assets/index.css",
        &format!("/assets/index.css?v={version}"),
    )
}

fn is_existing_file_within_dir(dir: &Path, file_path: &Path) -> bool {
    let Ok(canonical_dir) = dir.canonicalize() else {
        return false;
    };
    let Ok(canonical_file) = file_path.canonicalize() else {
        return false;
    };
    canonical_file.starts_with(canonical_dir) && canonical_file.is_file()
}

fn content_type_for_path(path: &Path) -> Option<&'static str> {
    match path.extension().and_then(|ext| ext.to_str()) {
        Some("css") => Some("text/css; charset=utf-8"),
        Some("html") => Some("text/html; charset=utf-8"),
        Some("js") => Some("text/javascript; charset=utf-8"),
        Some("json") => Some("application/json"),
        Some("map") => Some("application/json"),
        Some("svg") => Some("image/svg+xml"),
        Some("wasm") => Some("application/wasm"),
        Some("woff") => Some("font/woff"),
        Some("woff2") => Some("font/woff2"),
        _ => None,
    }
}
