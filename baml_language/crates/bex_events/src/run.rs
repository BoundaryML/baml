//! Canonical Rust reconstruction for BEX profile events.
//!
//! The playground run store consumes this module instead of reconstructing
//! execution structure in TypeScript. Raw profile events do not carry
//! playground run identity; they carry BEX trace identity, and this module
//! reconstructs thread/call projections from those identities plus explicit
//! parent edges.

#[cfg(not(target_arch = "wasm32"))]
use std::time::{SystemTime, UNIX_EPOCH};
use std::{
    collections::{HashMap, HashSet, VecDeque},
    panic::AssertUnwindSafe,
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
    ids::{BexCallId, BexThreadId, CallRef, EngineId, FunctionId, ProcessEuid, ThreadRef},
    value::ValueRef,
};

pub trait ProfileEventObserver: Send + Sync + 'static {
    fn ingest_profile_event(&self, envelope: ProfileEventEnvelope);

    /// Called after an engine has been dropped and all of its remaining
    /// profile events have been delivered. Observers that buffer events may
    /// release everything associated with the engine — no further events or
    /// runs can arrive for it.
    fn engine_closed(&self, engine_id: EngineId) {
        let _ = engine_id;
    }
}

#[must_use]
pub fn register_profile_observer<T>(observer: Arc<T>) -> ProfileObserverRegistration
where
    T: ProfileEventObserver,
{
    let mut state = profile_observers()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let id = state.next_id;
    state.next_id = state.next_id.saturating_add(1);
    state.observers.push((id, observer));
    ProfileObserverRegistration { id }
}

#[cfg_attr(target_arch = "wasm32", allow(dead_code))]
pub(crate) fn publish_profile_event(envelope: &ProfileEventEnvelope) {
    let observers = profile_observers()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .observers
        .iter()
        .map(|(_, observer)| observer.clone())
        .collect::<Vec<_>>();
    for observer in observers {
        let envelope = envelope.clone();
        let _ =
            std::panic::catch_unwind(AssertUnwindSafe(|| observer.ingest_profile_event(envelope)));
    }
}

#[cfg_attr(target_arch = "wasm32", allow(dead_code))]
pub(crate) fn publish_engine_closed(engine_id: EngineId) {
    let observers = profile_observers()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .observers
        .iter()
        .map(|(_, observer)| observer.clone())
        .collect::<Vec<_>>();
    for observer in observers {
        let _ = std::panic::catch_unwind(AssertUnwindSafe(|| observer.engine_closed(engine_id)));
    }
}

pub struct ProfileObserverRegistration {
    id: u64,
}

impl Drop for ProfileObserverRegistration {
    fn drop(&mut self) {
        profile_observers()
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .observers
            .retain(|(id, _)| *id != self.id);
    }
}

struct ProfileObserverState {
    next_id: u64,
    observers: Vec<(u64, Arc<dyn ProfileEventObserver>)>,
}

fn profile_observers() -> &'static Mutex<ProfileObserverState> {
    static OBSERVERS: std::sync::OnceLock<Mutex<ProfileObserverState>> = std::sync::OnceLock::new();
    OBSERVERS.get_or_init(|| {
        Mutex::new(ProfileObserverState {
            next_id: 1,
            observers: Vec::new(),
        })
    })
}

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

#[derive(Clone, Debug)]
pub struct RunContext {
    pub boundary_id: BoundaryId,
    pub request: RunRequestSummary,
    pub time_anchor: RunTimeAnchor,
    pub host_call_id: Option<HostCallId>,
    pub root_trace: Option<TraceCallKey>,
    pub cancellation: Option<CancellationState>,
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
    pub call_node_id: Option<CallNodeId>,
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
    CapturedValue(CapturedValuePayload),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CapturedValuePayload {
    pub role: CapturedValueRole,
    pub label: Option<String>,
    pub value_ref: Option<ValueRef>,
    pub trace_call: Option<TraceCallKey>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CapturedValueRole {
    RootInput,
    CallInput,
    CallOutput,
    CallError,
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
    pub trace_call: Option<TraceCallKey>,
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
pub enum RunPatchChange {
    UpsertCallNode(CallNode),
    UpsertThreadNode(ThreadNode),
    UpsertPayload(PayloadEvent),
    UpsertDiagnostic(RunDiagnostic),
    SetRootCallNode(Option<CallNodeId>),
    SetGraphRuntimeOverlay(GraphRuntimeOverlay),
    SetStatus(RunStatus),
    Complete(RunOutcome),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RunDiagnostic {
    pub severity: DiagnosticSeverity,
    pub code: Option<String>,
    pub message: String,
    pub call_node_id: Option<CallNodeId>,
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
    pub root_call_node_id: Option<CallNodeId>,
    pub graph_runtime_overlay: Option<GraphRuntimeOverlay>,
    pub calls: Vec<CallNode>,
    pub threads: Vec<ThreadNode>,
    pub payloads: Vec<PayloadEvent>,
    pub diagnostics: Vec<RunDiagnostic>,
    pub cursor: RunCursor,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GraphRuntimeOverlay {
    pub boundary_id: BoundaryId,
    pub project_generation: ProjectGeneration,
    pub entries: Vec<GraphRuntimeOverlayEntry>,
    pub unattached_call_node_ids: Vec<CallNodeId>,
    pub diagnostics: Vec<RunDiagnostic>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GraphRuntimeOverlayEntry {
    pub cfg_node_id: CfgNodeId,
    pub call_node_ids: Vec<CallNodeId>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct CfgNodeId(pub u64);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CfgNodeSourceSpan {
    pub cfg_node_id: CfgNodeId,
    pub file_id: u64,
    pub start_offset: u32,
    pub end_offset: u32,
}

pub trait GraphRuntimeOverlaySpanProvider: Send + Sync + 'static {
    fn cfg_node_spans_for_run(&self, run: &Run) -> GraphRuntimeOverlaySpanResolution;
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum GraphRuntimeOverlaySpanResolution {
    Available(Vec<CfgNodeSourceSpan>),
    Unavailable(RunDiagnostic),
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
pub enum AttachRootTraceResult {
    Attached { patches: Vec<RunPatch> },
    AlreadyAttached,
    RunMissing,
    Conflict { existing: CallRef },
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
    graph_overlay_span_provider: Arc<Mutex<Option<Arc<dyn GraphRuntimeOverlaySpanProvider>>>>,
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
    profile_events: Vec<ProfileEventEnvelope>,
    retention: RunRetentionPolicy,
    next_payload_id: u64,
}

#[derive(Clone, Debug)]
struct RunRecord {
    run: Run,
    host_call_id: Option<HostCallId>,
    root_trace: Option<TraceCallKey>,
    profile_function_table: Vec<ProfileFunctionMetadata>,
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
                profile_events: Vec::new(),
                retention,
                next_payload_id: 1,
            })),
            graph_overlay_span_provider: Arc::new(Mutex::new(None)),
        }
    }

    pub fn set_graph_runtime_overlay_span_provider(
        &self,
        provider: Arc<dyn GraphRuntimeOverlaySpanProvider>,
    ) {
        *self
            .graph_overlay_span_provider
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(provider);
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
            root_call_node_id: None,
            graph_runtime_overlay: None,
            calls: Vec::new(),
            threads: Vec::new(),
            payloads: Vec::new(),
            diagnostics: Vec::new(),
            cursor: RunCursor(0),
        };
        let record = RunRecord {
            run,
            host_call_id: None,
            root_trace: None,
            profile_function_table: Vec::new(),
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
        let root_trace = run
            .root_call_node_id
            .and_then(|root_id| run.calls.iter().find(|call| call.id == root_id))
            .map(|call| call.trace_key);
        let domain_diagnostics = run.diagnostics.clone();
        inner.runs.insert(
            run.boundary_id,
            RunRecord {
                run,
                host_call_id: None,
                root_trace,
                profile_function_table: Vec::new(),
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

    pub fn attach_root_trace(
        &self,
        boundary_id: BoundaryId,
        root_call_ref: CallRef,
    ) -> AttachRootTraceResult {
        let graph_overlay_span_provider = self
            .graph_overlay_span_provider
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone();
        let root_trace = TraceCallKey {
            process_euid: root_call_ref.process_euid,
            engine_id: root_call_ref.engine_id,
            thread_id: root_call_ref.thread_id,
            call_id: root_call_ref.call_id,
        };
        let mut inner = self
            .inner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let inner = &mut *inner;
        let Some(record) = inner.runs.get_mut(&boundary_id) else {
            return AttachRootTraceResult::RunMissing;
        };
        match record.root_trace {
            Some(existing) if existing == root_trace => {
                return AttachRootTraceResult::AlreadyAttached;
            }
            Some(existing) => {
                return AttachRootTraceResult::Conflict {
                    existing: existing.call_ref(),
                };
            }
            None => record.root_trace = Some(root_trace),
        }
        let patches = recompute_record_profile(
            record,
            &inner.profile_events,
            &inner.retention,
            graph_overlay_span_provider.as_deref(),
        );
        AttachRootTraceResult::Attached { patches }
    }

    pub fn ingest_profile_event(&self, envelope: ProfileEventEnvelope) -> Vec<RunPatch> {
        let graph_overlay_span_provider = self
            .graph_overlay_span_provider
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone();
        let mut inner = self
            .inner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let inner = &mut *inner;
        // An event can only belong to runs rooted in the same
        // process/engine: component membership is resolved over the
        // thread/call parent graph, which never crosses engines.
        let event_scope = (envelope.process_euid, envelope.engine_id);
        inner.profile_events.push(envelope);
        trim_profile_events(&mut inner.profile_events);
        let mut patches = Vec::new();
        for record in inner.runs.values_mut() {
            let Some(root_trace) = record.root_trace else {
                continue;
            };
            if (root_trace.process_euid, root_trace.engine_id) != event_scope {
                continue;
            }
            patches.extend(recompute_record_profile(
                record,
                &inner.profile_events,
                &inner.retention,
                graph_overlay_span_provider.as_deref(),
            ));
        }
        patches
    }

    /// Number of buffered profile events (diagnostics/tests).
    #[must_use]
    pub fn profile_events_len(&self) -> usize {
        self.inner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .profile_events
            .len()
    }

    /// Release everything buffered for a closed engine. Runs rooted in the
    /// engine get one final profile recompute (the engine is drained before
    /// this is called), then the engine's profile events are dropped — no
    /// further events or runs can arrive for a closed engine.
    pub fn engine_closed(&self, engine_id: EngineId) -> Vec<RunPatch> {
        let graph_overlay_span_provider = self
            .graph_overlay_span_provider
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone();
        let mut inner = self
            .inner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let inner = &mut *inner;
        let mut patches = Vec::new();
        for record in inner.runs.values_mut() {
            let Some(root_trace) = record.root_trace else {
                continue;
            };
            if root_trace.engine_id != engine_id {
                continue;
            }
            patches.extend(recompute_record_profile(
                record,
                &inner.profile_events,
                &inner.retention,
                graph_overlay_span_provider.as_deref(),
            ));
        }
        inner
            .profile_events
            .retain(|envelope| envelope.engine_id != engine_id);
        patches
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

    pub fn ingest_root_input_value_ref(
        &self,
        boundary_id: BoundaryId,
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
            call_node_id: record.run.root_call_node_id,
            timestamp_ms: epoch_ms(),
            kind: PayloadKind::CapturedValue(CapturedValuePayload {
                role: CapturedValueRole::RootInput,
                label: Some("inputs".to_string()),
                value_ref,
                trace_call: None,
            }),
            redaction: RedactionMetadata::display_safe(),
            body: None,
        };
        Some(push_payload_patch(record, &retention, payload, None))
    }

    pub fn ingest_call_value_ref(
        &self,
        boundary_id: BoundaryId,
        call: TraceCallKey,
        role: CapturedValueRole,
        label: Option<String>,
        value_ref: Option<ValueRef>,
    ) -> Option<RunPatch> {
        debug_assert!(
            matches!(
                role,
                CapturedValueRole::CallInput
                    | CapturedValueRole::CallOutput
                    | CapturedValueRole::CallError
            ),
            "call value ingestion only accepts call input/output/error roles"
        );
        let mut inner = self
            .inner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let payload_id = inner.allocate_payload_id();
        let retention = inner.retention.clone();
        let record = inner.runs.get_mut(&boundary_id)?;
        let call_node_id = record
            .run
            .calls
            .iter()
            .any(|node| node.trace_key == call)
            .then(|| call_node_id(&call));
        let payload = PayloadEvent {
            id: payload_id,
            call_node_id,
            timestamp_ms: epoch_ms(),
            kind: PayloadKind::CapturedValue(CapturedValuePayload {
                role,
                label,
                value_ref,
                trace_call: Some(call),
            }),
            redaction: RedactionMetadata::display_safe(),
            body: None,
        };
        Some(push_payload_patch(record, &retention, payload, None))
    }

    pub fn ingest_log_value_ref(
        &self,
        boundary_id: BoundaryId,
        call: TraceCallKey,
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
        let call_node_id = record
            .run
            .calls
            .iter()
            .any(|node| node.trace_key == call)
            .then(|| call_node_id(&call));
        let payload = PayloadEvent {
            id: payload_id,
            call_node_id,
            timestamp_ms: epoch_ms(),
            kind: PayloadKind::Log(LogPayload {
                level,
                message,
                source,
                value_ref,
                trace_call: Some(call),
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
            call_node_id: None,
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
            call_node_id: None,
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
            call_node_id: None,
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
            call_node_id: None,
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
            call_node_id: None,
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
            call_node_id: None,
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
            call_node_id: None,
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
            call_node_id: None,
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
            call_node_id: None,
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
                call_node_id: None,
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

impl ProfileEventObserver for InMemoryRunStore {
    fn ingest_profile_event(&self, envelope: ProfileEventEnvelope) {
        let _ = InMemoryRunStore::ingest_profile_event(self, envelope);
    }
}

impl RunStoreInner {
    fn allocate_payload_id(&mut self) -> PayloadId {
        let id = PayloadId(self.next_payload_id);
        self.next_payload_id = self.next_payload_id.saturating_add(1);
        id
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProfileEventEnvelope {
    pub source: ProfileEventSource,
    pub process_euid: ProcessEuid,
    pub engine_id: EngineId,
    pub event: ProfileEvent,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ProfileEventSource {
    Live {
        target: RuntimeTarget,
        source_id: String,
    },
    Replay {
        artifact_id: String,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RuntimeTarget {
    Native,
    Node,
    Wasm,
    Replay,
    Other(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProfileEvent {
    pub timestamp_ns: u64,
    pub kind: ProfileEventKind,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ProfileEventKind {
    StartThread {
        thread_id: BexThreadId,
        parent_thread_id: Option<BexThreadId>,
        parent_call_id: Option<BexCallId>,
        name: Option<String>,
    },
    EndThread {
        thread_id: BexThreadId,
        status: ThreadStatus,
    },
    CallFunction {
        thread_id: BexThreadId,
        call_id: BexCallId,
        parent_call_id: Option<BexCallId>,
        function_id: FunctionId,
        call_site_source: Option<SourceLocation>,
    },
    EndFunction {
        thread_id: BexThreadId,
        call_id: BexCallId,
        status: CallStatus,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct TraceThreadKey {
    pub process_euid: ProcessEuid,
    pub engine_id: EngineId,
    pub thread_id: BexThreadId,
}

impl TraceThreadKey {
    #[must_use]
    pub fn thread_ref(self) -> ThreadRef {
        ThreadRef {
            process_euid: self.process_euid,
            engine_id: self.engine_id,
            thread_id: self.thread_id,
        }
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

    #[must_use]
    pub fn thread_key(self) -> TraceThreadKey {
        TraceThreadKey {
            process_euid: self.process_euid,
            engine_id: self.engine_id,
            thread_id: self.thread_id,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ThreadNodeId(u64);

impl ThreadNodeId {
    #[must_use]
    pub fn get(self) -> u64 {
        self.0
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct CallNodeId(u64);

impl CallNodeId {
    #[must_use]
    pub fn get(self) -> u64 {
        self.0
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ThreadStatus {
    Running,
    Completed,
    Cancelled,
    Errored,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CallStatus {
    Running,
    Ok,
    Errored,
    Cancelled,
    Exited,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProfileFunctionMetadata {
    pub function_id: FunctionId,
    pub fqn: String,
    pub source_file: Option<String>,
    pub span_start: Option<u32>,
    pub span_end: Option<u32>,
    pub kind: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FunctionOrigin {
    User,
    Builtin,
    Companion,
    Internal,
    Unknown,
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

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ThreadNode {
    pub id: ThreadNodeId,
    pub trace_key: TraceThreadKey,
    pub trace_ref: ThreadRef,
    pub parent_thread_id: Option<ThreadNodeId>,
    pub parent_call_node_id: Option<CallNodeId>,
    pub name: Option<String>,
    pub started_at_ns: Option<u64>,
    pub ended_at_ns: Option<u64>,
    pub status: ThreadStatus,
    pub call_node_ids: Vec<CallNodeId>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CallNode {
    pub id: CallNodeId,
    pub trace_key: TraceCallKey,
    pub trace_ref: CallRef,
    pub thread_id: ThreadNodeId,
    pub parent_id: Option<CallNodeId>,
    pub function_id: FunctionId,
    pub function_name: Option<String>,
    pub function_origin: Option<FunctionOrigin>,
    pub callee_source: Option<SourceLocation>,
    pub call_site_source: Option<SourceLocation>,
    pub started_at_ns: Option<u64>,
    pub ended_at_ns: Option<u64>,
    pub status: CallStatus,
    pub payload_ids: Vec<PayloadId>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReconstructedProfile {
    pub function_table: Vec<ProfileFunctionMetadata>,
    pub threads: Vec<ThreadNode>,
    pub calls: Vec<CallNode>,
    pub diagnostics: Vec<ReconstructionDiagnostic>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReconstructionDiagnostic {
    pub severity: DiagnosticSeverity,
    pub code: ReconstructionDiagnosticCode,
    pub message: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DiagnosticSeverity {
    Error,
    Warning,
    Info,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ReconstructionDiagnosticCode {
    DuplicateThreadStart,
    DuplicateThreadEnd,
    DuplicateCallStart,
    DuplicateCallEnd,
    MissingThreadStart,
    MissingThreadEnd,
    MissingCallStart,
    MissingCallEnd,
    MissingParentThread,
    MissingParentCall,
    ParentCallWithoutParentThread,
    UnknownFunctionId,
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
        let call_matches = run
            .calls
            .iter()
            .any(|call| call.function_name.as_ref() == Some(function_name));
        if !target_matches && !call_matches {
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
    for call in &run.calls {
        if let Some(function_name) = &call.function_name
            && !touched.contains(function_name)
        {
            touched.push(function_name.clone());
        }
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

pub(crate) fn attach_payload_ids_to_calls(calls: &mut [CallNode], payloads: &[PayloadEvent]) {
    for call in calls.iter_mut() {
        call.payload_ids.clear();
    }
    for payload in payloads {
        let Some(call_node_id) = payload.call_node_id else {
            continue;
        };
        let Some(call) = calls.iter_mut().find(|call| call.id == call_node_id) else {
            continue;
        };
        if !call.payload_ids.contains(&payload.id) {
            call.payload_ids.push(payload.id);
        }
    }
}

fn push_payload_patch(
    record: &mut RunRecord,
    retention: &RunRetentionPolicy,
    payload: PayloadEvent,
    status: Option<RunStatus>,
) -> RunPatch {
    let call_update = payload.call_node_id.and_then(|call_node_id| {
        let call = record
            .run
            .calls
            .iter_mut()
            .find(|call| call.id == call_node_id)?;
        if !call.payload_ids.contains(&payload.id) {
            call.payload_ids.push(payload.id);
        }
        Some(call.clone())
    });
    record.run.payloads.push(payload.clone());
    let mut changes = vec![RunPatchChange::UpsertPayload(payload)];
    if let Some(call) = call_update {
        changes.push(RunPatchChange::UpsertCallNode(call));
    }
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

/// Hard backstop on the retained profile-event window. Engine closure is the
/// primary release point (`InMemoryRunStore::engine_closed`); this cap only
/// bounds a single long-lived engine that emits events faster than runs
/// complete. Matches `BoundaryTraceRouter`'s bound in `history::router`.
const PROFILE_EVENTS_CAP: usize = 100_000;

fn trim_profile_events(profile_events: &mut Vec<ProfileEventEnvelope>) {
    if profile_events.len() > PROFILE_EVENTS_CAP {
        let excess = profile_events.len() - PROFILE_EVENTS_CAP;
        profile_events.drain(..excess);
    }
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

fn recompute_record_profile(
    record: &mut RunRecord,
    profile_events: &[ProfileEventEnvelope],
    retention: &RunRetentionPolicy,
    graph_overlay_span_provider: Option<&dyn GraphRuntimeOverlaySpanProvider>,
) -> Vec<RunPatch> {
    let Some(root_trace) = record.root_trace else {
        return Vec::new();
    };
    let component_events = component_events_for_root(profile_events, root_trace);
    if component_events.is_empty() {
        return Vec::new();
    }

    if record.profile_function_table.is_empty() {
        record.profile_function_table = live_function_table(root_trace.engine_id);
    }
    let reconstructed =
        reconstruct_with_function_table(component_events, record.profile_function_table.clone());
    let root_call_node_id = Some(call_node_id(&root_trace));
    let diagnostics = reconstructed
        .diagnostics
        .iter()
        .map(run_diagnostic_from_reconstruction)
        .collect::<Vec<_>>();
    record.run.root_call_node_id = root_call_node_id;
    record.run.calls.clone_from(&reconstructed.calls);
    record.run.threads.clone_from(&reconstructed.threads);
    let mut payload_updates = Vec::new();
    if let Some(root_call_node_id) = root_call_node_id {
        for payload in &mut record.run.payloads {
            if payload.call_node_id.is_none()
                && matches!(
                    &payload.kind,
                    PayloadKind::CapturedValue(CapturedValuePayload {
                        role: CapturedValueRole::RootInput,
                        ..
                    })
                )
            {
                payload.call_node_id = Some(root_call_node_id);
                payload_updates.push(payload.clone());
            }
        }
    }
    for payload in &mut record.run.payloads {
        if payload.call_node_id.is_none()
            && let PayloadKind::CapturedValue(CapturedValuePayload {
                role:
                    CapturedValueRole::CallInput
                    | CapturedValueRole::CallOutput
                    | CapturedValueRole::CallError,
                trace_call: Some(call),
                ..
            }) = &payload.kind
            && record.run.calls.iter().any(|node| node.trace_key == *call)
        {
            payload.call_node_id = Some(call_node_id(call));
            payload_updates.push(payload.clone());
        }
    }
    for payload in &mut record.run.payloads {
        if payload.call_node_id.is_none()
            && let PayloadKind::Log(LogPayload {
                trace_call: Some(call),
                ..
            }) = &payload.kind
            && record.run.calls.iter().any(|node| node.trace_key == *call)
        {
            payload.call_node_id = Some(call_node_id(call));
            payload_updates.push(payload.clone());
        }
    }
    attach_payload_ids_to_calls(&mut record.run.calls, &record.run.payloads);
    let graph_runtime_overlay =
        build_graph_runtime_overlay(&record.run, graph_overlay_span_provider);
    record.run.graph_runtime_overlay = Some(graph_runtime_overlay.clone());
    record.run.diagnostics = record
        .domain_diagnostics
        .iter()
        .cloned()
        .chain(diagnostics.clone())
        .collect();

    let mut changes = Vec::new();
    changes.push(RunPatchChange::SetRootCallNode(root_call_node_id));
    changes.push(RunPatchChange::SetGraphRuntimeOverlay(
        graph_runtime_overlay,
    ));
    changes.extend(
        reconstructed
            .threads
            .into_iter()
            .map(RunPatchChange::UpsertThreadNode),
    );
    changes.extend(
        record
            .run
            .calls
            .iter()
            .cloned()
            .map(RunPatchChange::UpsertCallNode),
    );
    changes.extend(
        payload_updates
            .into_iter()
            .map(RunPatchChange::UpsertPayload),
    );
    changes.extend(
        diagnostics
            .into_iter()
            .map(RunPatchChange::UpsertDiagnostic),
    );
    vec![push_patch(record, retention, changes)]
}

fn live_function_table(engine_id: EngineId) -> Vec<ProfileFunctionMetadata> {
    crate::prof::metadata::get_engine_metadata(engine_id.0)
        .map(|metadata| {
            metadata
                .functions
                .into_iter()
                .map(|function| ProfileFunctionMetadata {
                    function_id: FunctionId(function.function_id),
                    fqn: function.fqn,
                    source_file: (!function.source_file.is_empty()).then_some(function.source_file),
                    span_start: Some(function.span_start),
                    span_end: Some(function.span_end),
                    kind: (!function.kind.is_empty()).then_some(function.kind),
                })
                .collect()
        })
        .unwrap_or_default()
}

fn build_graph_runtime_overlay(
    run: &Run,
    graph_overlay_span_provider: Option<&dyn GraphRuntimeOverlaySpanProvider>,
) -> GraphRuntimeOverlay {
    if let Some(provider) = graph_overlay_span_provider {
        match provider.cfg_node_spans_for_run(run) {
            GraphRuntimeOverlaySpanResolution::Available(cfg_node_spans) => {
                return build_graph_runtime_overlay_from_cfg_spans(run, &cfg_node_spans);
            }
            GraphRuntimeOverlaySpanResolution::Unavailable(diagnostic) => {
                return build_unattached_graph_runtime_overlay(run, Some(diagnostic));
            }
        }
    }
    build_unattached_graph_runtime_overlay(run, None)
}

fn build_unattached_graph_runtime_overlay(
    run: &Run,
    project_store_diagnostic: Option<RunDiagnostic>,
) -> GraphRuntimeOverlay {
    let entries: Vec<GraphRuntimeOverlayEntry> = Vec::new();
    let attached_call_node_ids = entries
        .iter()
        .flat_map(|entry| entry.call_node_ids.iter().copied())
        .collect::<HashSet<_>>();
    let unattached_call_node_ids = run
        .calls
        .iter()
        .filter(|call| !attached_call_node_ids.contains(&call.id))
        .map(|call| call.id)
        .collect::<Vec<_>>();

    let mut diagnostics = Vec::new();
    if run.calls.iter().any(|call| call.call_site_source.is_none()) {
        diagnostics.push(RunDiagnostic {
            severity: DiagnosticSeverity::Info,
            code: Some("GraphOverlayCallSiteUnavailable".to_string()),
            message: "Runtime graph overlay left calls unattached because call-site provenance is unavailable; no function-name fallback was used.".to_string(),
            call_node_id: None,
            payload_id: None,
        });
    }
    if run.calls.iter().any(|call| call.call_site_source.is_some()) {
        diagnostics.push(project_store_diagnostic.unwrap_or_else(|| RunDiagnostic {
            severity: DiagnosticSeverity::Info,
            code: Some("GraphOverlayProjectStoreUnavailable".to_string()),
            message: "Runtime graph overlay left call-site-provenance calls unattached because ProjectStore CFG resolution is not wired into this projection yet; no browser fallback was used.".to_string(),
            call_node_id: None,
            payload_id: None,
        }));
    }

    GraphRuntimeOverlay {
        boundary_id: run.boundary_id,
        project_generation: run.request.project_generation,
        entries,
        unattached_call_node_ids,
        diagnostics,
    }
}

#[must_use]
pub fn build_graph_runtime_overlay_from_cfg_spans(
    run: &Run,
    cfg_node_spans: &[CfgNodeSourceSpan],
) -> GraphRuntimeOverlay {
    let mut entries = Vec::<GraphRuntimeOverlayEntry>::new();
    let mut entry_by_cfg_node_id = HashMap::<CfgNodeId, usize>::new();
    let mut unattached_call_node_ids = Vec::new();

    let mut saw_missing_call_site = false;
    let mut saw_incomplete_call_site = false;
    let mut saw_unmatched_call_site = false;
    let mut saw_ambiguous_call_site = false;

    for call in &run.calls {
        let Some(call_site) = call.call_site_source.as_ref() else {
            saw_missing_call_site = true;
            unattached_call_node_ids.push(call.id);
            continue;
        };
        let (Some(file_id), Some(start_offset), Some(end_offset)) = (
            call_site.file_id,
            call_site.start_offset,
            call_site.end_offset,
        ) else {
            saw_incomplete_call_site = true;
            unattached_call_node_ids.push(call.id);
            continue;
        };
        if end_offset < start_offset {
            saw_incomplete_call_site = true;
            unattached_call_node_ids.push(call.id);
            continue;
        }

        match resolve_cfg_node_for_call_site(cfg_node_spans, file_id, start_offset, end_offset) {
            CfgNodeResolution::Matched(cfg_node_id) => {
                let entry_index = *entry_by_cfg_node_id.entry(cfg_node_id).or_insert_with(|| {
                    let index = entries.len();
                    entries.push(GraphRuntimeOverlayEntry {
                        cfg_node_id,
                        call_node_ids: Vec::new(),
                    });
                    index
                });
                entries[entry_index].call_node_ids.push(call.id);
            }
            CfgNodeResolution::Unmatched => {
                saw_unmatched_call_site = true;
                unattached_call_node_ids.push(call.id);
            }
            CfgNodeResolution::Ambiguous => {
                saw_ambiguous_call_site = true;
                unattached_call_node_ids.push(call.id);
            }
        }
    }

    let mut diagnostics = Vec::new();
    if saw_missing_call_site {
        diagnostics.push(RunDiagnostic {
            severity: DiagnosticSeverity::Info,
            code: Some("GraphOverlayCallSiteUnavailable".to_string()),
            message: "Runtime graph overlay left calls unattached because call-site provenance is unavailable; no function-name fallback was used.".to_string(),
            call_node_id: None,
            payload_id: None,
        });
    }
    if saw_incomplete_call_site {
        diagnostics.push(RunDiagnostic {
            severity: DiagnosticSeverity::Info,
            code: Some("GraphOverlayCallSiteIncomplete".to_string()),
            message: "Runtime graph overlay left calls unattached because call-site provenance lacked a complete file/offset span; no function-name fallback was used.".to_string(),
            call_node_id: None,
            payload_id: None,
        });
    }
    if saw_unmatched_call_site {
        diagnostics.push(RunDiagnostic {
            severity: DiagnosticSeverity::Info,
            code: Some("GraphOverlayCfgNodeUnavailable".to_string()),
            message: "Runtime graph overlay left calls unattached because no captured CFG node source span matched the call-site provenance; no function-name fallback was used.".to_string(),
            call_node_id: None,
            payload_id: None,
        });
    }
    if saw_ambiguous_call_site {
        diagnostics.push(RunDiagnostic {
            severity: DiagnosticSeverity::Warning,
            code: Some("GraphOverlayCfgNodeAmbiguous".to_string()),
            message: "Runtime graph overlay left calls unattached because multiple captured CFG node source spans matched the call-site provenance equally; no arbitrary graph node was selected.".to_string(),
            call_node_id: None,
            payload_id: None,
        });
    }

    GraphRuntimeOverlay {
        boundary_id: run.boundary_id,
        project_generation: run.request.project_generation,
        entries,
        unattached_call_node_ids,
        diagnostics,
    }
}

enum CfgNodeResolution {
    Matched(CfgNodeId),
    Unmatched,
    Ambiguous,
}

fn resolve_cfg_node_for_call_site(
    cfg_node_spans: &[CfgNodeSourceSpan],
    file_id: u64,
    start_offset: u32,
    end_offset: u32,
) -> CfgNodeResolution {
    let mut exact_matches = cfg_node_spans
        .iter()
        .filter(|span| {
            span.file_id == file_id
                && span.start_offset == start_offset
                && span.end_offset == end_offset
        })
        .map(|span| span.cfg_node_id);
    let Some(first_exact) = exact_matches.next() else {
        return resolve_containing_cfg_node(cfg_node_spans, file_id, start_offset, end_offset);
    };
    if exact_matches.next().is_some() {
        CfgNodeResolution::Ambiguous
    } else {
        CfgNodeResolution::Matched(first_exact)
    }
}

fn resolve_containing_cfg_node(
    cfg_node_spans: &[CfgNodeSourceSpan],
    file_id: u64,
    start_offset: u32,
    end_offset: u32,
) -> CfgNodeResolution {
    let mut best: Option<(u32, CfgNodeId)> = None;
    let mut ambiguous = false;

    for span in cfg_node_spans {
        if span.file_id != file_id
            || span.start_offset > start_offset
            || span.end_offset < end_offset
        {
            continue;
        }

        let width = span.end_offset.saturating_sub(span.start_offset);
        match best {
            None => {
                best = Some((width, span.cfg_node_id));
                ambiguous = false;
            }
            Some((best_width, _)) if width < best_width => {
                best = Some((width, span.cfg_node_id));
                ambiguous = false;
            }
            Some((best_width, _)) if width == best_width => {
                ambiguous = true;
            }
            Some(_) => {}
        }
    }

    if ambiguous {
        CfgNodeResolution::Ambiguous
    } else if let Some((_, cfg_node_id)) = best {
        CfgNodeResolution::Matched(cfg_node_id)
    } else {
        CfgNodeResolution::Unmatched
    }
}

fn run_diagnostic_from_reconstruction(diagnostic: &ReconstructionDiagnostic) -> RunDiagnostic {
    RunDiagnostic {
        severity: diagnostic.severity,
        code: Some(format!("{:?}", diagnostic.code)),
        message: diagnostic.message.clone(),
        call_node_id: None,
        payload_id: None,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum TraceGraphNode {
    Thread(TraceThreadKey),
    Call(TraceCallKey),
}

fn component_events_for_root(
    events: &[ProfileEventEnvelope],
    root_trace: TraceCallKey,
) -> Vec<ProfileEventEnvelope> {
    component_event_indices_for_root(events, root_trace)
        .into_iter()
        .map(|index| events[index].clone())
        .collect()
}

pub(crate) fn component_event_indices_for_root<'a>(
    events: impl IntoIterator<Item = &'a ProfileEventEnvelope>,
    root_trace: TraceCallKey,
) -> Vec<usize> {
    let mut adjacency: HashMap<TraceGraphNode, HashSet<TraceGraphNode>> = HashMap::new();
    let mut event_nodes = Vec::<Vec<TraceGraphNode>>::new();

    for envelope in events {
        if envelope.process_euid != root_trace.process_euid
            || envelope.engine_id != root_trace.engine_id
        {
            event_nodes.push(Vec::new());
            continue;
        }
        let nodes = match &envelope.event.kind {
            ProfileEventKind::StartThread {
                thread_id,
                parent_thread_id,
                parent_call_id,
                ..
            } => {
                let thread = TraceGraphNode::Thread(TraceThreadKey {
                    process_euid: envelope.process_euid,
                    engine_id: envelope.engine_id,
                    thread_id: *thread_id,
                });
                ensure_graph_node(&mut adjacency, thread);
                if let Some(parent_thread_id) = parent_thread_id {
                    let parent_thread = TraceGraphNode::Thread(TraceThreadKey {
                        process_euid: envelope.process_euid,
                        engine_id: envelope.engine_id,
                        thread_id: *parent_thread_id,
                    });
                    link_graph_nodes(&mut adjacency, thread, parent_thread);
                    if let Some(parent_call_id) = parent_call_id {
                        let parent_call = TraceGraphNode::Call(TraceCallKey {
                            process_euid: envelope.process_euid,
                            engine_id: envelope.engine_id,
                            thread_id: *parent_thread_id,
                            call_id: *parent_call_id,
                        });
                        link_graph_nodes(&mut adjacency, thread, parent_call);
                    }
                }
                vec![thread]
            }
            ProfileEventKind::EndThread { thread_id, .. } => {
                let thread = TraceGraphNode::Thread(TraceThreadKey {
                    process_euid: envelope.process_euid,
                    engine_id: envelope.engine_id,
                    thread_id: *thread_id,
                });
                ensure_graph_node(&mut adjacency, thread);
                vec![thread]
            }
            ProfileEventKind::CallFunction {
                thread_id,
                call_id,
                parent_call_id,
                ..
            } => {
                let thread_key = TraceThreadKey {
                    process_euid: envelope.process_euid,
                    engine_id: envelope.engine_id,
                    thread_id: *thread_id,
                };
                let thread = TraceGraphNode::Thread(thread_key);
                let call = TraceGraphNode::Call(TraceCallKey {
                    process_euid: envelope.process_euid,
                    engine_id: envelope.engine_id,
                    thread_id: *thread_id,
                    call_id: *call_id,
                });
                link_graph_nodes(&mut adjacency, thread, call);
                if let Some(parent_call_id) = parent_call_id {
                    let parent_call = TraceGraphNode::Call(TraceCallKey {
                        process_euid: envelope.process_euid,
                        engine_id: envelope.engine_id,
                        thread_id: *thread_id,
                        call_id: *parent_call_id,
                    });
                    link_graph_nodes(&mut adjacency, call, parent_call);
                }
                vec![call, thread]
            }
            ProfileEventKind::EndFunction {
                thread_id, call_id, ..
            } => {
                let call = TraceGraphNode::Call(TraceCallKey {
                    process_euid: envelope.process_euid,
                    engine_id: envelope.engine_id,
                    thread_id: *thread_id,
                    call_id: *call_id,
                });
                ensure_graph_node(&mut adjacency, call);
                vec![call]
            }
        };
        event_nodes.push(nodes);
    }

    let root = TraceGraphNode::Call(root_trace);
    if !adjacency.contains_key(&root) {
        return Vec::new();
    }
    let mut visited = HashSet::new();
    let mut queue = VecDeque::from([root]);
    while let Some(node) = queue.pop_front() {
        if !visited.insert(node) {
            continue;
        }
        if let Some(neighbors) = adjacency.get(&node) {
            queue.extend(neighbors.iter().copied());
        }
    }

    event_nodes
        .into_iter()
        .enumerate()
        .filter_map(|(index, nodes)| {
            nodes
                .iter()
                .any(|node| visited.contains(node))
                .then_some(index)
        })
        .collect()
}

fn ensure_graph_node(
    adjacency: &mut HashMap<TraceGraphNode, HashSet<TraceGraphNode>>,
    node: TraceGraphNode,
) {
    adjacency.entry(node).or_default();
}

fn link_graph_nodes(
    adjacency: &mut HashMap<TraceGraphNode, HashSet<TraceGraphNode>>,
    a: TraceGraphNode,
    b: TraceGraphNode,
) {
    adjacency.entry(a).or_default().insert(b);
    adjacency.entry(b).or_default().insert(a);
}

#[derive(Clone, Debug)]
struct ThreadBuilder {
    node: ThreadNode,
    saw_start: bool,
    saw_end: bool,
}

#[derive(Clone, Debug)]
struct CallBuilder {
    node: CallNode,
    saw_start: bool,
    saw_end: bool,
}

#[must_use]
pub fn reconstruct<I>(events: I) -> ReconstructedProfile
where
    I: IntoIterator<Item = ProfileEventEnvelope>,
{
    reconstruct_with_function_table(events, Vec::new())
}

#[must_use]
pub fn reconstruct_with_function_table<I>(
    events: I,
    function_table: Vec<ProfileFunctionMetadata>,
) -> ReconstructedProfile
where
    I: IntoIterator<Item = ProfileEventEnvelope>,
{
    let metadata_by_id: HashMap<FunctionId, &ProfileFunctionMetadata> = function_table
        .iter()
        .map(|meta| (meta.function_id, meta))
        .collect();
    let mut threads: HashMap<TraceThreadKey, ThreadBuilder> = HashMap::new();
    let mut calls: HashMap<TraceCallKey, CallBuilder> = HashMap::new();
    let mut diagnostics = Vec::new();

    for envelope in events {
        let process_euid = envelope.process_euid;
        let engine_id = envelope.engine_id;
        let timestamp_ns = envelope.event.timestamp_ns;
        match envelope.event.kind {
            ProfileEventKind::StartThread {
                thread_id,
                parent_thread_id,
                parent_call_id,
                name,
            } => {
                let key = TraceThreadKey {
                    process_euid,
                    engine_id,
                    thread_id,
                };
                let builder = ensure_thread(&mut threads, key);
                if builder.saw_start {
                    diagnostics.push(diagnostic(
                        DiagnosticSeverity::Error,
                        ReconstructionDiagnosticCode::DuplicateThreadStart,
                        format!("duplicate StartThread for thread {}", thread_id.0),
                    ));
                } else {
                    builder.saw_start = true;
                    builder.node.started_at_ns = Some(timestamp_ns);
                    builder.node.name = name;
                    if let Some(parent_thread_id) = parent_thread_id {
                        let parent_thread_key = TraceThreadKey {
                            process_euid,
                            engine_id,
                            thread_id: parent_thread_id,
                        };
                        builder.node.parent_thread_id = Some(thread_node_id(&parent_thread_key));
                        builder.node.parent_call_node_id = parent_call_id.map(|call_id| {
                            call_node_id(&TraceCallKey {
                                process_euid,
                                engine_id,
                                thread_id: parent_thread_id,
                                call_id,
                            })
                        });
                    } else if parent_call_id.is_some() {
                        diagnostics.push(diagnostic(
                            DiagnosticSeverity::Error,
                            ReconstructionDiagnosticCode::ParentCallWithoutParentThread,
                            format!(
                                "StartThread {} has parent_call_id without parent_thread_id",
                                thread_id.0
                            ),
                        ));
                    }
                }
            }
            ProfileEventKind::EndThread { thread_id, status } => {
                let key = TraceThreadKey {
                    process_euid,
                    engine_id,
                    thread_id,
                };
                let builder = ensure_thread(&mut threads, key);
                if builder.saw_end {
                    diagnostics.push(diagnostic(
                        DiagnosticSeverity::Error,
                        ReconstructionDiagnosticCode::DuplicateThreadEnd,
                        format!("duplicate EndThread for thread {}", thread_id.0),
                    ));
                } else {
                    builder.saw_end = true;
                    builder.node.ended_at_ns = Some(timestamp_ns);
                    builder.node.status = status;
                }
            }
            ProfileEventKind::CallFunction {
                thread_id,
                call_id,
                parent_call_id,
                function_id,
                call_site_source,
            } => {
                let thread_key = TraceThreadKey {
                    process_euid,
                    engine_id,
                    thread_id,
                };
                ensure_thread(&mut threads, thread_key);
                let key = TraceCallKey {
                    process_euid,
                    engine_id,
                    thread_id,
                    call_id,
                };
                let builder = ensure_call(&mut calls, key);
                if builder.saw_start {
                    diagnostics.push(diagnostic(
                        DiagnosticSeverity::Error,
                        ReconstructionDiagnosticCode::DuplicateCallStart,
                        format!(
                            "duplicate CallFunction for thread {} call {}",
                            thread_id.0, call_id.0
                        ),
                    ));
                } else {
                    builder.saw_start = true;
                    builder.node.started_at_ns = Some(timestamp_ns);
                    builder.node.parent_id = parent_call_id.map(|parent_call_id| {
                        call_node_id(&TraceCallKey {
                            process_euid,
                            engine_id,
                            thread_id,
                            call_id: parent_call_id,
                        })
                    });
                    builder.node.function_id = function_id;
                    builder.node.call_site_source = call_site_source;
                    if let Some(metadata) = metadata_by_id.get(&function_id) {
                        builder.node.function_name = Some(metadata.fqn.clone());
                        builder.node.function_origin =
                            Some(function_origin_from_metadata(metadata));
                        builder.node.callee_source = source_location_from_metadata(metadata);
                    }
                    if function_id.0 != 0
                        && !metadata_by_id.is_empty()
                        && builder.node.function_name.is_none()
                    {
                        diagnostics.push(diagnostic(
                            DiagnosticSeverity::Warning,
                            ReconstructionDiagnosticCode::UnknownFunctionId,
                            format!(
                                "CallFunction thread {} call {} references unknown function_id {}",
                                thread_id.0, call_id.0, function_id.0
                            ),
                        ));
                    }
                }
            }
            ProfileEventKind::EndFunction {
                thread_id,
                call_id,
                status,
            } => {
                let key = TraceCallKey {
                    process_euid,
                    engine_id,
                    thread_id,
                    call_id,
                };
                let builder = ensure_call(&mut calls, key);
                if builder.saw_end {
                    diagnostics.push(diagnostic(
                        DiagnosticSeverity::Error,
                        ReconstructionDiagnosticCode::DuplicateCallEnd,
                        format!(
                            "duplicate EndFunction for thread {} call {}",
                            thread_id.0, call_id.0
                        ),
                    ));
                } else {
                    builder.saw_end = true;
                    builder.node.ended_at_ns = Some(timestamp_ns);
                    builder.node.status = status;
                }
            }
        }
    }

    validate_links(&threads, &calls, &mut diagnostics);

    let mut calls_vec: Vec<CallNode> = calls.into_values().map(|builder| builder.node).collect();
    calls_vec.sort_by_key(|call| {
        (
            call.trace_key.thread_id.0,
            call.started_at_ns.unwrap_or(u64::MAX),
            call.trace_key.call_id.0,
            call.id,
        )
    });

    let call_ids_by_thread = calls_vec.iter().fold(
        HashMap::<ThreadNodeId, Vec<CallNodeId>>::new(),
        |mut by_thread, call| {
            by_thread.entry(call.thread_id).or_default().push(call.id);
            by_thread
        },
    );
    let mut threads_vec: Vec<ThreadNode> = threads
        .into_values()
        .map(|mut builder| {
            builder.node.call_node_ids = call_ids_by_thread
                .get(&builder.node.id)
                .cloned()
                .unwrap_or_default();
            builder.node
        })
        .collect();
    threads_vec.sort_by_key(|thread| {
        (
            thread.started_at_ns.unwrap_or(u64::MAX),
            thread.trace_key.thread_id.0,
            thread.id,
        )
    });
    diagnostics.sort_by_key(|diag| (diag.severity.rank(), diag.code as u8, diag.message.clone()));

    ReconstructedProfile {
        function_table,
        threads: threads_vec,
        calls: calls_vec,
        diagnostics,
    }
}

fn function_origin_from_metadata(metadata: &ProfileFunctionMetadata) -> FunctionOrigin {
    match metadata.kind.as_deref() {
        Some("bytecode") => FunctionOrigin::User,
        Some("sysop" | "native") => FunctionOrigin::Builtin,
        Some("companion") => FunctionOrigin::Companion,
        Some("internal") => FunctionOrigin::Internal,
        Some(_) | None => FunctionOrigin::Unknown,
    }
}

fn source_location_from_metadata(metadata: &ProfileFunctionMetadata) -> Option<SourceLocation> {
    if metadata.source_file.is_none()
        && metadata.span_start.is_none()
        && metadata.span_end.is_none()
    {
        return None;
    }
    Some(SourceLocation {
        file_path: metadata.source_file.clone(),
        file_id: None,
        line: 0,
        column: 0,
        end_line: None,
        end_column: None,
        start_offset: metadata.span_start,
        end_offset: metadata.span_end,
    })
}

fn ensure_thread(
    threads: &mut HashMap<TraceThreadKey, ThreadBuilder>,
    key: TraceThreadKey,
) -> &mut ThreadBuilder {
    threads.entry(key).or_insert_with(|| ThreadBuilder {
        node: ThreadNode {
            id: thread_node_id(&key),
            trace_key: key,
            trace_ref: key.thread_ref(),
            parent_thread_id: None,
            parent_call_node_id: None,
            name: None,
            started_at_ns: None,
            ended_at_ns: None,
            status: ThreadStatus::Running,
            call_node_ids: Vec::new(),
        },
        saw_start: false,
        saw_end: false,
    })
}

fn ensure_call(
    calls: &mut HashMap<TraceCallKey, CallBuilder>,
    key: TraceCallKey,
) -> &mut CallBuilder {
    calls.entry(key).or_insert_with(|| CallBuilder {
        node: CallNode {
            id: call_node_id(&key),
            trace_key: key,
            trace_ref: key.call_ref(),
            thread_id: thread_node_id(&key.thread_key()),
            parent_id: None,
            function_id: FunctionId(0),
            function_name: None,
            function_origin: None,
            callee_source: None,
            call_site_source: None,
            started_at_ns: None,
            ended_at_ns: None,
            status: CallStatus::Running,
            payload_ids: Vec::new(),
        },
        saw_start: false,
        saw_end: false,
    })
}

fn validate_links(
    threads: &HashMap<TraceThreadKey, ThreadBuilder>,
    calls: &HashMap<TraceCallKey, CallBuilder>,
    diagnostics: &mut Vec<ReconstructionDiagnostic>,
) {
    let thread_ids: HashSet<ThreadNodeId> =
        threads.values().map(|builder| builder.node.id).collect();
    let call_ids: HashSet<CallNodeId> = calls.values().map(|builder| builder.node.id).collect();

    for builder in threads.values() {
        if !builder.saw_start {
            diagnostics.push(diagnostic(
                DiagnosticSeverity::Error,
                ReconstructionDiagnosticCode::MissingThreadStart,
                format!(
                    "thread {} ended or received calls without StartThread",
                    builder.node.trace_key.thread_id.0
                ),
            ));
        }
        if !builder.saw_end {
            diagnostics.push(diagnostic(
                DiagnosticSeverity::Warning,
                ReconstructionDiagnosticCode::MissingThreadEnd,
                format!(
                    "thread {} has no EndThread",
                    builder.node.trace_key.thread_id.0
                ),
            ));
        }
        if let Some(parent_thread_id) = builder.node.parent_thread_id
            && !thread_ids.contains(&parent_thread_id)
        {
            diagnostics.push(diagnostic(
                DiagnosticSeverity::Error,
                ReconstructionDiagnosticCode::MissingParentThread,
                format!(
                    "thread {} references missing parent thread",
                    builder.node.trace_key.thread_id.0
                ),
            ));
        }
        if let Some(parent_call_node_id) = builder.node.parent_call_node_id
            && !call_ids.contains(&parent_call_node_id)
        {
            diagnostics.push(diagnostic(
                DiagnosticSeverity::Error,
                ReconstructionDiagnosticCode::MissingParentCall,
                format!(
                    "thread {} references missing parent call",
                    builder.node.trace_key.thread_id.0
                ),
            ));
        }
    }

    for builder in calls.values() {
        if !builder.saw_start {
            diagnostics.push(diagnostic(
                DiagnosticSeverity::Error,
                ReconstructionDiagnosticCode::MissingCallStart,
                format!(
                    "thread {} call {} ended without CallFunction",
                    builder.node.trace_key.thread_id.0, builder.node.trace_key.call_id.0
                ),
            ));
        }
        if !builder.saw_end {
            diagnostics.push(diagnostic(
                DiagnosticSeverity::Warning,
                ReconstructionDiagnosticCode::MissingCallEnd,
                format!(
                    "thread {} call {} has no EndFunction",
                    builder.node.trace_key.thread_id.0, builder.node.trace_key.call_id.0
                ),
            ));
        }
        if let Some(parent_id) = builder.node.parent_id
            && !call_ids.contains(&parent_id)
        {
            diagnostics.push(diagnostic(
                DiagnosticSeverity::Error,
                ReconstructionDiagnosticCode::MissingParentCall,
                format!(
                    "thread {} call {} references missing parent call",
                    builder.node.trace_key.thread_id.0, builder.node.trace_key.call_id.0
                ),
            ));
        }
    }
}

fn diagnostic(
    severity: DiagnosticSeverity,
    code: ReconstructionDiagnosticCode,
    message: String,
) -> ReconstructionDiagnostic {
    ReconstructionDiagnostic {
        severity,
        code,
        message,
    }
}

impl DiagnosticSeverity {
    fn rank(self) -> u8 {
        match self {
            Self::Error => 0,
            Self::Warning => 1,
            Self::Info => 2,
        }
    }
}

fn thread_node_id(key: &TraceThreadKey) -> ThreadNodeId {
    let mut hash = StableHasher::new(b"baml-thread-node");
    hash.write(&key.process_euid.0);
    hash.write_u64(key.engine_id.0);
    hash.write_u64(key.thread_id.0);
    ThreadNodeId(hash.finish())
}

pub(crate) fn call_node_id(key: &TraceCallKey) -> CallNodeId {
    let mut hash = StableHasher::new(b"baml-call-node");
    hash.write(&key.process_euid.0);
    hash.write_u64(key.engine_id.0);
    hash.write_u64(key.thread_id.0);
    hash.write_u64(key.call_id.0);
    CallNodeId(hash.finish())
}

struct StableHasher {
    state: u64,
}

impl StableHasher {
    fn new(domain: &[u8]) -> Self {
        let mut hasher = Self {
            state: 0xcbf2_9ce4_8422_2325,
        };
        hasher.write(domain);
        hasher
    }

    fn write(&mut self, bytes: &[u8]) {
        for byte in bytes {
            self.state ^= u64::from(*byte);
            self.state = self.state.wrapping_mul(0x0000_0100_0000_01b3);
        }
    }

    fn write_u64(&mut self, value: u64) {
        self.write(&value.to_le_bytes());
    }

    fn finish(self) -> u64 {
        self.state
    }
}

#[must_use]
pub fn profile_event_envelope_from_disk_event(
    source: ProfileEventSource,
    process_euid: ProcessEuid,
    engine_id: EngineId,
    event: &crate::prof::pb::DiskEventV1,
) -> Option<ProfileEventEnvelope> {
    normalize_disk_event(event.event.as_ref()?).map(|event| ProfileEventEnvelope {
        source,
        process_euid,
        engine_id,
        event,
    })
}

fn normalize_disk_event(event: &crate::prof::pb::disk_event_v1::Event) -> Option<ProfileEvent> {
    use crate::prof::pb;

    match event {
        pb::disk_event_v1::Event::StartThread(start) => Some(ProfileEvent {
            timestamp_ns: start.timestamp_ns,
            kind: ProfileEventKind::StartThread {
                thread_id: BexThreadId(start.thread_id),
                parent_thread_id: start.parent_thread_id.map(BexThreadId),
                parent_call_id: start.parent_call_id.map(BexCallId),
                name: start.name.clone(),
            },
        }),
        pb::disk_event_v1::Event::EndThread(end) => Some(ProfileEvent {
            timestamp_ns: end.timestamp_ns,
            kind: ProfileEventKind::EndThread {
                thread_id: BexThreadId(end.thread_id),
                status: match pb::ThreadEndStatus::try_from(end.status).ok() {
                    Some(pb::ThreadEndStatus::Cancelled) => ThreadStatus::Cancelled,
                    Some(pb::ThreadEndStatus::Errored) => ThreadStatus::Errored,
                    _ => ThreadStatus::Completed,
                },
            },
        }),
        pb::disk_event_v1::Event::CallFunction(call) => Some(ProfileEvent {
            timestamp_ns: call.timestamp_ns,
            kind: ProfileEventKind::CallFunction {
                thread_id: BexThreadId(call.thread_id),
                call_id: BexCallId(call.call_id),
                parent_call_id: call.parent_call_id.map(BexCallId),
                function_id: FunctionId(call.function_id),
                call_site_source: call_site_source_from_disk_call(call),
            },
        }),
        pb::disk_event_v1::Event::EndFunction(end) => Some(ProfileEvent {
            timestamp_ns: end.timestamp_ns,
            kind: ProfileEventKind::EndFunction {
                thread_id: BexThreadId(end.thread_id),
                call_id: BexCallId(end.call_id),
                status: match pb::FunctionEndStatus::try_from(end.status).ok() {
                    Some(pb::FunctionEndStatus::Errored) => CallStatus::Errored,
                    Some(pb::FunctionEndStatus::Cancelled) => CallStatus::Cancelled,
                    Some(pb::FunctionEndStatus::Exited) => CallStatus::Exited,
                    _ => CallStatus::Ok,
                },
            },
        }),
        pb::disk_event_v1::Event::SetFunctionId(_) | pb::disk_event_v1::Event::Heartbeat(_) => None,
    }
}

fn call_site_source_from_disk_call(call: &crate::prof::pb::CallFunction) -> Option<SourceLocation> {
    let file_id = call.call_site_file_id?;
    let start_offset = call.call_site_start_offset?;
    let end_offset = call.call_site_end_offset?;
    Some(SourceLocation {
        file_path: None,
        file_id: Some(u64::from(file_id)),
        line: call.call_site_line.unwrap_or(0),
        column: 0,
        end_line: None,
        end_column: None,
        start_offset: Some(start_offset),
        end_offset: Some(end_offset),
    })
}

pub mod bamlprof {
    use crate::{
        ids::{EngineId, FunctionId, ProcessEuid},
        prof::{pb, read::BamlprofContents},
        run::{
            ProfileEventEnvelope, ProfileEventSource, ProfileFunctionMetadata,
            ReconstructedProfile, RuntimeTarget, profile_event_envelope_from_disk_event,
            reconstruct_with_function_table,
        },
    };

    #[derive(Debug)]
    pub enum ImportError {
        InvalidProcessIdLength(usize),
    }

    impl std::fmt::Display for ImportError {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            match self {
                Self::InvalidProcessIdLength(length) => {
                    write!(
                        f,
                        "invalid .bamlprof process_id length: expected 16, got {length}"
                    )
                }
            }
        }
    }

    impl std::error::Error for ImportError {}

    pub fn reconstruct_bamlprof(
        contents: &BamlprofContents,
    ) -> Result<ReconstructedProfile, ImportError> {
        let events = normalized_events(contents)?;
        Ok(reconstruct_with_function_table(
            events,
            function_table(&contents.header),
        ))
    }

    pub fn normalized_events(
        contents: &BamlprofContents,
    ) -> Result<Vec<ProfileEventEnvelope>, ImportError> {
        let process_euid = process_euid(&contents.header)?;
        let engine_id = EngineId(contents.header.engine_id);
        Ok(contents
            .events
            .iter()
            .filter_map(|event| {
                profile_event_envelope_from_disk_event(
                    ProfileEventSource::Replay {
                        artifact_id: format!("{}:{}", hex_process_id(process_euid), engine_id.0),
                    },
                    process_euid,
                    engine_id,
                    event,
                )
            })
            .collect())
    }

    fn process_euid(header: &pb::EventFileHeaderV1) -> Result<ProcessEuid, ImportError> {
        let bytes: [u8; 16] = header
            .process_id
            .as_slice()
            .try_into()
            .map_err(|_| ImportError::InvalidProcessIdLength(header.process_id.len()))?;
        Ok(ProcessEuid(bytes))
    }

    pub(crate) fn function_table(header: &pb::EventFileHeaderV1) -> Vec<ProfileFunctionMetadata> {
        header
            .function_table
            .as_ref()
            .map(|table| {
                table
                    .functions
                    .iter()
                    .map(|function| ProfileFunctionMetadata {
                        function_id: FunctionId(function.function_id),
                        fqn: function.fqn.clone(),
                        source_file: (!function.source_file.is_empty())
                            .then(|| function.source_file.clone()),
                        span_start: Some(function.span_start),
                        span_end: Some(function.span_end),
                        kind: (!function.kind.is_empty()).then(|| function.kind.clone()),
                    })
                    .collect()
            })
            .unwrap_or_default()
    }

    fn hex_process_id(process_euid: ProcessEuid) -> String {
        let mut s = String::with_capacity(32);
        for b in process_euid.0 {
            use std::fmt::Write as _;
            let _ = write!(s, "{b:02x}");
        }
        s
    }

    #[allow(dead_code)]
    fn _target_marker() -> RuntimeTarget {
        RuntimeTarget::Replay
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn run_contract_keeps_identity_spaces_separate() {
        let boundary_id = test_boundary_id(7);
        let host_call_id = HostCallId::Native(sys_types::CallId(7));
        assert_eq!(boundary_id.as_bytes(), [7; 16]);
        assert_eq!(
            boundary_id.to_wire_string(),
            "baml_id_1_BwcHBwcHBwcHBwcHBwcHBw"
        );
        assert_eq!(
            BoundaryId::from_wire_str("baml_id_1_BwcHBwcHBwcHBwcHBwcHBw"),
            Some(boundary_id)
        );
        assert_eq!(BoundaryId::from_wire_str("7"), None);
        assert_eq!(host_call_id, HostCallId::Native(sys_types::CallId(7)));
    }

    #[test]
    fn run_kind_is_derived_and_visibility_scope_is_explicit() {
        let function = RunTarget::Function {
            function_name: "Extract".to_string(),
        };
        assert_eq!(function.kind(), RunKind::Function);
        assert_eq!(function.default_visibility(None), RunVisibility::History);

        let preview = RunTarget::Preview {
            parent_function_name: "Extract".to_string(),
            helper: "render_prompt".to_string(),
        };
        assert_eq!(preview.kind(), RunKind::Preview);
        assert_eq!(preview.default_visibility(None), RunVisibility::Hidden);
        assert_eq!(
            preview.default_visibility(Some("panel-1".to_string())),
            RunVisibility::Scoped {
                scope_id: "panel-1".to_string()
            }
        );
    }

    #[test]
    fn run_outcome_is_the_only_terminal_status_source() {
        let outcome = RunOutcome::Failed(RunError {
            class: RunErrorClass::Runtime,
            message: "boom".to_string(),
            details: None,
            value_ref: None,
        });
        assert_eq!(outcome.status(), RunStatus::Failed);
        assert!(outcome.status().is_terminal());
        assert!(!RunStatus::Running.is_terminal());
    }

    #[test]
    fn start_guard_cancellation_is_shared() {
        let guard = StartGuard::new();
        let clone = guard.clone();
        assert!(!guard.is_cancelled());
        clone.cancel();
        assert!(guard.is_cancelled());
    }

    fn envelope(event: ProfileEventKind, timestamp_ns: u64) -> ProfileEventEnvelope {
        ProfileEventEnvelope {
            source: ProfileEventSource::Live {
                target: RuntimeTarget::Native,
                source_id: "test".to_string(),
            },
            process_euid: ProcessEuid([1; 16]),
            engine_id: EngineId(2),
            event: ProfileEvent {
                timestamp_ns,
                kind: event,
            },
        }
    }

    fn request(function_name: &str) -> ExecutionRequest {
        ExecutionRequest {
            project_id: ProjectId("project".to_string()),
            project_generation: ProjectGeneration(1),
            target: RunTarget::Function {
                function_name: function_name.to_string(),
            },
            args_summary: None,
            options_summary: None,
        }
    }

    fn test_boundary_id(byte: u8) -> BoundaryId {
        BoundaryId::from_bytes([byte; 16])
    }

    fn create_test_run(
        store: &InMemoryRunStore,
        request: ExecutionRequest,
        request_id: RequestId,
    ) -> StartRunContext {
        store.create_run(BoundaryId::new_random(), request, request_id)
    }

    fn create_test_run_at(
        store: &InMemoryRunStore,
        request: ExecutionRequest,
        request_id: RequestId,
        time_anchor: RunTimeAnchor,
    ) -> StartRunContext {
        store.create_run_at(BoundaryId::new_random(), request, request_id, time_anchor)
    }

    fn root_call_ref(thread_id: u64, call_id: u64) -> CallRef {
        CallRef {
            process_euid: ProcessEuid([1; 16]),
            engine_id: EngineId(2),
            thread_id: BexThreadId(thread_id),
            call_id: BexCallId(call_id),
        }
    }

    fn source_location(file_id: u64, start_offset: u32, end_offset: u32) -> SourceLocation {
        SourceLocation {
            file_path: None,
            file_id: Some(file_id),
            line: 0,
            column: 0,
            end_line: None,
            end_column: None,
            start_offset: Some(start_offset),
            end_offset: Some(end_offset),
        }
    }

    fn incomplete_source_location(file_id: u64) -> SourceLocation {
        SourceLocation {
            file_path: None,
            file_id: Some(file_id),
            line: 0,
            column: 0,
            end_line: None,
            end_column: None,
            start_offset: None,
            end_offset: Some(10),
        }
    }

    fn run_call(
        raw_id: u64,
        function_id: u32,
        call_site_source: Option<SourceLocation>,
    ) -> CallNode {
        let trace_key = TraceCallKey {
            process_euid: ProcessEuid([1; 16]),
            engine_id: EngineId(2),
            thread_id: BexThreadId(1),
            call_id: BexCallId(raw_id),
        };
        CallNode {
            id: CallNodeId(raw_id),
            trace_key,
            trace_ref: trace_key.call_ref(),
            thread_id: ThreadNodeId(1),
            parent_id: None,
            function_id: FunctionId(function_id),
            function_name: Some("user.child".to_string()),
            function_origin: Some(FunctionOrigin::User),
            callee_source: None,
            call_site_source,
            started_at_ns: Some(raw_id),
            ended_at_ns: None,
            status: CallStatus::Running,
            payload_ids: Vec::new(),
        }
    }

    fn run_with_calls(calls: Vec<CallNode>) -> Run {
        let store = InMemoryRunStore::default();
        let start = create_test_run(&store, request("main"), RequestId(1));
        let mut run = store.snapshot(start.boundary_id).unwrap();
        run.calls = calls;
        run
    }

    #[test]
    fn graph_overlay_maps_same_callee_calls_by_distinct_call_site_spans() {
        let run = run_with_calls(vec![
            run_call(1, 2, Some(source_location(7, 20, 31))),
            run_call(2, 2, Some(source_location(7, 80, 91))),
        ]);

        let overlay = build_graph_runtime_overlay_from_cfg_spans(
            &run,
            &[
                CfgNodeSourceSpan {
                    cfg_node_id: CfgNodeId(101),
                    file_id: 7,
                    start_offset: 20,
                    end_offset: 31,
                },
                CfgNodeSourceSpan {
                    cfg_node_id: CfgNodeId(102),
                    file_id: 7,
                    start_offset: 80,
                    end_offset: 91,
                },
            ],
        );

        assert_eq!(overlay.project_generation, ProjectGeneration(1));
        assert_eq!(
            overlay.entries,
            vec![
                GraphRuntimeOverlayEntry {
                    cfg_node_id: CfgNodeId(101),
                    call_node_ids: vec![CallNodeId(1)],
                },
                GraphRuntimeOverlayEntry {
                    cfg_node_id: CfgNodeId(102),
                    call_node_ids: vec![CallNodeId(2)],
                },
            ]
        );
        assert!(overlay.unattached_call_node_ids.is_empty());
        assert!(overlay.diagnostics.is_empty());
    }

    #[test]
    fn graph_overlay_groups_repeated_call_site_executions_on_one_cfg_node() {
        let run = run_with_calls(vec![
            run_call(1, 2, Some(source_location(7, 20, 31))),
            run_call(2, 2, Some(source_location(7, 20, 31))),
        ]);

        let overlay = build_graph_runtime_overlay_from_cfg_spans(
            &run,
            &[CfgNodeSourceSpan {
                cfg_node_id: CfgNodeId(101),
                file_id: 7,
                start_offset: 20,
                end_offset: 31,
            }],
        );

        assert_eq!(
            overlay.entries,
            vec![GraphRuntimeOverlayEntry {
                cfg_node_id: CfgNodeId(101),
                call_node_ids: vec![CallNodeId(1), CallNodeId(2)],
            }]
        );
        assert!(overlay.unattached_call_node_ids.is_empty());
        assert!(overlay.diagnostics.is_empty());
    }

    #[test]
    fn graph_overlay_uses_single_smallest_containing_cfg_span() {
        let run = run_with_calls(vec![run_call(1, 2, Some(source_location(7, 32, 38)))]);

        let overlay = build_graph_runtime_overlay_from_cfg_spans(
            &run,
            &[
                CfgNodeSourceSpan {
                    cfg_node_id: CfgNodeId(100),
                    file_id: 7,
                    start_offset: 20,
                    end_offset: 60,
                },
                CfgNodeSourceSpan {
                    cfg_node_id: CfgNodeId(101),
                    file_id: 7,
                    start_offset: 30,
                    end_offset: 40,
                },
            ],
        );

        assert_eq!(
            overlay.entries,
            vec![GraphRuntimeOverlayEntry {
                cfg_node_id: CfgNodeId(101),
                call_node_ids: vec![CallNodeId(1)],
            }]
        );
        assert!(overlay.unattached_call_node_ids.is_empty());
        assert!(overlay.diagnostics.is_empty());
    }

    #[test]
    fn graph_overlay_leaves_missing_unmatched_and_ambiguous_calls_unattached() {
        let run = run_with_calls(vec![
            run_call(1, 2, None),
            run_call(2, 2, Some(incomplete_source_location(7))),
            run_call(3, 2, Some(source_location(8, 10, 20))),
            run_call(4, 2, Some(source_location(7, 30, 40))),
        ]);

        let overlay = build_graph_runtime_overlay_from_cfg_spans(
            &run,
            &[
                CfgNodeSourceSpan {
                    cfg_node_id: CfgNodeId(101),
                    file_id: 7,
                    start_offset: 30,
                    end_offset: 40,
                },
                CfgNodeSourceSpan {
                    cfg_node_id: CfgNodeId(102),
                    file_id: 7,
                    start_offset: 30,
                    end_offset: 40,
                },
            ],
        );

        assert!(overlay.entries.is_empty());
        assert_eq!(
            overlay.unattached_call_node_ids,
            vec![CallNodeId(1), CallNodeId(2), CallNodeId(3), CallNodeId(4)]
        );
        let codes = overlay
            .diagnostics
            .iter()
            .filter_map(|diagnostic| diagnostic.code.as_deref())
            .collect::<Vec<_>>();
        assert_eq!(
            codes,
            vec![
                "GraphOverlayCallSiteUnavailable",
                "GraphOverlayCallSiteIncomplete",
                "GraphOverlayCfgNodeUnavailable",
                "GraphOverlayCfgNodeAmbiguous",
            ]
        );
    }

    #[test]
    fn run_store_indexes_host_call_and_replays_retained_patches() {
        let store = InMemoryRunStore::new(RunRetentionPolicy {
            patch_window_capacity: 8,
            ..RunRetentionPolicy::default()
        });
        let start = create_test_run_at(
            &store,
            request("main"),
            RequestId(1),
            RunTimeAnchor {
                epoch_created_at_ms: 100,
                trace_zero_ns: 0,
            },
        );
        assert_eq!(
            store.snapshot(start.boundary_id).unwrap().status,
            RunStatus::Pending
        );

        let host_call_id = HostCallId::Native(sys_types::CallId(42));
        let patch = store
            .attach_host_call(start.boundary_id, host_call_id.clone())
            .expect("pending run transitions to running");
        assert_eq!(patch.cursor, RunCursor(1));
        assert_eq!(
            store.boundary_id_for_host_call(&host_call_id),
            Some(start.boundary_id)
        );
        assert_eq!(
            store.snapshot(start.boundary_id).unwrap().status,
            RunStatus::Running
        );

        let RunSubscription::Snapshot { snapshot, patches } =
            store.subscribe(start.boundary_id, Some(RunCursor(0)))
        else {
            panic!("expected snapshot plus retained patches");
        };
        assert_eq!(snapshot.boundary_id, start.boundary_id);
        assert_eq!(patches, vec![patch]);
    }

    #[test]
    fn list_runs_filters_by_project_generation_kind_and_touched_function() {
        let store = InMemoryRunStore::default();
        let main = create_test_run_at(
            &store,
            request("main"),
            RequestId(1),
            RunTimeAnchor {
                epoch_created_at_ms: 100,
                trace_zero_ns: 0,
            },
        );
        let other = create_test_run_at(
            &store,
            request("other"),
            RequestId(2),
            RunTimeAnchor {
                epoch_created_at_ms: 200,
                trace_zero_ns: 0,
            },
        );

        let all = store.list_runs(&RunFilter::default());
        assert_eq!(
            all.iter()
                .map(|summary| summary.boundary_id)
                .collect::<Vec<_>>(),
            vec![other.boundary_id, main.boundary_id]
        );

        let summaries = store.list_runs(&RunFilter {
            project_id: Some(ProjectId("project".to_string())),
            project_generation: Some(ProjectGeneration(1)),
            kinds: vec![RunKind::Function],
            call_tree_contains_function: Some("main".to_string()),
            visibility: RunVisibilityFilter::HistoryOnly,
            ..RunFilter::default()
        });

        assert_eq!(summaries.len(), 1);
        assert_eq!(summaries[0].boundary_id, main.boundary_id);
        assert_eq!(summaries[0].touched_functions, vec!["main"]);

        let wrong_generation = store.list_runs(&RunFilter {
            project_id: Some(ProjectId("project".to_string())),
            project_generation: Some(ProjectGeneration(2)),
            call_tree_contains_function: Some("main".to_string()),
            visibility: RunVisibilityFilter::HistoryOnly,
            ..RunFilter::default()
        });
        assert!(wrong_generation.is_empty());
    }

    #[test]
    fn live_reconstruction_uses_registered_engine_function_metadata() {
        const ENGINE: u64 = 90_001;
        let _ = crate::prof::metadata::remove_engine_metadata(ENGINE);
        crate::prof::register_engine_metadata(
            ENGINE,
            crate::prof::EngineProfileMetadata {
                program_id: "program".to_string(),
                source_snapshot_id: Some("snapshot".to_string()),
                revision_id: Some("revision".to_string()),
                functions: vec![
                    crate::prof::FunctionMetaEntry {
                        function_id: 1,
                        fqn: "user.main".to_string(),
                        source_file: "main.baml".to_string(),
                        span_start: 1,
                        span_end: 10,
                        kind: "bytecode".to_string(),
                        definition_key: None,
                        owner_type: None,
                        parent_function: None,
                        lambda_path: None,
                        package_name: None,
                        namespace: Vec::new(),
                    },
                    crate::prof::FunctionMetaEntry {
                        function_id: 2,
                        fqn: "user.child".to_string(),
                        source_file: "main.baml".to_string(),
                        span_start: 11,
                        span_end: 20,
                        kind: "bytecode".to_string(),
                        definition_key: None,
                        owner_type: None,
                        parent_function: None,
                        lambda_path: None,
                        package_name: None,
                        namespace: Vec::new(),
                    },
                ],
            },
        );

        let store = InMemoryRunStore::default();
        let start = create_test_run(&store, request("main"), RequestId(1));
        let root = CallRef {
            process_euid: ProcessEuid([9; 16]),
            engine_id: EngineId(ENGINE),
            thread_id: BexThreadId(1),
            call_id: BexCallId(1),
        };
        assert!(matches!(
            store.attach_root_trace(start.boundary_id, root),
            AttachRootTraceResult::Attached { .. }
        ));

        let live = |kind, timestamp_ns| ProfileEventEnvelope {
            source: ProfileEventSource::Live {
                target: RuntimeTarget::Native,
                source_id: "test".to_string(),
            },
            process_euid: root.process_euid,
            engine_id: root.engine_id,
            event: ProfileEvent { timestamp_ns, kind },
        };

        for event in [
            live(
                ProfileEventKind::StartThread {
                    thread_id: BexThreadId(1),
                    parent_thread_id: None,
                    parent_call_id: None,
                    name: None,
                },
                10,
            ),
            live(
                ProfileEventKind::CallFunction {
                    thread_id: BexThreadId(1),
                    call_id: BexCallId(1),
                    parent_call_id: None,
                    function_id: FunctionId(1),
                    call_site_source: None,
                },
                20,
            ),
            live(
                ProfileEventKind::CallFunction {
                    thread_id: BexThreadId(1),
                    call_id: BexCallId(2),
                    parent_call_id: Some(BexCallId(1)),
                    function_id: FunctionId(2),
                    call_site_source: None,
                },
                30,
            ),
        ] {
            store.ingest_profile_event(event);
        }

        let snapshot = store.snapshot(start.boundary_id).unwrap();
        let child = snapshot
            .calls
            .iter()
            .find(|call| call.function_name.as_deref() == Some("user.child"))
            .expect("child call reconstructed with metadata name");
        assert_eq!(child.function_origin, Some(FunctionOrigin::User));
        assert_eq!(
            child.callee_source,
            Some(SourceLocation {
                file_path: Some("main.baml".to_string()),
                file_id: None,
                line: 0,
                column: 0,
                end_line: None,
                end_column: None,
                start_offset: Some(11),
                end_offset: Some(20),
            })
        );

        let summaries = store.list_runs(&RunFilter {
            call_tree_contains_function: Some("user.child".to_string()),
            visibility: RunVisibilityFilter::HistoryOnly,
            ..RunFilter::default()
        });
        assert_eq!(summaries.len(), 1);
        assert!(
            summaries[0]
                .touched_functions
                .contains(&"user.child".to_string())
        );

        let _ = crate::prof::metadata::remove_engine_metadata(ENGINE);
        store.ingest_profile_event(live(
            ProfileEventKind::EndFunction {
                thread_id: BexThreadId(1),
                call_id: BexCallId(2),
                status: CallStatus::Ok,
            },
            40,
        ));
        let after_metadata_removal = store.snapshot(start.boundary_id).unwrap();
        let child = after_metadata_removal
            .calls
            .iter()
            .find(|call| call.function_name.as_deref() == Some("user.child"))
            .expect("run keeps function metadata after global registry cleanup");
        assert_eq!(child.function_origin, Some(FunctionOrigin::User));
    }

    #[test]
    fn run_store_pre_host_cancel_trips_start_guard_and_seals_run() {
        let store = InMemoryRunStore::default();
        let start = create_test_run(&store, request("main"), RequestId(1));
        let effect = store.cancel_run(start.boundary_id, 500, Some("user".to_string()));

        let CancelRunEffect::CancelledBeforeHost { patch } = effect else {
            panic!("expected pre-host cancellation");
        };
        assert!(start.start_guard.is_cancelled());
        assert!(matches!(
            patch.changes.as_slice(),
            [
                RunPatchChange::SetStatus(RunStatus::Cancelled),
                RunPatchChange::Complete(RunOutcome::Cancelled(_))
            ]
        ));
        let snapshot = store.snapshot(start.boundary_id).unwrap();
        assert_eq!(snapshot.status, RunStatus::Cancelled);
        assert_eq!(snapshot.root_call_node_id, None);
    }

    #[test]
    fn run_store_cancel_after_host_returns_adapter_owned_host_id() {
        let store = InMemoryRunStore::default();
        let start = create_test_run(&store, request("main"), RequestId(1));
        let host_call_id = HostCallId::Native(sys_types::CallId(42));
        store.attach_host_call(start.boundary_id, host_call_id.clone());

        let effect = store.cancel_run(start.boundary_id, 500, None);
        let CancelRunEffect::CancelHostCall {
            host_call_id: routed,
            patch,
        } = effect
        else {
            panic!("expected host-routed cancellation");
        };
        assert_eq!(routed, host_call_id);
        assert_eq!(
            store.snapshot(start.boundary_id).unwrap().status,
            RunStatus::Cancelling
        );
        assert_eq!(
            patch.changes,
            vec![RunPatchChange::SetStatus(RunStatus::Cancelling)]
        );
    }

    #[test]
    fn run_store_routes_payloads_through_host_call_index() {
        let store = InMemoryRunStore::default();
        let start_a = create_test_run(&store, request("first"), RequestId(1));
        let start_b = create_test_run(&store, request("second"), RequestId(2));
        let host_a = HostCallId::Native(sys_types::CallId(11));
        let host_b = HostCallId::Native(sys_types::CallId(12));
        store.attach_host_call(start_a.boundary_id, host_a);
        store.attach_host_call(start_b.boundary_id, host_b.clone());

        let patch = store
            .ingest_fetch_started(
                &host_b,
                9,
                "POST".to_string(),
                "https://example.test".to_string(),
                vec![HeaderObservation {
                    name: "authorization".to_string(),
                    value_redacted: true,
                    value: None,
                }],
                Some(18),
            )
            .expect("host call maps to second run");
        assert!(matches!(
            patch.changes.as_slice(),
            [RunPatchChange::UpsertPayload(PayloadEvent {
                kind: PayloadKind::FetchStarted(_),
                ..
            })]
        ));

        let first = store.snapshot(start_a.boundary_id).unwrap();
        let second = store.snapshot(start_b.boundary_id).unwrap();
        assert!(first.payloads.is_empty());
        assert_eq!(second.payloads.len(), 1);
        assert!(second.payloads[0].redaction.value_redacted);
        assert_eq!(
            store.ingest_fetch_updated(
                &HostCallId::Wasm(12),
                9,
                Some(200),
                Some(7),
                Vec::new(),
                None,
                None,
            ),
            None
        );
    }

    #[test]
    fn run_store_input_and_env_requests_drive_wait_status() {
        let store = InMemoryRunStore::default();
        let start = create_test_run(&store, request("main"), RequestId(1));
        let host = HostCallId::Native(sys_types::CallId(42));
        store.attach_host_call(start.boundary_id, host.clone());

        store
            .ingest_input_requested(&host, 1, Some("name?".to_string()))
            .unwrap();
        assert_eq!(
            store.snapshot(start.boundary_id).unwrap().status,
            RunStatus::WaitingForInput
        );

        store
            .ingest_env_requested(&host, 2, "API_KEY".to_string())
            .unwrap();
        store
            .ingest_input_resolved(&host, 1, RunRequestState::Resolved)
            .unwrap();
        assert_eq!(
            store.snapshot(start.boundary_id).unwrap().status,
            RunStatus::WaitingForEnv
        );

        store
            .ingest_env_resolved(
                &host,
                2,
                "API_KEY".to_string(),
                EnvResolutionStatus::DeclinedMissing,
                None,
            )
            .unwrap();
        let snapshot = store.snapshot(start.boundary_id).unwrap();
        assert_eq!(snapshot.status, RunStatus::Running);
        assert_eq!(snapshot.payloads.len(), 4);
        assert!(matches!(
            &snapshot.payloads[3].kind,
            PayloadKind::EnvResolved(EnvResolved {
                status: EnvResolutionStatus::DeclinedMissing,
                value_redacted: true,
                display_value: None,
                ..
            })
        ));
    }

    #[test]
    fn run_store_request_commands_are_idempotent_and_run_scoped() {
        let store = InMemoryRunStore::default();
        let start_a = create_test_run(&store, request("first"), RequestId(1));
        let start_b = create_test_run(&store, request("second"), RequestId(2));
        let host_a = HostCallId::Native(sys_types::CallId(41));
        let host_b = HostCallId::Native(sys_types::CallId(42));
        store.attach_host_call(start_a.boundary_id, host_a.clone());
        store.attach_host_call(start_b.boundary_id, host_b);
        store
            .ingest_input_requested(&host_a, 7, Some("name?".to_string()))
            .unwrap();

        assert_eq!(
            store.input_request_outcome_for_run(start_b.boundary_id, 7),
            RequestCommandOutcome::Missing
        );

        let result =
            store.resolve_input_request_for_run(start_a.boundary_id, 7, RunRequestState::Resolved);
        assert_eq!(result.outcome, RequestCommandOutcome::Accepted);
        assert_eq!(result.host_call_id, Some(host_a));
        assert!(result.patch.is_some());
        assert_eq!(
            store.input_request_outcome_for_run(start_a.boundary_id, 7),
            RequestCommandOutcome::AlreadyResolved
        );
        let duplicate =
            store.resolve_input_request_for_run(start_a.boundary_id, 7, RunRequestState::Resolved);
        assert_eq!(duplicate.outcome, RequestCommandOutcome::AlreadyResolved);
        assert!(duplicate.patch.is_none());
    }

    #[test]
    fn run_store_env_request_commands_resolve_with_redacted_payloads() {
        let store = InMemoryRunStore::default();
        let start = create_test_run(&store, request("main"), RequestId(1));
        let host = HostCallId::Native(sys_types::CallId(42));
        store.attach_host_call(start.boundary_id, host.clone());
        store
            .ingest_env_requested(&host, 9, "API_KEY".to_string())
            .unwrap();

        let result = store.resolve_env_request_for_run(
            start.boundary_id,
            9,
            EnvResolutionStatus::ResolvedFromUser,
            None,
        );
        assert_eq!(result.outcome, RequestCommandOutcome::Accepted);
        assert_eq!(result.host_call_id, Some(host));

        let snapshot = store.snapshot(start.boundary_id).unwrap();
        assert_eq!(snapshot.status, RunStatus::Running);
        assert!(matches!(
            snapshot.payloads.last().map(|payload| &payload.kind),
            Some(PayloadKind::EnvResolved(EnvResolved {
                request_id: 9,
                key,
                status: EnvResolutionStatus::ResolvedFromUser,
                state: RunRequestState::Resolved,
                value_redacted: true,
                display_value: None,
            })) if key == "API_KEY"
        ));

        let duplicate = store.resolve_env_request_for_run(
            start.boundary_id,
            9,
            EnvResolutionStatus::DeclinedMissing,
            None,
        );
        assert_eq!(duplicate.outcome, RequestCommandOutcome::AlreadyResolved);
        assert!(duplicate.patch.is_none());
    }

    #[test]
    fn run_store_projects_root_value_refs_to_wire() {
        use crate::value::{ValueCodec, ValueRef};

        let store = InMemoryRunStore::default();
        let success = create_test_run(&store, request("success"), RequestId(1));
        let output_ref = ValueRef::available("value_output", ValueCodec::BamlOutboundValue, 4, 4);
        store.complete_run(
            success.boundary_id,
            RunOutcome::Succeeded(RunResult {
                value_ref: Some(output_ref),
                renderer_hint: None,
                supporting_payload_ids: Vec::new(),
            }),
            200,
        );
        let success_wire = run_to_wire(&store.snapshot(success.boundary_id).unwrap());
        let result = success_wire["result"].as_object().unwrap();
        assert!(!result.contains_key("value"));
        assert_eq!(result["valueRef"]["id"], "value_output");
        assert_eq!(result["valueRef"]["codec"], "bamlOutboundValue");
        assert_eq!(result["valueRef"]["availability"], "available");

        let failure = create_test_run(&store, request("failure"), RequestId(2));
        let error_ref = ValueRef::available("value_error", ValueCodec::BamlOutboundValue, 2, 2);
        store.complete_run(
            failure.boundary_id,
            RunOutcome::Failed(RunError {
                class: RunErrorClass::Runtime,
                message: "boom".to_string(),
                details: None,
                value_ref: Some(error_ref),
            }),
            300,
        );
        let failure_wire = run_to_wire(&store.snapshot(failure.boundary_id).unwrap());
        assert_eq!(failure_wire["error"]["valueRef"]["id"], "value_error");

        let inputs = create_test_run(&store, request("inputs"), RequestId(3));
        let input_ref = ValueRef::available("value_inputs", ValueCodec::BamlOutboundValue, 8, 8);
        store
            .ingest_root_input_value_ref(inputs.boundary_id, Some(input_ref))
            .unwrap();
        assert_eq!(
            store.snapshot(inputs.boundary_id).unwrap().payloads[0].call_node_id,
            None
        );
        for event in [
            envelope(
                ProfileEventKind::StartThread {
                    thread_id: BexThreadId(1),
                    parent_thread_id: None,
                    parent_call_id: None,
                    name: None,
                },
                10,
            ),
            envelope(
                ProfileEventKind::CallFunction {
                    thread_id: BexThreadId(1),
                    call_id: BexCallId(1),
                    parent_call_id: None,
                    function_id: FunctionId(1),
                    call_site_source: None,
                },
                20,
            ),
        ] {
            store.ingest_profile_event(event);
        }
        let AttachRootTraceResult::Attached { patches } =
            store.attach_root_trace(inputs.boundary_id, root_call_ref(1, 1))
        else {
            panic!("root trace should attach");
        };
        assert!(patches.iter().any(|patch| {
            patch.changes.iter().any(|change| {
                matches!(
                    change,
                    RunPatchChange::UpsertPayload(PayloadEvent {
                        call_node_id: Some(_),
                        kind: PayloadKind::CapturedValue(CapturedValuePayload {
                            role: CapturedValueRole::RootInput,
                            ..
                        }),
                        ..
                    })
                )
            })
        }));
        let inputs_wire = run_to_wire(&store.snapshot(inputs.boundary_id).unwrap());
        let payload = &inputs_wire["payloads"][0];
        assert!(payload["callNodeId"].is_string());
        assert_eq!(payload["kind"]["type"], "capturedValue");
        assert_eq!(payload["kind"]["role"], "rootInput");
        assert_eq!(payload["kind"]["label"], "inputs");
        assert_eq!(payload["kind"]["valueRef"]["id"], "value_inputs");
    }

    #[test]
    fn run_store_backfills_identified_log_payloads_to_reconstructed_calls() {
        use crate::value::{ValueCodec, ValueRef};

        let store = InMemoryRunStore::default();
        let start = create_test_run(&store, request("logs"), RequestId(1));
        let logged_call = TraceCallKey {
            process_euid: ProcessEuid([1; 16]),
            engine_id: EngineId(2),
            thread_id: BexThreadId(1),
            call_id: BexCallId(2),
        };
        let source = SourceLocation {
            file_path: None,
            file_id: Some(9),
            line: 12,
            column: 3,
            end_line: None,
            end_column: None,
            start_offset: Some(30),
            end_offset: Some(44),
        };
        let value_ref = ValueRef::available("value_log", ValueCodec::BamlOutboundValue, 6, 6);
        let initial_patch = store
            .ingest_log_value_ref(
                start.boundary_id,
                logged_call,
                Some("warn".to_string()),
                "watch this".to_string(),
                Some(source.clone()),
                Some(value_ref),
            )
            .expect("run should exist");

        assert!(initial_patch.changes.iter().any(|change| {
            matches!(
                change,
                RunPatchChange::UpsertPayload(PayloadEvent {
                    call_node_id: None,
                    kind: PayloadKind::Log(LogPayload {
                        trace_call: Some(_),
                        ..
                    }),
                    ..
                })
            )
        }));

        for event in [
            envelope(
                ProfileEventKind::StartThread {
                    thread_id: BexThreadId(1),
                    parent_thread_id: None,
                    parent_call_id: None,
                    name: None,
                },
                10,
            ),
            envelope(
                ProfileEventKind::CallFunction {
                    thread_id: BexThreadId(1),
                    call_id: BexCallId(1),
                    parent_call_id: None,
                    function_id: FunctionId(1),
                    call_site_source: None,
                },
                20,
            ),
            envelope(
                ProfileEventKind::CallFunction {
                    thread_id: BexThreadId(1),
                    call_id: BexCallId(2),
                    parent_call_id: Some(BexCallId(1)),
                    function_id: FunctionId(2),
                    call_site_source: Some(source),
                },
                30,
            ),
        ] {
            store.ingest_profile_event(event);
        }
        let AttachRootTraceResult::Attached { patches } =
            store.attach_root_trace(start.boundary_id, root_call_ref(1, 1))
        else {
            panic!("root trace should attach");
        };

        let expected_call_id = call_node_id(&logged_call);
        assert!(patches.iter().any(|patch| {
            patch.changes.iter().any(|change| {
                matches!(
                    change,
                    RunPatchChange::UpsertPayload(PayloadEvent {
                        call_node_id: Some(call_id),
                        kind: PayloadKind::Log(LogPayload { .. }),
                        ..
                    }) if *call_id == expected_call_id
                )
            })
        }));
        let snapshot = store.snapshot(start.boundary_id).unwrap();
        let payload = snapshot
            .payloads
            .iter()
            .find(|payload| matches!(payload.kind, PayloadKind::Log(_)))
            .expect("log payload");
        assert_eq!(payload.call_node_id, Some(expected_call_id));
        let child = snapshot
            .calls
            .iter()
            .find(|call| call.id == expected_call_id)
            .expect("logged call");
        assert_eq!(child.payload_ids, vec![payload.id]);

        let wire = run_to_wire(&snapshot);
        let payload = &wire["payloads"][0];
        assert_eq!(
            payload["callNodeId"],
            format!("call_node_{}", expected_call_id.get())
        );
        assert_eq!(payload["kind"]["type"], "log");
        assert_eq!(payload["kind"]["level"], "warn");
        assert_eq!(payload["kind"]["message"], "watch this");
        assert_eq!(payload["kind"]["source"]["line"], 12);
        assert_eq!(payload["kind"]["valueRef"]["id"], "value_log");
    }

    #[test]
    fn run_store_backfills_call_value_payloads_to_reconstructed_calls() {
        use crate::value::{ValueCodec, ValueRef};

        let store = InMemoryRunStore::default();
        let start = create_test_run(&store, request("call-values"), RequestId(1));
        let input_call = TraceCallKey {
            process_euid: ProcessEuid([1; 16]),
            engine_id: EngineId(2),
            thread_id: BexThreadId(1),
            call_id: BexCallId(2),
        };
        let output_call = TraceCallKey {
            process_euid: ProcessEuid([1; 16]),
            engine_id: EngineId(2),
            thread_id: BexThreadId(1),
            call_id: BexCallId(3),
        };
        let error_call = TraceCallKey {
            process_euid: ProcessEuid([1; 16]),
            engine_id: EngineId(2),
            thread_id: BexThreadId(1),
            call_id: BexCallId(4),
        };
        let input_ref = ValueRef::available("value_input", ValueCodec::BamlOutboundValue, 8, 8);
        let output_ref = ValueRef::available("value_output", ValueCodec::BamlOutboundValue, 6, 6);
        let error_ref = ValueRef::available("value_error", ValueCodec::BamlOutboundValue, 4, 4);

        let input_patch = store
            .ingest_call_value_ref(
                start.boundary_id,
                input_call,
                CapturedValueRole::CallInput,
                Some("inputs".to_string()),
                Some(input_ref),
            )
            .expect("run should exist");
        let output_patch = store
            .ingest_call_value_ref(
                start.boundary_id,
                output_call,
                CapturedValueRole::CallOutput,
                Some("output".to_string()),
                Some(output_ref),
            )
            .expect("run should exist");
        let error_patch = store
            .ingest_call_value_ref(
                start.boundary_id,
                error_call,
                CapturedValueRole::CallError,
                Some("error".to_string()),
                Some(error_ref),
            )
            .expect("run should exist");

        for patch in [input_patch, output_patch, error_patch] {
            assert!(patch.changes.iter().any(|change| {
                matches!(
                    change,
                    RunPatchChange::UpsertPayload(PayloadEvent {
                        call_node_id: None,
                        kind: PayloadKind::CapturedValue(CapturedValuePayload {
                            trace_call: Some(_),
                            ..
                        }),
                        ..
                    })
                )
            }));
        }

        for event in [
            envelope(
                ProfileEventKind::StartThread {
                    thread_id: BexThreadId(1),
                    parent_thread_id: None,
                    parent_call_id: None,
                    name: None,
                },
                10,
            ),
            envelope(
                ProfileEventKind::CallFunction {
                    thread_id: BexThreadId(1),
                    call_id: BexCallId(1),
                    parent_call_id: None,
                    function_id: FunctionId(1),
                    call_site_source: None,
                },
                20,
            ),
            envelope(
                ProfileEventKind::CallFunction {
                    thread_id: BexThreadId(1),
                    call_id: BexCallId(2),
                    parent_call_id: Some(BexCallId(1)),
                    function_id: FunctionId(2),
                    call_site_source: None,
                },
                30,
            ),
            envelope(
                ProfileEventKind::CallFunction {
                    thread_id: BexThreadId(1),
                    call_id: BexCallId(3),
                    parent_call_id: Some(BexCallId(1)),
                    function_id: FunctionId(3),
                    call_site_source: None,
                },
                40,
            ),
            envelope(
                ProfileEventKind::CallFunction {
                    thread_id: BexThreadId(1),
                    call_id: BexCallId(4),
                    parent_call_id: Some(BexCallId(1)),
                    function_id: FunctionId(4),
                    call_site_source: None,
                },
                50,
            ),
        ] {
            store.ingest_profile_event(event);
        }
        let AttachRootTraceResult::Attached { patches } =
            store.attach_root_trace(start.boundary_id, root_call_ref(1, 1))
        else {
            panic!("root trace should attach");
        };

        let input_call_id = call_node_id(&input_call);
        let output_call_id = call_node_id(&output_call);
        let error_call_id = call_node_id(&error_call);
        for expected_call_id in [input_call_id, output_call_id, error_call_id] {
            assert!(patches.iter().any(|patch| {
                patch.changes.iter().any(|change| {
                    matches!(
                        change,
                        RunPatchChange::UpsertPayload(PayloadEvent {
                            call_node_id: Some(call_id),
                            kind: PayloadKind::CapturedValue(CapturedValuePayload { .. }),
                            ..
                        }) if *call_id == expected_call_id
                    )
                })
            }));
        }

        let snapshot = store.snapshot(start.boundary_id).unwrap();
        let input_payload = snapshot
            .payloads
            .iter()
            .find(|payload| {
                matches!(
                    &payload.kind,
                    PayloadKind::CapturedValue(CapturedValuePayload {
                        role: CapturedValueRole::CallInput,
                        ..
                    })
                )
            })
            .expect("input payload");
        assert_eq!(input_payload.call_node_id, Some(input_call_id));
        let output_payload = snapshot
            .payloads
            .iter()
            .find(|payload| {
                matches!(
                    &payload.kind,
                    PayloadKind::CapturedValue(CapturedValuePayload {
                        role: CapturedValueRole::CallOutput,
                        ..
                    })
                )
            })
            .expect("output payload");
        assert_eq!(output_payload.call_node_id, Some(output_call_id));
        let error_payload = snapshot
            .payloads
            .iter()
            .find(|payload| {
                matches!(
                    &payload.kind,
                    PayloadKind::CapturedValue(CapturedValuePayload {
                        role: CapturedValueRole::CallError,
                        ..
                    })
                )
            })
            .expect("error payload");
        assert_eq!(error_payload.call_node_id, Some(error_call_id));

        for (expected_call_id, expected_payload) in [
            (input_call_id, input_payload),
            (output_call_id, output_payload),
            (error_call_id, error_payload),
        ] {
            let call = snapshot
                .calls
                .iter()
                .find(|call| call.id == expected_call_id)
                .expect("call node");
            assert_eq!(call.payload_ids, vec![expected_payload.id]);
        }

        let wire = run_to_wire(&snapshot);
        let input_wire = wire["payloads"]
            .as_array()
            .unwrap()
            .iter()
            .find(|payload| payload["kind"]["role"] == "callInput")
            .expect("call input wire payload");
        assert_eq!(input_wire["kind"]["label"], "inputs");
        assert_eq!(input_wire["kind"]["valueRef"]["id"], "value_input");
        let output_wire = wire["payloads"]
            .as_array()
            .unwrap()
            .iter()
            .find(|payload| payload["kind"]["role"] == "callOutput")
            .expect("call output wire payload");
        assert_eq!(output_wire["kind"]["label"], "output");
        assert_eq!(output_wire["kind"]["valueRef"]["id"], "value_output");
        let error_wire = wire["payloads"]
            .as_array()
            .unwrap()
            .iter()
            .find(|payload| payload["kind"]["role"] == "callError")
            .expect("call error wire payload");
        assert_eq!(error_wire["kind"]["label"], "error");
        assert_eq!(error_wire["kind"]["valueRef"]["id"], "value_error");
    }

    #[test]
    fn run_store_request_commands_report_terminal_and_cancelled_state() {
        let store = InMemoryRunStore::default();
        let start_terminal = create_test_run(&store, request("main"), RequestId(1));
        store.complete_run(
            start_terminal.boundary_id,
            RunOutcome::Succeeded(RunResult {
                value_ref: None,
                renderer_hint: None,
                supporting_payload_ids: Vec::new(),
            }),
            200,
        );
        assert_eq!(
            store.input_request_outcome_for_run(start_terminal.boundary_id, 77),
            RequestCommandOutcome::AlreadyTerminal
        );

        let start_cancelling = create_test_run(&store, request("cancel_me"), RequestId(2));
        let host = HostCallId::Native(sys_types::CallId(42));
        store.attach_host_call(start_cancelling.boundary_id, host.clone());
        store
            .ingest_input_requested(&host, 88, Some("name?".to_string()))
            .unwrap();
        store
            .ingest_env_requested(&host, 89, "API_KEY".to_string())
            .unwrap();
        let effect = store.cancel_run(start_cancelling.boundary_id, 300, None);
        let CancelRunEffect::CancelHostCall { patch, .. } = effect else {
            panic!("expected host-routed cancellation");
        };
        assert!(patch.changes.iter().any(|change| matches!(
            change,
            RunPatchChange::UpsertPayload(PayloadEvent {
                kind: PayloadKind::InputResolved(InputResolved {
                    request_id: 88,
                    state: RunRequestState::Cancelled,
                }),
                ..
            })
        )));
        assert!(patch.changes.iter().any(|change| matches!(
            change,
            RunPatchChange::UpsertPayload(PayloadEvent {
                kind: PayloadKind::EnvResolved(EnvResolved {
                    request_id: 89,
                    state: RunRequestState::Cancelled,
                    ..
                }),
                ..
            })
        )));
        assert_eq!(
            store.env_request_outcome_for_run(start_cancelling.boundary_id, 89),
            RequestCommandOutcome::Cancelled
        );
        assert_eq!(
            store.input_request_outcome_for_run(start_cancelling.boundary_id, 88),
            RequestCommandOutcome::Cancelled
        );
    }

    #[test]
    fn run_store_retained_cursor_reports_compacted_gap() {
        let store = InMemoryRunStore::new(RunRetentionPolicy {
            patch_window_capacity: 1,
            ..RunRetentionPolicy::default()
        });
        let start = create_test_run(&store, request("main"), RequestId(1));
        store.attach_host_call(start.boundary_id, HostCallId::Native(sys_types::CallId(1)));
        store.complete_run(
            start.boundary_id,
            RunOutcome::Succeeded(RunResult {
                value_ref: None,
                renderer_hint: None,
                supporting_payload_ids: Vec::new(),
            }),
            200,
        );

        let RunSubscription::CursorExpired { reason, .. } =
            store.subscribe(start.boundary_id, Some(RunCursor(0)))
        else {
            panic!("old cursor should be outside the retained patch window");
        };
        assert_eq!(reason, RunCursorExpiredReason::Compacted);
    }

    #[test]
    fn run_store_root_trace_claims_only_connected_profile_component() {
        let store = InMemoryRunStore::default();
        let start = create_test_run(&store, request("main"), RequestId(1));
        let AttachRootTraceResult::Attached { patches } =
            store.attach_root_trace(start.boundary_id, root_call_ref(1, 1))
        else {
            panic!("root trace should attach");
        };
        assert!(patches.is_empty(), "no events have arrived yet");

        for event in [
            envelope(
                ProfileEventKind::StartThread {
                    thread_id: BexThreadId(1),
                    parent_thread_id: None,
                    parent_call_id: None,
                    name: None,
                },
                10,
            ),
            envelope(
                ProfileEventKind::CallFunction {
                    thread_id: BexThreadId(1),
                    call_id: BexCallId(1),
                    parent_call_id: None,
                    function_id: FunctionId(1),
                    call_site_source: None,
                },
                20,
            ),
            envelope(
                ProfileEventKind::CallFunction {
                    thread_id: BexThreadId(1),
                    call_id: BexCallId(2),
                    parent_call_id: Some(BexCallId(1)),
                    function_id: FunctionId(2),
                    call_site_source: None,
                },
                30,
            ),
            envelope(
                ProfileEventKind::EndFunction {
                    thread_id: BexThreadId(1),
                    call_id: BexCallId(2),
                    status: CallStatus::Ok,
                },
                40,
            ),
            envelope(
                ProfileEventKind::EndFunction {
                    thread_id: BexThreadId(1),
                    call_id: BexCallId(1),
                    status: CallStatus::Ok,
                },
                50,
            ),
            envelope(
                ProfileEventKind::EndThread {
                    thread_id: BexThreadId(1),
                    status: ThreadStatus::Completed,
                },
                60,
            ),
            envelope(
                ProfileEventKind::StartThread {
                    thread_id: BexThreadId(99),
                    parent_thread_id: None,
                    parent_call_id: None,
                    name: None,
                },
                10,
            ),
            envelope(
                ProfileEventKind::CallFunction {
                    thread_id: BexThreadId(99),
                    call_id: BexCallId(1),
                    parent_call_id: None,
                    function_id: FunctionId(99),
                    call_site_source: None,
                },
                20,
            ),
            envelope(
                ProfileEventKind::EndFunction {
                    thread_id: BexThreadId(99),
                    call_id: BexCallId(1),
                    status: CallStatus::Ok,
                },
                30,
            ),
            envelope(
                ProfileEventKind::EndThread {
                    thread_id: BexThreadId(99),
                    status: ThreadStatus::Completed,
                },
                40,
            ),
        ] {
            store.ingest_profile_event(event);
        }

        let snapshot = store.snapshot(start.boundary_id).unwrap();
        assert_eq!(
            snapshot.root_call_node_id,
            Some(call_node_id(&TraceCallKey {
                process_euid: ProcessEuid([1; 16]),
                engine_id: EngineId(2),
                thread_id: BexThreadId(1),
                call_id: BexCallId(1),
            }))
        );
        assert_eq!(snapshot.threads.len(), 1, "{:#?}", snapshot.threads);
        assert_eq!(snapshot.calls.len(), 2, "{:#?}", snapshot.calls);
        assert!(
            snapshot
                .calls
                .iter()
                .all(|call| call.trace_key.thread_id != BexThreadId(99))
        );
        assert!(
            snapshot.diagnostics.is_empty(),
            "{:#?}",
            snapshot.diagnostics
        );
    }

    #[test]
    fn graph_overlay_leaves_calls_unattached_without_call_site_provenance() {
        let store = InMemoryRunStore::default();
        let start = create_test_run(&store, request("main"), RequestId(1));
        let AttachRootTraceResult::Attached { patches } =
            store.attach_root_trace(start.boundary_id, root_call_ref(1, 1))
        else {
            panic!("root trace should attach");
        };
        assert!(patches.is_empty());

        for event in [
            envelope(
                ProfileEventKind::StartThread {
                    thread_id: BexThreadId(1),
                    parent_thread_id: None,
                    parent_call_id: None,
                    name: None,
                },
                10,
            ),
            envelope(
                ProfileEventKind::CallFunction {
                    thread_id: BexThreadId(1),
                    call_id: BexCallId(1),
                    parent_call_id: None,
                    function_id: FunctionId(1),
                    call_site_source: None,
                },
                20,
            ),
        ] {
            store.ingest_profile_event(event);
        }

        let snapshot = store.snapshot(start.boundary_id).unwrap();
        let overlay = snapshot
            .graph_runtime_overlay
            .expect("profile reconstruction should set graph overlay");
        assert!(overlay.entries.is_empty());
        assert_eq!(
            overlay.unattached_call_node_ids,
            snapshot
                .calls
                .iter()
                .map(|call| call.id)
                .collect::<Vec<_>>()
        );
        assert!(overlay.diagnostics.iter().any(|diagnostic| {
            diagnostic.code.as_deref() == Some("GraphOverlayCallSiteUnavailable")
        }));
        assert!(
            snapshot
                .calls
                .iter()
                .all(|call| call.call_site_source.is_none())
        );
    }

    #[test]
    fn graph_overlay_keeps_call_site_calls_unattached_until_project_store_join() {
        let store = InMemoryRunStore::default();
        let start = create_test_run(&store, request("main"), RequestId(1));
        let AttachRootTraceResult::Attached { patches } =
            store.attach_root_trace(start.boundary_id, root_call_ref(1, 1))
        else {
            panic!("root trace should attach");
        };
        assert!(patches.is_empty());
        let call_site_source = SourceLocation {
            file_path: None,
            file_id: Some(1),
            line: 7,
            column: 0,
            end_line: None,
            end_column: None,
            start_offset: Some(40),
            end_offset: Some(51),
        };

        for event in [
            envelope(
                ProfileEventKind::StartThread {
                    thread_id: BexThreadId(1),
                    parent_thread_id: None,
                    parent_call_id: None,
                    name: None,
                },
                10,
            ),
            envelope(
                ProfileEventKind::CallFunction {
                    thread_id: BexThreadId(1),
                    call_id: BexCallId(1),
                    parent_call_id: None,
                    function_id: FunctionId(1),
                    call_site_source: Some(call_site_source.clone()),
                },
                20,
            ),
        ] {
            store.ingest_profile_event(event);
        }

        let snapshot = store.snapshot(start.boundary_id).unwrap();
        assert_eq!(snapshot.calls[0].call_site_source, Some(call_site_source));
        let overlay = snapshot
            .graph_runtime_overlay
            .expect("profile reconstruction should set graph overlay");
        assert!(overlay.entries.is_empty());
        assert_eq!(
            overlay.unattached_call_node_ids,
            snapshot
                .calls
                .iter()
                .map(|call| call.id)
                .collect::<Vec<_>>()
        );
        assert!(overlay.diagnostics.iter().any(|diagnostic| {
            diagnostic.code.as_deref() == Some("GraphOverlayProjectStoreUnavailable")
        }));
    }

    struct StaticSpanProvider {
        spans: Vec<CfgNodeSourceSpan>,
    }

    impl GraphRuntimeOverlaySpanProvider for StaticSpanProvider {
        fn cfg_node_spans_for_run(&self, run: &Run) -> GraphRuntimeOverlaySpanResolution {
            assert_eq!(run.request.project_generation, ProjectGeneration(1));
            GraphRuntimeOverlaySpanResolution::Available(self.spans.clone())
        }
    }

    #[test]
    fn graph_overlay_span_provider_populates_live_overlay_entries() {
        let store = InMemoryRunStore::default();
        store.set_graph_runtime_overlay_span_provider(Arc::new(StaticSpanProvider {
            spans: vec![CfgNodeSourceSpan {
                cfg_node_id: CfgNodeId(701),
                file_id: 1,
                start_offset: 40,
                end_offset: 51,
            }],
        }));
        let start = create_test_run(&store, request("main"), RequestId(1));
        let AttachRootTraceResult::Attached { patches } =
            store.attach_root_trace(start.boundary_id, root_call_ref(1, 1))
        else {
            panic!("root trace should attach");
        };
        assert!(patches.is_empty());
        let call_site_source = SourceLocation {
            file_path: None,
            file_id: Some(1),
            line: 7,
            column: 0,
            end_line: None,
            end_column: None,
            start_offset: Some(40),
            end_offset: Some(51),
        };

        for event in [
            envelope(
                ProfileEventKind::StartThread {
                    thread_id: BexThreadId(1),
                    parent_thread_id: None,
                    parent_call_id: None,
                    name: None,
                },
                10,
            ),
            envelope(
                ProfileEventKind::CallFunction {
                    thread_id: BexThreadId(1),
                    call_id: BexCallId(1),
                    parent_call_id: None,
                    function_id: FunctionId(1),
                    call_site_source: Some(call_site_source),
                },
                20,
            ),
        ] {
            store.ingest_profile_event(event);
        }

        let snapshot = store.snapshot(start.boundary_id).unwrap();
        let overlay = snapshot
            .graph_runtime_overlay
            .expect("profile reconstruction should set graph overlay");
        assert_eq!(
            overlay.entries,
            vec![GraphRuntimeOverlayEntry {
                cfg_node_id: CfgNodeId(701),
                call_node_ids: vec![snapshot.calls[0].id],
            }]
        );
        assert!(overlay.unattached_call_node_ids.is_empty());
        assert!(overlay.diagnostics.is_empty());
    }

    #[test]
    fn registered_profile_observer_feeds_run_store_before_root_attach() {
        let store = Arc::new(InMemoryRunStore::default());
        let _registration = register_profile_observer(store.clone());
        let start = create_test_run(&store, request("main"), RequestId(1));

        for event in [
            envelope(
                ProfileEventKind::StartThread {
                    thread_id: BexThreadId(1),
                    parent_thread_id: None,
                    parent_call_id: None,
                    name: None,
                },
                10,
            ),
            envelope(
                ProfileEventKind::CallFunction {
                    thread_id: BexThreadId(1),
                    call_id: BexCallId(1),
                    parent_call_id: None,
                    function_id: FunctionId(1),
                    call_site_source: None,
                },
                20,
            ),
            envelope(
                ProfileEventKind::EndFunction {
                    thread_id: BexThreadId(1),
                    call_id: BexCallId(1),
                    status: CallStatus::Ok,
                },
                30,
            ),
            envelope(
                ProfileEventKind::EndThread {
                    thread_id: BexThreadId(1),
                    status: ThreadStatus::Completed,
                },
                40,
            ),
        ] {
            publish_profile_event(&event);
        }

        let AttachRootTraceResult::Attached { patches } =
            store.attach_root_trace(start.boundary_id, root_call_ref(1, 1))
        else {
            panic!("root trace should attach");
        };

        assert!(
            !patches.is_empty(),
            "retained live events should reconstruct"
        );
        let snapshot = store.snapshot(start.boundary_id).unwrap();
        assert_eq!(snapshot.threads.len(), 1);
        assert_eq!(snapshot.calls.len(), 1);
        assert_eq!(
            snapshot.root_call_node_id,
            Some(call_node_id(&TraceCallKey {
                process_euid: ProcessEuid([1; 16]),
                engine_id: EngineId(2),
                thread_id: BexThreadId(1),
                call_id: BexCallId(1),
            }))
        );
    }

    #[test]
    fn reconstructs_by_identity_and_edges_not_arrival_order() {
        let function_table = vec![
            ProfileFunctionMetadata {
                function_id: FunctionId(1),
                fqn: "user.main".to_string(),
                source_file: None,
                span_start: None,
                span_end: None,
                kind: None,
            },
            ProfileFunctionMetadata {
                function_id: FunctionId(2),
                fqn: "user.child".to_string(),
                source_file: None,
                span_start: None,
                span_end: None,
                kind: None,
            },
        ];
        let profile = reconstruct_with_function_table(
            [
                envelope(
                    ProfileEventKind::EndFunction {
                        thread_id: BexThreadId(1),
                        call_id: BexCallId(2),
                        status: CallStatus::Ok,
                    },
                    40,
                ),
                envelope(
                    ProfileEventKind::CallFunction {
                        thread_id: BexThreadId(1),
                        call_id: BexCallId(1),
                        parent_call_id: None,
                        function_id: FunctionId(1),
                        call_site_source: None,
                    },
                    10,
                ),
                envelope(
                    ProfileEventKind::StartThread {
                        thread_id: BexThreadId(1),
                        parent_thread_id: None,
                        parent_call_id: None,
                        name: None,
                    },
                    5,
                ),
                envelope(
                    ProfileEventKind::CallFunction {
                        thread_id: BexThreadId(1),
                        call_id: BexCallId(2),
                        parent_call_id: Some(BexCallId(1)),
                        function_id: FunctionId(2),
                        call_site_source: None,
                    },
                    20,
                ),
                envelope(
                    ProfileEventKind::EndFunction {
                        thread_id: BexThreadId(1),
                        call_id: BexCallId(1),
                        status: CallStatus::Ok,
                    },
                    50,
                ),
                envelope(
                    ProfileEventKind::EndThread {
                        thread_id: BexThreadId(1),
                        status: ThreadStatus::Completed,
                    },
                    60,
                ),
            ],
            function_table,
        );

        assert!(profile.diagnostics.is_empty(), "{:#?}", profile.diagnostics);
        assert_eq!(profile.threads.len(), 1);
        assert_eq!(profile.calls.len(), 2);
        let root = profile
            .calls
            .iter()
            .find(|call| call.trace_key.call_id == BexCallId(1))
            .expect("root call exists");
        let child = profile
            .calls
            .iter()
            .find(|call| call.trace_key.call_id == BexCallId(2))
            .expect("child call exists");
        assert_eq!(child.parent_id, Some(root.id));
        assert_eq!(child.function_name.as_deref(), Some("user.child"));
        assert_eq!(root.started_at_ns, Some(10));
        assert_eq!(root.ended_at_ns, Some(50));
    }

    #[test]
    fn preserves_distinct_call_sites_for_repeated_callee_calls() {
        let function_table = vec![
            ProfileFunctionMetadata {
                function_id: FunctionId(1),
                fqn: "user.main".to_string(),
                source_file: None,
                span_start: None,
                span_end: None,
                kind: Some("bytecode".to_string()),
            },
            ProfileFunctionMetadata {
                function_id: FunctionId(2),
                fqn: "user.child".to_string(),
                source_file: Some("main.baml".to_string()),
                span_start: Some(100),
                span_end: Some(160),
                kind: Some("bytecode".to_string()),
            },
        ];
        let site_a = SourceLocation {
            file_path: None,
            file_id: Some(7),
            line: 12,
            column: 0,
            end_line: None,
            end_column: None,
            start_offset: Some(20),
            end_offset: Some(31),
        };
        let site_b = SourceLocation {
            file_path: None,
            file_id: Some(7),
            line: 18,
            column: 0,
            end_line: None,
            end_column: None,
            start_offset: Some(80),
            end_offset: Some(91),
        };

        let profile = reconstruct_with_function_table(
            [
                envelope(
                    ProfileEventKind::StartThread {
                        thread_id: BexThreadId(1),
                        parent_thread_id: None,
                        parent_call_id: None,
                        name: None,
                    },
                    1,
                ),
                envelope(
                    ProfileEventKind::CallFunction {
                        thread_id: BexThreadId(1),
                        call_id: BexCallId(1),
                        parent_call_id: None,
                        function_id: FunctionId(1),
                        call_site_source: None,
                    },
                    2,
                ),
                envelope(
                    ProfileEventKind::CallFunction {
                        thread_id: BexThreadId(1),
                        call_id: BexCallId(2),
                        parent_call_id: Some(BexCallId(1)),
                        function_id: FunctionId(2),
                        call_site_source: Some(site_a.clone()),
                    },
                    3,
                ),
                envelope(
                    ProfileEventKind::EndFunction {
                        thread_id: BexThreadId(1),
                        call_id: BexCallId(2),
                        status: CallStatus::Ok,
                    },
                    4,
                ),
                envelope(
                    ProfileEventKind::CallFunction {
                        thread_id: BexThreadId(1),
                        call_id: BexCallId(3),
                        parent_call_id: Some(BexCallId(1)),
                        function_id: FunctionId(2),
                        call_site_source: Some(site_b.clone()),
                    },
                    5,
                ),
                envelope(
                    ProfileEventKind::EndFunction {
                        thread_id: BexThreadId(1),
                        call_id: BexCallId(3),
                        status: CallStatus::Ok,
                    },
                    6,
                ),
                envelope(
                    ProfileEventKind::EndFunction {
                        thread_id: BexThreadId(1),
                        call_id: BexCallId(1),
                        status: CallStatus::Ok,
                    },
                    7,
                ),
                envelope(
                    ProfileEventKind::EndThread {
                        thread_id: BexThreadId(1),
                        status: ThreadStatus::Completed,
                    },
                    8,
                ),
            ],
            function_table,
        );

        assert!(profile.diagnostics.is_empty(), "{:#?}", profile.diagnostics);
        let child_sites = profile
            .calls
            .iter()
            .filter(|call| call.function_name.as_deref() == Some("user.child"))
            .map(|call| call.call_site_source.clone())
            .collect::<Vec<_>>();
        assert_eq!(
            child_sites,
            vec![Some(site_a), Some(site_b)],
            "same callee calls must keep caller-side provenance distinct"
        );
    }

    #[test]
    fn reports_balance_and_parent_diagnostics() {
        let profile = reconstruct([envelope(
            ProfileEventKind::CallFunction {
                thread_id: BexThreadId(1),
                call_id: BexCallId(2),
                parent_call_id: Some(BexCallId(99)),
                function_id: FunctionId(0),
                call_site_source: None,
            },
            10,
        )]);
        let codes: HashSet<_> = profile
            .diagnostics
            .iter()
            .map(|diagnostic| diagnostic.code)
            .collect();
        assert!(codes.contains(&ReconstructionDiagnosticCode::MissingThreadStart));
        assert!(codes.contains(&ReconstructionDiagnosticCode::MissingThreadEnd));
        assert!(codes.contains(&ReconstructionDiagnosticCode::MissingCallEnd));
        assert!(codes.contains(&ReconstructionDiagnosticCode::MissingParentCall));
    }

    fn succeeded() -> RunOutcome {
        RunOutcome::Succeeded(RunResult {
            value_ref: None,
            renderer_hint: None,
            supporting_payload_ids: Vec::new(),
        })
    }

    #[test]
    fn terminal_runs_evicted_beyond_retention_cap() {
        let store = InMemoryRunStore::new(RunRetentionPolicy {
            max_terminal_runs: Some(2),
            ..Default::default()
        });
        let mut boundary_ids = Vec::new();
        for i in 0..4u64 {
            let start = create_test_run(&store, request("main"), RequestId(i));
            let host_call_id = HostCallId::Native(sys_types::CallId(i));
            assert!(
                store
                    .attach_host_call(start.boundary_id, host_call_id)
                    .is_some()
            );
            boundary_ids.push(start.boundary_id);
            store.complete_run(start.boundary_id, succeeded(), 100 + i);
        }

        // The two oldest terminal runs are gone, along with their host-call
        // index entries; the two newest survive.
        assert!(store.snapshot(boundary_ids[0]).is_none());
        assert!(store.snapshot(boundary_ids[1]).is_none());
        assert!(store.snapshot(boundary_ids[2]).is_some());
        assert!(store.snapshot(boundary_ids[3]).is_some());
        assert_eq!(
            store.boundary_id_for_host_call(&HostCallId::Native(sys_types::CallId(0))),
            None
        );
        assert_eq!(
            store.boundary_id_for_host_call(&HostCallId::Native(sys_types::CallId(3))),
            Some(boundary_ids[3])
        );
        assert_eq!(store.list_runs(&RunFilter::default()).len(), 2);
    }

    #[test]
    fn active_runs_are_never_evicted_by_terminal_cap() {
        let store = InMemoryRunStore::new(RunRetentionPolicy {
            max_terminal_runs: Some(1),
            ..Default::default()
        });
        let active = create_test_run(&store, request("active"), RequestId(1));
        let mut terminal_ids = Vec::new();
        for i in 0..3u64 {
            let start = create_test_run(&store, request("main"), RequestId(10 + i));
            terminal_ids.push(start.boundary_id);
            store.complete_run(start.boundary_id, succeeded(), 100 + i);
        }
        assert!(store.snapshot(active.boundary_id).is_some());
        assert!(store.snapshot(terminal_ids[2]).is_some());
        assert!(store.snapshot(terminal_ids[0]).is_none());
    }

    #[test]
    fn terminal_ttl_evicts_old_runs() {
        let store = InMemoryRunStore::new(RunRetentionPolicy {
            terminal_ttl_ms: Some(1_000),
            ..Default::default()
        });
        let old = create_test_run(&store, request("old"), RequestId(1));
        store.complete_run(old.boundary_id, succeeded(), 1_000);
        let new = create_test_run(&store, request("new"), RequestId(2));
        store.complete_run(new.boundary_id, succeeded(), 5_000);

        assert!(store.snapshot(old.boundary_id).is_none());
        assert!(store.snapshot(new.boundary_id).is_some());
    }

    fn thread_and_root_call_events(engine_id: EngineId) -> Vec<ProfileEventEnvelope> {
        let live = |kind, timestamp_ns| ProfileEventEnvelope {
            source: ProfileEventSource::Live {
                target: RuntimeTarget::Native,
                source_id: "test".to_string(),
            },
            process_euid: ProcessEuid([1; 16]),
            engine_id,
            event: ProfileEvent { timestamp_ns, kind },
        };
        vec![
            live(
                ProfileEventKind::StartThread {
                    thread_id: BexThreadId(1),
                    parent_thread_id: None,
                    parent_call_id: None,
                    name: None,
                },
                10,
            ),
            live(
                ProfileEventKind::CallFunction {
                    thread_id: BexThreadId(1),
                    call_id: BexCallId(1),
                    parent_call_id: None,
                    function_id: FunctionId(1),
                    call_site_source: None,
                },
                20,
            ),
        ]
    }

    fn engine_root_call_ref(engine_id: EngineId) -> CallRef {
        CallRef {
            process_euid: ProcessEuid([1; 16]),
            engine_id,
            thread_id: BexThreadId(1),
            call_id: BexCallId(1),
        }
    }

    #[test]
    fn profile_event_recompute_is_scoped_to_the_event_engine() {
        let store = InMemoryRunStore::default();
        let run_a = create_test_run(&store, request("a"), RequestId(1));
        let run_b = create_test_run(&store, request("b"), RequestId(2));
        assert!(matches!(
            store.attach_root_trace(run_a.boundary_id, engine_root_call_ref(EngineId(2))),
            AttachRootTraceResult::Attached { .. }
        ));
        assert!(matches!(
            store.attach_root_trace(run_b.boundary_id, engine_root_call_ref(EngineId(3))),
            AttachRootTraceResult::Attached { .. }
        ));

        // Events for engine 2 must only patch the run rooted in engine 2.
        for event in thread_and_root_call_events(EngineId(2)) {
            for patch in store.ingest_profile_event(event) {
                assert_eq!(patch.boundary_id, run_a.boundary_id);
            }
        }
        assert!(!store.snapshot(run_a.boundary_id).unwrap().calls.is_empty());
        assert!(store.snapshot(run_b.boundary_id).unwrap().calls.is_empty());
    }

    #[test]
    fn engine_closed_releases_events_and_keeps_reconstructed_runs() {
        let store = InMemoryRunStore::default();
        let start = create_test_run(&store, request("main"), RequestId(1));
        assert!(matches!(
            store.attach_root_trace(start.boundary_id, engine_root_call_ref(EngineId(2))),
            AttachRootTraceResult::Attached { .. }
        ));
        for event in thread_and_root_call_events(EngineId(2)) {
            store.ingest_profile_event(event);
        }
        let calls_before = store.snapshot(start.boundary_id).unwrap().calls;
        assert!(!calls_before.is_empty());

        store.engine_closed(EngineId(2));
        assert_eq!(store.profile_events_len(), 0);
        // The run keeps the call tree reconstructed before the release.
        assert_eq!(
            store.snapshot(start.boundary_id).unwrap().calls,
            calls_before
        );
    }

    #[test]
    fn trim_profile_events_drops_oldest_beyond_cap() {
        let mut events: Vec<ProfileEventEnvelope> = (0..PROFILE_EVENTS_CAP + 5)
            .map(|i| {
                thread_and_root_call_events(EngineId(2))
                    .into_iter()
                    .next()
                    .map(|mut envelope| {
                        envelope.event.timestamp_ns = i as u64;
                        envelope
                    })
                    .unwrap()
            })
            .collect();
        trim_profile_events(&mut events);
        assert_eq!(events.len(), PROFILE_EVENTS_CAP);
        assert_eq!(events[0].event.timestamp_ns, 5);
    }

    fn output_texts(store: &InMemoryRunStore, boundary_id: BoundaryId) -> Vec<String> {
        store
            .snapshot(boundary_id)
            .expect("run snapshot")
            .payloads
            .iter()
            .filter_map(|payload| match &payload.kind {
                PayloadKind::Output(output) => Some(output.text.clone()),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn ingest_output_caps_a_runaway_print_loop() {
        let store = InMemoryRunStore::default();
        let start = create_test_run(&store, request("output"), RequestId(1));
        let host = HostCallId::Native(sys_types::CallId(1));
        store.attach_host_call(start.boundary_id, host.clone());

        let chunk = "x".repeat(64 * 1024);
        // Well past MAX_RUN_OUTPUT_BYTES worth of writes.
        for _ in 0..64 {
            store.ingest_output(&host, OutputStream::Stdout, chunk.clone());
        }

        let texts = output_texts(&store, start.boundary_id);
        let notices = texts
            .iter()
            .filter(|text| text.contains("output truncated"))
            .count();
        assert_eq!(notices, 1, "exactly one truncation notice");
        assert!(
            texts
                .last()
                .is_some_and(|text| text.contains("output truncated")),
            "the notice is the last thing recorded"
        );
        let total: usize = texts.iter().map(String::len).sum();
        assert!(
            total < MAX_RUN_OUTPUT_BYTES * 2,
            "retained output stays bounded, got {total} bytes"
        );
    }

    #[test]
    fn ingest_output_without_an_attached_host_call_records_nothing() {
        let store = InMemoryRunStore::default();
        let start = create_test_run(&store, request("output"), RequestId(1));
        let orphan = HostCallId::Native(sys_types::CallId(99));

        assert!(
            store
                .ingest_output(&orphan, OutputStream::Stdout, "hi".to_string())
                .is_none()
        );
        assert!(output_texts(&store, start.boundary_id).is_empty());
    }
}
