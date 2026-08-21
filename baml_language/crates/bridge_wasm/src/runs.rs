//! Runs, as the browser sees them.
//!
//! Everything below the wire is `bex_events` machinery the desktop host uses
//! too — `InMemoryRunStore`, `LiveValueCache`, the trace logger.
//! What is browser-specific is the *history* store (there is no disk, so
//! completed runs are retained in memory behind the same read API) and the
//! fact that every answer leaves as a playground notification rather than a
//! WebSocket frame: the host has exactly one callback.

use std::{cell::RefCell, collections::HashMap, io, rc::Rc};

use base64::Engine as _;
use bex_events::{
    history::{
        HistoryValueReadResult, HistoryValueSegment, history_run_matches_filter,
        open_boundary_from_value_segments, read_value_from_segments_result, summarize_history_run,
    },
    run::{
        BoundaryId, CancellationState, EnvResolutionStatus, ExecutionRequest, HostCallId,
        InMemoryRunStore, ProjectGeneration, ProjectId, RequestId, RunCursor,
        RunCursorExpiredReason, RunDiagnostic, RunError, RunErrorClass, RunFilter, RunKind,
        RunOutcome, RunPatch, RunRequestState, RunResult, RunSubscription, RunSummary, RunTarget,
        RunVisibilityFilter, StartRunContext, StartedHostRun, patch_to_wire, run_summary_to_wire,
        run_to_wire,
    },
    value::{
        ByteValueArtifactSink, CaptureLossKind, CaptureLossReason, CaptureLossRecord,
        DEFAULT_WASM_LIVE_VALUE_CACHE_BYTES, LiveValueBody, LiveValueCache, LiveValueLookup,
        LogEventRecord, RunCompletedRecord, RunStartedRecord, ValueCodec, ValueIdAllocator,
        ValueRef, ValueWriteOutcome, ValueWriter,
    },
};
use bridge_ctypes::{HANDLE_TABLE, playground_run_args_to_bex_values};
use js_sys::Function;
use serde::Deserialize;
use wasm_bindgen::prelude::*;

use crate::{
    BamlWasmRuntime, playground_notify::PlaygroundNotification, send_wrapper::SendWrapper,
};


#[derive(Debug, Deserialize)]
struct WasmRunListFilter {
    #[serde(rename = "projectId")]
    project_id: Option<String>,
    #[serde(rename = "projectGeneration")]
    project_generation: Option<u64>,
    kinds: Option<Vec<WasmRunListKind>>,
    #[serde(rename = "callTreeContainsFunction")]
    call_tree_contains_function: Option<String>,
    visibility: Option<WasmRunListVisibility>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
enum WasmRunListKind {
    Function,
    Test,
    Preview,
    Companion,
    Internal,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
enum WasmRunListVisibility {
    HistoryOnly,
    IncludeHidden,
    AllForDebug,
}

#[derive(Debug, Deserialize)]
struct WasmValueRef {
    id: String,
    codec: Option<String>,
}

/// Captured values, bounded so a long session cannot grow without limit.
pub(crate) type WasmLiveValueStore = Rc<RefCell<LiveValueCache>>;
/// Terminal runs, retained in memory (there is no disk here).
pub(crate) type WasmHistoryStore = Rc<RefCell<WasmHistoryStoreInner>>;


#[derive(Debug, Default)]
pub(crate) struct WasmHistoryStoreInner {
    boundaries: HashMap<BoundaryId, WasmHistoryBoundary>,
}

#[derive(Debug)]
struct WasmHistoryBoundary {
    value_writer: ValueWriter<ByteValueArtifactSink>,
}

impl WasmHistoryStoreInner {
    fn begin(&mut self, start: &StartRunContext) -> io::Result<()> {
        let mut value_writer = ValueWriter::new(ByteValueArtifactSink::new(), start.boundary_id)?;
        value_writer.append_run_started(&RunStartedRecord {
            request: start.request.clone(),
            created_at_ms: start.created_at_ms,
            time_anchor: start.time_anchor,
        })?;
        self.boundaries
            .insert(start.boundary_id, WasmHistoryBoundary { value_writer });
        Ok(())
    }

    fn append_log_body(
        &mut self,
        boundary_id: BoundaryId,
        event: LogEventRecord,
        codec: ValueCodec,
        body: Vec<u8>,
    ) -> io::Result<ValueWriteOutcome> {
        let boundary = self.boundaries.get_mut(&boundary_id).ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::NotFound,
                format!(
                    "WASM history boundary {} was not begun",
                    boundary_id.to_wire_string()
                ),
            )
        })?;
        boundary.value_writer.append_log_body(codec, body, event)
    }

    fn append_capture_loss(
        &mut self,
        boundary_id: BoundaryId,
        record: &CaptureLossRecord,
    ) -> io::Result<()> {
        let boundary = self.boundaries.get_mut(&boundary_id).ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::NotFound,
                format!(
                    "WASM history boundary {} was not begun",
                    boundary_id.to_wire_string()
                ),
            )
        })?;
        boundary.value_writer.append_capture_loss(record)
    }

    fn complete(
        &mut self,
        boundary_id: BoundaryId,
        outcome: &RunOutcome,
        completed_at_ms: u64,
    ) -> io::Result<()> {
        let Some(boundary) = self.boundaries.get_mut(&boundary_id) else {
            return Ok(());
        };
        let record = RunCompletedRecord {
            status: outcome.status(),
            completed_at_ms,
            renderer_hint: match outcome {
                RunOutcome::Succeeded(result) => result.renderer_hint.clone(),
                RunOutcome::Failed(_) | RunOutcome::Cancelled(_) | RunOutcome::Panicked(_) => None,
            },
            result_value_ref: match outcome {
                RunOutcome::Succeeded(result) => result.value_ref.clone(),
                RunOutcome::Failed(_) | RunOutcome::Cancelled(_) | RunOutcome::Panicked(_) => None,
            },
            error: match outcome {
                RunOutcome::Failed(error) | RunOutcome::Panicked(error) => Some(error.clone()),
                RunOutcome::Succeeded(_) | RunOutcome::Cancelled(_) => None,
            },
            cancellation: match outcome {
                RunOutcome::Cancelled(cancellation) => Some(cancellation.clone()),
                RunOutcome::Succeeded(_) | RunOutcome::Failed(_) | RunOutcome::Panicked(_) => None,
            },
        };
        boundary.value_writer.append_run_completed(&record)?;
        boundary.value_writer.flush()?;
        Ok(())
    }

    fn list(&self, filter: &RunFilter) -> Vec<RunSummary> {
        let mut summaries = self
            .boundaries
            .keys()
            .filter_map(|boundary_id| self.open(*boundary_id).ok())
            .filter(|run| history_run_matches_filter(run, filter))
            .map(|run| summarize_history_run(&run))
            .collect::<Vec<_>>();
        summaries.sort_by_key(|summary| std::cmp::Reverse(summary.created_at_ms));
        summaries
    }

    fn open(&self, boundary_id: BoundaryId) -> io::Result<bex_events::run::Run> {
        let boundary = self.boundaries.get(&boundary_id).ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::NotFound,
                format!(
                    "WASM history boundary {} was not found",
                    boundary_id.to_wire_string()
                ),
            )
        })?;
        let value_segments = boundary.value_segments();
        open_boundary_from_value_segments(&value_segments)
    }

    fn read_value(
        &self,
        boundary_id: BoundaryId,
        value_ref_id: &str,
    ) -> io::Result<HistoryValueReadResult> {
        let Some(boundary) = self.boundaries.get(&boundary_id) else {
            return Ok(HistoryValueReadResult::Missing);
        };
        read_value_from_segments_result(&boundary.value_segments(), value_ref_id)
    }
}

impl WasmHistoryBoundary {
    fn value_segments(&self) -> Vec<HistoryValueSegment> {
        vec![HistoryValueSegment {
            label: "wasm-value-0.bamlvalue".to_string(),
            bytes: self.value_writer.sink().bytes().to_vec(),
        }]
    }
}

fn run_filter_from_js(filter: JsValue) -> Result<RunFilter, String> {
    if filter.is_undefined() || filter.is_null() {
        return Ok(RunFilter::default());
    }
    let filter: WasmRunListFilter =
        serde_wasm_bindgen::from_value(filter).map_err(|err| err.to_string())?;
    Ok(RunFilter {
        project_id: filter.project_id.map(ProjectId),
        project_generation: filter.project_generation.map(ProjectGeneration),
        kinds: filter
            .kinds
            .unwrap_or_default()
            .into_iter()
            .map(|kind| match kind {
                WasmRunListKind::Function => RunKind::Function,
                WasmRunListKind::Test => RunKind::Test,
                WasmRunListKind::Preview => RunKind::Preview,
                WasmRunListKind::Companion => RunKind::Companion,
                WasmRunListKind::Internal => RunKind::Internal,
            })
            .collect(),
        statuses: Vec::new(),
        call_tree_contains_function: filter.call_tree_contains_function,
        visibility: match filter.visibility {
            Some(WasmRunListVisibility::HistoryOnly) | None => RunVisibilityFilter::HistoryOnly,
            Some(WasmRunListVisibility::IncludeHidden) => RunVisibilityFilter::IncludeHidden,
            Some(WasmRunListVisibility::AllForDebug) => RunVisibilityFilter::AllForDebug,
        },
    })
}

/// Milliseconds since the epoch, for run timestamps.
fn epoch_ms() -> u64 {
    let millis = web_time::SystemTime::now()
        .duration_since(web_time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    u64::try_from(millis).unwrap_or(u64::MAX)
}

fn parse_boundary_id(boundary_id: &str) -> Result<BoundaryId, JsValue> {
    BoundaryId::from_wire_str(boundary_id)
        .ok_or_else(|| JsError::new(&format!("Invalid BoundaryId: {boundary_id}")).into())
}

fn parse_request_id(request_id: &str) -> Result<u64, JsValue> {
    request_id
        .parse::<u64>()
        .map_err(|_| JsError::new(&format!("Invalid request id: {request_id}")).into())
}

fn next_wasm_call_id() -> Result<sys_types::CallId, JsError> {
    let call_id = sys_types::CallId::next().0;
    let _ = u32::try_from(call_id).map_err(|_| JsError::new("Function call ID overflowed u32"))?;
    Ok(sys_types::CallId(call_id))
}

#[allow(clippy::needless_pass_by_value)]
fn send_wasm_notification(callback: &SendWrapper<Function>, notification: PlaygroundNotification) {
    crate::playground_notify::send_wasm_playground_notification(callback.inner(), &notification);
}

fn send_run_started(
    callback: &SendWrapper<Function>,
    run_store: &InMemoryRunStore,
    boundary_id: BoundaryId,
    request_id: Option<u64>,
) {
    if let Some(run) = run_store.snapshot(boundary_id) {
        send_wasm_notification(
            callback,
            PlaygroundNotification::RunStarted {
                request_id,
                run: run_to_wire(&run),
            },
        );
    }
}

fn send_started_host_run(
    callback: &SendWrapper<Function>,
    run_store: &InMemoryRunStore,
    started: &StartedHostRun,
    request_id: Option<u64>,
) {
    send_run_started(callback, run_store, started.start.boundary_id, request_id);
    if let Some(patch) = &started.started_patch {
        send_run_patch(callback, patch);
    }
}

pub(crate) fn wasm_host_call_id(call_id: sys_types::CallId) -> Option<HostCallId> {
    u32::try_from(call_id.0).ok().map(HostCallId::Wasm)
}

pub(crate) fn send_run_patch(callback: &SendWrapper<Function>, patch: &bex_events::run::RunPatch) {
    send_wasm_notification(
        callback,
        PlaygroundNotification::RunPatch {
            patch: patch_to_wire(patch),
        },
    );
}

fn send_run_cursor_expired(
    callback: &SendWrapper<Function>,
    request_id: Option<u64>,
    subscription_id: String,
    boundary_id: String,
    reason: RunCursorExpiredReason,
) {
    let reason = match reason {
        RunCursorExpiredReason::Expired => "expired",
        RunCursorExpiredReason::Compacted => "compacted",
        RunCursorExpiredReason::Unknown => "unknown",
        RunCursorExpiredReason::Future => "future",
        RunCursorExpiredReason::Unavailable => "unavailable",
    };
    send_wasm_notification(
        callback,
        PlaygroundNotification::RunCursorExpired {
            request_id,
            subscription_id,
            boundary_id,
            reason: reason.to_string(),
        },
    );
}

fn send_command_ack(callback: &SendWrapper<Function>, request_id: u64, outcome: &str) {
    send_wasm_notification(
        callback,
        PlaygroundNotification::CommandAck {
            request_id,
            outcome: outcome.to_string(),
        },
    );
}

fn send_command_error(
    callback: &SendWrapper<Function>,
    request_id: u64,
    code: &str,
    message: impl Into<String>,
) {
    send_wasm_notification(
        callback,
        PlaygroundNotification::CommandError {
            request_id,
            code: code.to_string(),
            message: message.into(),
        },
    );
}

fn begin_wasm_history(
    callback: &SendWrapper<Function>,
    run_store: &InMemoryRunStore,
    history_store: &WasmHistoryStore,
    start: &StartRunContext,
) {
    if let Err(err) = history_store.borrow_mut().begin(start) {
        send_wasm_history_diagnostic(callback, run_store, start.boundary_id, err);
    }
}

fn complete_wasm_run(
    callback: &SendWrapper<Function>,
    run_store: &InMemoryRunStore,
    history_store: &WasmHistoryStore,
    boundary_id: BoundaryId,
    outcome: RunOutcome,
) {
    let completed_at_ms = epoch_ms();
    if let Err(err) = history_store
        .borrow_mut()
        .complete(boundary_id, &outcome, completed_at_ms)
    {
        send_wasm_history_diagnostic(callback, run_store, boundary_id, err);
    }
    if let Some(patch) = run_store.complete_run(boundary_id, outcome, completed_at_ms) {
        send_run_patch(callback, &patch);
    }
}

fn root_value_success_outcome(value_ref: Option<ValueRef>, renderer_hint: &str) -> RunOutcome {
    RunOutcome::Succeeded(RunResult {
        value_ref,
        renderer_hint: Some(renderer_hint.to_string()),
        supporting_payload_ids: Vec::new(),
    })
}

fn drain_wasm_logs(
    callback: &SendWrapper<Function>,
    run_store: &InMemoryRunStore,
    history_store: &WasmHistoryStore,
    value_store: &WasmLiveValueStore,
    boundary_id: BoundaryId,
    logger: &bex_project::TraceLogger,
) {
    let mut writer = match ValueWriter::new_with_id_allocator(
        ByteValueArtifactSink::new(),
        boundary_id,
        ValueIdAllocator::live_fallback(),
    ) {
        Ok(writer) => writer,
        Err(err) => {
            send_log_diagnostic(callback, run_store, boundary_id, err);
            return;
        }
    };
    let report = logger.drain_encoded_logs();
    for failure in &report.failures {
        send_log_diagnostic(
            callback,
            run_store,
            failure.boundary_id,
            format!("log capture failed: {}", failure.diagnostic),
        );
    }
    let stats = logger.stats();
    if stats.skipped_log_queue_full > 0 {
        append_wasm_capture_loss_record(
            history_store,
            boundary_id,
            CaptureLossKind::Log,
            stats.skipped_log_queue_full,
        )
        .unwrap_or_else(|err| {
            send_wasm_history_diagnostic(
                callback,
                run_store,
                boundary_id,
                format!("history capture-loss retention failed: {err}"),
            );
        });
        send_log_loss_diagnostic(
            callback,
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
        let outcome = match history_store.borrow_mut().append_log_body(
            encoded.boundary_id,
            event.clone(),
            ValueCodec::BamlOutboundValue,
            encoded.body.clone(),
        ) {
            Ok(outcome) => outcome,
            Err(err) => {
                send_wasm_history_diagnostic(
                    callback,
                    run_store,
                    encoded.boundary_id,
                    format!("history log retention failed; retained live bytes only: {err}"),
                );
                match writer.append_log_body(
                    ValueCodec::BamlOutboundValue,
                    encoded.body.clone(),
                    event,
                ) {
                    Ok(outcome) => outcome,
                    Err(err) => {
                        send_log_diagnostic(callback, run_store, encoded.boundary_id, err);
                        continue;
                    }
                }
            }
        };
        let value_ref = outcome.value_ref;
        let insert = value_store.borrow_mut().insert(
            encoded.boundary_id,
            &value_ref,
            LiveValueBody {
                codec: value_ref.codec,
                body: encoded.body,
            },
        );
        if let Some(diagnostic) = insert.diagnostic {
            send_log_diagnostic(callback, run_store, encoded.boundary_id, diagnostic);
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
            send_run_patch(callback, &patch);
        }
    }
}

fn append_wasm_capture_loss_record(
    history_store: &WasmHistoryStore,
    boundary_id: BoundaryId,
    kind: CaptureLossKind,
    skipped: u64,
) -> io::Result<()> {
    history_store.borrow_mut().append_capture_loss(
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

fn send_wasm_history_diagnostic(
    callback: &SendWrapper<Function>,
    run_store: &InMemoryRunStore,
    boundary_id: BoundaryId,
    err: impl std::fmt::Display,
) {
    if let Some(patch) = run_store.add_diagnostic(
        boundary_id,
        RunDiagnostic {
            severity: bex_events::run::DiagnosticSeverity::Warning,
            code: Some("historyRetentionFailed".to_string()),
            message: format!("Failed to retain WASM history bytes: {err}"),
            payload_id: None,
        },
    ) {
        send_run_patch(callback, &patch);
    }
}

fn send_log_diagnostic(
    callback: &SendWrapper<Function>,
    run_store: &InMemoryRunStore,
    boundary_id: BoundaryId,
    err: impl std::fmt::Display,
) {
    if let Some(patch) = run_store.add_diagnostic(
        boundary_id,
        RunDiagnostic {
            severity: bex_events::run::DiagnosticSeverity::Warning,
            code: Some("logCaptureFailed".to_string()),
            message: format!("Failed to retain captured log bytes: {err}"),
            payload_id: None,
        },
    ) {
        send_run_patch(callback, &patch);
    }
}

fn send_log_loss_diagnostic(
    callback: &SendWrapper<Function>,
    run_store: &InMemoryRunStore,
    boundary_id: BoundaryId,
    capture_kind: &str,
    skipped: u64,
) {
    if let Some(patch) = log_loss_diagnostic_patch(run_store, boundary_id, capture_kind, skipped) {
        send_run_patch(callback, &patch);
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
            severity: bex_events::run::DiagnosticSeverity::Warning,
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
fn runtime_error_outcome_with_ref(
    error: &impl std::fmt::Display,
    value_ref: Option<ValueRef>,
) -> RunOutcome {
    let message = format!("{error}");
    if message.to_lowercase().contains("cancel") {
        let now = epoch_ms();
        RunOutcome::Cancelled(CancellationState {
            requested_at_ms: now,
            completed_at_ms: Some(now),
            reason: Some(message),
        })
    } else {
        RunOutcome::Failed(RunError {
            class: RunErrorClass::Runtime,
            message,
            details: None,
            value_ref,
        })
    }
}

pub(crate) fn new_history_store() -> WasmHistoryStore {
    Rc::new(RefCell::new(WasmHistoryStoreInner::default()))
}

pub(crate) fn new_value_store() -> WasmLiveValueStore {
    Rc::new(RefCell::new(LiveValueCache::with_max_bytes(
        DEFAULT_WASM_LIVE_VALUE_CACHE_BYTES,
    )))
}

// ── The JS surface ──────────────────────────────────────────────────────────

#[wasm_bindgen]
impl BamlWasmRuntime {
    /// Start a RunStore-owned function run.
    ///
    /// Run lifecycle updates are emitted through the playground notification
    /// callback as `runStarted` / `runPatch` messages.
    #[wasm_bindgen(js_name = startRun)]
    pub fn start_run(
        &self,
        request_id: u32,
        project: String,
        name: &str,
        args_bytes: &[u8],
    ) -> Result<(), JsValue> {
        let kwargs = playground_run_args_to_bex_values(args_bytes, &HANDLE_TABLE)
            .map_err(|e| JsError::new(&format!("Failed to convert arguments: {e}")))?;
        let call_id = next_wasm_call_id()?;
        let host_call_id = HostCallId::Wasm(
            u32::try_from(call_id.0)
                .map_err(|_| JsError::new("Function call ID overflowed u32"))?,
        );
        let boundary_id = BoundaryId::new_random();
        let fs_path = bex_project::FsPath::from_str(project);
        let revision = self.state.borrow().revision();
        let prepared = self
            .playground
            .borrow()
            .prepare_run(revision)
            .ok_or_else(|| {
                JsError::new(
                    "the engine is not current with the sources; wait for the rebuild to finish",
                )
            })?;
        let project_generation = prepared.generation;
        let bex = prepared.engine;
        let started = self.run_store.create_attached_run(
            boundary_id,
            ExecutionRequest {
                project_id: ProjectId(fs_path.as_path().to_string_lossy().to_string()),
                project_generation: ProjectGeneration(project_generation),
                target: RunTarget::Function {
                    function_name: name.to_string(),
                },
                args_summary: None,
                options_summary: None,
            },
            RequestId(u64::from(request_id)),
            host_call_id,
        );
        send_started_host_run(
            &self.playground_callback,
            &self.run_store,
            &started,
            Some(u64::from(request_id)),
        );
        begin_wasm_history(
            &self.playground_callback,
            &self.run_store,
            &self.history_store,
            &started.start,
        );

        let run_store = self.run_store.clone();
        let callback = self.playground_callback.clone();
        let history_store = self.history_store.clone();
        let value_store = self.value_store.clone();
        let function_name = name.to_string();
        let logger = bex_project::TraceLogger::bounded(16);
        let ctx = bex_project::FunctionCallContextBuilder::new(call_id)
            .with_boundary_id(boundary_id)
            .with_logger(logger.clone())
            .build();
        wasm_bindgen_futures::spawn_local(async move {
            match bex_project::Bex::call_function_with_trace(
                bex,
                &function_name,
                kwargs.into(),
                ctx,
            )
            .await
            {
                Ok(traced) => {
                    drain_wasm_logs(
                        &callback,
                        &run_store,
                        &history_store,
                        &value_store,
                        boundary_id,
                        &logger,
                    );
                    let outcome = match traced.value {
                        Ok(_result) => {
                            root_value_success_outcome(None, "baml.outbound.base64")
                        }
                        Err(e) => runtime_error_outcome_with_ref(&e, None),
                    };
                    complete_wasm_run(&callback, &run_store, &history_store, boundary_id, outcome);
                }
                Err(e) => {
                    drain_wasm_logs(
                        &callback,
                        &run_store,
                        &history_store,
                        &value_store,
                        boundary_id,
                        &logger,
                    );
                    complete_wasm_run(
                        &callback,
                        &run_store,
                        &history_store,
                        boundary_id,
                        runtime_error_outcome_with_ref(&e, None),
                    );
                }
            }
        });

        Ok(())
    }

    /// Start a RunStore-owned prompt/cURL preview run.
    #[wasm_bindgen(js_name = startPreviewRun)]
    pub fn start_preview_run(
        &self,
        request_id: u32,
        project: String,
        parent_function_name: &str,
        helper: &str,
        function_name: &str,
        args_bytes: &[u8],
    ) -> Result<(), JsValue> {
        let kwargs = playground_run_args_to_bex_values(args_bytes, &HANDLE_TABLE)
            .map_err(|e| JsError::new(&format!("Failed to convert arguments: {e}")))?;
        let call_id = next_wasm_call_id()?;
        let host_call_id = HostCallId::Wasm(
            u32::try_from(call_id.0)
                .map_err(|_| JsError::new("Function call ID overflowed u32"))?,
        );
        let boundary_id = BoundaryId::new_random();
        let fs_path = bex_project::FsPath::from_str(project);
        let revision = self.state.borrow().revision();
        let prepared = self
            .playground
            .borrow()
            .prepare_run(revision)
            .ok_or_else(|| {
                JsError::new(
                    "the engine is not current with the sources; wait for the rebuild to finish",
                )
            })?;
        let project_generation = prepared.generation;
        let bex = prepared.engine;
        let started = self.run_store.create_attached_run(
            boundary_id,
            ExecutionRequest {
                project_id: ProjectId(fs_path.as_path().to_string_lossy().to_string()),
                project_generation: ProjectGeneration(project_generation),
                target: RunTarget::Preview {
                    parent_function_name: parent_function_name.to_string(),
                    helper: helper.to_string(),
                },
                args_summary: None,
                options_summary: None,
            },
            RequestId(u64::from(request_id)),
            host_call_id,
        );
        send_started_host_run(
            &self.playground_callback,
            &self.run_store,
            &started,
            Some(u64::from(request_id)),
        );
        begin_wasm_history(
            &self.playground_callback,
            &self.run_store,
            &self.history_store,
            &started.start,
        );

        let run_store = self.run_store.clone();
        let callback = self.playground_callback.clone();
        let history_store = self.history_store.clone();
        let value_store = self.value_store.clone();
        let function_name = function_name.to_string();
        let logger = bex_project::TraceLogger::bounded(16);
        let ctx = bex_project::FunctionCallContextBuilder::new(call_id)
            .with_boundary_id(boundary_id)
            .with_logger(logger.clone())
            .build();
        wasm_bindgen_futures::spawn_local(async move {
            match bex_project::Bex::call_function_with_trace(
                bex,
                &function_name,
                kwargs.into(),
                ctx,
            )
            .await
            {
                Ok(traced) => {
                    drain_wasm_logs(
                        &callback,
                        &run_store,
                        &history_store,
                        &value_store,
                        boundary_id,
                        &logger,
                    );
                    let outcome = match traced.value {
                        Ok(_result) => {
                            root_value_success_outcome(None, "baml.outbound.base64")
                        }
                        Err(e) => runtime_error_outcome_with_ref(&e, None),
                    };
                    complete_wasm_run(&callback, &run_store, &history_store, boundary_id, outcome);
                }
                Err(e) => {
                    drain_wasm_logs(
                        &callback,
                        &run_store,
                        &history_store,
                        &value_store,
                        boundary_id,
                        &logger,
                    );
                    complete_wasm_run(
                        &callback,
                        &run_store,
                        &history_store,
                        boundary_id,
                        runtime_error_outcome_with_ref(&e, None),
                    );
                }
            }
        });

        Ok(())
    }

    /// Cancel a RunStore-owned WASM run.
    #[wasm_bindgen(js_name = cancelRun)]
    pub fn cancel_run(&self, request_id: u32, boundary_id: &str) -> Result<(), JsValue> {
        let boundary_id = parse_boundary_id(boundary_id)?;
        // The generation the run pinned at launch is how its engine is found
        // again; the run store is the only record of it once the engine has
        // been replaced.
        let run_generation = self
            .run_store
            .snapshot(boundary_id)
            .map(|run| run.request.project_generation.0);
        match self.run_store.cancel_run(boundary_id, epoch_ms(), None) {
            bex_events::run::CancelRunEffect::CancelHostCall {
                host_call_id,
                patch,
            } => {
                send_run_patch(&self.playground_callback, &patch);
                match (host_call_id, run_generation) {
                    (HostCallId::Wasm(call_id), Some(run_generation)) => {
                        // Reach the engine the run launched on, not whichever
                        // is installed now: a rebuild during the run leaves the
                        // call owned by the retired engine, and only that one
                        // can cancel it.
                        let bex = self
                            .playground
                            .borrow()
                            .engine_for_generation(run_generation)
                            .ok_or_else(|| {
                                JsError::new(
                                    "the engine this run launched on is no longer installed",
                                )
                            })?;
                        bex_project::Bex::cancel_function_call(
                            bex.as_ref(),
                            sys_types::CallId(u64::from(call_id)),
                        )
                        .map_err(|e| {
                            JsError::new(&format!("Failed to cancel function call: {e}"))
                        })?;
                        send_command_ack(
                            &self.playground_callback,
                            u64::from(request_id),
                            "accepted",
                        );
                    }
                    (other, _) => {
                        send_command_error(
                            &self.playground_callback,
                            u64::from(request_id),
                            "unsupportedHostCallId",
                            format!("cancelRun resolved to unsupported host id: {other:?}"),
                        );
                    }
                }
            }
            bex_events::run::CancelRunEffect::CancelledBeforeHost { patch } => {
                send_run_patch(&self.playground_callback, &patch);
                send_command_ack(&self.playground_callback, u64::from(request_id), "accepted");
            }
            bex_events::run::CancelRunEffect::AlreadyTerminal => {
                send_command_ack(
                    &self.playground_callback,
                    u64::from(request_id),
                    "alreadyTerminal",
                );
            }
            bex_events::run::CancelRunEffect::RunMissing => {
                send_command_error(
                    &self.playground_callback,
                    u64::from(request_id),
                    "runMissing",
                    "Run not found",
                );
            }
        }
        Ok(())
    }

    #[wasm_bindgen(js_name = respondToInput)]
    pub fn respond_to_input(
        &self,
        request_id: u32,
        boundary_id: &str,
        input_request_id: &str,
    ) -> Result<String, JsValue> {
        let boundary_id = parse_boundary_id(boundary_id)?;
        let input_request_id = parse_request_id(input_request_id)?;
        let result = self.run_store.resolve_input_request_for_run(
            boundary_id,
            input_request_id,
            RunRequestState::Resolved,
        );
        if let Some(patch) = result.patch {
            send_run_patch(&self.playground_callback, &patch);
        }
        let outcome = result.outcome.as_wire_str();
        send_command_ack(&self.playground_callback, u64::from(request_id), outcome);
        Ok(outcome.to_string())
    }

    /// Record that the host answered an env request.
    ///
    /// Only the *status* is recorded here. The value itself never travels
    /// through this call: the browser resolves `baml.env` by settling the
    /// promise its `env` callback returned (see `wasm_env`), so by the time
    /// the host reports back, the program already has it. `value` is present
    /// to say whether the user supplied one or declined.
    #[wasm_bindgen(js_name = respondToEnv)]
    #[expect(
        clippy::needless_pass_by_value,
        reason = "wasm-bindgen's ABI for an optional JS string; only its \
                  presence is read, per the doc comment above"
    )]
    pub fn respond_to_env(
        &self,
        request_id: u32,
        boundary_id: &str,
        env_request_id: &str,
        value: Option<String>,
    ) -> Result<String, JsValue> {
        let boundary_id = parse_boundary_id(boundary_id)?;
        let env_request_id = parse_request_id(env_request_id)?;
        let status = if value.is_some() {
            EnvResolutionStatus::ResolvedFromUser
        } else {
            EnvResolutionStatus::DeclinedMissing
        };
        let result =
            self.run_store
                .resolve_env_request_for_run(boundary_id, env_request_id, status, None);
        if let Some(patch) = result.patch {
            send_run_patch(&self.playground_callback, &patch);
        }
        let outcome = result.outcome.as_wire_str();
        send_command_ack(&self.playground_callback, u64::from(request_id), outcome);
        Ok(outcome.to_string())
    }

    #[wasm_bindgen(js_name = listRuns)]
    pub fn list_runs(&self, request_id: u32, filter: JsValue) {
        let filter = match run_filter_from_js(filter) {
            Ok(filter) => filter,
            Err(error) => {
                send_command_error(
                    &self.playground_callback,
                    u64::from(request_id),
                    "invalidRunListFilter",
                    format!("Invalid run list filter: {error}"),
                );
                return;
            }
        };
        let runs = self
            .run_store
            .list_runs(&filter)
            .into_iter()
            .map(|summary| run_summary_to_wire(&summary))
            .collect();
        send_wasm_notification(
            &self.playground_callback,
            PlaygroundNotification::RunList {
                request_id: u64::from(request_id),
                runs,
            },
        );
    }

    #[wasm_bindgen(js_name = listHistory)]
    pub fn list_history(&self, request_id: u32, filter: JsValue) {
        let filter = match run_filter_from_js(filter) {
            Ok(filter) => filter,
            Err(error) => {
                send_command_error(
                    &self.playground_callback,
                    u64::from(request_id),
                    "invalidHistoryListFilter",
                    format!("Invalid history list filter: {error}"),
                );
                return;
            }
        };
        let runs = self
            .history_store
            .borrow()
            .list(&filter)
            .into_iter()
            .map(|summary| run_summary_to_wire(&summary))
            .collect();
        send_wasm_notification(
            &self.playground_callback,
            PlaygroundNotification::HistoryList {
                request_id: u64::from(request_id),
                runs,
            },
        );
    }

    #[wasm_bindgen(js_name = openHistory)]
    pub fn open_history(&self, request_id: u32, boundary_id: String) -> Result<(), JsValue> {
        let parsed = parse_boundary_id(&boundary_id)?;
        if let Some(snapshot) = self.run_store.snapshot(parsed) {
            send_wasm_notification(
                &self.playground_callback,
                PlaygroundNotification::RunSnapshot {
                    request_id: Some(u64::from(request_id)),
                    boundary_id,
                    snapshot: run_to_wire(&snapshot),
                },
            );
            return Ok(());
        }
        let replayed = match self.history_store.borrow().open(parsed) {
            Ok(run) => run,
            Err(err) => {
                let code = if err.kind() == io::ErrorKind::NotFound {
                    "historyMissing"
                } else {
                    "historyOpenFailed"
                };
                send_command_error(
                    &self.playground_callback,
                    u64::from(request_id),
                    code,
                    err.to_string(),
                );
                return Ok(());
            }
        };
        let snapshot = if self.run_store.insert_replayed_run(replayed.clone()) {
            replayed
        } else {
            self.run_store.snapshot(parsed).unwrap_or(replayed)
        };
        send_wasm_notification(
            &self.playground_callback,
            PlaygroundNotification::RunSnapshot {
                request_id: Some(u64::from(request_id)),
                boundary_id,
                snapshot: run_to_wire(&snapshot),
            },
        );
        Ok(())
    }

    #[wasm_bindgen(js_name = snapshot)]
    pub fn snapshot(&self, request_id: u32, boundary_id: String) -> Result<(), JsValue> {
        let parsed = parse_boundary_id(&boundary_id)?;
        let Some(snapshot) = self.run_store.snapshot(parsed) else {
            send_command_error(
                &self.playground_callback,
                u64::from(request_id),
                "runMissing",
                "Run not found",
            );
            return Ok(());
        };
        send_wasm_notification(
            &self.playground_callback,
            PlaygroundNotification::RunSnapshot {
                request_id: Some(u64::from(request_id)),
                boundary_id,
                snapshot: run_to_wire(&snapshot),
            },
        );
        Ok(())
    }

    #[wasm_bindgen(js_name = readValue)]
    pub fn read_value(
        &self,
        request_id: u32,
        boundary_id: String,
        value_ref: JsValue,
    ) -> Result<(), JsValue> {
        let parsed = parse_boundary_id(&boundary_id)?;
        let value_ref: WasmValueRef = serde_wasm_bindgen::from_value(value_ref)
            .map_err(|err| JsError::new(&format!("Invalid valueRef: {err}")))?;
        let value_ref_id = value_ref.id;
        let live_value = self.value_store.borrow_mut().get(parsed, &value_ref_id);
        let live_diagnostic = match live_value {
            LiveValueLookup::Available(stored) => {
                send_wasm_notification(
                    &self.playground_callback,
                    PlaygroundNotification::ValueBody {
                        request_id: u64::from(request_id),
                        boundary_id,
                        value_ref_id,
                        codec: stored.codec.as_wire_str().to_string(),
                        availability: "available".to_string(),
                        body_base64: Some(
                            base64::engine::general_purpose::STANDARD.encode(stored.body),
                        ),
                        diagnostic: None,
                    },
                );
                return Ok(());
            }
            LiveValueLookup::Evicted(eviction) => Some(eviction.diagnostic),
            LiveValueLookup::Missing => None,
        };

        let requested_codec = value_ref
            .codec
            .unwrap_or_else(|| ValueCodec::BamlOutboundValue.as_wire_str().to_string());
        match self
            .history_store
            .borrow()
            .read_value(parsed, &value_ref_id)
            .map_err(|err| JsError::new(&format!("Failed to read retained value: {err}")))?
        {
            HistoryValueReadResult::Available(stored) => send_wasm_notification(
                &self.playground_callback,
                PlaygroundNotification::ValueBody {
                    request_id: u64::from(request_id),
                    boundary_id,
                    value_ref_id,
                    codec: stored.codec.as_wire_str().to_string(),
                    availability: "available".to_string(),
                    body_base64: Some(
                        base64::engine::general_purpose::STANDARD.encode(stored.body),
                    ),
                    diagnostic: None,
                },
            ),
            HistoryValueReadResult::Missing => send_wasm_notification(
                &self.playground_callback,
                PlaygroundNotification::ValueBody {
                    request_id: u64::from(request_id),
                    boundary_id,
                    value_ref_id,
                    codec: requested_codec,
                    availability: "missing".to_string(),
                    body_base64: None,
                    diagnostic: Some(
                        live_diagnostic
                            .unwrap_or_else(|| "value body is not available".to_string()),
                    ),
                },
            ),
            HistoryValueReadResult::BodyUnavailable(unavailable) => send_wasm_notification(
                &self.playground_callback,
                PlaygroundNotification::ValueBody {
                    request_id: u64::from(request_id),
                    boundary_id,
                    value_ref_id,
                    codec: requested_codec,
                    availability: "missing".to_string(),
                    body_base64: None,
                    diagnostic: Some(unavailable.diagnostic),
                },
            ),
        }
        Ok(())
    }

    #[wasm_bindgen(js_name = subscribe)]
    pub fn subscribe(
        &self,
        request_id: u32,
        subscription_id: String,
        boundary_id: String,
        after_cursor: Option<u64>,
    ) -> Result<(), JsValue> {
        let parsed = parse_boundary_id(&boundary_id)?;
        match self
            .run_store
            .subscribe(parsed, after_cursor.map(RunCursor))
        {
            RunSubscription::Missing { .. } => {
                send_command_error(
                    &self.playground_callback,
                    u64::from(request_id),
                    "runMissing",
                    "Run not found",
                );
            }
            RunSubscription::CursorExpired { reason, .. } => {
                send_run_cursor_expired(
                    &self.playground_callback,
                    Some(u64::from(request_id)),
                    subscription_id,
                    boundary_id,
                    reason,
                );
            }
            RunSubscription::Snapshot { snapshot, patches } => {
                send_wasm_notification(
                    &self.playground_callback,
                    PlaygroundNotification::RunSnapshot {
                        request_id: Some(u64::from(request_id)),
                        boundary_id,
                        snapshot: run_to_wire(&snapshot),
                    },
                );
                for patch in patches {
                    send_run_patch(&self.playground_callback, &patch);
                }
            }
        }
        Ok(())
    }

    #[wasm_bindgen(js_name = unsubscribe)]
    pub fn unsubscribe(&self, request_id: u32, _subscription_id: String) {
        send_command_ack(&self.playground_callback, u64::from(request_id), "accepted");
    }

    /// Start a RunStore-owned test run.
    #[wasm_bindgen(js_name = "startTestRun")]
    pub fn start_test_run(
        &self,
        request_id: u32,
        project: &str,
        generation: u32,
        test_name: &str,
    ) -> Result<(), JsValue> {
        let call_id = next_wasm_call_id()?;
        let host_call_id = HostCallId::Wasm(
            u32::try_from(call_id.0)
                .map_err(|_| JsError::new("Function call ID overflowed u32"))?,
        );
        let boundary_id = BoundaryId::new_random();
        // The registry has to belong to the generation the host asked for and
        // to an engine still current with the sources; a test run against a
        // tree that no longer describes the code is worse than a refusal.
        let revision = self.state.borrow().revision();
        let lease = self
            .playground
            .borrow()
            .lease_registry(u64::from(generation), revision)
            .map_err(|error| JsError::new(&format!("Cannot run the test: {error}")))?;
        let started = self.run_store.create_attached_run(
            boundary_id,
            ExecutionRequest {
                project_id: ProjectId(project.to_string()),
                project_generation: ProjectGeneration(u64::from(generation)),
                target: RunTarget::Test {
                    generation: ProjectGeneration(u64::from(generation)),
                    test_name: test_name.to_string(),
                },
                args_summary: None,
                options_summary: None,
            },
            RequestId(u64::from(request_id)),
            host_call_id,
        );
        send_started_host_run(
            &self.playground_callback,
            &self.run_store,
            &started,
            Some(u64::from(request_id)),
        );
        begin_wasm_history(
            &self.playground_callback,
            &self.run_store,
            &self.history_store,
            &started.start,
        );

        let run_store = self.run_store.clone();
        let callback = self.playground_callback.clone();
        let history_store = self.history_store.clone();
        let value_store = self.value_store.clone();
        let test_name = test_name.to_string();
        let logger = bex_project::TraceLogger::bounded(16);
        let ctx = bex_project::FunctionCallContextBuilder::new(call_id)
            .with_boundary_id(boundary_id)
            .with_logger(logger.clone())
            .build();
        wasm_bindgen_futures::spawn_local(async move {
            match run_collected_test(&lease, &test_name, ctx).await {
                Ok(traced) => {
                    drain_wasm_logs(
                        &callback,
                        &run_store,
                        &history_store,
                        &value_store,
                        boundary_id,
                        &logger,
                    );
                    let outcome = match traced.value {
                        Ok(_result) => root_value_success_outcome(None, "testReport"),
                        Err(e) => runtime_error_outcome_with_ref(&e, None),
                    };
                    complete_wasm_run(&callback, &run_store, &history_store, boundary_id, outcome);
                }
                Err(e) => {
                    drain_wasm_logs(
                        &callback,
                        &run_store,
                        &history_store,
                        &value_store,
                        boundary_id,
                        &logger,
                    );
                    complete_wasm_run(
                        &callback,
                        &run_store,
                        &history_store,
                        boundary_id,
                        runtime_error_outcome_with_ref(&e, None),
                    );
                }
            }
        });

        Ok(())
    }
}

/// Run one collected test against its leased registry.
async fn run_collected_test(
    lease: &crate::playground::RegistryLease,
    test_name: &str,
    context: bex_project::FunctionCallContext,
) -> Result<bex_project::BexCallResult, bex_project::EngineError> {
    lease
        .engine
        .call_function_with_trace(
            "testing.TestRegistry.run_test",
            vec![
                bex_project::BexExternalValue::Handle(lease.handle.clone()),
                bex_project::BexExternalValue::String(test_name.into()),
            ],
            context,
            true, // deep copy the TestReport for the wire
        )
        .await
}

#[cfg(test)]
mod history_tests {
    use bex_events::{
        ids::{BexCallId, BexThreadId, EngineId, ProcessEuid},
        run::{
            ExecutionRequest, PayloadKind, ProjectGeneration, RunPatchChange, RunRequestSummary,
            RunStatus, RunTimeAnchor, StartGuard, TraceCallKey,
        },
    };
    use wasm_bindgen_test::wasm_bindgen_test;

    use super::*;

    fn start_context(boundary_id: BoundaryId) -> StartRunContext {
        StartRunContext {
            boundary_id,
            request_id: RequestId(1),
            request: RunRequestSummary {
                project_id: ProjectId("wasm-project".to_string()),
                project_generation: ProjectGeneration(1),
                target: RunTarget::Function {
                    function_name: "user.Extract".to_string(),
                },
                args_summary: None,
                options_summary: None,
            },
            created_at_ms: 10,
            time_anchor: RunTimeAnchor {
                epoch_created_at_ms: 10,
                trace_zero_ns: 0,
            },
            start_guard: StartGuard::new(),
        }
    }

    fn root_trace() -> TraceCallKey {
        TraceCallKey {
            process_euid: ProcessEuid([4; 16]),
            engine_id: EngineId(9),
            thread_id: BexThreadId(1),
            call_id: BexCallId(2),
        }
    }

    #[wasm_bindgen_test]
    fn wasm_queue_full_stats_create_log_capture_loss_patch() {
        let boundary_id = BoundaryId::from_bytes([22; 16]);
        let logger = bex_project::TraceLogger::bounded(0);
        logger.capture_with(boundary_id, root_trace(), |_| {
            panic!("zero-capacity logger must not copy a value")
        });
        let stats = logger.stats();
        assert_eq!(stats.skipped_log_queue_full, 1);

        let run_store = InMemoryRunStore::default();
        run_store.create_run_at(
            boundary_id,
            ExecutionRequest {
                project_id: ProjectId("wasm-project".to_string()),
                project_generation: ProjectGeneration(1),
                target: RunTarget::Function {
                    function_name: "user.Extract".to_string(),
                },
                args_summary: None,
                options_summary: None,
            },
            RequestId(1),
            RunTimeAnchor {
                epoch_created_at_ms: 20,
                trace_zero_ns: 0,
            },
        );

        let patch =
            log_loss_diagnostic_patch(&run_store, boundary_id, "log", stats.skipped_log_queue_full)
                .expect("live capture-loss diagnostic should produce a patch");
        assert!(
            patch.changes.iter().any(|change| matches!(
                change,
                RunPatchChange::UpsertDiagnostic(diagnostic)
                    if diagnostic.code.as_deref() == Some("logCaptureLoss")
                        && diagnostic.message == "Skipped 1 captured log value(s) because the log capture queue was full"
            )),
            "expected logCaptureLoss diagnostic patch, got {patch:#?}"
        );
        assert!(
            run_store
                .snapshot(boundary_id)
                .expect("run should exist")
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.code.as_deref() == Some("logCaptureLoss"))
        );
    }

    /// A terminal run reads back out of the in-memory history alone: the
    /// browser has no disk, so the value segments the writer produced are
    /// the only copy, and `open`/`read_value` must reconstruct the log
    /// payload and its body from them without a live `InMemoryRunStore`.
    #[wasm_bindgen_test]
    fn warm_history_replays_log_payload_and_body_without_live_runstore() {
        let boundary_id = BoundaryId::from_bytes([5; 16]);
        let start = start_context(boundary_id);

        let mut store = WasmHistoryStoreInner::default();
        store.begin(&start).unwrap();
        let outcome = store
            .append_log_body(
                boundary_id,
                LogEventRecord {
                    call: root_trace(),
                    level: Some("warn".to_string()),
                    source: None,
                    timestamp_ms: 12,
                    message_preview: Some("warm log".to_string()),
                },
                ValueCodec::BamlOutboundValue,
                vec![7, 8, 9],
            )
            .unwrap();
        store
            .complete(
                boundary_id,
                &RunOutcome::Succeeded(RunResult {
                    value_ref: None,
                    renderer_hint: None,
                    supporting_payload_ids: Vec::new(),
                }),
                20,
            )
            .unwrap();

        let summaries = store.list(&RunFilter::default());
        assert_eq!(summaries.len(), 1);
        assert_eq!(summaries[0].boundary_id, boundary_id);

        let replayed = store.open(boundary_id).unwrap();
        assert_eq!(replayed.status, RunStatus::Succeeded);
        let log = replayed
            .payloads
            .iter()
            .find_map(|payload| match &payload.kind {
                PayloadKind::Log(log) => Some(log),
                _ => None,
            })
            .expect("log payload should replay");
        assert_eq!(log.level.as_deref(), Some("warn"));
        assert_eq!(log.message, "warm log");
        assert_eq!(
            log.value_ref.as_ref().expect("log value ref").id,
            outcome.value_ref.id
        );
        let HistoryValueReadResult::Available(body) = store
            .read_value(boundary_id, &outcome.value_ref.id)
            .unwrap()
        else {
            panic!("expected replayed log body");
        };
        assert_eq!(body.body, vec![7, 8, 9]);
    }
}
