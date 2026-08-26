//! Process-scoped, transport-neutral LSP ingress scheduling.
//!
//! The scheduler is deliberately synchronous: callers put one instance behind
//! their process-owned mutex and let both stdio and browser adapters submit the
//! same [`lsp_server::Message`] representation. It owns admission ordering,
//! lifecycle validation, request response ownership, and bounded memory
//! accounting; handlers still own the actual LSP operation.
//!
//! Cancellation tokens returned by this module are operation-owned response
//! signals: claiming one settles who answers the client. The running query is
//! cancelled separately: the host forwards the `$/cancelRequest` to the owner
//! thread, where the protocol layer cancels the read's snapshot Salsa token
//! and the query unwinds instead of completing into a claimed response.

use std::{
    collections::{BTreeMap, HashMap, HashSet, VecDeque},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

use lsp_server::{Message, RequestId};

/// Operation-owned cancellation flag handed to a running request's handler.
///
/// Set exactly once, by the scheduler, when a `$/cancelRequest` wins response
/// ownership of a running request whose policy is
/// [`CancellationBehavior::SignalAtSafePoints`], or when the owning session
/// terminates. It is a plain flag for handlers to poll at safe boundaries; it
/// is not Salsa's per-snapshot token, and the scheduler never unwinds anything.
#[derive(Debug, Clone, Default)]
pub struct CancellationToken(Arc<AtomicBool>);

impl CancellationToken {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn cancel(&self) {
        self.0.store(true, Ordering::Release);
    }

    #[must_use]
    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::Acquire)
    }
}

/// Monotonic identity for one transport connection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SessionId(u64);

impl SessionId {
    /// The process-local numeric identity. This is useful for logging only.
    #[must_use]
    pub const fn get(self) -> u64 {
        self.0
    }
}

/// Request response identity. Request IDs may be reused by a replacement
/// browser connection, so a bare [`RequestId`] is never sufficient.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ResponseToken {
    session_id: SessionId,
    request_id: RequestId,
}

impl ResponseToken {
    #[must_use]
    pub const fn session_id(&self) -> SessionId {
        self.session_id
    }

    #[must_use]
    pub const fn request_id(&self) -> &RequestId {
        &self.request_id
    }
}

/// Sequence assigned to each admitted non-cancellation message.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct IngressSequence(u64);

impl IngressSequence {
    #[must_use]
    pub const fn get(self) -> u64 {
        self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransportKind {
    Stdio,
    Browser,
}

/// Per-connection lifecycle. `Superseded` is reported in takeover effects;
/// superseded sessions themselves are removed so reconnect churn cannot grow
/// an unbounded tombstone map.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LifecycleState {
    PreInitialize,
    InitializeResponding,
    InitializeResponded,
    Initialized,
    ShutdownResponding,
    Shutdown,
    Exited,
    Superseded,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AdmissionClass {
    /// Read-only editor queries and formatting. These use the smaller read
    /// budget and are rejected rather than backpressuring mutation traffic.
    Read,
    /// A lifecycle barrier such as initialize, initialized, or shutdown.
    Lifecycle,
    /// A source/workspace mutation barrier.
    Mutation,
    /// A request whose handler may commit an external side effect.
    SideEffecting,
    /// An ordinary notification that is neither a mutation nor lifecycle.
    Other,
}

impl AdmissionClass {
    const fn is_reserved(self) -> bool {
        matches!(self, Self::Lifecycle | Self::Mutation | Self::SideEffecting)
    }
}

/// What a running handler should do when cancellation wins response ownership.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CancellationBehavior {
    /// Claim the response and signal the operation-owned token.
    SignalAtSafePoints,
    /// Claim the response, but let already-started effects finish normally.
    ClaimResponseOnly,
    /// Once running, cancellation is a no-op. Queued work is still removable.
    NonCancellableWhenRunning,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MessagePolicy {
    pub class: AdmissionClass,
    pub cancellation: CancellationBehavior,
}

impl MessagePolicy {
    #[must_use]
    pub const fn read() -> Self {
        Self {
            class: AdmissionClass::Read,
            cancellation: CancellationBehavior::SignalAtSafePoints,
        }
    }

    #[must_use]
    pub const fn side_effecting(cancellation: CancellationBehavior) -> Self {
        Self {
            class: AdmissionClass::SideEffecting,
            cancellation,
        }
    }

    /// Shared default classification used by both transports. Integrators may
    /// override ordinary custom requests, but known lifecycle and mutation
    /// methods are always upgraded to their protected class.
    #[must_use]
    pub fn for_message(message: &Message) -> Self {
        match message {
            Message::Request(request) => match request.method.as_str() {
                "initialize" | "shutdown" => Self {
                    class: AdmissionClass::Lifecycle,
                    // Once dispatched these handlers freeze session config or
                    // apply shutdown side effects. Queued cancellation still
                    // works, but running lifecycle barriers are committed.
                    cancellation: CancellationBehavior::NonCancellableWhenRunning,
                },
                "workspace/executeCommand" => {
                    // Dispatch is the linearization point for command side
                    // effects. A cancellation that wins while the command is
                    // queued removes it and owns the response; once dispatch
                    // begins, cancellation must not report RequestCanceled
                    // while the handler can still commit its effect.
                    Self::side_effecting(CancellationBehavior::NonCancellableWhenRunning)
                }
                _ => Self::read(),
            },
            Message::Notification(notification) => {
                let class = match notification.method.as_str() {
                    "initialized" | "exit" => AdmissionClass::Lifecycle,
                    "textDocument/didOpen"
                    | "textDocument/didChange"
                    | "textDocument/didClose"
                    | "textDocument/didSave"
                    | "workspace/didChangeConfiguration"
                    | "workspace/didChangeWatchedFiles"
                    | "workspace/didChangeWorkspaceFolders"
                    | "workspace/didCreateFiles"
                    | "workspace/didRenameFiles"
                    | "workspace/didDeleteFiles" => AdmissionClass::Mutation,
                    _ => AdmissionClass::Other,
                };
                Self {
                    class,
                    cancellation: CancellationBehavior::NonCancellableWhenRunning,
                }
            }
            Message::Response(_) => Self {
                class: AdmissionClass::Other,
                cancellation: CancellationBehavior::NonCancellableWhenRunning,
            },
        }
    }
}

/// Independent normal, reserve, control, read, and outbound-response budgets.
/// A request reserves `response_reservation_bytes` until its response has been
/// ordered onto the owning session's sink.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IngressLimits {
    pub normal_items: usize,
    pub normal_bytes: usize,
    pub reserved_items: usize,
    pub reserved_bytes: usize,
    pub read_items: usize,
    pub read_bytes: usize,
    pub control_items: usize,
    pub control_bytes: usize,
    pub outbound_response_items: usize,
    pub outbound_response_bytes: usize,
    pub response_reservation_bytes: usize,
}

impl Default for IngressLimits {
    fn default() -> Self {
        Self {
            normal_items: 256,
            normal_bytes: 4 * 1024 * 1024,
            reserved_items: 64,
            reserved_bytes: 2 * 1024 * 1024,
            read_items: 128,
            read_bytes: 2 * 1024 * 1024,
            control_items: 64,
            control_bytes: 64 * 1024,
            outbound_response_items: 256,
            outbound_response_bytes: 4 * 1024 * 1024,
            response_reservation_bytes: 16 * 1024,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum LimitsError {
    #[error("all ingress capacities and the response reservation must be non-zero")]
    ZeroCapacity,
    #[error("read capacity must fit inside normal capacity")]
    ReadExceedsNormal,
    #[error("one response reservation must fit in the outbound byte capacity")]
    ResponseReservationTooLarge,
    #[error("combined normal and reserved capacity overflowed usize")]
    CapacityOverflow,
}

impl IngressLimits {
    fn validate(self) -> Result<Self, LimitsError> {
        if self.normal_items == 0
            || self.normal_bytes == 0
            || self.reserved_items == 0
            || self.reserved_bytes == 0
            || self.read_items == 0
            || self.read_bytes == 0
            || self.control_items == 0
            || self.control_bytes == 0
            || self.outbound_response_items == 0
            || self.outbound_response_bytes == 0
            || self.response_reservation_bytes == 0
        {
            return Err(LimitsError::ZeroCapacity);
        }
        if self.read_items > self.normal_items || self.read_bytes > self.normal_bytes {
            return Err(LimitsError::ReadExceedsNormal);
        }
        if self.response_reservation_bytes > self.outbound_response_bytes {
            return Err(LimitsError::ResponseReservationTooLarge);
        }
        self.normal_items
            .checked_add(self.reserved_items)
            .ok_or(LimitsError::CapacityOverflow)?;
        self.normal_bytes
            .checked_add(self.reserved_bytes)
            .ok_or(LimitsError::CapacityOverflow)?;
        Ok(self)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct QueueUsage {
    pub items: usize,
    pub bytes: usize,
    pub read_items: usize,
    pub read_bytes: usize,
    pub control_items: usize,
    pub control_bytes: usize,
    pub outbound_response_items: usize,
    pub outbound_response_bytes: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProtocolErrorKind {
    ParseError,
    InvalidRequest,
    ServerNotInitialized,
    MethodNotFound,
    InvalidParams,
    InternalError,
    RequestCanceled,
    ContentModified,
    RequestFailed,
}

impl ProtocolErrorKind {
    #[must_use]
    pub const fn json_rpc_code(self) -> i32 {
        match self {
            Self::ParseError => -32700,
            Self::InvalidRequest => -32600,
            Self::ServerNotInitialized => -32002,
            Self::MethodNotFound => -32601,
            Self::InvalidParams => -32602,
            Self::InternalError => -32603,
            Self::RequestCanceled => -32800,
            Self::ContentModified => -32801,
            Self::RequestFailed => -32803,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProtocolError {
    pub kind: ProtocolErrorKind,
    pub message: String,
}

impl ProtocolError {
    fn new(kind: ProtocolErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BackpressureReason {
    NormalCapacity,
    ReservedCapacity,
    ControlCapacity,
    OutboundResponseCapacity,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DropReason {
    NotificationBeforeInitialized,
    InitializedInWrongState,
    NotificationAfterShutdown,
    /// The serialized message alone exceeds its class's byte capacity: it can
    /// never be admitted, so backpressure would livelock the transport. The
    /// notification (or client response) is dropped; the session stays alive.
    OversizedMessage,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AdmissionResult {
    Admitted {
        sequence: IngressSequence,
        response_token: Option<ResponseToken>,
        coalesced: bool,
    },
    Rejected {
        response_token: ResponseToken,
        error: ProtocolError,
    },
    Dropped(DropReason),
    Backpressure(BackpressureReason),
    /// `$/cancelRequest` must enter [`IngressScheduler::admit_cancel`].
    UseControlPath,
    Exited(SessionTermination),
    UnknownSession,
    DuplicateRequestId(ResponseToken),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ControlAdmission {
    Admitted,
    Duplicate,
    Backpressure,
    /// The control alone exceeds the control byte budget and can never be
    /// admitted; retrying would livelock. Drop it and keep the session alive.
    Oversized,
    UnknownSession,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResponseOutcome {
    Success,
    Error,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResponseClaim {
    Claimed,
    AlreadyClaimed,
    UnknownOrSuperseded,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResponseOrder {
    Ordered,
    UnknownOrSuperseded,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResponseSizeError {
    UnknownOrSuperseded,
    ExceedsReservation,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutstandingState {
    Queued,
    Running,
    Responded,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OwnedDocument {
    pub uri: String,
    pub version: Option<i32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TerminationReason {
    NormalExit,
    EarlyExit,
    TransportClosed,
    BrowserSuperseded,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionTermination {
    pub session_id: SessionId,
    pub prior_lifecycle: LifecycleState,
    pub final_lifecycle: LifecycleState,
    pub reason: TerminationReason,
    pub dropped_normal_items: usize,
    pub dropped_control_items: usize,
    pub revoked_response_tokens: Vec<ResponseToken>,
    pub owned_documents: Vec<OwnedDocument>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpenSession {
    pub session_id: SessionId,
    /// Present only when a new browser session atomically revoked the previous
    /// browser session. The adapter must close these document overlays before
    /// exposing `session_id` to the replacement transport.
    pub takeover: Option<SessionTermination>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CancelEffect {
    Respond {
        response_token: ResponseToken,
        was_running: bool,
        signaled_operation: bool,
        error: ProtocolError,
    },
    Noop,
}

#[derive(Debug)]
pub enum SchedulerEvent {
    Control(CancelEffect),
    Dispatch(DispatchItem),
}

#[derive(Debug)]
pub struct DispatchItem {
    pub sequence: IngressSequence,
    pub session_id: SessionId,
    pub class: AdmissionClass,
    pub message: Message,
    pub response_token: Option<ResponseToken>,
    pub cancellation: Option<CancellationToken>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SessionConfigError {
    UnknownSession,
    WrongLifecycle(LifecycleState),
    AlreadySet,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RequestRole {
    Initialize,
    Shutdown,
    Ordinary,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ResponseOwner {
    Normal(ResponseOutcome),
    Canceled,
    ImmediateError,
}

struct OutstandingRequest {
    state: OutstandingState,
    cancellation: CancellationToken,
    behavior: CancellationBehavior,
    cancel_committed: bool,
    role: RequestRole,
    response_owner: Option<ResponseOwner>,
}

struct Session {
    lifecycle: LifecycleState,
    capabilities: Option<Arc<serde_json::Value>>,
    documents: BTreeMap<String, Option<i32>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct FullChangeKey {
    uri: String,
}

struct QueuedItem {
    sequence: IngressSequence,
    session_id: SessionId,
    class: AdmissionClass,
    charge_bytes: usize,
    read_charge: bool,
    message: Message,
    response_token: Option<ResponseToken>,
    full_change: Option<FullChangeKey>,
    /// An `initialized` notification admitted while `initialize` was still in
    /// flight (a pipelined client). Its acceptance is decided at dispatch:
    /// it is valid only once the initialize response has been ordered
    /// (`InitializeResponded`), and is dropped in any other state.
    initialized_gate: bool,
}

struct QueuedCancel {
    token: ResponseToken,
    charge_bytes: usize,
}

/// Process-owned scheduler shared by every LSP transport adapter.
pub struct IngressScheduler {
    limits: IngressLimits,
    next_session_id: u64,
    next_sequence: u64,
    sessions: HashMap<SessionId, Session>,
    active_browser: Option<SessionId>,
    normal: VecDeque<QueuedItem>,
    control: VecDeque<QueuedCancel>,
    pending_cancels: HashSet<ResponseToken>,
    outstanding: HashMap<ResponseToken, OutstandingRequest>,
    normal_items: usize,
    normal_bytes: usize,
    read_items: usize,
    read_bytes: usize,
    control_bytes: usize,
    outbound_response_items: usize,
    outbound_response_bytes: usize,
}

impl IngressScheduler {
    pub fn new(limits: IngressLimits) -> Result<Self, LimitsError> {
        let limits = limits.validate()?;
        Ok(Self {
            limits,
            next_session_id: 1,
            next_sequence: 1,
            sessions: HashMap::new(),
            active_browser: None,
            normal: VecDeque::new(),
            control: VecDeque::new(),
            pending_cancels: HashSet::new(),
            outstanding: HashMap::new(),
            normal_items: 0,
            normal_bytes: 0,
            read_items: 0,
            read_bytes: 0,
            control_bytes: 0,
            outbound_response_items: 0,
            outbound_response_bytes: 0,
        })
    }

    /// Opens a fresh connection. Opening a browser connection atomically
    /// revokes the previous browser session and returns the cleanup work that
    /// must be applied before the replacement socket is exposed.
    pub fn open_session(&mut self, transport: TransportKind) -> OpenSession {
        let takeover = if transport == TransportKind::Browser {
            self.active_browser
                .and_then(|old| self.terminate(old, TerminationReason::BrowserSuperseded))
        } else {
            None
        };
        let session_id = SessionId(self.next_session_id);
        self.next_session_id = self.next_session_id.saturating_add(1);
        self.sessions.insert(
            session_id,
            Session {
                lifecycle: LifecycleState::PreInitialize,
                capabilities: None,
                documents: BTreeMap::new(),
            },
        );
        if transport == TransportKind::Browser {
            self.active_browser = Some(session_id);
        }
        OpenSession {
            session_id,
            takeover,
        }
    }

    #[must_use]
    pub fn lifecycle(&self, session_id: SessionId) -> Option<LifecycleState> {
        self.sessions.get(&session_id).map(|s| s.lifecycle)
    }

    #[must_use]
    pub fn active_browser(&self) -> Option<SessionId> {
        self.active_browser
    }

    pub fn set_negotiated_capabilities(
        &mut self,
        session_id: SessionId,
        capabilities: serde_json::Value,
    ) -> Result<(), SessionConfigError> {
        let Some(session) = self.sessions.get_mut(&session_id) else {
            return Err(SessionConfigError::UnknownSession);
        };
        if !matches!(
            session.lifecycle,
            LifecycleState::InitializeResponding | LifecycleState::InitializeResponded
        ) {
            return Err(SessionConfigError::WrongLifecycle(session.lifecycle));
        }
        if session.capabilities.is_some() {
            return Err(SessionConfigError::AlreadySet);
        }
        session.capabilities = Some(Arc::new(capabilities));
        Ok(())
    }

    #[must_use]
    pub fn negotiated_capabilities(&self, session_id: SessionId) -> Option<Arc<serde_json::Value>> {
        self.sessions
            .get(&session_id)
            .and_then(|session| session.capabilities.clone())
    }

    /// Records an applied didOpen. Call this from the mutation handler's commit
    /// point, not merely when the notification is admitted.
    pub fn record_document_open(
        &mut self,
        session_id: SessionId,
        uri: impl Into<String>,
        version: Option<i32>,
    ) -> bool {
        let Some(session) = self.sessions.get_mut(&session_id) else {
            return false;
        };
        session.documents.insert(uri.into(), version);
        true
    }

    pub fn record_document_version(
        &mut self,
        session_id: SessionId,
        uri: &str,
        version: Option<i32>,
    ) -> bool {
        let Some(session) = self.sessions.get_mut(&session_id) else {
            return false;
        };
        let Some(current) = session.documents.get_mut(uri) else {
            return false;
        };
        *current = version;
        true
    }

    pub fn record_document_close(&mut self, session_id: SessionId, uri: &str) -> bool {
        self.sessions
            .get_mut(&session_id)
            .is_some_and(|session| session.documents.remove(uri).is_some())
    }

    #[must_use]
    pub fn usage(&self) -> QueueUsage {
        QueueUsage {
            items: self.normal_items,
            bytes: self.normal_bytes,
            read_items: self.read_items,
            read_bytes: self.read_bytes,
            control_items: self.control.len(),
            control_bytes: self.control_bytes,
            outbound_response_items: self.outbound_response_items,
            outbound_response_bytes: self.outbound_response_bytes,
        }
    }

    pub fn admit_message(
        &mut self,
        session_id: SessionId,
        message: Message,
        serialized_bytes: usize,
    ) -> AdmissionResult {
        let policy = MessagePolicy::for_message(&message);
        self.admit_message_with_policy(session_id, message, serialized_bytes, policy)
    }

    pub fn admit_message_with_policy(
        &mut self,
        session_id: SessionId,
        message: Message,
        serialized_bytes: usize,
        requested_policy: MessagePolicy,
    ) -> AdmissionResult {
        let Some(session) = self.sessions.get(&session_id) else {
            return AdmissionResult::UnknownSession;
        };
        if matches!(
            &message,
            Message::Notification(notification) if notification.method == "$/cancelRequest"
        ) {
            return AdmissionResult::UseControlPath;
        }
        let forced = MessagePolicy::for_message(&message);
        let policy = MessagePolicy {
            class: stronger_class(forced.class, requested_policy.class),
            cancellation: stronger_cancellation(forced.cancellation, requested_policy.cancellation),
        };

        let lifecycle_action = match lifecycle_action(session.lifecycle, &message) {
            LifecycleAction::Reject(kind, text) => {
                let Message::Request(request) = &message else {
                    unreachable!("only requests receive lifecycle rejections")
                };
                return self.reject_request(
                    session_id,
                    request.id.clone(),
                    ProtocolError::new(kind, text),
                );
            }
            LifecycleAction::Drop(reason) => return AdmissionResult::Dropped(reason),
            LifecycleAction::Exit(normal) => {
                let reason = if normal {
                    TerminationReason::NormalExit
                } else {
                    TerminationReason::EarlyExit
                };
                let termination = self
                    .terminate(session_id, reason)
                    .expect("session existence checked above");
                return AdmissionResult::Exited(termination);
            }
            action => action,
        };

        let request_data = match &message {
            Message::Request(request) => {
                let token = ResponseToken {
                    session_id,
                    request_id: request.id.clone(),
                };
                if self.outstanding.contains_key(&token) {
                    return AdmissionResult::DuplicateRequestId(token);
                }
                if !self.reserve_response() {
                    return AdmissionResult::Backpressure(
                        BackpressureReason::OutboundResponseCapacity,
                    );
                }
                let role = match request.method.as_str() {
                    "initialize" => RequestRole::Initialize,
                    "shutdown" => RequestRole::Shutdown,
                    _ => RequestRole::Ordinary,
                };
                Some((token, role))
            }
            Message::Notification(_) => None,
            Message::Response(_) => None,
        };

        let full_change = full_change_key(&message);
        if let Some(key) = &full_change
            && self.try_coalesce_tail(
                session_id,
                key,
                message.clone(),
                serialized_bytes,
                policy.class,
            )
        {
            debug_assert!(request_data.is_none(), "didChange is a notification");
            let sequence = self.normal.back().expect("coalesced tail exists").sequence;
            return AdmissionResult::Admitted {
                sequence,
                response_token: None,
                coalesced: true,
            };
        }

        let read_charge = policy.class == AdmissionClass::Read;
        match self.has_normal_capacity(policy.class, serialized_bytes, read_charge) {
            Ok(()) => {}
            Err(reason) => {
                // A mutation notification (didOpen/didChange text,
                // watched-file batches) is never dropped for size: silently
                // losing one desyncs the server's overlay from the editor
                // buffer until the document reopens, and no later message
                // repairs that. The bytes are already in memory by the time
                // admission runs, so admitting it merely lets the queue's
                // byte counter overshoot its budget by at most this one
                // transport-capped message; item capacity still applies, and
                // everything else backpressures until the overshoot drains.
                let oversized_mutation = policy.class == AdmissionClass::Mutation
                    && matches!(message, Message::Notification(_))
                    && self.permanently_oversized(policy.class, serialized_bytes);
                if !(oversized_mutation
                    && self
                        .has_normal_capacity(policy.class, 0, read_charge)
                        .is_ok())
                {
                    if request_data.is_some() {
                        self.release_response();
                    }
                    // Any other message whose size alone exceeds its class's
                    // byte capacity can never be admitted: report it once
                    // instead of backpressuring the transport forever. The
                    // session stays alive; a request receives a typed
                    // RequestFailed response and non-mutation notifications
                    // and client responses are dropped.
                    if self.permanently_oversized(policy.class, serialized_bytes)
                        && !oversized_mutation
                    {
                        return match message {
                            Message::Request(request) => self.reject_request(
                                session_id,
                                request.id,
                                ProtocolError::new(
                                    ProtocolErrorKind::RequestFailed,
                                    "message exceeds the LSP ingress byte capacity for its class",
                                ),
                            ),
                            Message::Notification(_) | Message::Response(_) => {
                                AdmissionResult::Dropped(DropReason::OversizedMessage)
                            }
                        };
                    }
                    if policy.class == AdmissionClass::Read {
                        let Message::Request(request) = message else {
                            return AdmissionResult::Backpressure(reason);
                        };
                        return self.reject_request(
                            session_id,
                            request.id,
                            ProtocolError::new(
                                ProtocolErrorKind::RequestFailed,
                                "LSP ingress read budget is saturated",
                            ),
                        );
                    }
                    // An oversized mutation waiting on item capacity (or on a
                    // previous overshoot draining) is transient backpressure.
                    return AdmissionResult::Backpressure(reason);
                }
            }
        }

        let sequence = IngressSequence(self.next_sequence);
        self.next_sequence = self.next_sequence.saturating_add(1);
        let response_token = request_data.as_ref().map(|(token, _)| token.clone());
        if let Some((token, role)) = request_data {
            self.outstanding.insert(
                token,
                OutstandingRequest {
                    state: OutstandingState::Queued,
                    cancellation: CancellationToken::new(),
                    behavior: policy.cancellation,
                    cancel_committed: false,
                    role,
                    response_owner: None,
                },
            );
        }
        self.normal_items += 1;
        self.normal_bytes += serialized_bytes;
        if read_charge {
            self.read_items += 1;
            self.read_bytes += serialized_bytes;
        }
        self.normal.push_back(QueuedItem {
            sequence,
            session_id,
            class: policy.class,
            charge_bytes: serialized_bytes,
            read_charge,
            message,
            response_token: response_token.clone(),
            full_change,
            initialized_gate: lifecycle_action == LifecycleAction::QueueInitialized,
        });
        self.apply_lifecycle_admission(session_id, lifecycle_action);
        AdmissionResult::Admitted {
            sequence,
            response_token,
            coalesced: false,
        }
    }

    /// Enqueues cancellation on a separate bounded path. [`Self::next_event`]
    /// always drains this path before normal work.
    pub fn admit_cancel(
        &mut self,
        session_id: SessionId,
        request_id: RequestId,
        serialized_bytes: usize,
    ) -> ControlAdmission {
        if !self.sessions.contains_key(&session_id) {
            return ControlAdmission::UnknownSession;
        }
        let token = ResponseToken {
            session_id,
            request_id,
        };
        if self.pending_cancels.contains(&token) {
            return ControlAdmission::Duplicate;
        }
        if serialized_bytes > self.limits.control_bytes {
            return ControlAdmission::Oversized;
        }
        if self.control.len() + 1 > self.limits.control_items
            || self.control_bytes.saturating_add(serialized_bytes) > self.limits.control_bytes
        {
            return ControlAdmission::Backpressure;
        }
        self.control.push_back(QueuedCancel {
            token: token.clone(),
            charge_bytes: serialized_bytes,
        });
        self.pending_cancels.insert(token);
        self.control_bytes += serialized_bytes;
        ControlAdmission::Admitted
    }

    /// Controls are returned before normal work. Popping a request transitions
    /// it atomically from queued to running and returns its safe-point token.
    pub fn next_event(&mut self) -> Option<SchedulerEvent> {
        if let Some(effect) = self.next_control() {
            return Some(SchedulerEvent::Control(effect));
        }
        loop {
            let item = self.normal.pop_front()?;
            self.remove_normal_charge(&item);
            if !self.sessions.contains_key(&item.session_id) {
                continue;
            }
            if item.initialized_gate {
                // Pipelined `initialized`: accepted only if the initialize
                // response has been ordered by now (FIFO dispatch runs it
                // after the initialize dispatch). A response still pending on
                // a saturated sink — or a failed initialize — means the
                // client cannot have seen the response, so the premature
                // notification is dropped ("initialized in any other state
                // is ignored").
                let session = self
                    .sessions
                    .get_mut(&item.session_id)
                    .expect("session existence checked above");
                if session.lifecycle != LifecycleState::InitializeResponded {
                    continue;
                }
                session.lifecycle = LifecycleState::Initialized;
            }
            let cancellation = item.response_token.as_ref().and_then(|token| {
                let outstanding = self.outstanding.get_mut(token)?;
                if outstanding.state != OutstandingState::Queued {
                    return None;
                }
                outstanding.state = OutstandingState::Running;
                Some(outstanding.cancellation.clone())
            });
            return Some(SchedulerEvent::Dispatch(DispatchItem {
                sequence: item.sequence,
                session_id: item.session_id,
                class: item.class,
                message: item.message,
                response_token: item.response_token,
                cancellation,
            }));
        }
    }

    /// Drains one cancellation/close control without touching the normal FIFO.
    ///
    /// Transport runtimes call this directly after admitting cancellation so a
    /// control can claim the response of a handler currently blocking the
    /// dispatch worker. Calling [`Self::next_event`] alone would defer that
    /// control until the synchronous handler returned.
    pub fn next_control(&mut self) -> Option<CancelEffect> {
        let control = self.control.pop_front()?;
        self.control_bytes -= control.charge_bytes;
        self.pending_cancels.remove(&control.token);
        Some(self.apply_cancel(&control.token))
    }

    /// After a side-effecting handler crosses its documented commit point,
    /// cancellation becomes a no-op and the normal result retains ownership.
    pub fn mark_non_cancellable(&mut self, token: &ResponseToken) -> bool {
        let Some(outstanding) = self.outstanding.get_mut(token) else {
            return false;
        };
        if outstanding.state != OutstandingState::Running {
            return false;
        }
        outstanding.cancel_committed = true;
        true
    }

    pub fn claim_response(
        &mut self,
        token: &ResponseToken,
        outcome: ResponseOutcome,
    ) -> ResponseClaim {
        if !self.sessions.contains_key(&token.session_id) {
            return ResponseClaim::UnknownOrSuperseded;
        }
        let Some(outstanding) = self.outstanding.get_mut(token) else {
            return ResponseClaim::UnknownOrSuperseded;
        };
        if outstanding.response_owner.is_some() {
            return ResponseClaim::AlreadyClaimed;
        }
        outstanding.response_owner = Some(ResponseOwner::Normal(outcome));
        outstanding.state = OutstandingState::Responded;
        ResponseClaim::Claimed
    }

    /// Verifies that the serialized response fits the bytes reserved at request
    /// admission. Oversize responses must close the transport; they cannot be
    /// dropped or placed on an unbounded writer queue.
    pub fn validate_response_size(
        &self,
        token: &ResponseToken,
        serialized_bytes: usize,
    ) -> Result<(), ResponseSizeError> {
        if !self.outstanding.contains_key(token) || !self.sessions.contains_key(&token.session_id) {
            return Err(ResponseSizeError::UnknownOrSuperseded);
        }
        if serialized_bytes > self.limits.response_reservation_bytes {
            return Err(ResponseSizeError::ExceedsReservation);
        }
        Ok(())
    }

    /// Releases the response reservation only after the response has been
    /// ordered onto the owning sink. Initialize/shutdown lifecycle transitions
    /// happen at this exact boundary.
    pub fn response_ordered(&mut self, token: &ResponseToken) -> ResponseOrder {
        if !self.sessions.contains_key(&token.session_id) {
            return ResponseOrder::UnknownOrSuperseded;
        }
        let Some(outstanding) = self.outstanding.remove(token) else {
            return ResponseOrder::UnknownOrSuperseded;
        };
        let Some(owner) = outstanding.response_owner else {
            self.outstanding.insert(token.clone(), outstanding);
            return ResponseOrder::UnknownOrSuperseded;
        };
        self.release_response();
        if let Some(session) = self.sessions.get_mut(&token.session_id) {
            match (outstanding.role, owner) {
                (RequestRole::Initialize, ResponseOwner::Normal(ResponseOutcome::Success)) => {
                    session.lifecycle = LifecycleState::InitializeResponded;
                }
                (RequestRole::Initialize, _) => {
                    // Initialize is accepted at most once even if its handler
                    // fails or a queued cancellation owns the response.
                    session.lifecycle = LifecycleState::Exited;
                    session.capabilities = None;
                }
                (RequestRole::Shutdown, ResponseOwner::Normal(ResponseOutcome::Success)) => {
                    session.lifecycle = LifecycleState::Shutdown;
                }
                (RequestRole::Shutdown, _) => {
                    // Shutdown handlers may already have committed process
                    // state; never reopen an initialized session on failure.
                    session.lifecycle = LifecycleState::Shutdown;
                }
                (RequestRole::Ordinary, _) => {}
            }
        }
        ResponseOrder::Ordered
    }

    #[must_use]
    pub fn outstanding_state(&self, token: &ResponseToken) -> Option<OutstandingState> {
        self.outstanding.get(token).map(|request| request.state)
    }

    pub fn close_session(&mut self, session_id: SessionId) -> Option<SessionTermination> {
        self.terminate(session_id, TerminationReason::TransportClosed)
    }

    fn reject_request(
        &mut self,
        session_id: SessionId,
        request_id: RequestId,
        error: ProtocolError,
    ) -> AdmissionResult {
        let token = ResponseToken {
            session_id,
            request_id,
        };
        if self.outstanding.contains_key(&token) {
            return AdmissionResult::DuplicateRequestId(token);
        }
        if !self.reserve_response() {
            return AdmissionResult::Backpressure(BackpressureReason::OutboundResponseCapacity);
        }
        self.outstanding.insert(
            token.clone(),
            OutstandingRequest {
                state: OutstandingState::Responded,
                cancellation: CancellationToken::new(),
                behavior: CancellationBehavior::NonCancellableWhenRunning,
                cancel_committed: true,
                role: RequestRole::Ordinary,
                response_owner: Some(ResponseOwner::ImmediateError),
            },
        );
        AdmissionResult::Rejected {
            response_token: token,
            error,
        }
    }

    fn reserve_response(&mut self) -> bool {
        if self.outbound_response_items + 1 > self.limits.outbound_response_items
            || self
                .outbound_response_bytes
                .saturating_add(self.limits.response_reservation_bytes)
                > self.limits.outbound_response_bytes
        {
            return false;
        }
        self.outbound_response_items += 1;
        self.outbound_response_bytes += self.limits.response_reservation_bytes;
        true
    }

    fn release_response(&mut self) {
        self.outbound_response_items -= 1;
        self.outbound_response_bytes -= self.limits.response_reservation_bytes;
    }

    /// True when `bytes` alone exceeds the class's byte capacity — such a
    /// message cannot be admitted even from an empty queue, so retrying is a
    /// livelock, not backpressure.
    fn permanently_oversized(&self, class: AdmissionClass, bytes: usize) -> bool {
        let byte_limit = if class.is_reserved() {
            self.limits.normal_bytes + self.limits.reserved_bytes
        } else {
            self.limits.normal_bytes
        };
        let class_limit = if class == AdmissionClass::Read {
            byte_limit.min(self.limits.read_bytes)
        } else {
            byte_limit
        };
        bytes > class_limit
    }

    fn has_normal_capacity(
        &self,
        class: AdmissionClass,
        bytes: usize,
        read_charge: bool,
    ) -> Result<(), BackpressureReason> {
        let (item_limit, byte_limit, reason) = if class.is_reserved() {
            (
                self.limits.normal_items + self.limits.reserved_items,
                self.limits.normal_bytes + self.limits.reserved_bytes,
                BackpressureReason::ReservedCapacity,
            )
        } else {
            (
                self.limits.normal_items,
                self.limits.normal_bytes,
                BackpressureReason::NormalCapacity,
            )
        };
        if self.normal_items + 1 > item_limit
            || self.normal_bytes.saturating_add(bytes) > byte_limit
        {
            return Err(reason);
        }
        if read_charge
            && (self.read_items + 1 > self.limits.read_items
                || self.read_bytes.saturating_add(bytes) > self.limits.read_bytes)
        {
            return Err(BackpressureReason::NormalCapacity);
        }
        Ok(())
    }

    fn try_coalesce_tail(
        &mut self,
        session_id: SessionId,
        key: &FullChangeKey,
        message: Message,
        bytes: usize,
        class: AdmissionClass,
    ) -> bool {
        let Some(tail) = self.normal.back() else {
            return false;
        };
        if tail.session_id != session_id || tail.full_change.as_ref() != Some(key) {
            return false;
        }
        let new_total_bytes = self
            .normal_bytes
            .saturating_sub(tail.charge_bytes)
            .saturating_add(bytes);
        let byte_limit = if class.is_reserved() {
            self.limits.normal_bytes + self.limits.reserved_bytes
        } else {
            self.limits.normal_bytes
        };
        if new_total_bytes > byte_limit {
            return false;
        }
        let sequence = IngressSequence(self.next_sequence);
        self.next_sequence = self.next_sequence.saturating_add(1);
        let tail = self.normal.back_mut().expect("tail checked above");
        self.normal_bytes = new_total_bytes;
        tail.sequence = sequence;
        tail.charge_bytes = bytes;
        tail.message = message;
        true
    }

    fn remove_normal_charge(&mut self, item: &QueuedItem) {
        self.normal_items -= 1;
        self.normal_bytes -= item.charge_bytes;
        if item.read_charge {
            self.read_items -= 1;
            self.read_bytes -= item.charge_bytes;
        }
    }

    fn apply_lifecycle_admission(&mut self, session_id: SessionId, action: LifecycleAction) {
        let Some(session) = self.sessions.get_mut(&session_id) else {
            return;
        };
        match action {
            LifecycleAction::BeginInitialize => {
                session.lifecycle = LifecycleState::InitializeResponding;
            }
            LifecycleAction::Initialized => {
                session.lifecycle = LifecycleState::Initialized;
            }
            LifecycleAction::BeginShutdown => {
                session.lifecycle = LifecycleState::ShutdownResponding;
            }
            LifecycleAction::NoChange
            | LifecycleAction::QueueInitialized
            | LifecycleAction::Reject(_, _)
            | LifecycleAction::Drop(_)
            | LifecycleAction::Exit(_) => {}
        }
    }

    fn apply_cancel(&mut self, token: &ResponseToken) -> CancelEffect {
        let Some(state) = self.outstanding.get(token).map(|request| request.state) else {
            return CancelEffect::Noop;
        };
        if state == OutstandingState::Responded {
            return CancelEffect::Noop;
        }
        if state == OutstandingState::Queued {
            if let Some(index) = self
                .normal
                .iter()
                .position(|item| item.response_token.as_ref() == Some(token))
            {
                let removed = self.normal.remove(index).expect("index came from queue");
                self.remove_normal_charge(&removed);
            }
            let outstanding = self
                .outstanding
                .get_mut(token)
                .expect("state read from outstanding map");
            outstanding.state = OutstandingState::Responded;
            outstanding.response_owner = Some(ResponseOwner::Canceled);
            return CancelEffect::Respond {
                response_token: token.clone(),
                was_running: false,
                signaled_operation: false,
                error: ProtocolError::new(
                    ProtocolErrorKind::RequestCanceled,
                    "request canceled before dispatch",
                ),
            };
        }

        let outstanding = self
            .outstanding
            .get_mut(token)
            .expect("state read from outstanding map");
        if outstanding.cancel_committed
            || outstanding.behavior == CancellationBehavior::NonCancellableWhenRunning
        {
            return CancelEffect::Noop;
        }
        let signaled = outstanding.behavior == CancellationBehavior::SignalAtSafePoints;
        if signaled {
            outstanding.cancellation.cancel();
        }
        outstanding.state = OutstandingState::Responded;
        outstanding.response_owner = Some(ResponseOwner::Canceled);
        CancelEffect::Respond {
            response_token: token.clone(),
            was_running: true,
            signaled_operation: signaled,
            error: ProtocolError::new(
                ProtocolErrorKind::RequestCanceled,
                "request canceled while running",
            ),
        }
    }

    fn terminate(
        &mut self,
        session_id: SessionId,
        reason: TerminationReason,
    ) -> Option<SessionTermination> {
        let session = self.sessions.remove(&session_id)?;
        if self.active_browser == Some(session_id) {
            self.active_browser = None;
        }
        let final_lifecycle = if reason == TerminationReason::BrowserSuperseded {
            LifecycleState::Superseded
        } else {
            LifecycleState::Exited
        };

        let mut dropped_normal_items = 0;
        let mut retained = VecDeque::with_capacity(self.normal.len());
        while let Some(item) = self.normal.pop_front() {
            if item.session_id == session_id {
                self.remove_normal_charge(&item);
                dropped_normal_items += 1;
            } else {
                retained.push_back(item);
            }
        }
        self.normal = retained;

        let mut dropped_control_items = 0;
        let mut retained_control = VecDeque::with_capacity(self.control.len());
        while let Some(control) = self.control.pop_front() {
            if control.token.session_id == session_id {
                self.control_bytes -= control.charge_bytes;
                self.pending_cancels.remove(&control.token);
                dropped_control_items += 1;
            } else {
                retained_control.push_back(control);
            }
        }
        self.control = retained_control;

        let mut tokens: Vec<_> = self
            .outstanding
            .keys()
            .filter(|token| token.session_id == session_id)
            .cloned()
            .collect();
        tokens.sort();
        for token in &tokens {
            if let Some(outstanding) = self.outstanding.remove(token) {
                outstanding.cancellation.cancel();
                self.release_response();
            }
        }

        Some(SessionTermination {
            session_id,
            prior_lifecycle: session.lifecycle,
            final_lifecycle,
            reason,
            dropped_normal_items,
            dropped_control_items,
            revoked_response_tokens: tokens,
            owned_documents: session
                .documents
                .into_iter()
                .map(|(uri, version)| OwnedDocument { uri, version })
                .collect(),
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LifecycleAction {
    NoChange,
    BeginInitialize,
    Initialized,
    /// Pipelined `initialized` while `initialize` is still responding: admit
    /// FIFO, decide acceptance at dispatch (see [`QueuedItem::initialized_gate`]).
    QueueInitialized,
    BeginShutdown,
    Reject(ProtocolErrorKind, &'static str),
    Drop(DropReason),
    Exit(bool),
}

fn lifecycle_action(state: LifecycleState, message: &Message) -> LifecycleAction {
    match message {
        Message::Request(request) if request.method == "initialize" => {
            if state == LifecycleState::PreInitialize {
                LifecycleAction::BeginInitialize
            } else {
                LifecycleAction::Reject(
                    ProtocolErrorKind::InvalidRequest,
                    "initialize may be requested exactly once",
                )
            }
        }
        Message::Request(request) if request.method == "shutdown" => {
            if state == LifecycleState::Initialized {
                LifecycleAction::BeginShutdown
            } else if matches!(
                state,
                LifecycleState::PreInitialize
                    | LifecycleState::InitializeResponding
                    | LifecycleState::InitializeResponded
            ) {
                LifecycleAction::Reject(
                    ProtocolErrorKind::ServerNotInitialized,
                    "shutdown requested before initialized",
                )
            } else {
                LifecycleAction::Reject(
                    ProtocolErrorKind::InvalidRequest,
                    "shutdown is not valid in the current lifecycle state",
                )
            }
        }
        Message::Request(_) => match state {
            LifecycleState::Initialized => LifecycleAction::NoChange,
            LifecycleState::PreInitialize
            | LifecycleState::InitializeResponding
            | LifecycleState::InitializeResponded => LifecycleAction::Reject(
                ProtocolErrorKind::ServerNotInitialized,
                "request received before initialized",
            ),
            LifecycleState::ShutdownResponding
            | LifecycleState::Shutdown
            | LifecycleState::Exited
            | LifecycleState::Superseded => LifecycleAction::Reject(
                ProtocolErrorKind::InvalidRequest,
                "request received after shutdown began",
            ),
        },
        Message::Notification(notification) if notification.method == "initialized" => {
            match state {
                LifecycleState::InitializeResponded => LifecycleAction::Initialized,
                // A pipelined client may send `initialized` before it could
                // have read the initialize response. Queue it FIFO behind the
                // in-flight initialize; dispatch decides acceptance.
                LifecycleState::InitializeResponding => LifecycleAction::QueueInitialized,
                _ => LifecycleAction::Drop(DropReason::InitializedInWrongState),
            }
        }
        Message::Notification(notification) if notification.method == "exit" => {
            LifecycleAction::Exit(state == LifecycleState::Shutdown)
        }
        Message::Notification(_) => match state {
            LifecycleState::Initialized => LifecycleAction::NoChange,
            LifecycleState::PreInitialize
            | LifecycleState::InitializeResponding
            | LifecycleState::InitializeResponded => {
                LifecycleAction::Drop(DropReason::NotificationBeforeInitialized)
            }
            LifecycleState::ShutdownResponding
            | LifecycleState::Shutdown
            | LifecycleState::Exited
            | LifecycleState::Superseded => {
                LifecycleAction::Drop(DropReason::NotificationAfterShutdown)
            }
        },
        Message::Response(_) => match state {
            LifecycleState::Initialized => LifecycleAction::NoChange,
            LifecycleState::PreInitialize
            | LifecycleState::InitializeResponding
            | LifecycleState::InitializeResponded => {
                LifecycleAction::Drop(DropReason::NotificationBeforeInitialized)
            }
            LifecycleState::ShutdownResponding
            | LifecycleState::Shutdown
            | LifecycleState::Exited
            | LifecycleState::Superseded => {
                LifecycleAction::Drop(DropReason::NotificationAfterShutdown)
            }
        },
    }
}

fn stronger_class(forced: AdmissionClass, requested: AdmissionClass) -> AdmissionClass {
    if forced.is_reserved() {
        forced
    } else if requested.is_reserved() {
        requested
    } else if forced == AdmissionClass::Read || requested == AdmissionClass::Read {
        AdmissionClass::Read
    } else {
        AdmissionClass::Other
    }
}

/// Callers may harden an ordinary message's cancellation behavior but can
/// never downgrade the forced protection of lifecycle and side-effecting
/// methods: reopening cancellation for a running `initialize`/`shutdown`/
/// `workspace/executeCommand` would reintroduce the canceled-response vs
/// committed-effect race.
fn stronger_cancellation(
    forced: CancellationBehavior,
    requested: CancellationBehavior,
) -> CancellationBehavior {
    const fn strength(behavior: CancellationBehavior) -> u8 {
        match behavior {
            CancellationBehavior::SignalAtSafePoints => 0,
            CancellationBehavior::ClaimResponseOnly => 1,
            CancellationBehavior::NonCancellableWhenRunning => 2,
        }
    }
    if strength(requested) >= strength(forced) {
        requested
    } else {
        forced
    }
}

fn full_change_key(message: &Message) -> Option<FullChangeKey> {
    let Message::Notification(notification) = message else {
        return None;
    };
    if notification.method != "textDocument/didChange" {
        return None;
    }
    let uri = notification.params.pointer("/textDocument/uri")?.as_str()?;
    notification
        .params
        .pointer("/textDocument/version")?
        .as_i64()?;
    let changes = notification.params.pointer("/contentChanges")?.as_array()?;
    let [change] = changes.as_slice() else {
        return None;
    };
    if change
        .get("text")
        .and_then(serde_json::Value::as_str)
        .is_none()
        || change.get("range").is_some_and(|range| !range.is_null())
        || change
            .get("rangeLength")
            .is_some_and(|length| !length.is_null())
    {
        return None;
    }
    Some(FullChangeKey {
        uri: uri.to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn limits() -> IngressLimits {
        IngressLimits {
            normal_items: 2,
            normal_bytes: 200,
            reserved_items: 2,
            reserved_bytes: 200,
            read_items: 1,
            read_bytes: 100,
            control_items: 2,
            control_bytes: 100,
            outbound_response_items: 4,
            outbound_response_bytes: 400,
            response_reservation_bytes: 100,
        }
    }

    fn request(id: i32, method: &str) -> Message {
        Message::Request(lsp_server::Request {
            id: RequestId::from(id),
            method: method.to_string(),
            params: serde_json::Value::Null,
        })
    }

    fn notification(method: &str, params: serde_json::Value) -> Message {
        Message::Notification(lsp_server::Notification {
            method: method.to_string(),
            params,
        })
    }

    fn full_change(uri: &str, version: i32, text: &str) -> Message {
        notification(
            "textDocument/didChange",
            serde_json::json!({
                "textDocument": { "uri": uri, "version": version },
                "contentChanges": [{ "text": text }],
            }),
        )
    }

    fn initialize(scheduler: &mut IngressScheduler, session: SessionId, id: i32) {
        let AdmissionResult::Admitted {
            response_token: Some(token),
            ..
        } = scheduler.admit_message(session, request(id, "initialize"), 20)
        else {
            panic!("initialize must be admitted");
        };
        assert_eq!(
            scheduler.lifecycle(session),
            Some(LifecycleState::InitializeResponding)
        );
        let SchedulerEvent::Dispatch(dispatch) = scheduler.next_event().unwrap() else {
            panic!("expected initialize dispatch");
        };
        assert_eq!(dispatch.response_token.as_ref(), Some(&token));
        assert_eq!(
            scheduler.claim_response(&token, ResponseOutcome::Success),
            ResponseClaim::Claimed
        );
        assert_eq!(
            scheduler.lifecycle(session),
            Some(LifecycleState::InitializeResponding),
            "claiming is not the outbound ordering boundary"
        );
        assert_eq!(scheduler.response_ordered(&token), ResponseOrder::Ordered);
        assert_eq!(
            scheduler.lifecycle(session),
            Some(LifecycleState::InitializeResponded)
        );
        assert!(matches!(
            scheduler.admit_message(
                session,
                notification("initialized", serde_json::Value::Null),
                10,
            ),
            AdmissionResult::Admitted { .. }
        ));
        assert_eq!(
            scheduler.lifecycle(session),
            Some(LifecycleState::Initialized)
        );
        let _ = scheduler.next_event();
    }

    #[test]
    fn lifecycle_advances_only_after_ordered_responses() {
        let mut scheduler = IngressScheduler::new(limits()).unwrap();
        let session = scheduler.open_session(TransportKind::Stdio).session_id;

        assert!(matches!(
            scheduler.admit_message(session, request(1, "textDocument/hover"), 20),
            AdmissionResult::Rejected {
                error: ProtocolError {
                    kind: ProtocolErrorKind::ServerNotInitialized,
                    ..
                },
                ..
            }
        ));
        initialize(&mut scheduler, session, 2);

        let AdmissionResult::Admitted {
            response_token: Some(shutdown),
            ..
        } = scheduler.admit_message(session, request(3, "shutdown"), 20)
        else {
            panic!("shutdown must be admitted");
        };
        assert_eq!(
            scheduler.lifecycle(session),
            Some(LifecycleState::ShutdownResponding)
        );
        assert!(matches!(
            scheduler.admit_message(session, request(4, "textDocument/hover"), 20),
            AdmissionResult::Rejected {
                error: ProtocolError {
                    kind: ProtocolErrorKind::InvalidRequest,
                    ..
                },
                ..
            }
        ));
        let _ = scheduler.next_event();
        assert_eq!(
            scheduler.claim_response(&shutdown, ResponseOutcome::Success),
            ResponseClaim::Claimed
        );
        assert_eq!(
            scheduler.response_ordered(&shutdown),
            ResponseOrder::Ordered
        );
        assert_eq!(scheduler.lifecycle(session), Some(LifecycleState::Shutdown));
        assert!(matches!(
            scheduler.admit_message(session, notification("exit", serde_json::Value::Null), 5),
            AdmissionResult::Exited(SessionTermination {
                reason: TerminationReason::NormalExit,
                ..
            })
        ));
    }

    #[test]
    fn duplicate_initialize_and_wrong_state_initialized_are_deterministic() {
        let mut scheduler = IngressScheduler::new(limits()).unwrap();
        let session = scheduler.open_session(TransportKind::Browser).session_id;
        assert_eq!(
            scheduler.admit_message(
                session,
                notification("initialized", serde_json::Value::Null),
                5
            ),
            AdmissionResult::Dropped(DropReason::InitializedInWrongState)
        );
        assert!(matches!(
            scheduler.admit_message(session, request(1, "initialize"), 20),
            AdmissionResult::Admitted { .. }
        ));
        assert!(matches!(
            scheduler.admit_message(session, request(2, "initialize"), 20),
            AdmissionResult::Rejected {
                error: ProtocolError {
                    kind: ProtocolErrorKind::InvalidRequest,
                    ..
                },
                ..
            }
        ));
    }

    #[test]
    fn read_overload_rejects_new_request_while_reserve_admits_mutation() {
        let mut scheduler = IngressScheduler::new(limits()).unwrap();
        let session = scheduler.open_session(TransportKind::Stdio).session_id;
        initialize(&mut scheduler, session, 1);

        assert!(matches!(
            scheduler.admit_message(session, request(2, "textDocument/hover"), 80),
            AdmissionResult::Admitted { .. }
        ));
        let rejected = scheduler.admit_message(session, request(3, "textDocument/inlayHint"), 20);
        assert!(matches!(
            rejected,
            AdmissionResult::Rejected {
                error: ProtocolError {
                    kind: ProtocolErrorKind::RequestFailed,
                    ..
                },
                ..
            }
        ));
        assert!(matches!(
            scheduler.admit_message(session, full_change("file:///a.baml", 2, "new"), 150),
            AdmissionResult::Admitted { .. }
        ));
        assert_eq!(scheduler.usage().items, 2);
    }

    #[test]
    fn protected_capacity_backpressures_without_evicting_admitted_work() {
        let mut tight = limits();
        tight.normal_items = 1;
        tight.reserved_items = 1;
        let mut scheduler = IngressScheduler::new(tight).unwrap();
        let session = scheduler.open_session(TransportKind::Stdio).session_id;
        initialize(&mut scheduler, session, 1);

        assert!(matches!(
            scheduler.admit_message(session, full_change("file:///a.baml", 2, "a"), 20),
            AdmissionResult::Admitted { .. }
        ));
        assert!(matches!(
            scheduler.admit_message(session, full_change("file:///b.baml", 2, "b"), 20),
            AdmissionResult::Admitted { .. }
        ));
        assert_eq!(
            scheduler.admit_message(session, full_change("file:///c.baml", 2, "c"), 20),
            AdmissionResult::Backpressure(BackpressureReason::ReservedCapacity)
        );
        assert_eq!(scheduler.usage().items, 2);
    }

    #[test]
    fn only_adjacent_same_uri_full_changes_coalesce() {
        let mut scheduler = IngressScheduler::new(limits()).unwrap();
        let session = scheduler.open_session(TransportKind::Stdio).session_id;
        initialize(&mut scheduler, session, 1);

        let first = scheduler.admit_message(session, full_change("file:///a.baml", 2, "old"), 80);
        let second = scheduler.admit_message(session, full_change("file:///a.baml", 3, "new"), 40);
        assert!(matches!(
            first,
            AdmissionResult::Admitted {
                coalesced: false,
                ..
            }
        ));
        assert!(matches!(
            second,
            AdmissionResult::Admitted {
                coalesced: true,
                ..
            }
        ));
        assert_eq!(scheduler.usage().items, 1);
        assert_eq!(scheduler.usage().bytes, 40);

        let SchedulerEvent::Dispatch(item) = scheduler.next_event().unwrap() else {
            panic!("expected coalesced change");
        };
        let Message::Notification(notification) = item.message else {
            panic!("expected notification");
        };
        assert_eq!(notification.params["textDocument"]["version"], 3);
        assert_eq!(notification.params["contentChanges"][0]["text"], "new");

        assert!(matches!(
            scheduler.admit_message(session, full_change("file:///a.baml", 4, "four"), 20),
            AdmissionResult::Admitted {
                coalesced: false,
                ..
            }
        ));
        assert!(matches!(
            scheduler.admit_message(session, full_change("file:///b.baml", 4, "other"), 20),
            AdmissionResult::Admitted {
                coalesced: false,
                ..
            }
        ));
        assert!(matches!(
            scheduler.admit_message(session, full_change("file:///a.baml", 5, "five"), 20),
            AdmissionResult::Admitted {
                coalesced: false,
                ..
            }
        ));
        assert_eq!(scheduler.usage().items, 3);
    }

    #[test]
    fn request_and_incremental_change_are_coalescing_barriers_and_fifo_is_preserved() {
        let mut roomy = limits();
        roomy.normal_items = 8;
        roomy.normal_bytes = 800;
        roomy.reserved_items = 8;
        roomy.reserved_bytes = 800;
        roomy.read_items = 4;
        roomy.read_bytes = 400;
        roomy.outbound_response_items = 8;
        roomy.outbound_response_bytes = 800;
        let mut scheduler = IngressScheduler::new(roomy).unwrap();
        let session = scheduler.open_session(TransportKind::Stdio).session_id;
        initialize(&mut scheduler, session, 1);

        let first = scheduler.admit_message(session, full_change("file:///a.baml", 2, "first"), 20);
        let request = scheduler.admit_message(session, request(2, "textDocument/hover"), 20);
        let after_request =
            scheduler.admit_message(session, full_change("file:///a.baml", 3, "second"), 20);
        let incremental = notification(
            "textDocument/didChange",
            serde_json::json!({
                "textDocument": { "uri": "file:///a.baml", "version": 4 },
                "contentChanges": [{
                    "range": {
                        "start": { "line": 0, "character": 0 },
                        "end": { "line": 0, "character": 1 }
                    },
                    "text": "x"
                }]
            }),
        );
        let incremental = scheduler.admit_message(session, incremental, 20);
        assert!(matches!(
            first,
            AdmissionResult::Admitted {
                coalesced: false,
                ..
            }
        ));
        assert!(matches!(request, AdmissionResult::Admitted { .. }));
        assert!(matches!(
            after_request,
            AdmissionResult::Admitted {
                coalesced: false,
                ..
            }
        ));
        assert!(matches!(
            incremental,
            AdmissionResult::Admitted {
                coalesced: false,
                ..
            }
        ));

        let mut sequences = Vec::new();
        for _ in 0..4 {
            let SchedulerEvent::Dispatch(item) = scheduler.next_event().unwrap() else {
                panic!("normal messages should dispatch in FIFO order");
            };
            sequences.push(item.sequence.get());
        }
        assert!(sequences.windows(2).all(|pair| pair[0] < pair[1]));
    }

    #[test]
    fn outbound_and_control_paths_enforce_item_and_byte_bounds() {
        let mut bounded = limits();
        bounded.outbound_response_items = 1;
        bounded.outbound_response_bytes = 100;
        bounded.response_reservation_bytes = 100;
        bounded.control_items = 1;
        bounded.control_bytes = 10;
        let mut scheduler = IngressScheduler::new(bounded).unwrap();
        let session = scheduler.open_session(TransportKind::Stdio).session_id;
        initialize(&mut scheduler, session, 1);

        let AdmissionResult::Admitted {
            response_token: Some(token),
            ..
        } = scheduler.admit_message(session, request(2, "textDocument/hover"), 20)
        else {
            panic!("first response slot must be reserved");
        };
        assert_eq!(
            scheduler.admit_message(session, request(3, "textDocument/hover"), 20),
            AdmissionResult::Backpressure(BackpressureReason::OutboundResponseCapacity)
        );
        assert_eq!(
            scheduler.validate_response_size(&token, 101),
            Err(ResponseSizeError::ExceedsReservation)
        );
        assert_eq!(
            scheduler.admit_cancel(session, RequestId::from(2), 10),
            ControlAdmission::Admitted
        );
        assert_eq!(
            scheduler.admit_cancel(session, RequestId::from(3), 1),
            ControlAdmission::Backpressure
        );
        // A control that alone exceeds the control byte budget can never be
        // admitted: it is dropped, not retried forever.
        assert_eq!(
            scheduler.admit_cancel(session, RequestId::from(4), 11),
            ControlAdmission::Oversized
        );
    }

    #[test]
    fn browser_replacement_starts_with_fresh_capabilities() {
        let mut scheduler = IngressScheduler::new(limits()).unwrap();
        let old = scheduler.open_session(TransportKind::Browser).session_id;
        assert!(matches!(
            scheduler.admit_message(old, request(1, "initialize"), 20),
            AdmissionResult::Admitted { .. }
        ));
        scheduler
            .set_negotiated_capabilities(old, serde_json::json!({ "positionEncoding": "utf-8" }))
            .unwrap();
        assert!(scheduler.negotiated_capabilities(old).is_some());

        let replacement = scheduler.open_session(TransportKind::Browser);
        assert!(replacement.takeover.is_some());
        assert!(
            scheduler
                .negotiated_capabilities(replacement.session_id)
                .is_none()
        );
        assert_eq!(
            scheduler.lifecycle(replacement.session_id),
            Some(LifecycleState::PreInitialize)
        );
    }

    #[test]
    fn client_responses_share_fifo_ingress_after_initialize() {
        let mut scheduler = IngressScheduler::new(limits()).unwrap();
        let session = scheduler.open_session(TransportKind::Stdio).session_id;
        initialize(&mut scheduler, session, 1);
        let response = Message::Response(lsp_server::Response {
            id: RequestId::from(99),
            result: Some(serde_json::Value::Null),
            error: None,
        });
        assert!(matches!(
            scheduler.admit_message(session, response, 20),
            AdmissionResult::Admitted {
                response_token: None,
                ..
            }
        ));
        let SchedulerEvent::Dispatch(item) = scheduler.next_event().unwrap() else {
            panic!("client response must be dispatched");
        };
        assert!(matches!(item.message, Message::Response(_)));
    }

    #[test]
    fn queued_cancel_removes_work_and_normal_result_loses_response_race() {
        let mut scheduler = IngressScheduler::new(limits()).unwrap();
        let session = scheduler.open_session(TransportKind::Stdio).session_id;
        initialize(&mut scheduler, session, 1);
        let AdmissionResult::Admitted {
            response_token: Some(token),
            ..
        } = scheduler.admit_message(session, request(2, "textDocument/hover"), 20)
        else {
            panic!("request must be admitted");
        };
        assert_eq!(
            scheduler.admit_cancel(session, RequestId::from(2), 10),
            ControlAdmission::Admitted
        );
        assert_eq!(
            scheduler.admit_cancel(session, RequestId::from(2), 10),
            ControlAdmission::Duplicate
        );
        let SchedulerEvent::Control(CancelEffect::Respond {
            response_token,
            was_running: false,
            ..
        }) = scheduler.next_event().unwrap()
        else {
            panic!("expected queued cancel response");
        };
        assert_eq!(response_token, token);
        assert_eq!(scheduler.usage().items, 0);
        assert_eq!(
            scheduler.claim_response(&token, ResponseOutcome::Success),
            ResponseClaim::AlreadyClaimed
        );
        assert_eq!(scheduler.response_ordered(&token), ResponseOrder::Ordered);
    }

    #[test]
    fn running_cancel_signals_once_and_late_result_is_discarded() {
        let mut scheduler = IngressScheduler::new(limits()).unwrap();
        let session = scheduler.open_session(TransportKind::Stdio).session_id;
        initialize(&mut scheduler, session, 1);
        let AdmissionResult::Admitted {
            response_token: Some(token),
            ..
        } = scheduler.admit_message(session, request(2, "textDocument/hover"), 20)
        else {
            panic!("request must be admitted");
        };
        let SchedulerEvent::Dispatch(dispatch) = scheduler.next_event().unwrap() else {
            panic!("expected request dispatch");
        };
        let cancellation = dispatch.cancellation.unwrap();
        assert!(!cancellation.is_cancelled());
        assert_eq!(
            scheduler.admit_cancel(session, RequestId::from(2), 10),
            ControlAdmission::Admitted
        );
        let SchedulerEvent::Control(CancelEffect::Respond {
            was_running: true,
            signaled_operation: true,
            ..
        }) = scheduler.next_event().unwrap()
        else {
            panic!("expected running cancel response");
        };
        assert!(cancellation.is_cancelled());
        assert_eq!(
            scheduler.claim_response(&token, ResponseOutcome::Success),
            ResponseClaim::AlreadyClaimed
        );
        assert_eq!(
            scheduler.admit_cancel(session, RequestId::from(2), 10),
            ControlAdmission::Admitted
        );
        assert!(matches!(
            scheduler.next_event(),
            Some(SchedulerEvent::Control(CancelEffect::Noop))
        ));
    }

    #[test]
    fn committed_side_effect_ignores_running_cancel() {
        let mut scheduler = IngressScheduler::new(limits()).unwrap();
        let session = scheduler.open_session(TransportKind::Stdio).session_id;
        initialize(&mut scheduler, session, 1);
        let AdmissionResult::Admitted {
            response_token: Some(token),
            ..
        } = scheduler.admit_message_with_policy(
            session,
            request(2, "custom/commit"),
            20,
            MessagePolicy::side_effecting(CancellationBehavior::ClaimResponseOnly),
        )
        else {
            panic!("request must be admitted");
        };
        let _ = scheduler.next_event();
        assert!(scheduler.mark_non_cancellable(&token));
        assert_eq!(
            scheduler.admit_cancel(session, RequestId::from(2), 10),
            ControlAdmission::Admitted
        );
        assert!(matches!(
            scheduler.next_event(),
            Some(SchedulerEvent::Control(CancelEffect::Noop))
        ));
        assert_eq!(
            scheduler.claim_response(&token, ResponseOutcome::Success),
            ResponseClaim::Claimed
        );
    }

    /// A custom policy must not reopen the canceled-response vs
    /// committed-effect race on protected methods: the forced
    /// `NonCancellableWhenRunning` survives an attempted downgrade.
    #[test]
    fn forced_cancellation_of_protected_methods_cannot_be_downgraded() {
        let mut scheduler = IngressScheduler::new(limits()).unwrap();
        let session = scheduler.open_session(TransportKind::Stdio).session_id;
        initialize(&mut scheduler, session, 1);

        let AdmissionResult::Admitted {
            response_token: Some(token),
            ..
        } = scheduler.admit_message_with_policy(
            session,
            request(2, "workspace/executeCommand"),
            20,
            MessagePolicy::side_effecting(CancellationBehavior::SignalAtSafePoints),
        )
        else {
            panic!("request must be admitted");
        };
        let _ = scheduler.next_event();

        // Running: cancellation must be a no-op despite the requested policy.
        assert_eq!(
            scheduler.admit_cancel(session, RequestId::from(2), 10),
            ControlAdmission::Admitted
        );
        assert!(matches!(
            scheduler.next_event(),
            Some(SchedulerEvent::Control(CancelEffect::Noop))
        ));
        assert_eq!(
            scheduler.claim_response(&token, ResponseOutcome::Success),
            ResponseClaim::Claimed
        );
    }

    #[test]
    fn execute_command_cancel_is_linearized_at_dispatch() {
        let mut scheduler = IngressScheduler::new(limits()).unwrap();
        let session = scheduler.open_session(TransportKind::Stdio).session_id;
        initialize(&mut scheduler, session, 1);

        let AdmissionResult::Admitted {
            response_token: Some(queued_token),
            ..
        } = scheduler.admit_message(session, request(2, "workspace/executeCommand"), 20)
        else {
            panic!("queued command must be admitted");
        };
        assert_eq!(
            scheduler.admit_cancel(session, RequestId::from(2), 10),
            ControlAdmission::Admitted
        );
        let Some(SchedulerEvent::Control(CancelEffect::Respond {
            response_token,
            was_running: false,
            signaled_operation: false,
            error,
        })) = scheduler.next_event()
        else {
            panic!("cancellation before command dispatch must own the response");
        };
        assert_eq!(response_token, queued_token);
        assert_eq!(error.kind, ProtocolErrorKind::RequestCanceled);
        assert_eq!(
            scheduler.claim_response(&queued_token, ResponseOutcome::Success),
            ResponseClaim::AlreadyClaimed
        );

        let AdmissionResult::Admitted {
            response_token: Some(running_token),
            ..
        } = scheduler.admit_message(session, request(3, "workspace/executeCommand"), 20)
        else {
            panic!("running command must be admitted");
        };
        let Some(SchedulerEvent::Dispatch(dispatch)) = scheduler.next_event() else {
            panic!("command must transition to running at dispatch");
        };
        assert_eq!(dispatch.response_token.as_ref(), Some(&running_token));
        assert!(!dispatch.cancellation.unwrap().is_cancelled());
        assert_eq!(
            scheduler.admit_cancel(session, RequestId::from(3), 10),
            ControlAdmission::Admitted
        );
        assert!(
            matches!(
                scheduler.next_event(),
                Some(SchedulerEvent::Control(CancelEffect::Noop))
            ),
            "cancellation after dispatch must not report a canceled side effect"
        );
        assert_eq!(
            scheduler.claim_response(&running_token, ResponseOutcome::Success),
            ResponseClaim::Claimed
        );
    }

    #[test]
    fn browser_takeover_revokes_old_work_docs_and_response_identity() {
        let mut scheduler = IngressScheduler::new(limits()).unwrap();
        let old = scheduler.open_session(TransportKind::Browser).session_id;
        initialize(&mut scheduler, old, 1);
        assert!(scheduler.record_document_open(old, "file:///old.baml", Some(7)));
        let AdmissionResult::Admitted {
            response_token: Some(old_token),
            ..
        } = scheduler.admit_message(old, request(9, "textDocument/hover"), 20)
        else {
            panic!("old request must be admitted");
        };
        let _ = scheduler.next_event();

        let replacement = scheduler.open_session(TransportKind::Browser);
        let takeover = replacement
            .takeover
            .expect("old browser must be superseded");
        assert_eq!(takeover.session_id, old);
        assert_eq!(takeover.final_lifecycle, LifecycleState::Superseded);
        assert_eq!(takeover.owned_documents[0].uri, "file:///old.baml");
        assert_eq!(scheduler.active_browser(), Some(replacement.session_id));
        assert_eq!(
            scheduler.claim_response(&old_token, ResponseOutcome::Success),
            ResponseClaim::UnknownOrSuperseded
        );

        // Reusing request id 9 in the replacement is safe because the session
        // component of ResponseToken differs.
        assert!(matches!(
            scheduler.admit_message(replacement.session_id, request(9, "initialize"), 20),
            AdmissionResult::Admitted {
                response_token: Some(_),
                ..
            }
        ));
    }

    #[test]
    fn control_path_remains_available_when_normal_queue_is_saturated() {
        let mut scheduler = IngressScheduler::new(limits()).unwrap();
        let session = scheduler.open_session(TransportKind::Stdio).session_id;
        initialize(&mut scheduler, session, 1);
        let AdmissionResult::Admitted {
            response_token: Some(_),
            ..
        } = scheduler.admit_message(session, request(2, "textDocument/hover"), 80)
        else {
            panic!("read must be admitted");
        };
        assert_eq!(
            scheduler.admit_cancel(session, RequestId::from(2), 10),
            ControlAdmission::Admitted
        );
        assert!(matches!(
            scheduler.next_event(),
            Some(SchedulerEvent::Control(CancelEffect::Respond { .. }))
        ));
    }

    /// A pipelined client (initialize + initialized in one burst, without
    /// waiting for the response) must still reach `Initialized`: the early
    /// `initialized` queues FIFO behind the in-flight initialize and is
    /// accepted at dispatch, strictly after the response was ordered.
    #[test]
    fn pipelined_initialized_is_accepted_after_the_response_is_ordered() {
        let mut scheduler = IngressScheduler::new(limits()).unwrap();
        let session = scheduler.open_session(TransportKind::Stdio).session_id;

        let AdmissionResult::Admitted {
            response_token: Some(token),
            ..
        } = scheduler.admit_message(session, request(1, "initialize"), 20)
        else {
            panic!("initialize must be admitted");
        };
        assert!(matches!(
            scheduler.admit_message(
                session,
                notification("initialized", serde_json::Value::Null),
                10,
            ),
            AdmissionResult::Admitted { .. }
        ));
        // Still not initialized: requests between the response and a *valid*
        // initialized are rejected, and the state has not advanced.
        assert_eq!(
            scheduler.lifecycle(session),
            Some(LifecycleState::InitializeResponding)
        );
        assert!(matches!(
            scheduler.admit_message(session, request(2, "textDocument/hover"), 20),
            AdmissionResult::Rejected {
                error: ProtocolError {
                    kind: ProtocolErrorKind::ServerNotInitialized,
                    ..
                },
                ..
            }
        ));

        // FIFO: initialize dispatches first; its response is ordered.
        let SchedulerEvent::Dispatch(dispatch) = scheduler.next_event().unwrap() else {
            panic!("expected initialize dispatch");
        };
        assert_eq!(dispatch.response_token.as_ref(), Some(&token));
        assert_eq!(
            scheduler.claim_response(&token, ResponseOutcome::Success),
            ResponseClaim::Claimed
        );
        assert_eq!(scheduler.response_ordered(&token), ResponseOrder::Ordered);

        // The queued initialized now dispatches and advances the lifecycle.
        let SchedulerEvent::Dispatch(dispatch) = scheduler.next_event().unwrap() else {
            panic!("expected the queued initialized to dispatch");
        };
        assert!(matches!(
            dispatch.message,
            Message::Notification(ref n) if n.method == "initialized"
        ));
        assert_eq!(
            scheduler.lifecycle(session),
            Some(LifecycleState::Initialized)
        );
    }

    /// If the initialize response was never ordered (failed handler → the
    /// session is torn down; saturated sink → still responding), a queued
    /// pipelined `initialized` is dropped at dispatch instead of advancing
    /// the lifecycle past an undelivered response.
    #[test]
    fn pipelined_initialized_is_dropped_when_the_response_was_not_ordered() {
        let mut scheduler = IngressScheduler::new(limits()).unwrap();
        let session = scheduler.open_session(TransportKind::Stdio).session_id;

        assert!(matches!(
            scheduler.admit_message(session, request(1, "initialize"), 20),
            AdmissionResult::Admitted { .. }
        ));
        assert!(matches!(
            scheduler.admit_message(
                session,
                notification("initialized", serde_json::Value::Null),
                10,
            ),
            AdmissionResult::Admitted { .. }
        ));
        let Some(SchedulerEvent::Dispatch(_)) = scheduler.next_event() else {
            panic!("expected initialize dispatch");
        };
        // The response is never claimed/ordered: the queued initialized is
        // skipped and the state stays InitializeResponding.
        assert!(scheduler.next_event().is_none());
        assert_eq!(
            scheduler.lifecycle(session),
            Some(LifecycleState::InitializeResponding)
        );
    }

    /// Defect containment: a message that can never fit its class's byte
    /// budget must not close the session or backpressure forever — a request
    /// gets one typed `RequestFailed`, a notification/client response is
    /// dropped, and later traffic on the same session is admitted normally.
    #[test]
    fn permanently_oversized_messages_are_contained_per_message() {
        let mut scheduler = IngressScheduler::new(limits()).unwrap();
        let session = scheduler.open_session(TransportKind::Stdio).session_id;
        initialize(&mut scheduler, session, 1);

        // Side-effecting request over normal+reserve bytes (200+200).
        assert!(matches!(
            scheduler.admit_message(session, request(2, "workspace/executeCommand"), 500),
            AdmissionResult::Rejected {
                error: ProtocolError {
                    kind: ProtocolErrorKind::RequestFailed,
                    ..
                },
                ..
            }
        ));
        // Read request over the read byte budget (100).
        assert!(matches!(
            scheduler.admit_message(session, request(3, "textDocument/inlayHint"), 150),
            AdmissionResult::Rejected {
                error: ProtocolError {
                    kind: ProtocolErrorKind::RequestFailed,
                    ..
                },
                ..
            }
        ));
        // Client response over the ordinary byte budget: dropped.
        let response = Message::Response(lsp_server::Response {
            id: RequestId::from(77),
            result: Some(serde_json::Value::Null),
            error: None,
        });
        assert_eq!(
            scheduler.admit_message(session, response, 500),
            AdmissionResult::Dropped(DropReason::OversizedMessage)
        );

        // The session survives: ordinary traffic is still admitted.
        assert_eq!(
            scheduler.lifecycle(session),
            Some(LifecycleState::Initialized)
        );
        assert!(matches!(
            scheduler.admit_message(session, full_change("file:///a.baml", 3, "ok"), 20),
            AdmissionResult::Admitted { .. }
        ));
    }

    /// A mutation notification whose size alone exceeds its class's byte
    /// budget is admitted anyway (dropping it would silently desync the
    /// editor overlay): the queue's byte counter overshoots by that one
    /// message, other traffic backpressures until it drains, and draining
    /// restores normal admission.
    #[test]
    fn oversized_mutation_notification_is_admitted_with_byte_overshoot() {
        let mut scheduler = IngressScheduler::new(limits()).unwrap();
        let session = scheduler.open_session(TransportKind::Stdio).session_id;
        initialize(&mut scheduler, session, 1);

        // 500 bytes > normal+reserve (200+200): admitted, not dropped.
        assert!(matches!(
            scheduler.admit_message(session, full_change("file:///a.baml", 2, "huge"), 500),
            AdmissionResult::Admitted {
                coalesced: false,
                ..
            }
        ));
        // The overshoot saturates the byte budget for everything else:
        // a read request is rejected (not backpressured), a second
        // oversized mutation backpressures until the first drains.
        assert!(matches!(
            scheduler.admit_message(session, request(3, "textDocument/hover"), 20),
            AdmissionResult::Rejected { .. }
        ));
        assert!(matches!(
            scheduler.admit_message(session, full_change("file:///b.baml", 2, "huge"), 500),
            AdmissionResult::Backpressure(_)
        ));

        // Draining the big change releases its charge and restores normal
        // admission.
        let Some(SchedulerEvent::Dispatch(item)) = scheduler.next_event() else {
            panic!("the oversized change must dispatch");
        };
        let Message::Notification(notification) = &item.message else {
            panic!("expected the didChange notification");
        };
        assert_eq!(notification.method, "textDocument/didChange");
        assert!(matches!(
            scheduler.admit_message(session, full_change("file:///a.baml", 3, "ok"), 20),
            AdmissionResult::Admitted { .. }
        ));
    }

    /// Transient saturation of the same class still backpressures: the
    /// permanent-oversize path only triggers when the size alone can never
    /// fit, never for a queue that merely happens to be full.
    #[test]
    fn transient_saturation_still_backpressures_admissible_sizes() {
        let mut tight = limits();
        tight.normal_items = 1;
        tight.reserved_items = 1;
        let mut scheduler = IngressScheduler::new(tight).unwrap();
        let session = scheduler.open_session(TransportKind::Stdio).session_id;
        initialize(&mut scheduler, session, 1);

        assert!(matches!(
            scheduler.admit_message(session, full_change("file:///a.baml", 2, "a"), 20),
            AdmissionResult::Admitted { .. }
        ));
        assert!(matches!(
            scheduler.admit_message(session, full_change("file:///b.baml", 2, "b"), 20),
            AdmissionResult::Admitted { .. }
        ));
        assert_eq!(
            scheduler.admit_message(session, full_change("file:///c.baml", 2, "c"), 20),
            AdmissionResult::Backpressure(BackpressureReason::ReservedCapacity)
        );
    }

    #[test]
    fn protocol_error_codes_never_use_unknown_error_fallback() {
        assert_eq!(ProtocolErrorKind::ParseError.json_rpc_code(), -32700);
        assert_eq!(ProtocolErrorKind::InvalidRequest.json_rpc_code(), -32600);
        assert_eq!(
            ProtocolErrorKind::ServerNotInitialized.json_rpc_code(),
            -32002
        );
        assert_eq!(ProtocolErrorKind::MethodNotFound.json_rpc_code(), -32601);
        assert_eq!(ProtocolErrorKind::InvalidParams.json_rpc_code(), -32602);
        assert_eq!(ProtocolErrorKind::InternalError.json_rpc_code(), -32603);
        assert_eq!(ProtocolErrorKind::RequestCanceled.json_rpc_code(), -32800);
        assert_eq!(ProtocolErrorKind::ContentModified.json_rpc_code(), -32801);
        assert_eq!(ProtocolErrorKind::RequestFailed.json_rpc_code(), -32803);
    }
}
