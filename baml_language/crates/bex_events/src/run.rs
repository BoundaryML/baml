//! Host run lifecycle, payload, request, and wire-domain state.

#[cfg(not(target_arch = "wasm32"))]
use std::time::{SystemTime, UNIX_EPOCH};
use std::{
    collections::{HashMap, HashSet},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
};

#[cfg(target_arch = "wasm32")]
use web_time::{SystemTime, UNIX_EPOCH};

pub use crate::{
    ids::BoundaryId,
    run_wire::{patch_to_wire, run_summary_to_wire, run_to_wire},
};
use crate::{
    ids::{BexCallId, BexThreadId, CallRef, EngineId, ProcessEuid},
    value::ValueRef,
};

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum HostCallId {
    Native(sys_types::CallId),
    Wasm(u32),
    Adapter { adapter: String, id: u64 },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct RequestId(pub u64);

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct RunCursor(pub u64);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum RunCursorExpiredReason {
    Expired,
    Compacted,
    Unknown,
    Future,
    Unavailable,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct ProjectId(pub String);

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ProjectGeneration(pub u64);

pub type FunctionName = String;
pub type RunScopeId = String;

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum RunTarget {
    Function {
        function_name: FunctionName,
    },
    Test {
        generation: ProjectGeneration,
        test_name: String,
    },
    Preview {
        parent_function_name: FunctionName,
        helper: String,
    },
    Companion {
        parent_boundary_id: Option<BoundaryId>,
        function_name: FunctionName,
    },
    Internal {
        name: String,
    },
}

impl RunTarget {
    #[must_use]
    pub fn kind(&self) -> RunKind {
        match self {
            Self::Function { .. } => RunKind::Function,
            Self::Test { .. } => RunKind::Test,
            Self::Preview { .. } => RunKind::Preview,
            Self::Companion { .. } => RunKind::Companion,
            Self::Internal { .. } => RunKind::Internal,
        }
    }

    #[must_use]
    pub fn default_visibility(&self, scope_id: Option<RunScopeId>) -> RunVisibility {
        match self {
            Self::Function { .. } | Self::Test { .. } => RunVisibility::History,
            Self::Preview { .. } | Self::Companion { .. } => scope_id
                .map(|scope_id| RunVisibility::Scoped { scope_id })
                .unwrap_or(RunVisibility::Hidden),
            Self::Internal { .. } => RunVisibility::Hidden,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum RunKind {
    Function,
    Test,
    Preview,
    Companion,
    Internal,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum RunVisibility {
    History,
    Scoped { scope_id: RunScopeId },
    Hidden,
    DebugOnly,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum RunStatus {
    Pending,
    Running,
    WaitingForInput,
    WaitingForEnv,
    Cancelling,
    Succeeded,
    Failed,
    Cancelled,
    Panicked,
}

impl RunStatus {
    #[must_use]
    pub fn is_terminal(self) -> bool {
        matches!(
            self,
            Self::Succeeded | Self::Failed | Self::Cancelled | Self::Panicked
        )
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExecutionRequest {
    pub project_id: ProjectId,
    pub project_generation: ProjectGeneration,
    pub target: RunTarget,
    pub args_summary: Option<String>,
    pub options_summary: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RunRequestSummary {
    pub project_id: ProjectId,
    pub project_generation: ProjectGeneration,
    pub target: RunTarget,
    pub args_summary: Option<String>,
    pub options_summary: Option<String>,
}

impl From<ExecutionRequest> for RunRequestSummary {
    fn from(request: ExecutionRequest) -> Self {
        Self {
            project_id: request.project_id,
            project_generation: request.project_generation,
            target: request.target,
            args_summary: request.args_summary,
            options_summary: request.options_summary,
        }
    }
}

#[derive(Clone, Debug)]
pub struct StartRunContext {
    pub boundary_id: BoundaryId,
    pub request_id: RequestId,
    pub request: RunRequestSummary,
    pub created_at_ms: u64,
    pub time_anchor: RunTimeAnchor,
    pub start_guard: StartGuard,
}

#[derive(Clone, Debug)]
pub struct StartedHostRun {
    pub start: StartRunContext,
    pub started_patch: Option<RunPatch>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RunTimeAnchor {
    pub epoch_created_at_ms: u64,
    pub trace_zero_ns: u64,
}

#[derive(Clone, Debug, Default)]
pub struct StartGuard {
    cancelled: Arc<AtomicBool>,
}

impl StartGuard {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::Release);
    }

    #[must_use]
    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Acquire)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RunOutcome {
    Succeeded(RunResult),
    Failed(RunError),
    Cancelled(CancellationState),
    Panicked(RunError),
}

impl RunOutcome {
    #[must_use]
    pub fn status(&self) -> RunStatus {
        match self {
            Self::Succeeded(_) => RunStatus::Succeeded,
            Self::Failed(_) => RunStatus::Failed,
            Self::Cancelled(_) => RunStatus::Cancelled,
            Self::Panicked(_) => RunStatus::Panicked,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RunResult {
    pub value_ref: Option<ValueRef>,
    pub renderer_hint: Option<String>,
    pub supporting_payload_ids: Vec<PayloadId>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RunError {
    pub class: RunErrorClass,
    pub message: String,
    pub details: Option<String>,
    pub value_ref: Option<ValueRef>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RunErrorClass {
    Validation,
    Runtime,
    Host,
    Panic,
    Cancelled,
    Internal,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CancellationState {
    pub requested_at_ms: u64,
    pub completed_at_ms: Option<u64>,
    pub reason: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PayloadId(pub u64);
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PayloadEvent {
    pub id: PayloadId,
    pub timestamp_ms: u64,
    pub kind: PayloadKind,
    pub redaction: RedactionMetadata,
    pub body: Option<PayloadBody>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PayloadKind {
    FetchStarted(FetchStarted),
    FetchUpdated(FetchUpdated),
    InputRequested(InputRequested),
    InputResolved(InputResolved),
    EnvRequested(EnvRequested),
    EnvResolved(EnvResolved),
    Log(LogPayload),
    Output(OutputPayload),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FetchStarted {
    pub fetch_id: u64,
    pub method: String,
    pub url: String,
    pub request_headers: Vec<HeaderObservation>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FetchUpdated {
    pub fetch_id: u64,
    pub status: Option<i64>,
    pub duration_ms: Option<u64>,
    pub response_headers: Vec<HeaderObservation>,
    pub error: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HeaderObservation {
    pub name: String,
    pub value_redacted: bool,
    /// Unredacted value, present only for display-safe headers (see
    /// [`HeaderObservation::observe`]). `None` whenever the value is redacted.
    pub value: Option<String>,
}

/// Header whose value is safe to surface in run logs: it carries request
/// routing info (the original upstream URL when requests go through the
/// playground proxy), not secrets.
pub const DISPLAY_SAFE_HEADER_ORIGINAL_URL: &str = "baml-original-url";

impl HeaderObservation {
    /// Observe a header, redacting its value unless the header is known to be
    /// display-safe.
    pub fn observe(name: &str, value: &str) -> Self {
        if name.eq_ignore_ascii_case(DISPLAY_SAFE_HEADER_ORIGINAL_URL) {
            HeaderObservation {
                name: name.to_string(),
                value_redacted: false,
                value: Some(value.to_string()),
            }
        } else {
            HeaderObservation {
                name: name.to_string(),
                value_redacted: true,
                value: None,
            }
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InputRequested {
    pub request_id: u64,
    pub prompt: Option<String>,
    pub state: RunRequestState,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InputResolved {
    pub request_id: u64,
    pub state: RunRequestState,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EnvRequested {
    pub request_id: u64,
    pub key: String,
    pub state: RunRequestState,
    pub waiter_count: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EnvResolved {
    pub request_id: u64,
    pub key: String,
    pub status: EnvResolutionStatus,
    pub state: RunRequestState,
    pub value_redacted: bool,
    pub display_value: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LogPayload {
    pub level: Option<String>,
    pub message: String,
    pub source: Option<SourceLocation>,
    pub value_ref: Option<ValueRef>,
}

/// A chunk written by `baml.io.print` / `println` / `eprint` / `eprintln`.
///
/// These are raw stream writes, not structured log records: `print` carries no
/// trailing newline, so consumers must concatenate consecutive chunks on the
/// same stream rather than treating each payload as one line.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OutputPayload {
    pub stream: OutputStream,
    pub text: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OutputStream {
    Stdout,
    Stderr,
}

impl OutputStream {
    #[must_use]
    pub fn as_wire_str(self) -> &'static str {
        match self {
            Self::Stdout => "stdout",
            Self::Stderr => "stderr",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RunRequestState {
    Pending,
    Resolved,
    Cancelled,
    Expired,
    RunTerminal,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RequestCommandOutcome {
    Accepted,
    AlreadyResolved,
    RejectedStale,
    Cancelled,
    Missing,
    AlreadyTerminal,
}

impl RequestCommandOutcome {
    #[must_use]
    pub fn as_wire_str(self) -> &'static str {
        match self {
            Self::Accepted => "accepted",
            Self::AlreadyResolved => "alreadyResolved",
            Self::RejectedStale => "rejectedStale",
            Self::Cancelled => "cancelled",
            Self::Missing => "missing",
            Self::AlreadyTerminal => "alreadyTerminal",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EnvResolutionStatus {
    ResolvedFromOverride,
    ResolvedFromProcess,
    ResolvedFromUser,
    DeclinedMissing,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RedactionMetadata {
    pub value_redacted: bool,
    pub display_safe: bool,
    pub reason: Option<String>,
    pub policy_id: Option<String>,
}

impl RedactionMetadata {
    #[must_use]
    pub fn display_safe() -> Self {
        Self {
            value_redacted: false,
            display_safe: true,
            reason: None,
            policy_id: None,
        }
    }

    #[must_use]
    pub fn omitted_by_policy(reason: impl Into<String>) -> Self {
        Self {
            value_redacted: true,
            display_safe: false,
            reason: Some(reason.into()),
            policy_id: Some("playground.default-redaction".to_string()),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PayloadBody {
    pub state: PayloadBodyState,
    pub content_type: Option<String>,
    pub original_size_bytes: Option<usize>,
    pub retained_size_bytes: Option<usize>,
}

impl PayloadBody {
    #[must_use]
    pub fn omitted_by_policy(original_size_bytes: Option<usize>) -> Self {
        Self {
            state: PayloadBodyState::OmittedByPolicy,
            content_type: None,
            original_size_bytes,
            retained_size_bytes: Some(0),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PayloadBodyState {
    InlineBytes,
    InlineJson,
    RetainedByRef(PayloadBodyRef),
    Truncated,
    Compacted,
    OmittedByPolicy,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PayloadBodyRef {
    pub id: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RunPatch {
    pub boundary_id: BoundaryId,
    pub cursor: RunCursor,
    pub changes: Vec<RunPatchChange>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
// Keeping payloads inline avoids an allocation for every incremental run patch.
#[allow(clippy::large_enum_variant)]
pub enum RunPatchChange {
    UpsertPayload(PayloadEvent),
    UpsertDiagnostic(RunDiagnostic),
    SetStatus(RunStatus),
    Complete(RunOutcome),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RunDiagnostic {
    pub severity: DiagnosticSeverity,
    pub code: Option<String>,
    pub message: String,
    pub payload_id: Option<PayloadId>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Run {
    pub boundary_id: BoundaryId,
    pub target: RunTarget,
    pub visibility: RunVisibility,
    pub status: RunStatus,
    pub created_at_ms: u64,
    pub started_at_ms: Option<u64>,
    pub completed_at_ms: Option<u64>,
    pub time_anchor: RunTimeAnchor,
    pub request: RunRequestSummary,
    pub result: Option<RunResult>,
    pub error: Option<RunError>,
    pub cancellation: Option<CancellationState>,
    pub payloads: Vec<PayloadEvent>,
    pub diagnostics: Vec<RunDiagnostic>,
    pub cursor: RunCursor,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RunSummary {
    pub boundary_id: BoundaryId,
    pub target: RunTarget,
    pub visibility: RunVisibility,
    pub status: RunStatus,
    pub request: RunRequestSummary,
    pub touched_functions: Vec<FunctionName>,
    pub created_at_ms: u64,
    pub completed_at_ms: Option<u64>,
    pub retention: RunRetentionState,
}

#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct RunFilter {
    pub project_id: Option<ProjectId>,
    pub project_generation: Option<ProjectGeneration>,
    pub kinds: Vec<RunKind>,
    pub statuses: Vec<RunStatus>,
    pub call_tree_contains_function: Option<FunctionName>,
    pub visibility: RunVisibilityFilter,
}

#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub enum RunVisibilityFilter {
    #[default]
    HistoryOnly,
    Scope {
        scope_id: RunScopeId,
    },
    IncludeHidden,
    AllForDebug,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RunRetentionState {
    Full,
    SnapshotCompacted,
    PayloadBodiesCompacted,
    Evicted,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RunRetentionPolicy {
    pub max_active_runs: Option<usize>,
    pub max_terminal_runs: Option<usize>,
    pub terminal_ttl_ms: Option<u64>,
    pub max_retained_run_bytes: Option<usize>,
    pub patch_window_capacity: usize,
}

impl Default for RunRetentionPolicy {
    fn default() -> Self {
        Self {
            max_active_runs: None,
            max_terminal_runs: None,
            terminal_ttl_ms: None,
            max_retained_run_bytes: None,
            patch_window_capacity: 256,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
#[allow(clippy::large_enum_variant)]
pub enum RunSubscription {
    Snapshot {
        snapshot: Run,
        patches: Vec<RunPatch>,
    },
    CursorExpired {
        boundary_id: BoundaryId,
        reason: RunCursorExpiredReason,
    },
    Missing {
        boundary_id: BoundaryId,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CancelRunEffect {
    CancelledBeforeHost {
        patch: RunPatch,
    },
    CancelHostCall {
        host_call_id: HostCallId,
        patch: RunPatch,
    },
    AlreadyTerminal,
    RunMissing,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RequestCommandResult {
    pub outcome: RequestCommandOutcome,
    pub host_call_id: Option<HostCallId>,
    pub patch: Option<RunPatch>,
}

impl RequestCommandResult {
    fn outcome(outcome: RequestCommandOutcome) -> Self {
        Self {
            outcome,
            host_call_id: None,
            patch: None,
        }
    }

    fn accepted(host_call_id: HostCallId, patch: RunPatch) -> Self {
        Self {
            outcome: RequestCommandOutcome::Accepted,
            host_call_id: Some(host_call_id),
            patch: Some(patch),
        }
    }
}

#[derive(Clone)]
pub struct InMemoryRunStore {
    inner: Arc<Mutex<RunStoreInner>>,
}

impl std::fmt::Debug for InMemoryRunStore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("InMemoryRunStore").finish_non_exhaustive()
    }
}

#[derive(Debug)]
struct RunStoreInner {
    runs: HashMap<BoundaryId, RunRecord>,
    host_call_index: HashMap<HostCallId, BoundaryId>,
    retention: RunRetentionPolicy,
    next_payload_id: u64,
}

#[derive(Clone, Debug)]
struct RunRecord {
    run: Run,
    host_call_id: Option<HostCallId>,
    start_guard: StartGuard,
    patches: Vec<RunPatch>,
    domain_diagnostics: Vec<RunDiagnostic>,
    pending_input_requests: HashSet<u64>,
    pending_env_requests: HashSet<u64>,
    output_bytes: usize,
    output_truncated: bool,
}

/// Per-run byte budget for `baml.io` stream output. A tight print loop must not
/// be able to grow the run store without bound; past the budget we emit one
/// truncation notice and drop the rest.
const MAX_RUN_OUTPUT_BYTES: usize = 1 << 20;

impl Default for InMemoryRunStore {
    fn default() -> Self {
        Self::new(RunRetentionPolicy::default())
    }
}

impl InMemoryRunStore {
    #[must_use]
    pub fn new(retention: RunRetentionPolicy) -> Self {
        Self {
            inner: Arc::new(Mutex::new(RunStoreInner {
                runs: HashMap::new(),
                host_call_index: HashMap::new(),
                retention,
                next_payload_id: 1,
            })),
        }
    }

    pub fn create_run(
        &self,
        boundary_id: BoundaryId,
        request: ExecutionRequest,
        request_id: RequestId,
    ) -> StartRunContext {
        let now_ms = epoch_ms();
        self.create_run_at(
            boundary_id,
            request,
            request_id,
            RunTimeAnchor {
                epoch_created_at_ms: now_ms,
                trace_zero_ns: 0,
            },
        )
    }

    pub fn create_run_at(
        &self,
        boundary_id: BoundaryId,
        request: ExecutionRequest,
        request_id: RequestId,
        time_anchor: RunTimeAnchor,
    ) -> StartRunContext {
        let request_summary = RunRequestSummary::from(request);
        let visibility = request_summary.target.default_visibility(None);
        let start_guard = StartGuard::new();
        let run = Run {
            boundary_id,
            target: request_summary.target.clone(),
            visibility,
            status: RunStatus::Pending,
            created_at_ms: time_anchor.epoch_created_at_ms,
            started_at_ms: None,
            completed_at_ms: None,
            time_anchor,
            request: request_summary.clone(),
            result: None,
            error: None,
            cancellation: None,
            payloads: Vec::new(),
            diagnostics: Vec::new(),
            cursor: RunCursor(0),
        };
        let record = RunRecord {
            run,
            host_call_id: None,
            start_guard: start_guard.clone(),
            patches: Vec::new(),
            domain_diagnostics: Vec::new(),
            pending_input_requests: HashSet::new(),
            pending_env_requests: HashSet::new(),
            output_bytes: 0,
            output_truncated: false,
        };
        self.inner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .runs
            .insert(boundary_id, record);

        StartRunContext {
            boundary_id,
            request_id,
            request: request_summary,
            created_at_ms: time_anchor.epoch_created_at_ms,
            time_anchor,
            start_guard,
        }
    }

    pub fn create_attached_run(
        &self,
        boundary_id: BoundaryId,
        request: ExecutionRequest,
        request_id: RequestId,
        host_call_id: HostCallId,
    ) -> StartedHostRun {
        let start = self.create_run(boundary_id, request, request_id);
        let started_patch = self.attach_host_call(start.boundary_id, host_call_id);
        StartedHostRun {
            start,
            started_patch,
        }
    }

    #[must_use]
    pub fn snapshot(&self, boundary_id: BoundaryId) -> Option<Run> {
        self.inner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .runs
            .get(&boundary_id)
            .map(|record| record.run.clone())
    }

    pub fn insert_replayed_run(&self, run: Run) -> bool {
        let mut inner = self
            .inner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if inner.runs.contains_key(&run.boundary_id) {
            return false;
        }
        let domain_diagnostics = run.diagnostics.clone();
        inner.runs.insert(
            run.boundary_id,
            RunRecord {
                run,
                host_call_id: None,
                start_guard: StartGuard::new(),
                patches: Vec::new(),
                domain_diagnostics,
                pending_input_requests: HashSet::new(),
                pending_env_requests: HashSet::new(),
                output_bytes: 0,
                output_truncated: false,
            },
        );
        true
    }

    #[must_use]
    pub fn list_runs(&self, filter: &RunFilter) -> Vec<RunSummary> {
        let mut runs: Vec<_> = self
            .inner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .runs
            .values()
            .filter(|record| run_matches_filter(&record.run, filter))
            .map(|record| summarize_run(&record.run))
            .collect();
        runs.sort_by_key(|summary| std::cmp::Reverse(summary.created_at_ms));
        runs
    }

    #[must_use]
    pub fn subscribe(&self, boundary_id: BoundaryId, cursor: Option<RunCursor>) -> RunSubscription {
        let inner = self
            .inner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let Some(record) = inner.runs.get(&boundary_id) else {
            return RunSubscription::Missing { boundary_id };
        };
        let Some(cursor) = cursor else {
            return RunSubscription::Snapshot {
                snapshot: record.run.clone(),
                patches: Vec::new(),
            };
        };
        if cursor > record.run.cursor {
            return RunSubscription::CursorExpired {
                boundary_id,
                reason: RunCursorExpiredReason::Future,
            };
        }
        if cursor == record.run.cursor {
            return RunSubscription::Snapshot {
                snapshot: record.run.clone(),
                patches: Vec::new(),
            };
        }
        let Some(oldest) = record.patches.first().map(|patch| patch.cursor) else {
            return RunSubscription::CursorExpired {
                boundary_id,
                reason: RunCursorExpiredReason::Expired,
            };
        };
        if cursor.0.saturating_add(1) < oldest.0 {
            return RunSubscription::CursorExpired {
                boundary_id,
                reason: RunCursorExpiredReason::Compacted,
            };
        }
        let patches = record
            .patches
            .iter()
            .filter(|patch| patch.cursor > cursor)
            .cloned()
            .collect();
        RunSubscription::Snapshot {
            snapshot: record.run.clone(),
            patches,
        }
    }

    pub fn attach_host_call(
        &self,
        boundary_id: BoundaryId,
        host_call_id: HostCallId,
    ) -> Option<RunPatch> {
        let mut inner = self
            .inner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let retention = inner.retention.clone();
        let patch = {
            let record = inner.runs.get_mut(&boundary_id)?;
            record.host_call_id = Some(host_call_id.clone());
            record.run.started_at_ms = record.run.started_at_ms.or_else(|| Some(epoch_ms()));
            matches!(record.run.status, RunStatus::Pending).then(|| {
                push_patch(
                    record,
                    &retention,
                    vec![RunPatchChange::SetStatus(RunStatus::Running)],
                )
            })
        };
        inner.host_call_index.insert(host_call_id, boundary_id);
        patch
    }

    #[must_use]
    pub fn boundary_id_for_host_call(&self, host_call_id: &HostCallId) -> Option<BoundaryId> {
        self.inner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .host_call_index
            .get(host_call_id)
            .copied()
    }

    pub fn complete_run(
        &self,
        boundary_id: BoundaryId,
        outcome: RunOutcome,
        completed_at_ms: u64,
    ) -> Option<RunPatch> {
        let mut inner = self
            .inner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let retention = inner.retention.clone();
        let record = inner.runs.get_mut(&boundary_id)?;
        if record.run.status.is_terminal() {
            return None;
        }
        record.pending_input_requests.clear();
        record.pending_env_requests.clear();
        record.run.completed_at_ms = Some(completed_at_ms);
        match &outcome {
            RunOutcome::Succeeded(result) => {
                record.run.result = Some(result.clone());
                record.run.error = None;
                record.run.cancellation = None;
            }
            RunOutcome::Failed(error) | RunOutcome::Panicked(error) => {
                record.run.result = None;
                record.run.error = Some(error.clone());
            }
            RunOutcome::Cancelled(cancellation) => {
                record.run.result = None;
                record.run.error = None;
                record.run.cancellation = Some(cancellation.clone());
            }
        }
        let status = outcome.status();
        let patch = push_patch(
            record,
            &retention,
            vec![
                RunPatchChange::SetStatus(status),
                RunPatchChange::Complete(outcome),
            ],
        );
        enforce_terminal_retention(&mut inner, completed_at_ms);
        Some(patch)
    }

    pub fn add_diagnostic(
        &self,
        boundary_id: BoundaryId,
        diagnostic: RunDiagnostic,
    ) -> Option<RunPatch> {
        let mut inner = self
            .inner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let retention = inner.retention.clone();
        let record = inner.runs.get_mut(&boundary_id)?;
        record.domain_diagnostics.push(diagnostic.clone());
        record.run.diagnostics.push(diagnostic.clone());
        Some(push_patch(
            record,
            &retention,
            vec![RunPatchChange::UpsertDiagnostic(diagnostic)],
        ))
    }

    pub fn ingest_log_value_ref(
        &self,
        boundary_id: BoundaryId,
        _call: TraceCallKey,
        level: Option<String>,
        message: String,
        source: Option<SourceLocation>,
        value_ref: Option<ValueRef>,
    ) -> Option<RunPatch> {
        let mut inner = self
            .inner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let payload_id = inner.allocate_payload_id();
        let retention = inner.retention.clone();
        let record = inner.runs.get_mut(&boundary_id)?;
        let payload = PayloadEvent {
            id: payload_id,
            timestamp_ms: epoch_ms(),
            kind: PayloadKind::Log(LogPayload {
                level,
                message,
                source,
                value_ref,
            }),
            redaction: RedactionMetadata::display_safe(),
            body: None,
        };
        Some(push_payload_patch(record, &retention, payload, None))
    }

    /// Record a `baml.io` stream write against the run owning `host_call_id`.
    ///
    /// Returns `None` when the write cannot be attributed to a live run (no
    /// attached host call, or the run is already evicted). Callers should treat
    /// that as "nothing to broadcast" rather than as a failure: panicking a
    /// program over an unroutable debug print costs more than the lost line.
    pub fn ingest_output(
        &self,
        host_call_id: &HostCallId,
        stream: OutputStream,
        text: String,
    ) -> Option<RunPatch> {
        let mut inner = self
            .inner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let boundary_id = *inner.host_call_index.get(host_call_id)?;
        let payload_id = inner.allocate_payload_id();
        let retention = inner.retention.clone();
        let record = inner.runs.get_mut(&boundary_id)?;

        if record.output_truncated {
            return None;
        }
        let text = if record.output_bytes.saturating_add(text.len()) > MAX_RUN_OUTPUT_BYTES {
            record.output_truncated = true;
            format!("\n[output truncated: run exceeded {MAX_RUN_OUTPUT_BYTES} bytes]\n")
        } else {
            record.output_bytes = record.output_bytes.saturating_add(text.len());
            text
        };

        let payload = PayloadEvent {
            id: payload_id,
            timestamp_ms: epoch_ms(),
            kind: PayloadKind::Output(OutputPayload { stream, text }),
            redaction: RedactionMetadata::display_safe(),
            body: None,
        };
        Some(push_payload_patch(record, &retention, payload, None))
    }

    pub fn ingest_fetch_started(
        &self,
        host_call_id: &HostCallId,
        fetch_id: u64,
        method: String,
        url: String,
        request_headers: Vec<HeaderObservation>,
        request_body_size: Option<usize>,
    ) -> Option<RunPatch> {
        let mut inner = self
            .inner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let boundary_id = *inner.host_call_index.get(host_call_id)?;
        let payload_id = inner.allocate_payload_id();
        let retention = inner.retention.clone();
        let record = inner.runs.get_mut(&boundary_id)?;
        let payload = PayloadEvent {
            id: payload_id,
            timestamp_ms: epoch_ms(),
            kind: PayloadKind::FetchStarted(FetchStarted {
                fetch_id,
                method,
                url,
                request_headers,
            }),
            redaction: RedactionMetadata::omitted_by_policy(
                "fetch headers and bodies are redacted in RunStore by default",
            ),
            body: request_body_size.map(|size| PayloadBody::omitted_by_policy(Some(size))),
        };
        Some(push_payload_patch(record, &retention, payload, None))
    }

    #[allow(clippy::too_many_arguments)]
    pub fn ingest_fetch_updated(
        &self,
        host_call_id: &HostCallId,
        fetch_id: u64,
        status: Option<i64>,
        duration_ms: Option<u64>,
        response_headers: Vec<HeaderObservation>,
        response_body_size: Option<usize>,
        error: Option<String>,
    ) -> Option<RunPatch> {
        let mut inner = self
            .inner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let boundary_id = *inner.host_call_index.get(host_call_id)?;
        let payload_id = inner.allocate_payload_id();
        let retention = inner.retention.clone();
        let record = inner.runs.get_mut(&boundary_id)?;
        let payload = PayloadEvent {
            id: payload_id,
            timestamp_ms: epoch_ms(),
            kind: PayloadKind::FetchUpdated(FetchUpdated {
                fetch_id,
                status,
                duration_ms,
                response_headers,
                error,
            }),
            redaction: RedactionMetadata::omitted_by_policy(
                "fetch headers and bodies are redacted in RunStore by default",
            ),
            body: response_body_size.map(|size| PayloadBody::omitted_by_policy(Some(size))),
        };
        Some(push_payload_patch(record, &retention, payload, None))
    }

    pub fn ingest_input_requested(
        &self,
        host_call_id: &HostCallId,
        request_id: u64,
        prompt: Option<String>,
    ) -> Option<RunPatch> {
        let mut inner = self
            .inner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let boundary_id = *inner.host_call_index.get(host_call_id)?;
        let payload_id = inner.allocate_payload_id();
        let retention = inner.retention.clone();
        let record = inner.runs.get_mut(&boundary_id)?;
        record.pending_input_requests.insert(request_id);
        let payload = PayloadEvent {
            id: payload_id,
            timestamp_ms: epoch_ms(),
            kind: PayloadKind::InputRequested(InputRequested {
                request_id,
                prompt,
                state: RunRequestState::Pending,
            }),
            redaction: RedactionMetadata::display_safe(),
            body: None,
        };
        let status = next_active_request_status(record);
        Some(push_payload_patch(record, &retention, payload, status))
    }

    pub fn ingest_input_resolved(
        &self,
        host_call_id: &HostCallId,
        request_id: u64,
        state: RunRequestState,
    ) -> Option<RunPatch> {
        let mut inner = self
            .inner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let boundary_id = *inner.host_call_index.get(host_call_id)?;
        let payload_id = inner.allocate_payload_id();
        let retention = inner.retention.clone();
        let record = inner.runs.get_mut(&boundary_id)?;
        record.pending_input_requests.remove(&request_id);
        let state = request_state_for_record(record, state);
        let payload = PayloadEvent {
            id: payload_id,
            timestamp_ms: epoch_ms(),
            kind: PayloadKind::InputResolved(InputResolved { request_id, state }),
            redaction: RedactionMetadata::display_safe(),
            body: None,
        };
        let status = next_active_request_status(record);
        Some(push_payload_patch(record, &retention, payload, status))
    }

    pub fn ingest_env_requested(
        &self,
        host_call_id: &HostCallId,
        request_id: u64,
        key: String,
    ) -> Option<RunPatch> {
        let mut inner = self
            .inner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let boundary_id = *inner.host_call_index.get(host_call_id)?;
        let payload_id = inner.allocate_payload_id();
        let retention = inner.retention.clone();
        let record = inner.runs.get_mut(&boundary_id)?;
        record.pending_env_requests.insert(request_id);
        let payload = PayloadEvent {
            id: payload_id,
            timestamp_ms: epoch_ms(),
            kind: PayloadKind::EnvRequested(EnvRequested {
                request_id,
                key,
                state: RunRequestState::Pending,
                waiter_count: 1,
            }),
            redaction: RedactionMetadata::display_safe(),
            body: None,
        };
        let status = next_active_request_status(record);
        Some(push_payload_patch(record, &retention, payload, status))
    }

    pub fn ingest_env_resolved(
        &self,
        host_call_id: &HostCallId,
        request_id: u64,
        key: String,
        status: EnvResolutionStatus,
        display_value: Option<String>,
    ) -> Option<RunPatch> {
        let mut inner = self
            .inner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let boundary_id = *inner.host_call_index.get(host_call_id)?;
        let payload_id = inner.allocate_payload_id();
        let retention = inner.retention.clone();
        let record = inner.runs.get_mut(&boundary_id)?;
        record.pending_env_requests.remove(&request_id);
        let state = request_state_for_record(record, RunRequestState::Resolved);
        let redacted = display_value.is_none();
        let payload = PayloadEvent {
            id: payload_id,
            timestamp_ms: epoch_ms(),
            kind: PayloadKind::EnvResolved(EnvResolved {
                request_id,
                key,
                status,
                state,
                value_redacted: redacted,
                display_value,
            }),
            redaction: if redacted {
                RedactionMetadata::omitted_by_policy(
                    "env values are redacted in RunStore by default",
                )
            } else {
                RedactionMetadata::display_safe()
            },
            body: None,
        };
        let status = next_active_request_status(record);
        Some(push_payload_patch(record, &retention, payload, status))
    }

    #[must_use]
    pub fn input_request_outcome_for_run(
        &self,
        boundary_id: BoundaryId,
        request_id: u64,
    ) -> RequestCommandOutcome {
        let inner = self
            .inner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let Some(record) = inner.runs.get(&boundary_id) else {
            return RequestCommandOutcome::Missing;
        };
        input_request_outcome(record, request_id)
    }

    pub fn resolve_input_request_for_run(
        &self,
        boundary_id: BoundaryId,
        request_id: u64,
        state: RunRequestState,
    ) -> RequestCommandResult {
        let mut inner = self
            .inner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let retention = inner.retention.clone();
        let host_call_id;
        let resolved_state;
        {
            let Some(record) = inner.runs.get_mut(&boundary_id) else {
                return RequestCommandResult::outcome(RequestCommandOutcome::Missing);
            };
            let outcome = input_request_outcome(record, request_id);
            if outcome != RequestCommandOutcome::Accepted {
                return RequestCommandResult::outcome(outcome);
            }
            let Some(current_host_call_id) = record.host_call_id.clone() else {
                return RequestCommandResult::outcome(RequestCommandOutcome::Missing);
            };
            record.pending_input_requests.remove(&request_id);
            resolved_state = request_state_for_record(record, state);
            host_call_id = current_host_call_id;
        }

        let payload_id = inner.allocate_payload_id();
        let Some(record) = inner.runs.get_mut(&boundary_id) else {
            return RequestCommandResult::outcome(RequestCommandOutcome::Missing);
        };
        let payload = PayloadEvent {
            id: payload_id,
            timestamp_ms: epoch_ms(),
            kind: PayloadKind::InputResolved(InputResolved {
                request_id,
                state: resolved_state,
            }),
            redaction: RedactionMetadata::display_safe(),
            body: None,
        };
        let status = next_active_request_status(record);
        let patch = push_payload_patch(record, &retention, payload, status);
        RequestCommandResult::accepted(host_call_id, patch)
    }

    #[must_use]
    pub fn env_request_outcome_for_run(
        &self,
        boundary_id: BoundaryId,
        request_id: u64,
    ) -> RequestCommandOutcome {
        let inner = self
            .inner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let Some(record) = inner.runs.get(&boundary_id) else {
            return RequestCommandOutcome::Missing;
        };
        env_request_outcome(record, request_id)
    }

    pub fn resolve_env_request_for_run(
        &self,
        boundary_id: BoundaryId,
        request_id: u64,
        status: EnvResolutionStatus,
        display_value: Option<String>,
    ) -> RequestCommandResult {
        let mut inner = self
            .inner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let retention = inner.retention.clone();
        let host_call_id;
        let resolved_state;
        let key;
        {
            let Some(record) = inner.runs.get_mut(&boundary_id) else {
                return RequestCommandResult::outcome(RequestCommandOutcome::Missing);
            };
            let outcome = env_request_outcome(record, request_id);
            if outcome != RequestCommandOutcome::Accepted {
                return RequestCommandResult::outcome(outcome);
            }
            let Some(current_host_call_id) = record.host_call_id.clone() else {
                return RequestCommandResult::outcome(RequestCommandOutcome::Missing);
            };
            let Some(current_key) = env_request_key(record, request_id) else {
                return RequestCommandResult::outcome(RequestCommandOutcome::Missing);
            };
            record.pending_env_requests.remove(&request_id);
            resolved_state = request_state_for_record(record, RunRequestState::Resolved);
            host_call_id = current_host_call_id;
            key = current_key;
        }

        let payload_id = inner.allocate_payload_id();
        let Some(record) = inner.runs.get_mut(&boundary_id) else {
            return RequestCommandResult::outcome(RequestCommandOutcome::Missing);
        };
        let redacted = display_value.is_none();
        let payload = PayloadEvent {
            id: payload_id,
            timestamp_ms: epoch_ms(),
            kind: PayloadKind::EnvResolved(EnvResolved {
                request_id,
                key,
                status,
                state: resolved_state,
                value_redacted: redacted,
                display_value,
            }),
            redaction: if redacted {
                RedactionMetadata::omitted_by_policy(
                    "env values are redacted in RunStore by default",
                )
            } else {
                RedactionMetadata::display_safe()
            },
            body: None,
        };
        let next_status = next_active_request_status(record);
        let patch = push_payload_patch(record, &retention, payload, next_status);
        RequestCommandResult::accepted(host_call_id, patch)
    }

    pub fn cancel_run(
        &self,
        boundary_id: BoundaryId,
        requested_at_ms: u64,
        reason: Option<String>,
    ) -> CancelRunEffect {
        let mut inner = self
            .inner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let retention = inner.retention.clone();
        let Some(record) = inner.runs.get_mut(&boundary_id) else {
            return CancelRunEffect::RunMissing;
        };
        if record.run.status.is_terminal() {
            return CancelRunEffect::AlreadyTerminal;
        }
        let (host_call_id, cancellation, cancelled_requests) = {
            let cancellation = CancellationState {
                requested_at_ms,
                completed_at_ms: None,
                reason,
            };
            record.run.cancellation = Some(cancellation.clone());
            let cancelled_requests = cancelled_request_payload_specs(record);
            (
                record.host_call_id.clone(),
                cancellation,
                cancelled_requests,
            )
        };
        let cancelled_payloads = cancelled_requests
            .into_iter()
            .map(|(kind, redaction)| PayloadEvent {
                id: inner.allocate_payload_id(),
                timestamp_ms: requested_at_ms,
                kind,
                redaction,
                body: None,
            })
            .collect::<Vec<_>>();
        let Some(record) = inner.runs.get_mut(&boundary_id) else {
            return CancelRunEffect::RunMissing;
        };
        record
            .run
            .payloads
            .extend(cancelled_payloads.iter().cloned());
        let cancelled_payload_changes = cancelled_payloads
            .into_iter()
            .map(RunPatchChange::UpsertPayload)
            .collect::<Vec<_>>();
        if let Some(host_call_id) = host_call_id {
            let mut changes = cancelled_payload_changes;
            changes.push(RunPatchChange::SetStatus(RunStatus::Cancelling));
            let patch = push_patch(record, &retention, changes);
            CancelRunEffect::CancelHostCall {
                host_call_id,
                patch,
            }
        } else {
            record.start_guard.cancel();
            let outcome = RunOutcome::Cancelled(CancellationState {
                completed_at_ms: Some(requested_at_ms),
                ..cancellation
            });
            record.run.completed_at_ms = Some(requested_at_ms);
            record.run.cancellation = match &outcome {
                RunOutcome::Cancelled(cancellation) => Some(cancellation.clone()),
                _ => None,
            };
            let mut changes = cancelled_payload_changes;
            changes.extend([
                RunPatchChange::SetStatus(RunStatus::Cancelled),
                RunPatchChange::Complete(outcome),
            ]);
            let patch = push_patch(record, &retention, changes);
            CancelRunEffect::CancelledBeforeHost { patch }
        }
    }
}

impl RunStoreInner {
    fn allocate_payload_id(&mut self) -> PayloadId {
        let id = PayloadId(self.next_payload_id);
        self.next_payload_id = self.next_payload_id.saturating_add(1);
        id
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct TraceCallKey {
    pub process_euid: ProcessEuid,
    pub engine_id: EngineId,
    pub thread_id: BexThreadId,
    pub call_id: BexCallId,
}

impl TraceCallKey {
    #[must_use]
    pub fn call_ref(self) -> CallRef {
        CallRef {
            process_euid: self.process_euid,
            engine_id: self.engine_id,
            thread_id: self.thread_id,
            call_id: self.call_id,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SourceLocation {
    pub file_path: Option<String>,
    pub file_id: Option<u64>,
    pub line: u32,
    pub column: u32,
    pub end_line: Option<u32>,
    pub end_column: Option<u32>,
    pub start_offset: Option<u32>,
    pub end_offset: Option<u32>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DiagnosticSeverity {
    Error,
    Warning,
    Info,
}

fn epoch_ms() -> u64 {
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    u64::try_from(millis).unwrap_or(u64::MAX)
}

fn run_matches_filter(run: &Run, filter: &RunFilter) -> bool {
    if let Some(project_id) = &filter.project_id
        && &run.request.project_id != project_id
    {
        return false;
    }
    if let Some(project_generation) = filter.project_generation
        && run.request.project_generation != project_generation
    {
        return false;
    }
    if !filter.kinds.is_empty() && !filter.kinds.contains(&run.target.kind()) {
        return false;
    }
    if !filter.statuses.is_empty() && !filter.statuses.contains(&run.status) {
        return false;
    }
    if let Some(function_name) = &filter.call_tree_contains_function {
        let target_matches = match &run.target {
            RunTarget::Function {
                function_name: target,
            }
            | RunTarget::Companion {
                function_name: target,
                ..
            } => target == function_name,
            RunTarget::Preview {
                parent_function_name,
                ..
            } => parent_function_name == function_name,
            RunTarget::Test { .. } | RunTarget::Internal { .. } => false,
        };
        if !target_matches {
            return false;
        }
    }
    match (&filter.visibility, &run.visibility) {
        (RunVisibilityFilter::HistoryOnly, RunVisibility::History) => true,
        (RunVisibilityFilter::HistoryOnly, _) => false,
        (
            RunVisibilityFilter::Scope { scope_id },
            RunVisibility::Scoped {
                scope_id: run_scope,
            },
        ) => scope_id == run_scope,
        (RunVisibilityFilter::Scope { .. }, _) => false,
        (RunVisibilityFilter::IncludeHidden, RunVisibility::DebugOnly) => false,
        (RunVisibilityFilter::IncludeHidden | RunVisibilityFilter::AllForDebug, _) => true,
    }
}

fn summarize_run(run: &Run) -> RunSummary {
    let mut touched = Vec::<FunctionName>::new();
    match &run.target {
        RunTarget::Function { function_name } | RunTarget::Companion { function_name, .. } => {
            touched.push(function_name.clone());
        }
        RunTarget::Preview {
            parent_function_name,
            ..
        } => touched.push(parent_function_name.clone()),
        RunTarget::Test { .. } | RunTarget::Internal { .. } => {}
    }
    RunSummary {
        boundary_id: run.boundary_id,
        target: run.target.clone(),
        visibility: run.visibility.clone(),
        status: run.status,
        request: run.request.clone(),
        touched_functions: touched,
        created_at_ms: run.created_at_ms,
        completed_at_ms: run.completed_at_ms,
        retention: RunRetentionState::Full,
    }
}

fn push_patch(
    record: &mut RunRecord,
    retention: &RunRetentionPolicy,
    changes: Vec<RunPatchChange>,
) -> RunPatch {
    let status = changes.iter().find_map(|change| match change {
        RunPatchChange::SetStatus(status) => Some(*status),
        RunPatchChange::Complete(outcome) => Some(outcome.status()),
        _ => None,
    });
    if let Some(status) = status {
        record.run.status = status;
    }
    record.run.cursor = RunCursor(record.run.cursor.0.saturating_add(1));
    let patch = RunPatch {
        boundary_id: record.run.boundary_id,
        cursor: record.run.cursor,
        changes,
    };
    record.patches.push(patch.clone());
    if record.patches.len() > retention.patch_window_capacity {
        let drop_count = record.patches.len() - retention.patch_window_capacity;
        record.patches.drain(..drop_count);
    }
    patch
}

fn push_payload_patch(
    record: &mut RunRecord,
    retention: &RunRetentionPolicy,
    payload: PayloadEvent,
    status: Option<RunStatus>,
) -> RunPatch {
    record.run.payloads.push(payload.clone());
    let mut changes = vec![RunPatchChange::UpsertPayload(payload)];
    if let Some(status) = status {
        changes.push(RunPatchChange::SetStatus(status));
    }
    push_patch(record, retention, changes)
}

fn request_state_for_record(record: &RunRecord, desired: RunRequestState) -> RunRequestState {
    if record.run.status.is_terminal() {
        RunRequestState::RunTerminal
    } else {
        desired
    }
}

fn input_request_outcome(record: &RunRecord, request_id: u64) -> RequestCommandOutcome {
    if let Some(state) = last_input_resolution_state(record, request_id) {
        return outcome_for_resolved_state(state);
    }
    if record.run.status.is_terminal() {
        return RequestCommandOutcome::AlreadyTerminal;
    }
    if matches!(record.run.status, RunStatus::Cancelling) {
        return RequestCommandOutcome::Cancelled;
    }
    if record.pending_input_requests.contains(&request_id) {
        return RequestCommandOutcome::Accepted;
    }
    RequestCommandOutcome::Missing
}

fn env_request_outcome(record: &RunRecord, request_id: u64) -> RequestCommandOutcome {
    if let Some(state) = last_env_resolution_state(record, request_id) {
        return outcome_for_resolved_state(state);
    }
    if record.run.status.is_terminal() {
        return RequestCommandOutcome::AlreadyTerminal;
    }
    if matches!(record.run.status, RunStatus::Cancelling) {
        return RequestCommandOutcome::Cancelled;
    }
    if record.pending_env_requests.contains(&request_id) {
        return RequestCommandOutcome::Accepted;
    }
    RequestCommandOutcome::Missing
}

fn last_input_resolution_state(record: &RunRecord, request_id: u64) -> Option<RunRequestState> {
    record
        .run
        .payloads
        .iter()
        .rev()
        .find_map(|payload| match &payload.kind {
            PayloadKind::InputResolved(resolved) if resolved.request_id == request_id => {
                Some(resolved.state)
            }
            _ => None,
        })
}

fn last_env_resolution_state(record: &RunRecord, request_id: u64) -> Option<RunRequestState> {
    record
        .run
        .payloads
        .iter()
        .rev()
        .find_map(|payload| match &payload.kind {
            PayloadKind::EnvResolved(resolved) if resolved.request_id == request_id => {
                Some(resolved.state)
            }
            _ => None,
        })
}

fn env_request_key(record: &RunRecord, request_id: u64) -> Option<String> {
    record
        .run
        .payloads
        .iter()
        .rev()
        .find_map(|payload| match &payload.kind {
            PayloadKind::EnvRequested(requested) if requested.request_id == request_id => {
                Some(requested.key.clone())
            }
            PayloadKind::EnvResolved(resolved) if resolved.request_id == request_id => {
                Some(resolved.key.clone())
            }
            _ => None,
        })
}

fn cancelled_request_payload_specs(
    record: &mut RunRecord,
) -> Vec<(PayloadKind, RedactionMetadata)> {
    let input_ids = record.pending_input_requests.drain().collect::<Vec<_>>();
    let env_ids = record.pending_env_requests.drain().collect::<Vec<_>>();
    let mut payloads = Vec::with_capacity(input_ids.len() + env_ids.len());
    payloads.extend(input_ids.into_iter().map(|request_id| {
        (
            PayloadKind::InputResolved(InputResolved {
                request_id,
                state: RunRequestState::Cancelled,
            }),
            RedactionMetadata::display_safe(),
        )
    }));
    for request_id in env_ids {
        let key = env_request_key(record, request_id).unwrap_or_default();
        payloads.push((
            PayloadKind::EnvResolved(EnvResolved {
                request_id,
                key,
                status: EnvResolutionStatus::DeclinedMissing,
                state: RunRequestState::Cancelled,
                value_redacted: true,
                display_value: None,
            }),
            RedactionMetadata::omitted_by_policy("env values are redacted in RunStore by default"),
        ));
    }
    payloads
}

fn outcome_for_resolved_state(state: RunRequestState) -> RequestCommandOutcome {
    match state {
        RunRequestState::Pending => RequestCommandOutcome::Missing,
        RunRequestState::Resolved => RequestCommandOutcome::AlreadyResolved,
        RunRequestState::Cancelled => RequestCommandOutcome::Cancelled,
        RunRequestState::Expired => RequestCommandOutcome::Missing,
        RunRequestState::RunTerminal => RequestCommandOutcome::AlreadyTerminal,
    }
}

fn next_active_request_status(record: &RunRecord) -> Option<RunStatus> {
    if record.run.status.is_terminal() || matches!(record.run.status, RunStatus::Cancelling) {
        return None;
    }
    if !record.pending_input_requests.is_empty() {
        return Some(RunStatus::WaitingForInput);
    }
    if !record.pending_env_requests.is_empty() {
        return Some(RunStatus::WaitingForEnv);
    }
    Some(RunStatus::Running)
}

/// Evict the oldest terminal runs beyond the retention policy's
/// `max_terminal_runs` / `terminal_ttl_ms`. Evicted runs disappear from
/// `list_runs`/`snapshot`; on native the playground rehydrates them from the
/// disk-backed history store on demand.
fn enforce_terminal_retention(inner: &mut RunStoreInner, now_ms: u64) {
    let RunRetentionPolicy {
        max_terminal_runs,
        terminal_ttl_ms,
        ..
    } = inner.retention;
    if max_terminal_runs.is_none() && terminal_ttl_ms.is_none() {
        return;
    }

    let mut terminal: Vec<(u64, BoundaryId)> = inner
        .runs
        .iter()
        .filter(|(_, record)| record.run.status.is_terminal())
        .map(|(boundary_id, record)| (record.run.completed_at_ms.unwrap_or_default(), *boundary_id))
        .collect();
    terminal.sort_unstable_by_key(|(completed_at_ms, _)| *completed_at_ms);

    let mut evict: HashSet<BoundaryId> = HashSet::new();
    if let Some(ttl_ms) = terminal_ttl_ms {
        evict.extend(
            terminal
                .iter()
                .filter(|(completed_at_ms, _)| now_ms.saturating_sub(*completed_at_ms) > ttl_ms)
                .map(|(_, boundary_id)| *boundary_id),
        );
    }
    if let Some(max) = max_terminal_runs {
        let retained = terminal.len() - evict.len();
        if retained > max {
            evict.extend(
                terminal
                    .iter()
                    .filter(|(_, boundary_id)| !evict.contains(boundary_id))
                    .take(retained - max)
                    .map(|(_, boundary_id)| *boundary_id)
                    .collect::<Vec<_>>(),
            );
        }
    }
    if evict.is_empty() {
        return;
    }

    inner
        .runs
        .retain(|boundary_id, _| !evict.contains(boundary_id));
    inner
        .host_call_index
        .retain(|_, boundary_id| !evict.contains(boundary_id));
}
