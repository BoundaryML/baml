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
    sync::{Arc, Mutex},
};

use axum::{
    Router,
    body::Body,
    extract::{
        FromRequestParts, Path as AxumPath, Query, State,
        ws::{Message as AxumWsMsg, WebSocket, WebSocketUpgrade},
    },
    http::{HeaderValue, Method, Request, StatusCode, Uri, header},
    middleware::{self, Next},
    response::Response,
    routing::{get, get_service},
};
use base64::Engine as _;
use bex_events::{
    history::{HistoryStore, HistoryValueReadResult},
    run::{
        BoundaryId, CancellationState, DiagnosticSeverity, ExecutionRequest, HostCallId,
        InMemoryRunStore, ProjectGeneration, ProjectId, RequestId, RunCursor,
        RunCursorExpiredReason, RunDiagnostic, RunError, RunErrorClass, RunFilter, RunKind,
        RunOutcome, RunPatch, RunResult, RunSubscription, RunTarget, RunVisibilityFilter,
        StartedHostRun,
    },
    value::{
        ByteValueArtifactSink, CaptureLossKind, CaptureLossReason, CaptureLossRecord,
        DEFAULT_NATIVE_LIVE_VALUE_CACHE_BYTES, LiveValueBody, LiveValueCache, LiveValueLookup,
        LogEventRecord, ValueCodec, ValueIdAllocator, ValueRef, ValueWriter,
    },
};
use bex_project::{is_cancelled_engine_error, is_cancelled_runtime_error};
use futures::{SinkExt, stream::StreamExt};
use tokio::{net::TcpListener, sync::broadcast};

use crate::{
    playground_env::PlaygroundEnvState,
    playground_io::PlaygroundIoState,
    playground_runs::{
        overlay_function_name_for_target, patch_to_wire, run_summary_to_wire, run_to_wire,
    },
    playground_ws::{RunListFilter, RunListKind, RunListVisibility, WsInMessage, WsOutMessage},
};

#[derive(Debug, thiserror::Error)]
#[error("Playground server requires either BAML_PLAYGROUND_DEV_PORT or BAML_PLAYGROUND_DIR")]
pub(crate) struct PlaygroundNotConfigured;

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
    boundary_id: bex_events::run::BoundaryId,
    request_id: Option<u64>,
) {
    if let Some(run) = run_store.snapshot(boundary_id) {
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

fn broadcast_started_host_run(
    broadcast_tx: &broadcast::Sender<WsOutMessage>,
    run_store: &InMemoryRunStore,
    started: &StartedHostRun,
    request_id: Option<u64>,
) {
    broadcast_run_started(
        broadcast_tx,
        run_store,
        started.start.boundary_id,
        request_id,
    );
    if let Some(patch) = &started.started_patch {
        broadcast_run_patch(broadcast_tx, patch);
    }
}

fn engine_error_outcome_with_ref(
    err: &bex_project::EngineError,
    value_ref: Option<ValueRef>,
) -> RunOutcome {
    if is_cancelled_engine_error(err) {
        let now = epoch_ms();
        return RunOutcome::Cancelled(CancellationState {
            requested_at_ms: now,
            completed_at_ms: Some(now),
            reason: Some(format!("{err}")),
        });
    }
    RunOutcome::Failed(RunError {
        class: RunErrorClass::Runtime,
        message: format!("{err}"),
        details: None,
        value_ref,
    })
}

fn complete_run_and_broadcast(
    broadcast_tx: &broadcast::Sender<WsOutMessage>,
    run_store: &InMemoryRunStore,
    history_store: &HistoryStore,
    boundary_id: BoundaryId,
    outcome: RunOutcome,
) {
    let completed_at_ms = epoch_ms();
    if let Err(err) = history_store.complete(boundary_id, &outcome, completed_at_ms) {
        tracing::warn!(
            "History completion failed for {}: {err}",
            boundary_id.to_wire_string()
        );
    }
    if let Some(patch) = run_store.complete_run(boundary_id, outcome, completed_at_ms) {
        broadcast_run_patch(broadcast_tx, &patch);
    }
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
fn parse_boundary_id_for_request(
    request_id: u64,
    boundary_id: &str,
) -> Result<BoundaryId, WsOutMessage> {
    BoundaryId::from_wire_str(boundary_id).ok_or_else(|| WsOutMessage::CommandError {
        request_id,
        code: "invalidBoundaryId".to_string(),
        message: format!("Invalid boundaryId: {boundary_id}"),
    })
}

fn value_body_response(
    request_id: u64,
    boundary_id: BoundaryId,
    value_ref_id: String,
    requested_codec: String,
    live_value: LiveValueLookup,
    history_result: Result<HistoryValueReadResult, String>,
) -> WsOutMessage {
    let live_diagnostic = match &live_value {
        LiveValueLookup::Evicted(eviction) => Some(eviction.diagnostic.clone()),
        LiveValueLookup::Available(_) | LiveValueLookup::Missing => None,
    };
    match live_value {
        LiveValueLookup::Available(stored) => WsOutMessage::ValueBody {
            request_id,
            boundary_id: boundary_id.to_wire_string(),
            value_ref_id,
            codec: stored.codec.as_wire_str().to_string(),
            availability: "available".to_string(),
            body_base64: Some(base64::engine::general_purpose::STANDARD.encode(stored.body)),
            diagnostic: None,
        },
        LiveValueLookup::Evicted(_) | LiveValueLookup::Missing => match history_result {
            Ok(HistoryValueReadResult::Available(stored)) => WsOutMessage::ValueBody {
                request_id,
                boundary_id: boundary_id.to_wire_string(),
                value_ref_id,
                codec: stored.codec.as_wire_str().to_string(),
                availability: "available".to_string(),
                body_base64: Some(base64::engine::general_purpose::STANDARD.encode(stored.body)),
                diagnostic: None,
            },
            Ok(HistoryValueReadResult::Missing) => WsOutMessage::ValueBody {
                request_id,
                boundary_id: boundary_id.to_wire_string(),
                value_ref_id,
                codec: requested_codec,
                availability: "missing".to_string(),
                body_base64: None,
                diagnostic: Some(
                    live_diagnostic.unwrap_or_else(|| "value body is not available".to_string()),
                ),
            },
            Ok(HistoryValueReadResult::BodyUnavailable(unavailable)) => WsOutMessage::ValueBody {
                request_id,
                boundary_id: boundary_id.to_wire_string(),
                value_ref_id,
                codec: requested_codec,
                availability: "missing".to_string(),
                body_base64: None,
                diagnostic: Some(unavailable.diagnostic),
            },
            Err(err) => {
                let diagnostic = match live_diagnostic {
                    Some(live_diagnostic) => {
                        format!("{live_diagnostic}; history value read failed: {err}")
                    }
                    None => format!("history value read failed: {err}"),
                };
                WsOutMessage::ValueBody {
                    request_id,
                    boundary_id: boundary_id.to_wire_string(),
                    value_ref_id,
                    codec: requested_codec,
                    availability: "lost".to_string(),
                    body_base64: None,
                    diagnostic: Some(diagnostic),
                }
            }
        },
    }
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

fn runtime_error_outcome_with_ref(
    err: &bex_project::RuntimeError,
    value_ref: Option<ValueRef>,
) -> RunOutcome {
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
        value_ref,
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

/// Resolve the given env var names against `lookup`, keeping only those set.
/// Pure so tests never have to mutate the process environment.
fn collect_referenced_env_vars(
    names: &[String],
    lookup: impl Fn(&str) -> Option<String>,
) -> std::collections::HashMap<String, String> {
    names
        .iter()
        .filter_map(|n| lookup(n).map(|v| (n.clone(), v)))
        .collect()
}

/// Bind exactly `port` on loopback, with an actionable error when taken.
pub async fn bind_exact_port(port: u16) -> anyhow::Result<TcpListener> {
    let addr = SocketAddr::from(([127, 0, 0, 1], port));
    TcpListener::bind(addr).await.map_err(|e| {
        anyhow::anyhow!(
            "Could not bind playground port {port}: {e}. Another process may be \
             using it; pass a different --port or omit it to auto-pick from 4265."
        )
    })
}

// ---------------------------------------------------------------------------
// Shared state for Axum handlers
// ---------------------------------------------------------------------------

/// Per-file mirror of the content the BROWSER currently has (set from
/// didOpen/didChange). The disk watcher consults it to avoid echoing the
/// browser's own write-throughs back as "external" changes — only content that
/// differs from the mirror is pushed to the browser. Shared between the `/api/lsp`
/// bridge (writer) and the disk watcher (reader). Keyed by canonical path.
pub type DocMirror = Arc<std::sync::Mutex<std::collections::HashMap<PathBuf, String>>>;

/// Custom LSP notification used to push external on-disk edits to the browser
/// editor so it can refresh its model. Not part of the LSP spec; the browser
/// registers a handler for this method.
const DISK_CHANGE_NOTIFICATION: &str = "baml/fileChangedOnDisk";

#[derive(Clone)]
struct WsState {
    bex: Arc<dyn bex_project::BexLsp>,
    broadcast_tx: broadcast::Sender<WsOutMessage>,
    env_state: Arc<PlaygroundEnvState>,
    io_state: Arc<PlaygroundIoState>,
    run_store: Arc<InMemoryRunStore>,
    history_store: Arc<HistoryStore>,
    value_store: LiveValueStore,
    /// Broadcast LSP output (root-dispatcher notifications, disk-change
    /// pushes) destined for `/api/lsp`. Responses never travel here: they are
    /// routed per-session by the ingress runtime.
    lsp_out_tx: broadcast::Sender<crate::OutboundFrame>,
    /// Process-owned ingress runtime shared with the stdio transport.
    lsp_runtime: Arc<crate::lsp_runtime::LspRuntime>,
    /// What the browser currently has per file (for disk-watcher echo avoidance).
    doc_mirror: DocMirror,
    /// Workspace roots that browser-mode LSP saves are allowed to write under.
    workspace_roots: Arc<Vec<PathBuf>>,
    /// Target of the most recent OpenPlayground; replayed to a page when it
    /// requests state so a freshly opened / reconnected window navigates there.
    current_open_target: crate::playground_sender::SharedOpenTarget,
}

type LiveValueStore = Arc<Mutex<LiveValueCache>>;

fn root_value_success_outcome(value_ref: Option<ValueRef>, renderer_hint: &str) -> RunOutcome {
    RunOutcome::Succeeded(RunResult {
        value_ref,
        renderer_hint: Some(renderer_hint.to_string()),
        supporting_payload_ids: Vec::new(),
    })
}

fn drain_logs_and_broadcast(
    broadcast_tx: &broadcast::Sender<WsOutMessage>,
    run_store: &InMemoryRunStore,
    history_store: &HistoryStore,
    value_store: &LiveValueStore,
    boundary_id: BoundaryId,
    logger: &bex_project::TraceLogger,
) {
    let mut fallback_writer = match ValueWriter::new_with_id_allocator(
        ByteValueArtifactSink::new(),
        boundary_id,
        ValueIdAllocator::live_fallback(),
    ) {
        Ok(writer) => writer,
        Err(err) => {
            broadcast_log_diagnostic(broadcast_tx, run_store, boundary_id, err);
            return;
        }
    };
    let report = logger.drain_encoded_logs();
    for failure in &report.failures {
        broadcast_log_diagnostic(
            broadcast_tx,
            run_store,
            failure.boundary_id,
            format!("log capture failed: {}", failure.diagnostic),
        );
    }
    let stats = logger.stats();
    if stats.skipped_log_queue_full > 0 {
        append_capture_loss_record(
            history_store,
            boundary_id,
            CaptureLossKind::Log,
            stats.skipped_log_queue_full,
        )
        .unwrap_or_else(|err| {
            broadcast_log_diagnostic(
                broadcast_tx,
                run_store,
                boundary_id,
                format!("history capture-loss persistence failed: {err}"),
            );
        });
        broadcast_log_loss_diagnostic(
            broadcast_tx,
            run_store,
            boundary_id,
            "log",
            stats.skipped_log_queue_full,
        );
    }

    for encoded in report.logs {
        let event = LogEventRecord {
            call: encoded.call,
            level: encoded.metadata.level.clone(),
            source: encoded.metadata.source.clone(),
            timestamp_ms: encoded.metadata.timestamp_ms,
            message_preview: encoded.metadata.message_preview.clone(),
        };
        let outcome = match history_store.append_log_body(
            encoded.boundary_id,
            event.clone(),
            ValueCodec::BamlOutboundValue,
            encoded.body.clone(),
        ) {
            Ok(outcome) => outcome,
            Err(err) => {
                broadcast_log_diagnostic(
                    broadcast_tx,
                    run_store,
                    encoded.boundary_id,
                    format!("history log persistence failed; retained live bytes only: {err}"),
                );
                match fallback_writer.append_log_body(
                    ValueCodec::BamlOutboundValue,
                    encoded.body.clone(),
                    event,
                ) {
                    Ok(outcome) => outcome,
                    Err(err) => {
                        broadcast_log_diagnostic(broadcast_tx, run_store, encoded.boundary_id, err);
                        continue;
                    }
                }
            }
        };
        let value_ref = outcome.value_ref;
        if let Ok(mut store) = value_store.lock() {
            let insert = store.insert(
                encoded.boundary_id,
                &value_ref,
                LiveValueBody {
                    codec: value_ref.codec,
                    body: encoded.body,
                },
            );
            if let Some(diagnostic) = insert.diagnostic {
                broadcast_log_diagnostic(broadcast_tx, run_store, encoded.boundary_id, diagnostic);
            }
        }

        if let Some(patch) = run_store.ingest_log_value_ref(
            encoded.boundary_id,
            encoded.call,
            encoded.metadata.level,
            encoded
                .metadata
                .message_preview
                .unwrap_or_else(|| "captured log".to_string()),
            encoded.metadata.source,
            Some(value_ref),
        ) {
            broadcast_run_patch(broadcast_tx, &patch);
        }
    }
}

fn append_capture_loss_record(
    history_store: &HistoryStore,
    boundary_id: BoundaryId,
    kind: CaptureLossKind,
    skipped: u64,
) -> std::io::Result<()> {
    history_store.append_capture_loss(
        boundary_id,
        &CaptureLossRecord {
            kind,
            reason: CaptureLossReason::QueueFull,
            skipped_count: skipped,
            call: None,
            message: Some(capture_loss_message(kind.as_wire_str(), skipped)),
            timestamp_ms: epoch_ms(),
        },
    )
}

fn broadcast_log_diagnostic(
    broadcast_tx: &broadcast::Sender<WsOutMessage>,
    run_store: &InMemoryRunStore,
    boundary_id: BoundaryId,
    err: impl std::fmt::Display,
) {
    if let Some(patch) = run_store.add_diagnostic(
        boundary_id,
        RunDiagnostic {
            severity: DiagnosticSeverity::Warning,
            code: Some("logCaptureFailed".to_string()),
            message: format!("Failed to retain captured log bytes: {err}"),
            payload_id: None,
        },
    ) {
        broadcast_run_patch(broadcast_tx, &patch);
    }
}

fn broadcast_log_loss_diagnostic(
    broadcast_tx: &broadcast::Sender<WsOutMessage>,
    run_store: &InMemoryRunStore,
    boundary_id: BoundaryId,
    capture_kind: &str,
    skipped: u64,
) {
    if let Some(patch) = log_loss_diagnostic_patch(run_store, boundary_id, capture_kind, skipped) {
        broadcast_run_patch(broadcast_tx, &patch);
    }
}

fn log_loss_diagnostic_patch(
    run_store: &InMemoryRunStore,
    boundary_id: BoundaryId,
    capture_kind: &str,
    skipped: u64,
) -> Option<RunPatch> {
    run_store.add_diagnostic(
        boundary_id,
        RunDiagnostic {
            severity: DiagnosticSeverity::Warning,
            code: Some("logCaptureLoss".to_string()),
            message: capture_loss_message(capture_kind, skipped),
            payload_id: None,
        },
    )
}

fn capture_loss_message(capture_kind: &str, skipped: u64) -> String {
    format!(
        "Skipped {skipped} captured {capture_kind} value(s) because the log capture queue was full"
    )
}

#[derive(Clone, Debug)]
struct PlaygroundAccessGuard {}

impl PlaygroundAccessGuard {
    fn new() -> Self {
        Self {}
    }

    fn is_allowed_origin(&self, origin: Option<&HeaderValue>) -> bool {
        let Some(origin) = origin else {
            return true;
        };
        let Ok(origin) = origin.to_str() else {
            return false;
        };
        is_loopback_origin(origin) || is_vscode_webview_origin(origin)
    }

    fn cors_origin(&self, origin: Option<&HeaderValue>) -> Option<HeaderValue> {
        let origin = origin?;
        if self.is_allowed_origin(Some(origin)) {
            Some(origin.clone())
        } else {
            None
        }
    }
}

/// True when `origin` is an http(s) origin whose host is loopback
/// (`localhost`, a 127.0.0.0/8 address, or `[::1]`), on any port.
///
/// Through an `ssh -L` tunnel the page's origin is the local tunnel endpoint:
/// loopback host, arbitrary port. The host, not the port, is the trust signal;
/// a hostile web page's fetch/WS still carries its real remote origin -> denied.
fn is_loopback_origin(origin: &str) -> bool {
    let Ok(uri) = origin.parse::<Uri>() else {
        return false;
    };
    if !matches!(uri.scheme_str(), Some("http") | Some("https")) {
        return false;
    }
    let Some(host) = uri.host() else {
        return false;
    };
    if host.eq_ignore_ascii_case("localhost") {
        return true;
    }
    host.trim_start_matches('[')
        .trim_end_matches(']')
        .parse::<std::net::IpAddr>()
        .is_ok_and(|ip| ip.is_loopback())
}

fn is_vscode_webview_origin(origin: &str) -> bool {
    if origin
        .get(..17)
        .is_some_and(|prefix| prefix.eq_ignore_ascii_case("vscode-webview://"))
    {
        return true;
    }
    let Ok(uri) = origin.parse::<Uri>() else {
        return false;
    };
    if uri.scheme_str() != Some("https") {
        return false;
    }
    let Some(host) = uri.host() else {
        return false;
    };
    host.eq_ignore_ascii_case("vscode-cdn.net")
        || host.to_ascii_lowercase().ends_with(".vscode-cdn.net")
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
pub(crate) async fn run(
    listener: TcpListener,
    bex: Arc<dyn bex_project::BexLsp>,
    broadcast_tx: broadcast::Sender<WsOutMessage>,
    env_state: Arc<PlaygroundEnvState>,
    io_state: Arc<PlaygroundIoState>,
    run_store: Arc<InMemoryRunStore>,
    playground_dir_override: Option<PathBuf>,
    lsp_out_tx: broadcast::Sender<crate::OutboundFrame>,
    lsp_runtime: Arc<crate::lsp_runtime::LspRuntime>,
    doc_mirror: DocMirror,
    workspace_roots: Vec<PathBuf>,
    current_open_target: crate::playground_sender::SharedOpenTarget,
) -> anyhow::Result<()> {
    let local_addr = listener.local_addr()?;
    let access_guard = PlaygroundAccessGuard::new();
    let app = build_router(
        bex,
        broadcast_tx,
        env_state,
        io_state,
        run_store,
        playground_dir_override,
        lsp_out_tx,
        lsp_runtime,
        doc_mirror,
        Arc::new(workspace_roots),
        access_guard,
        current_open_target,
    )?;

    tracing::info!("Playground: http://localhost:{}", local_addr.port());

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
    lsp_out_tx: broadcast::Sender<crate::OutboundFrame>,
    lsp_runtime: Arc<crate::lsp_runtime::LspRuntime>,
    doc_mirror: DocMirror,
    workspace_roots: Arc<Vec<PathBuf>>,
    access_guard: PlaygroundAccessGuard,
    current_open_target: crate::playground_sender::SharedOpenTarget,
) -> anyhow::Result<Router> {
    let value_store = Arc::new(Mutex::new(LiveValueCache::with_max_bytes(
        DEFAULT_NATIVE_LIVE_VALUE_CACHE_BYTES,
    )));
    let history_store = Arc::new(HistoryStore::new((*workspace_roots).clone()));
    let ws_state = WsState {
        bex,
        broadcast_tx,
        env_state,
        io_state,
        run_store,
        history_store,
        value_store,
        lsp_out_tx,
        lsp_runtime,
        doc_mirror,
        workspace_roots,
        current_open_target,
    };

    let api = Router::new()
        .route("/api/ws", get(playground_ws_handler))
        .route("/api/lsp", get(lsp_ws_handler))
        .route(
            "/api/source-files",
            get(source_files_handler).put(update_source_file_handler),
        )
        .with_state(ws_state)
        .layer(middleware::from_fn_with_state(
            access_guard,
            api_guard_middleware,
        ));

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
        return Err(PlaygroundNotConfigured.into());
    };

    Ok(api.fallback_service(fallback))
}

fn history_project_root_for_project(project: &str) -> PathBuf {
    let fs_path = bex_project::FsPath::from_str(project.to_string());
    bex_events::history::path::resolve_project_root(fs_path.as_path())
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

/// One pre-serialized outbound frame as a WS text message. The bytes already
/// carry the `jsonrpc` member (see `crate::serialize_jsonrpc_message`).
fn frame_to_ws_text(frame: &crate::OutboundFrame) -> Option<AxumWsMsg> {
    match std::str::from_utf8(frame.bytes()) {
        Ok(text) => Some(AxumWsMsg::Text(text.to_string().into())),
        Err(e) => {
            tracing::error!("LSP WS: outbound frame is not UTF-8: {e}");
            None
        }
    }
}

/// A browser session that cannot drain within this deadline is
/// deterministically closed; slow readers cannot grow an unbounded
/// writer queue).
async fn send_lsp_ws_message(
    sink: &mut futures::stream::SplitSink<WebSocket, AxumWsMsg>,
    message: AxumWsMsg,
) -> bool {
    matches!(
        tokio::time::timeout(std::time::Duration::from_secs(5), sink.send(message)).await,
        Ok(Ok(()))
    )
}

async fn lsp_ws_session(socket: WebSocket, state: WsState) {
    tracing::info!("Playground: LSP WS session started");
    let (mut sink, mut stream) = socket.split();
    let mut lsp_rx = state.lsp_out_tx.subscribe();

    // The session's response channel: the ingress runtime orders responses
    // (and session-scoped notifications) into this bounded, budgeted queue;
    // the WS loop forwards them. Saturation is backpressure — the runtime
    // retries with the response still reserved — never loss.
    let (response_tx, mut response_rx) = tokio::sync::mpsc::channel(256);
    let response_budget = crate::OutboundBudget::new();
    let response_sink_budget = response_budget.clone();
    let response_sink: crate::lsp_runtime::Sink = Arc::new(move |message| {
        let frame = match response_sink_budget.try_message(message) {
            Ok(frame) => frame,
            Err(crate::OutboundReserveError::Saturated) => {
                return crate::lsp_runtime::SinkDelivery::Saturated;
            }
            Err(
                crate::OutboundReserveError::Oversized | crate::OutboundReserveError::Serialization,
            ) => return crate::lsp_runtime::SinkDelivery::Oversized,
        };
        match response_tx.try_send(frame) {
            Ok(()) => crate::lsp_runtime::SinkDelivery::Sent,
            Err(tokio::sync::mpsc::error::TrySendError::Full(_)) => {
                crate::lsp_runtime::SinkDelivery::Saturated
            }
            Err(tokio::sync::mpsc::error::TrySendError::Closed(_)) => {
                crate::lsp_runtime::SinkDelivery::Closed
            }
        }
    });
    // Browser takeover closes the superseded socket through this signal.
    let (close_tx, mut close_rx) = tokio::sync::watch::channel(false);
    let close_endpoint: crate::lsp_runtime::Close = Arc::new(move || {
        let _ = close_tx.send(true);
    });
    // Latest full text per document URI, captured from applied didOpen/
    // didChange so didSave can write it through to disk. The hook runs on the
    // dispatch worker strictly after the mutation was accepted, so persisted
    // bytes always correspond to an applied overlay.
    let pending_text = Arc::new(std::sync::Mutex::new(std::collections::HashMap::<
        String,
        String,
    >::new()));
    let hook_pending_text = pending_text.clone();
    let hook_doc_mirror = state.doc_mirror.clone();
    let hook_workspace_roots = state.workspace_roots.clone();
    let after_notification: crate::lsp_runtime::NotificationHook = Arc::new(move |notification| {
        let Ok(mut pending_text) = hook_pending_text.lock() else {
            return;
        };
        track_and_persist_lsp_notification(
            notification,
            &mut pending_text,
            &hook_doc_mirror,
            &hook_workspace_roots,
        );
    });
    let opened = state.lsp_runtime.open_session(
        crate::lsp_ingress::TransportKind::Browser,
        state.bex.clone(),
        response_sink,
        close_endpoint,
        Some(after_notification),
    );
    let session_id = opened.session_id;

    loop {
        tokio::select! {
            client_msg = stream.next() => {
                match client_msg {
                    Some(Ok(AxumWsMsg::Text(text))) => {
                        let text_str: &str = &text;
                        handle_lsp_client_text(
                            text_str,
                            &state.lsp_runtime,
                            session_id,
                            &mut sink,
                        ).await;
                    }
                    Some(Ok(AxumWsMsg::Close(_))) | None => break,
                    _ => {}
                }
            }
            out_msg = lsp_rx.recv() => {
                match out_msg {
                    // Responses are per-session (routed via `response_rx`);
                    // a lossy broadcast is not a response transport.
                    Ok(frame) if !frame.is_response() => {
                        if let Some(ws_msg) = frame_to_ws_text(&frame)
                            && !send_lsp_ws_message(&mut sink, ws_msg).await
                        {
                            break;
                        }
                    }
                    Ok(_) => {}
                    Err(broadcast::error::RecvError::Lagged(n)) => {
                        // Overflow closes only this session; the process and
                        // other sessions continue.
                        tracing::warn!("LSP WS: broadcast lagged by {n} messages");
                        break;
                    }
                    Err(broadcast::error::RecvError::Closed) => break,
                }
            }
            response = response_rx.recv() => {
                match response {
                    Some(frame) => {
                        if let Some(ws_msg) = frame_to_ws_text(&frame)
                            && !send_lsp_ws_message(&mut sink, ws_msg).await
                        {
                            break;
                        }
                    }
                    None => break,
                }
            }
            changed = close_rx.changed() => {
                if changed.is_err() || *close_rx.borrow() {
                    break;
                }
            }
        }
    }

    state.lsp_runtime.close_session(session_id);
    tracing::debug!("LSP WS session ended");
}

async fn handle_lsp_client_text(
    text: &str,
    runtime: &Arc<crate::lsp_runtime::LspRuntime>,
    session_id: crate::lsp_ingress::SessionId,
    sink: &mut futures::stream::SplitSink<WebSocket, AxumWsMsg>,
) {
    // Malformed JSON and invalid envelopes get the same null-ID protocol
    // errors the stdio transport produces, keeping traces transport-identical.
    let value = match serde_json::from_str::<serde_json::Value>(text) {
        Ok(value) => value,
        Err(error) => {
            tracing::warn!("LSP WS: malformed JSON: {error}");
            let parse_error = serde_json::json!({
                "jsonrpc": "2.0",
                "id": serde_json::Value::Null,
                "error": { "code": -32700, "message": format!("Parse error: {error}") },
            });
            let _ =
                send_lsp_ws_message(sink, AxumWsMsg::Text(parse_error.to_string().into())).await;
            return;
        }
    };
    let msg = match crate::decode_lsp_message(value) {
        Ok(message) => message,
        Err(error) => {
            let invalid_request = serde_json::json!({
                "jsonrpc": "2.0",
                "id": serde_json::Value::Null,
                "error": { "code": -32600, "message": error },
            });
            let _ = send_lsp_ws_message(sink, AxumWsMsg::Text(invalid_request.to_string().into()))
                .await;
            return;
        }
    };
    loop {
        match runtime.submit(session_id, msg.clone()) {
            crate::lsp_runtime::SubmitResult::Accepted
            | crate::lsp_runtime::SubmitResult::Dropped
            | crate::lsp_runtime::SubmitResult::Exited { .. }
            | crate::lsp_runtime::SubmitResult::Closed => break,
            crate::lsp_runtime::SubmitResult::Backpressure => {
                tokio::time::sleep(std::time::Duration::from_millis(2)).await;
            }
        }
    }
}

/// Track the latest document text and, on save, write it through to disk.
fn track_and_persist_lsp_notification(
    notif: &lsp_server::Notification,
    pending_text: &mut std::collections::HashMap<String, String>,
    doc_mirror: &DocMirror,
    workspace_roots: &[PathBuf],
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
                update_doc_mirror(doc_mirror, uri, text);
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
                // Record what the browser now has so the disk watcher won't bounce
                // the user's own save back as an "external" change. NB: no disk
                // write here — edits persist only on an explicit save (didSave).
                update_doc_mirror(doc_mirror, uri, text);
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
                    write_lsp_document_to_disk(uri, &text, workspace_roots);
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

fn write_lsp_document_to_disk(uri: &str, text: &str, workspace_roots: &[PathBuf]) {
    let Some(path) = lsp_types::Url::parse(uri)
        .ok()
        .and_then(|u| u.to_file_path().ok())
    else {
        tracing::warn!("LSP WS: cannot resolve file path for uri {uri}");
        return;
    };
    if path.extension().and_then(|e| e.to_str()) != Some("baml") {
        tracing::warn!(
            "LSP WS: refusing to write non-BAML document {}",
            path.display()
        );
        return;
    }
    let Some(path) = allowed_lsp_save_path(&path, workspace_roots) else {
        tracing::warn!(
            "LSP WS: refusing to write {} outside configured workspace roots",
            path.display()
        );
        return;
    };
    match std::fs::write(&path, text) {
        Ok(()) => tracing::debug!("LSP WS: wrote {} ({} bytes)", path.display(), text.len()),
        Err(e) => tracing::warn!("LSP WS: failed to write {}: {e}", path.display()),
    }
}

fn allowed_lsp_save_path(path: &Path, workspace_roots: &[PathBuf]) -> Option<PathBuf> {
    let candidate = canonicalize_existing_or_parent(path)?;
    for root in workspace_roots {
        let Ok(root) = std::fs::canonicalize(root) else {
            continue;
        };
        if root.is_file() {
            if candidate == root {
                return Some(candidate);
            }
            continue;
        }
        if candidate == root || candidate.starts_with(&root) {
            return Some(candidate);
        }
    }
    None
}

fn canonicalize_existing_or_parent(path: &Path) -> Option<PathBuf> {
    if let Ok(canonical) = std::fs::canonicalize(path) {
        return Some(canonical);
    }
    let parent = path.parent()?;
    let file_name = path.file_name()?;
    Some(std::fs::canonicalize(parent).ok()?.join(file_name))
}

/// Resolve a `file://` URI to a canonical path (the key form used by the disk
/// watcher), so the browser's content and the watcher's reads compare equal.
fn uri_to_canonical_path(uri: &str) -> Option<PathBuf> {
    let path = lsp_types::Url::parse(uri).ok()?.to_file_path().ok()?;
    Some(std::fs::canonicalize(&path).unwrap_or(path))
}

/// Record the content the browser now has for `uri` (used for echo avoidance).
fn update_doc_mirror(doc_mirror: &DocMirror, uri: &str, text: &str) {
    if let Some(path) = uri_to_canonical_path(uri)
        && let Ok(mut mirror) = doc_mirror.lock()
    {
        mirror.insert(path, text.to_string());
    }
}

/// Watch `roots` for external `.baml` edits and push them to the browser editor
/// over `/api/lsp` (as a `baml/fileChangedOnDisk` notification). Echo avoidance:
/// a change whose content already matches `doc_mirror` (what the browser has, or
/// just wrote through) is NOT pushed back. The returned watcher must be kept
/// alive for as long as watching should continue.
pub(crate) fn spawn_disk_watcher(
    roots: &[PathBuf],
    lsp_out_tx: broadcast::Sender<crate::OutboundFrame>,
    lsp_out_budget: Arc<crate::OutboundBudget>,
    doc_mirror: DocMirror,
) -> Option<notify::RecommendedWatcher> {
    use notify::{EventKind, RecursiveMode, Watcher};

    let handler = move |res: notify::Result<notify::Event>| {
        let Ok(event) = res else { return };
        if !matches!(event.kind, EventKind::Modify(_) | EventKind::Create(_)) {
            return;
        }
        for path in event.paths {
            if path.extension().and_then(|e| e.to_str()) != Some("baml") {
                continue;
            }
            let canonical = std::fs::canonicalize(&path).unwrap_or(path);
            let Ok(content) = std::fs::read_to_string(&canonical) else {
                continue;
            };
            {
                let Ok(mut mirror) = doc_mirror.lock() else {
                    continue;
                };
                if mirror.get(&canonical).map(String::as_str) == Some(content.as_str()) {
                    // The browser already has this content (it wrote it). No echo.
                    continue;
                }
                mirror.insert(canonical.clone(), content.clone());
            }
            let Ok(url) = lsp_types::Url::from_file_path(&canonical) else {
                continue;
            };
            let notif = lsp_server::Notification {
                method: DISK_CHANGE_NOTIFICATION.to_string(),
                params: serde_json::json!({ "uri": url.to_string(), "text": content }),
            };
            let message = lsp_server::Message::Notification(notif);
            // Disk pushes are best-effort refresh hints: budget exhaustion
            // drops the hint (the browser re-reads on save/reload) instead of
            // growing an unbounded queue.
            let frame = match lsp_out_budget.try_message(message) {
                Ok(frame) => frame,
                Err(crate::OutboundReserveError::Saturated) => {
                    tracing::warn!("Disk watcher: LSP outbound byte budget is saturated");
                    continue;
                }
                Err(
                    crate::OutboundReserveError::Oversized
                    | crate::OutboundReserveError::Serialization,
                ) => {
                    tracing::warn!("Disk watcher: LSP outbound frame is not deliverable");
                    continue;
                }
            };
            let _ = lsp_out_tx.send(frame);
            tracing::debug!(
                "Disk watcher: pushed external change for {}",
                canonical.display()
            );
        }
    };

    let mut watcher = match notify::recommended_watcher(handler) {
        Ok(w) => w,
        Err(e) => {
            tracing::warn!("Disk watcher: failed to initialize: {e}");
            return None;
        }
    };
    for root in roots {
        match watcher.watch(root, RecursiveMode::Recursive) {
            Ok(()) => tracing::info!("Disk watcher: watching {}", root.display()),
            Err(e) => tracing::warn!("Disk watcher: failed to watch {}: {e}", root.display()),
        }
    }
    Some(watcher)
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
            // The edit may have added or removed an `env.FOO` reference, and
            // the declared set decides which keys are worth blocking a run to
            // prompt for. Without this refresh a removed key keeps prompting
            // and a newly added one resolves silently until the session
            // reconnects.
            state
                .env_state
                .set_declared_keys(&state.bex.all_env_var_names());
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
        playground_protocol: 2,
        min_client_playground_protocol: 2,
        capabilities: vec![
            "playgroundWebSocket.v2".to_string(),
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

    // Send the env vars the project actually references (names from BAML
    // source, values from the server process env). The full process
    // environment is deliberately NOT sent — it contains unrelated secrets.
    // Dynamically-computed keys still resolve lazily via the EnvVarRequest
    // round-trip (playground_env.rs).
    {
        let names = state.bex.all_env_var_names();
        // Only these keys are worth blocking a run to prompt for; everything
        // else resolves to unset without stalling. See `playground_env`.
        state.env_state.set_declared_keys(&names);
        let vars = collect_referenced_env_vars(&names, |name| std::env::var(name).ok());
        if let Some(msg) = to_ws_text(&WsOutMessage::ProcessEnvVars { vars })
            && sink.send(msg).await.is_err()
        {
            return;
        }
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
    // One coherent launch snapshot: engine + generation captured
    // in a single transaction, with the overlay control-flow graph pinned
    // for that generation so overlay spans stay resolvable after later
    // recompiles. Replaces the racy generation → graph → engine triple-read.
    let prepared = match state.bex.prepare_function_run(
        &project,
        overlay_function_name_for_target(&target.run_target),
    ) {
        Ok(prepared) => prepared,
        Err(e) => {
            send_ws(
                sink,
                &client.error("projectNotReady", format!("Cannot start run: {e}")),
            )
            .await;
            return;
        }
    };
    let project_generation = prepared.generation;
    let bex = prepared.engine;

    let fs_path = bex_project::FsPath::from_str(project);
    let boundary_id = BoundaryId::new_random();
    let logger = bex_project::TraceLogger::bounded(16);
    let function_call_ctx = bex_project::FunctionCallContextBuilder::new(call_id)
        .with_boundary_id(boundary_id)
        .with_logger(logger.clone());

    let run_store = state.run_store.clone();
    let history_store = state.history_store.clone();
    let value_store = state.value_store.clone();
    let started = run_store.create_attached_run(
        boundary_id,
        ExecutionRequest {
            project_id: ProjectId(fs_path.as_path().to_string_lossy().to_string()),
            project_generation: ProjectGeneration(project_generation),
            target: target.run_target.clone(),
            args_summary: None,
            options_summary: None,
        },
        RequestId(client.request_id()),
        HostCallId::Native(call_id),
    );
    let project_root =
        history_project_root_for_project(fs_path.as_path().to_string_lossy().as_ref());
    if let Err(err) = history_store.begin(&project_root, &started.start) {
        tracing::warn!(
            "History begin failed for {}: {err}",
            started.start.boundary_id.to_wire_string()
        );
    }
    broadcast_started_host_run(
        &broadcast_tx,
        &run_store,
        &started,
        client.run_started_request_id(),
    );

    tokio::spawn(async move {
        let function_name = target.call_function_name;
        match bex
            .call_function_with_trace(&function_name, kwargs.into(), function_call_ctx.build())
            .await
        {
            Ok(traced) => {
                drain_logs_and_broadcast(
                    &broadcast_tx,
                    &run_store,
                    &history_store,
                    &value_store,
                    boundary_id,
                    &logger,
                );
                let outcome = match traced.value {
                    Ok(_result) => root_value_success_outcome(None, "baml.outbound.base64"),
                    Err(e) => runtime_error_outcome_with_ref(&e, None),
                };
                complete_run_and_broadcast(
                    &broadcast_tx,
                    &run_store,
                    &history_store,
                    boundary_id,
                    outcome,
                );
            }
            Err(e) => {
                drain_logs_and_broadcast(
                    &broadcast_tx,
                    &run_store,
                    &history_store,
                    &value_store,
                    boundary_id,
                    &logger,
                );
                complete_run_and_broadcast(
                    &broadcast_tx,
                    &run_store,
                    &history_store,
                    boundary_id,
                    runtime_error_outcome_with_ref(&e, None),
                );
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
    let boundary_id = BoundaryId::new_random();
    let logger = bex_project::TraceLogger::bounded(16);
    let ctx = bex_project::FunctionCallContextBuilder::new(call_id)
        .with_boundary_id(boundary_id)
        .with_logger(logger.clone())
        .build();
    let broadcast_tx = state.broadcast_tx.clone();
    let bex = state.bex.clone();
    let run_store = state.run_store.clone();
    let history_store = state.history_store.clone();
    let value_store = state.value_store.clone();
    let started = run_store.create_attached_run(
        boundary_id,
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
        HostCallId::Native(call_id),
    );
    let project_root = history_project_root_for_project(&project);
    if let Err(err) = history_store.begin(&project_root, &started.start) {
        tracing::warn!(
            "History begin failed for {}: {err}",
            started.start.boundary_id.to_wire_string()
        );
    }
    broadcast_started_host_run(
        &broadcast_tx,
        &run_store,
        &started,
        client.run_started_request_id(),
    );

    tokio::spawn(async move {
        match bex
            .call_test_function_with_trace(&project, generation, &test_name, ctx)
            .await
        {
            Ok(traced) => {
                drain_logs_and_broadcast(
                    &broadcast_tx,
                    &run_store,
                    &history_store,
                    &value_store,
                    boundary_id,
                    &logger,
                );
                let outcome = match traced.value {
                    Ok(_result) => root_value_success_outcome(None, "testReport"),
                    Err(e) => engine_error_outcome_with_ref(&e, None),
                };
                complete_run_and_broadcast(
                    &broadcast_tx,
                    &run_store,
                    &history_store,
                    boundary_id,
                    outcome,
                );
            }
            Err(e) => {
                drain_logs_and_broadcast(
                    &broadcast_tx,
                    &run_store,
                    &history_store,
                    &value_store,
                    boundary_id,
                    &logger,
                );
                complete_run_and_broadcast(
                    &broadcast_tx,
                    &run_store,
                    &history_store,
                    boundary_id,
                    engine_error_outcome_with_ref(&e, None),
                );
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

        WsInMessage::CancelRun {
            request_id,
            boundary_id,
        } => {
            let boundary_id = match parse_boundary_id_for_request(request_id, &boundary_id) {
                Ok(boundary_id) => boundary_id,
                Err(msg) => {
                    send_ws(sink, &msg).await;
                    return;
                }
            };
            let run_identity = state
                .run_store
                .snapshot(boundary_id)
                .map(|run| (run.request.project_id.0, run.request.project_generation.0));

            let response = match state.run_store.cancel_run(boundary_id, epoch_ms(), None) {
                bex_events::run::CancelRunEffect::CancelHostCall {
                    host_call_id,
                    patch,
                } => {
                    broadcast_run_patch(&state.broadcast_tx, &patch);
                    state.io_state.cancel_for_host_call(&host_call_id);
                    state.env_state.cancel_for_host_call(&host_call_id);
                    match (host_call_id, run_identity) {
                        (HostCallId::Native(call_id), Some((project_id, generation))) => {
                            // Best-effort cancel: `engine_for_generation`
                            // resolves only while the run's generation is
                            // still the installed one (only one engine is
                            // retained), so a run pinned to a superseded
                            // engine is not reachable from here — the
                            // current-engine fallback cannot find its call
                            // and the run keeps executing. Registration that
                            // retains superseded run engines is deferred.
                            let engine = state
                                .bex
                                .engine_for_generation(&project_id, generation)
                                .map(Ok)
                                .unwrap_or_else(|| {
                                    let fs_path = bex_project::FsPath::from_str(project_id.clone());
                                    state.bex.get_bex_for_project(&fs_path)
                                });
                            match engine {
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

        WsInMessage::ListHistory { request_id, filter } => {
            let runs = state
                .history_store
                .list(&run_filter_from_wire(filter))
                .iter()
                .map(run_summary_to_wire)
                .collect();
            send_ws(sink, &WsOutMessage::HistoryList { request_id, runs }).await;
        }

        WsInMessage::OpenHistory {
            request_id,
            boundary_id,
        } => {
            let boundary_id = match parse_boundary_id_for_request(request_id, &boundary_id) {
                Ok(boundary_id) => boundary_id,
                Err(msg) => {
                    send_ws(sink, &msg).await;
                    return;
                }
            };
            let snapshot = match state.run_store.snapshot(boundary_id) {
                Some(snapshot) => Ok(snapshot),
                None => state.history_store.open(boundary_id).map(|run| {
                    let snapshot = run.clone();
                    let _ = state.run_store.insert_replayed_run(run);
                    snapshot
                }),
            };
            match snapshot {
                Ok(snapshot) => {
                    send_ws(
                        sink,
                        &WsOutMessage::RunSnapshot {
                            request_id: Some(request_id),
                            boundary_id: boundary_id.to_wire_string(),
                            snapshot: run_to_wire(&snapshot),
                        },
                    )
                    .await;
                }
                Err(err) => {
                    send_ws(
                        sink,
                        &WsOutMessage::CommandError {
                            request_id,
                            code: "historyOpenFailed".to_string(),
                            message: format!("{err}"),
                        },
                    )
                    .await;
                }
            }
        }

        WsInMessage::Snapshot {
            request_id,
            boundary_id,
        } => {
            let boundary_id = match parse_boundary_id_for_request(request_id, &boundary_id) {
                Ok(boundary_id) => boundary_id,
                Err(msg) => {
                    send_ws(sink, &msg).await;
                    return;
                }
            };
            // A terminal run may have been evicted from the in-memory store
            // by the retention policy; rehydrate it from disk history like
            // OpenHistory does.
            let snapshot = state.run_store.snapshot(boundary_id).or_else(|| {
                state.history_store.open(boundary_id).ok().map(|run| {
                    let snapshot = run.clone();
                    let _ = state.run_store.insert_replayed_run(run);
                    snapshot
                })
            });
            match snapshot {
                Some(snapshot) => {
                    send_ws(
                        sink,
                        &WsOutMessage::RunSnapshot {
                            request_id: Some(request_id),
                            boundary_id: boundary_id.to_wire_string(),
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

        WsInMessage::ReadValue {
            request_id,
            boundary_id,
            value_ref,
        } => {
            let boundary_id = match parse_boundary_id_for_request(request_id, &boundary_id) {
                Ok(boundary_id) => boundary_id,
                Err(msg) => {
                    send_ws(sink, &msg).await;
                    return;
                }
            };
            let value_ref_id = value_ref.id;
            let requested_codec = value_ref
                .codec
                .unwrap_or_else(|| ValueCodec::BamlOutboundValue.as_wire_str().to_string());
            let live_value = state
                .value_store
                .lock()
                .ok()
                .map_or(LiveValueLookup::Missing, |mut store| {
                    store.get(boundary_id, &value_ref_id)
                });
            let history_result = if !matches!(live_value, LiveValueLookup::Available(_)) {
                match state
                    .history_store
                    .read_value_result(boundary_id, &value_ref_id)
                {
                    Ok(value) => Ok(value),
                    Err(err) => {
                        tracing::warn!(
                            "History readValue failed for {} {}: {err}",
                            boundary_id.to_wire_string(),
                            value_ref_id
                        );
                        Err(err.to_string())
                    }
                }
            } else {
                Ok(HistoryValueReadResult::Missing)
            };
            let response = value_body_response(
                request_id,
                boundary_id,
                value_ref_id,
                requested_codec,
                live_value,
                history_result,
            );
            send_ws(sink, &response).await;
        }

        WsInMessage::Subscribe {
            request_id,
            subscription_id,
            boundary_id,
            after_cursor,
        } => {
            let boundary_id = match parse_boundary_id_for_request(request_id, &boundary_id) {
                Ok(boundary_id) => boundary_id,
                Err(msg) => {
                    send_ws(sink, &msg).await;
                    return;
                }
            };
            match state
                .run_store
                .subscribe(boundary_id, after_cursor.map(RunCursor))
            {
                RunSubscription::Snapshot { snapshot, patches } => {
                    send_ws(
                        sink,
                        &WsOutMessage::RunSnapshot {
                            request_id: Some(request_id),
                            boundary_id: boundary_id.to_wire_string(),
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
                            boundary_id: boundary_id.to_wire_string(),
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
            boundary_id,
            input_request_id,
            value,
        } => {
            let boundary_id = match parse_boundary_id_for_request(request_id, &boundary_id) {
                Ok(boundary_id) => boundary_id,
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
                .resolve_for_run(boundary_id, input_request_id, value);
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
            boundary_id,
            env_request_id,
            value,
        } => {
            let boundary_id = match parse_boundary_id_for_request(request_id, &boundary_id) {
                Ok(boundary_id) => boundary_id,
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
                .resolve_for_run(boundary_id, env_request_id, value);
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
            // A page that connected after an OpenPlayground request (a freshly
            // spawned browser window, or a reconnect) still needs to be told
            // where to navigate. Replay the last target directly to it.
            let target = state.current_open_target.lock().unwrap().clone();
            if let Some(target) = target {
                let notif = bex_project::PlaygroundNotification::OpenPlayground {
                    project: target.project,
                    function_name: target.function_name,
                    test_name: target.test_name,
                    testset_name: target.testset_name,
                };
                let json = serde_json::to_value(&notif).unwrap_or_default();
                let msg = WsOutMessage::PlaygroundNotification { notification: json };
                if let Some(ws_msg) = to_ws_text(&msg)
                    && sink.send(ws_msg).await.is_err()
                {
                    tracing::warn!("Failed to replay open-playground target");
                }
            }
        }

        WsInMessage::RequestCollectTests { project } => {
            state.bex.request_collect_tests(&project);
        }

        WsInMessage::RequestControlFlowGraph {
            project: _,
            function_name,
            request_id,
        } => {
            let graph = state.bex.ast_control_flow_graph(&function_name);
            let graph = graph.map(|g| {
                baml_compiler2_visualization::control_flow::prepare_control_flow_graph_for_visualization(&g)
            });
            let graph_json = graph.as_ref().and_then(|g| serde_json::to_value(g).ok());
            let msg = WsOutMessage::ControlFlowGraphResult {
                function_name,
                graph: graph_json,
                request_id,
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

async fn api_guard_middleware(
    State(access_guard): State<PlaygroundAccessGuard>,
    req: Request<Body>,
    next: Next,
) -> Response {
    let origin = req.headers().get(header::ORIGIN).cloned();
    if !access_guard.is_allowed_origin(origin.as_ref()) {
        return text_response(StatusCode::FORBIDDEN, "Forbidden origin".to_string());
    }

    if req.method() == Method::OPTIONS {
        let mut response = Response::builder()
            .status(StatusCode::NO_CONTENT)
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
        apply_api_response_headers(&access_guard, origin.as_ref(), &mut response);
        return response;
    }

    let mut resp = next.run(req).await;
    apply_api_response_headers(&access_guard, origin.as_ref(), &mut resp);
    resp
}

fn apply_api_response_headers(
    access_guard: &PlaygroundAccessGuard,
    origin: Option<&HeaderValue>,
    resp: &mut Response,
) {
    if let Some(origin) = access_guard.cors_origin(origin) {
        resp.headers_mut()
            .insert(header::ACCESS_CONTROL_ALLOW_ORIGIN, origin);
        resp.headers_mut()
            .insert(header::VARY, HeaderValue::from_static("Origin"));
    }
    resp.headers_mut().insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("no-store, no-cache, must-revalidate"),
    );
    resp.headers_mut()
        .insert(header::PRAGMA, HeaderValue::from_static("no-cache"));
    resp.headers_mut()
        .insert(header::EXPIRES, HeaderValue::from_static("0"));
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

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use bex_events::{
        history::HistoryValueBody,
        ids::{BexCallId, BexThreadId, EngineId, ProcessEuid},
        run::{PayloadKind, ProjectId, RunTimeAnchor, TraceCallKey},
    };
    use bex_heap::{BexHeap, HeapPermit as _, HeapPermitManager, Tlab, TlabHolder};
    use bex_vm_types::{RootHaver, Value};

    use super::*;

    fn unique_temp_dir(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "{name}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ))
    }

    struct EmptyRoots {
        tlab: Tlab,
    }

    impl RootHaver for EmptyRoots {
        fn collect_roots(&self, _roots: &mut Vec<bex_vm_types::HeapPtr>) {}

        fn forward_roots(
            &mut self,
            _forward: &std::collections::HashMap<bex_vm_types::HeapPtr, bex_vm_types::HeapPtr>,
        ) {
        }
    }

    impl TlabHolder for EmptyRoots {
        fn tlab(&self) -> &Tlab {
            &self.tlab
        }

        fn tlab_mut(&mut self) -> &mut Tlab {
            &mut self.tlab
        }
    }

    fn test_execution_request() -> ExecutionRequest {
        ExecutionRequest {
            project_id: ProjectId("native-project".to_string()),
            project_generation: ProjectGeneration(1),
            target: RunTarget::Function {
                function_name: "user.LogIt".to_string(),
            },
            args_summary: None,
            options_summary: None,
        }
    }

    fn test_trace_key() -> TraceCallKey {
        TraceCallKey {
            process_euid: ProcessEuid([8; 16]),
            engine_id: EngineId(2),
            thread_id: BexThreadId(1),
            call_id: BexCallId(3),
        }
    }

    #[test]
    fn value_body_response_distinguishes_history_read_failure_from_missing_value() {
        let boundary_id = BoundaryId::from_bytes([31; 16]);

        let missing = value_body_response(
            7,
            boundary_id,
            "value_missing".to_string(),
            ValueCodec::BamlOutboundValue.as_wire_str().to_string(),
            LiveValueLookup::Missing,
            Ok(HistoryValueReadResult::Missing),
        );
        let WsOutMessage::ValueBody {
            availability,
            diagnostic,
            body_base64,
            ..
        } = missing
        else {
            panic!("expected value body response");
        };
        assert_eq!(availability, "missing");
        assert_eq!(body_base64, None);
        assert_eq!(diagnostic.as_deref(), Some("value body is not available"));

        let failed = value_body_response(
            8,
            boundary_id,
            "value_failed".to_string(),
            ValueCodec::BamlOutboundValue.as_wire_str().to_string(),
            LiveValueLookup::Missing,
            Err("failed to read value segment value-0.bamlvalue".to_string()),
        );
        let WsOutMessage::ValueBody {
            availability,
            diagnostic,
            body_base64,
            ..
        } = failed
        else {
            panic!("expected value body response");
        };
        assert_eq!(availability, "lost");
        assert_eq!(body_base64, None);
        assert!(
            diagnostic
                .as_deref()
                .is_some_and(|message| message.contains("history value read failed")),
            "{diagnostic:?}"
        );
    }

    #[test]
    fn value_body_response_preserves_history_body() {
        let boundary_id = BoundaryId::from_bytes([32; 16]);
        let response = value_body_response(
            9,
            boundary_id,
            "value_1".to_string(),
            ValueCodec::BamlOutboundValue.as_wire_str().to_string(),
            LiveValueLookup::Missing,
            Ok(HistoryValueReadResult::Available(HistoryValueBody {
                codec: ValueCodec::BamlOutboundValue,
                body: vec![1, 2, 3],
            })),
        );
        let WsOutMessage::ValueBody {
            availability,
            body_base64,
            diagnostic,
            ..
        } = response
        else {
            panic!("expected value body response");
        };
        assert_eq!(availability, "available");
        assert_eq!(body_base64.as_deref(), Some("AQID"));
        assert_eq!(diagnostic, None);
    }

    #[tokio::test]
    async fn native_zero_capacity_drain_broadcasts_capture_loss_diagnostic() {
        let project = unique_temp_dir("baml-native-capture-loss");
        std::fs::create_dir_all(&project).expect("project dir should be created");
        std::fs::write(project.join("baml.toml"), "").expect("manifest should be created");

        let boundary_id = BoundaryId::from_bytes([21; 16]);
        let run_store = InMemoryRunStore::default();
        let start = run_store.create_run_at(
            boundary_id,
            test_execution_request(),
            RequestId(1),
            RunTimeAnchor {
                epoch_created_at_ms: 20,
                trace_zero_ns: 0,
            },
        );
        let history_store = HistoryStore::new(vec![project.clone()]);
        history_store.begin(&project, &start).unwrap();
        let value_store = Arc::new(Mutex::new(LiveValueCache::with_max_bytes(
            DEFAULT_NATIVE_LIVE_VALUE_CACHE_BYTES,
        )));
        let (broadcast_tx, mut broadcast_rx) = broadcast::channel(8);

        let logger = bex_project::TraceLogger::bounded(0);
        logger.capture_with(boundary_id, test_trace_key(), |_| {
            panic!("zero-capacity logger must not copy a value")
        });
        assert_eq!(logger.stats().skipped_log_queue_full, 1);

        drain_logs_and_broadcast(
            &broadcast_tx,
            &run_store,
            &history_store,
            &value_store,
            boundary_id,
            &logger,
        );

        let snapshot = run_store.snapshot(boundary_id).expect("run should exist");
        let diagnostic = snapshot
            .diagnostics
            .iter()
            .find(|diagnostic| diagnostic.code.as_deref() == Some("logCaptureLoss"))
            .expect("capture loss should be recorded live");
        assert_eq!(
            diagnostic.message,
            "Skipped 1 captured log value(s) because the log capture queue was full"
        );

        let patch = broadcast_rx
            .try_recv()
            .expect("drain should broadcast a capture-loss patch");
        assert!(matches!(patch, WsOutMessage::RunPatch { .. }));

        let _ = std::fs::remove_dir_all(project);
    }

    #[test]
    fn playground_access_guard_accepts_any_loopback_port_and_vscode_origins() {
        let guard = PlaygroundAccessGuard::new();
        assert!(guard.is_allowed_origin(None));
        for allowed in [
            "http://localhost:4265",
            "http://localhost:8000", // ssh -L remapped tunnel port
            "http://127.0.0.1:9999",
            "http://[::1]:4000",
            "https://localhost:8443",
            "vscode-webview://abc123",
            "https://abc123.vscode-cdn.net",
        ] {
            assert!(
                guard.is_allowed_origin(Some(&HeaderValue::from_static(allowed))),
                "{allowed}"
            );
        }
        for denied in [
            "https://example.com",
            "http://localhost.evil.com", // suffix trick
            "http://10.0.0.5:4265",      // non-loopback IP
            "http://127.0.0.1.evil.com:4265",
            "null",
            "https://vscode-cdn.net.example.com",
        ] {
            assert!(
                !guard.is_allowed_origin(Some(&HeaderValue::from_static(denied))),
                "{denied}"
            );
        }
    }

    #[test]
    fn collect_referenced_env_vars_filters_unset() {
        let names = vec![
            "OPENAI_API_KEY".to_string(),
            "UNSET_VAR".to_string(),
            "ANTHROPIC_API_KEY".to_string(),
        ];
        let vars = collect_referenced_env_vars(&names, |name| match name {
            "OPENAI_API_KEY" => Some("sk-1".to_string()),
            "ANTHROPIC_API_KEY" => Some("sk-2".to_string()),
            _ => None,
        });
        assert_eq!(vars.len(), 2);
        assert_eq!(vars["OPENAI_API_KEY"], "sk-1");
        assert_eq!(vars["ANTHROPIC_API_KEY"], "sk-2");
        assert!(!vars.contains_key("UNSET_VAR"));
    }

    #[tokio::test]
    async fn bind_exact_port_reports_conflict_with_actionable_error() {
        let (occupied, port) = pick_port(4265, 100).await.expect("a free port to occupy");

        let err = bind_exact_port(port)
            .await
            .expect_err("second bind of the same port should fail");
        let message = format!("{err}");
        assert!(message.contains(&port.to_string()), "{message}");
        assert!(message.contains("--port"), "{message}");

        drop(occupied);
    }

    #[test]
    fn lsp_save_path_must_stay_under_workspace_roots() {
        let root = unique_temp_dir("baml-lsp-save-root");
        let outside = unique_temp_dir("baml-lsp-save-outside");
        std::fs::create_dir_all(&root).expect("root should be created");
        std::fs::create_dir_all(&outside).expect("outside should be created");
        let inside_file = root.join("main.baml");
        let outside_file = outside.join("main.baml");
        std::fs::write(&inside_file, "function Test() -> int { 1 }\n")
            .expect("inside file should be created");
        std::fs::write(&outside_file, "function Test() -> int { 2 }\n")
            .expect("outside file should be created");

        assert!(allowed_lsp_save_path(&inside_file, std::slice::from_ref(&root)).is_some());
        assert!(allowed_lsp_save_path(&outside_file, std::slice::from_ref(&root)).is_none());
        assert!(
            allowed_lsp_save_path(&root.join("new.baml"), std::slice::from_ref(&root)).is_some()
        );

        let _ = std::fs::remove_dir_all(&root);
        let _ = std::fs::remove_dir_all(&outside);
    }

    #[test]
    fn history_project_root_for_project_uses_manifestless_project_owner() {
        let root = unique_temp_dir("baml-lsp-history-root");
        let source_dir = root.join("baml_src/ns");
        std::fs::create_dir_all(&source_dir).expect("source dir should be created");
        std::fs::write(
            source_dir.join("main.baml"),
            "function Test() -> int { 1 }\n",
        )
        .expect("source file should be created");

        assert_eq!(
            history_project_root_for_project(&source_dir.to_string_lossy()),
            root
        );

        let _ = std::fs::remove_dir_all(&root);
    }

    #[tokio::test]
    async fn native_drain_persists_identified_log_and_retains_live_body() {
        let project = unique_temp_dir("baml-native-log-history");
        std::fs::create_dir_all(&project).expect("project dir should be created");
        std::fs::write(project.join("baml.toml"), "").expect("manifest should be created");

        let boundary_id = BoundaryId::from_bytes([13; 16]);
        let run_store = InMemoryRunStore::default();
        let start = run_store.create_run_at(
            boundary_id,
            test_execution_request(),
            RequestId(1),
            RunTimeAnchor {
                epoch_created_at_ms: 20,
                trace_zero_ns: 0,
            },
        );
        let history_store = HistoryStore::new(vec![project.clone()]);
        history_store.begin(&project, &start).unwrap();
        let value_store = Arc::new(Mutex::new(LiveValueCache::with_max_bytes(
            DEFAULT_NATIVE_LIVE_VALUE_CACHE_BYTES,
        )));
        let (broadcast_tx, mut broadcast_rx) = broadcast::channel(8);

        let logger = bex_project::TraceLogger::bounded(4);
        let heap = BexHeap::new(Vec::new());
        let manager = HeapPermitManager::new();
        let permit = manager
            .new_permit(EmptyRoots {
                tlab: Tlab::new(Arc::clone(&heap)),
            })
            .await
            .acquire()
            .await;
        logger.capture_with(boundary_id, test_trace_key(), |trace_heap| {
            let snapshot =
                trace_heap.copy_value_from_bex_heap(&heap, permit.proof(), Value::int(42));
            (
                bex_project::TraceLogMetadata {
                    level: Some("info".to_string()),
                    source: None,
                    timestamp_ms: 21,
                    message_preview: Some("hello from log".to_string()),
                },
                snapshot,
            )
        });

        drain_logs_and_broadcast(
            &broadcast_tx,
            &run_store,
            &history_store,
            &value_store,
            boundary_id,
            &logger,
        );

        let snapshot = run_store.snapshot(boundary_id).expect("run should exist");
        let payload = snapshot
            .payloads
            .iter()
            .find_map(|payload| match &payload.kind {
                PayloadKind::Log(log) => Some(log),
                _ => None,
            })
            .expect("log payload should be ingested");
        assert_eq!(payload.level.as_deref(), Some("info"));
        assert_eq!(payload.message, "hello from log");
        let value_ref = payload.value_ref.as_ref().expect("log value ref");

        let live = match value_store.lock().unwrap().get(boundary_id, &value_ref.id) {
            LiveValueLookup::Available(live) => live,
            other => panic!("live value body should be retained, got {other:?}"),
        };
        assert_eq!(live.codec, ValueCodec::BamlOutboundValue);
        assert!(!live.body.is_empty());
        assert_eq!(
            history_store
                .read_value(boundary_id, &value_ref.id)
                .unwrap()
                .unwrap()
                .body,
            live.body
        );

        let patch = broadcast_rx
            .try_recv()
            .expect("drain should broadcast a RunStore patch");
        assert!(matches!(patch, WsOutMessage::RunPatch { .. }));

        let _ = std::fs::remove_dir_all(project);
    }
}
