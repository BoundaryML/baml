//! Process-owned, transport-neutral LSP ingress runtime and the owner loop.
//!
//! One [`IngressScheduler`] behind one process-owned mutex serves every LSP
//! transport: each connection submits the same [`lsp_server::Message`]
//! representation and shares admission, lifecycle state, cancellation, and
//! outbound-response accounting. Dispatch runs on one dedicated thread — the
//! **owner** (`baml-lsp-owner`), which also owns the
//! [`GlobalState`] outright: it is moved into the thread and never shared, so
//! every mutation and every handler entry happens there, in FIFO order, with
//! no lock around the database. `$/cancelRequest` is processed on the
//! *submitting transport thread* so it can claim a response while the owner
//! is inside a handler.
//!
//! The owner blocks in one `select!` over three sources and nothing else:
//! the ingress wake channel (drain the scheduler), the owner's event queue
//! ([`OwnerEvent`]s from pool jobs and hosts), and an armed-only timer for
//! debounce tails and saturated-sink retries. There is no idle polling.
//!
//! Cancellation tokens handed out by the scheduler are operation-owned flags
//! for response ownership; Salsa's own per-snapshot cancellation lives inside
//! the protocol layer and is not driven from here.

use std::{
    collections::{HashMap, HashSet, VecDeque},
    path::{Path, PathBuf},
    sync::{Arc, Weak},
    time::{Duration, Instant},
};

use baml_lsp::{
    GlobalState, LspError, OwnerEvent, OwnerHandle, SessionKey,
    state::{ClientSender, Responder},
};
use crossbeam_channel::{Receiver, Sender};
use parking_lot::Mutex;

use crate::lsp_ingress::{
    AdmissionResult, CancelEffect, ControlAdmission, DispatchItem, IngressLimits, IngressScheduler,
    LifecycleState, ProtocolError, ResponseClaim, ResponseOutcome, ResponseSizeError,
    ResponseToken, SchedulerEvent, SessionId, SessionTermination, TerminationReason, TransportKind,
};

/// How soon the owner retries a response parked behind a saturated sink.
/// Only armed while something is parked, so an idle server never ticks.
const PENDING_RESPONSE_RETRY: Duration = Duration::from_millis(5);

/// Result of a non-blocking transport enqueue attempt.
///
/// Saturation is deliberately distinct from closure: a full bounded queue is
/// a transient backpressure signal and must not tombstone the session sink.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SinkDelivery {
    Sent,
    Saturated,
    Oversized,
    Closed,
}

pub type Sink = Arc<dyn Fn(lsp_server::Message) -> SinkDelivery + Send + Sync>;
pub type Close = Arc<dyn Fn() + Send + Sync>;
/// Runs on the owner thread after a notification was applied, with the
/// state in hand — the hook for host-side follow-ups such as CLI workspace
/// roots after `initialized`.
pub type NotificationHook =
    Arc<dyn Fn(&mut GlobalState, SessionKey, &lsp_server::Notification) + Send + Sync>;

/// Responder for one server-initiated request's client response (see
/// [`LspRuntime::register_server_request`]).
pub type ServerRequestResponder = Box<dyn FnOnce(lsp_server::Response) + Send>;

/// The protocol layer's key for a transport session.
fn session_key(session_id: SessionId) -> SessionKey {
    SessionKey(session_id.get())
}

/// Connection-owned LSP output. The sink lives behind a tombstone so an
/// asynchronous tail retaining an old session handle cannot write after
/// browser takeover or transport close.
pub struct RevocableSessionSender {
    sink: Mutex<Option<Sink>>,
}

impl RevocableSessionSender {
    fn new(sink: Sink) -> Self {
        Self {
            sink: Mutex::new(Some(sink)),
        }
    }

    fn revoke(&self) {
        self.sink.lock().take();
    }

    #[cfg(test)]
    fn is_closed(&self) -> bool {
        self.sink.lock().is_none()
    }

    fn send(&self, message: lsp_server::Message) -> Result<(), LspError> {
        let mut sink = self.sink.lock();
        let Some(active) = sink.as_ref() else {
            return Err(LspError::ClientClosed);
        };
        match active(message) {
            SinkDelivery::Sent => Ok(()),
            SinkDelivery::Saturated => Err(LspError::OutboundSaturated),
            SinkDelivery::Oversized => Err(LspError::OutboundOversized),
            SinkDelivery::Closed => {
                sink.take();
                Err(LspError::ClientClosed)
            }
        }
    }
}

impl ClientSender for RevocableSessionSender {
    fn send_notification(&self, method: &str, params: serde_json::Value) -> Result<(), LspError> {
        self.send(lsp_server::Message::Notification(
            lsp_server::Notification::new(method.to_owned(), params),
        ))
    }
}

#[derive(Clone)]
struct Endpoint {
    outbound: Arc<RevocableSessionSender>,
    sink: Sink,
    close: Close,
    after_notification: Option<NotificationHook>,
}

struct PendingResponse {
    token: ResponseToken,
    message: lsp_server::Message,
}

#[derive(Debug)]
pub enum SubmitResult {
    Accepted,
    Dropped,
    Backpressure,
    Exited { normal: bool },
    Closed,
}

pub struct OpenedSession {
    pub session_id: SessionId,
}

/// One scheduler and one owner thread for every LSP transport in the
/// process. Endpoints are bounded sinks owned by the transport adapters.
///
/// Every field is transport bookkeeping behind short `parking_lot` locks; the
/// database state is *not* here — it lives on the owner thread and is only
/// reachable through [`OwnerEvent::Call`] (posted via `owner`) or as the
/// `&mut GlobalState` the owner passes into [`LspRuntime::drain`].
pub struct LspRuntime {
    scheduler: Mutex<IngressScheduler>,
    endpoints: Mutex<HashMap<SessionId, Endpoint>>,
    document_overlays: Mutex<HashMap<(SessionId, PathBuf), lsp_types::TextDocumentItem>>,
    pending_responses: Mutex<VecDeque<PendingResponse>>,
    /// Correlation seam for server-initiated requests (workspace/
    /// configuration and friends). This server currently never issues such
    /// requests, so no dispatch path populates the map today; it exists so
    /// client→server responses have a real route instead of being admitted
    /// and then silently lost. Uncorrelated responses are logged and dropped.
    server_requests: Mutex<HashMap<(SessionId, lsp_server::RequestId), ServerRequestResponder>>,
    delivery_gate: Mutex<()>,
    wake_tx: Sender<()>,
    owner: OwnerHandle,
    /// For continuations that must reach back into the runtime from the
    /// owner thread (termination replays); never upgraded while a runtime
    /// lock is held.
    weak_self: Weak<LspRuntime>,
}

impl LspRuntime {
    /// Build the runtime and start the owner thread, which takes `state`.
    pub fn new(state: GlobalState) -> anyhow::Result<Arc<Self>> {
        let (runtime, wake_rx) = Self::with_limits(Self::default_limits(), state.handle())?;
        let weak = Arc::downgrade(&runtime);
        std::thread::Builder::new()
            .name("baml-lsp-owner".to_string())
            .spawn(move || owner_loop(&weak, state, &wake_rx))?;
        Ok(runtime)
    }

    fn default_limits() -> IngressLimits {
        IngressLimits {
            outbound_response_bytes: 64 * 1024 * 1024,
            response_reservation_bytes: 4 * 1024 * 1024,
            ..IngressLimits::default()
        }
    }

    /// The runtime without an owner thread: the caller drives
    /// [`LspRuntime::drain`] itself (tests, embedding).
    fn with_limits(
        limits: IngressLimits,
        owner: OwnerHandle,
    ) -> anyhow::Result<(Arc<Self>, Receiver<()>)> {
        let scheduler = IngressScheduler::new(limits)?;
        let (wake_tx, wake_rx) = crossbeam_channel::bounded(1);
        let runtime = Arc::new_cyclic(|weak_self| Self {
            scheduler: Mutex::new(scheduler),
            endpoints: Mutex::new(HashMap::new()),
            document_overlays: Mutex::new(HashMap::new()),
            pending_responses: Mutex::new(VecDeque::new()),
            server_requests: Mutex::new(HashMap::new()),
            delivery_gate: Mutex::new(()),
            wake_tx,
            owner,
            weak_self: weak_self.clone(),
        });
        Ok((runtime, wake_rx))
    }

    /// The handle for posting owner-thread continuations.
    pub fn owner(&self) -> &OwnerHandle {
        &self.owner
    }

    pub fn open_session(
        &self,
        transport: TransportKind,
        sink: Sink,
        close: Close,
        after_notification: Option<NotificationHook>,
    ) -> OpenedSession {
        let _delivery = self.delivery_gate.lock();
        let opened = self.scheduler.lock().open_session(transport);
        if let Some(takeover) = &opened.takeover {
            // Revoke the old sink and reconcile its overlays before exposing
            // the replacement endpoint. A surviving stdio owner is
            // restored instead of being erased by the synthetic didClose.
            self.finish_termination(takeover);
        }
        let outbound = Arc::new(RevocableSessionSender::new(sink.clone()));
        self.endpoints.lock().insert(
            opened.session_id,
            Endpoint {
                outbound,
                sink,
                close,
                after_notification,
            },
        );
        OpenedSession {
            session_id: opened.session_id,
        }
    }

    pub fn submit(&self, session_id: SessionId, message: lsp_server::Message) -> SubmitResult {
        let serialized_bytes = serde_json::to_vec(&message).map_or(1, |bytes| bytes.len());
        if let lsp_server::Message::Notification(notification) = &message
            && notification.method == "$/cancelRequest"
        {
            let Some(request_id) = notification
                .params
                .get("id")
                .cloned()
                .and_then(|value| serde_json::from_value::<lsp_server::RequestId>(value).ok())
            else {
                return SubmitResult::Dropped;
            };
            let result = self.scheduler.lock().admit_cancel(
                session_id,
                request_id.clone(),
                serialized_bytes,
            );
            return match result {
                ControlAdmission::Admitted => {
                    // Cancellation is a true control path: process it on the
                    // submitting transport thread so it can claim a response
                    // while the owner is busy. The protocol layer also sees
                    // the cancel on the owner, but cancelling the running
                    // read's Salsa token is not wired yet — the job runs to
                    // completion, and whatever it reports finds its response
                    // already claimed here and is dropped.
                    self.drain_controls();
                    self.forward_cancel_to_owner(session_id, request_id);
                    self.wake();
                    SubmitResult::Accepted
                }
                ControlAdmission::Duplicate => SubmitResult::Dropped,
                ControlAdmission::Backpressure => SubmitResult::Backpressure,
                ControlAdmission::Oversized => {
                    tracing::warn!(
                        session = session_id.get(),
                        "dropping $/cancelRequest larger than the control byte budget"
                    );
                    SubmitResult::Dropped
                }
                ControlAdmission::UnknownSession => SubmitResult::Closed,
            };
        }

        let admission = self
            .scheduler
            .lock()
            .admit_message(session_id, message, serialized_bytes);
        match admission {
            AdmissionResult::Admitted { .. } => {
                self.wake();
                SubmitResult::Accepted
            }
            AdmissionResult::Rejected {
                response_token,
                error,
            } => {
                self.send_owned_error(&response_token, error);
                SubmitResult::Accepted
            }
            AdmissionResult::Dropped(reason) => {
                tracing::warn!(
                    session = session_id.get(),
                    ?reason,
                    "dropping LSP message (lifecycle state or permanent oversize)"
                );
                SubmitResult::Dropped
            }
            AdmissionResult::Backpressure(_) => SubmitResult::Backpressure,
            AdmissionResult::UseControlPath => SubmitResult::Dropped,
            AdmissionResult::Exited(termination) => {
                let normal = termination.reason == TerminationReason::NormalExit;
                let _delivery = self.delivery_gate.lock();
                self.finish_termination(&termination);
                SubmitResult::Exited { normal }
            }
            AdmissionResult::UnknownSession => SubmitResult::Closed,
            AdmissionResult::DuplicateRequestId(token) => {
                tracing::warn!(
                    session = token.session_id().get(),
                    request_id = %token.request_id(),
                    "duplicate outstanding LSP request id"
                );
                SubmitResult::Dropped
            }
        }
    }

    /// Registers interest in the client's response to a server-initiated
    /// request. Call this *before* writing the request to the session sink;
    /// the responder runs on the owner thread when the response arrives.
    /// (Correlation seam for a server that starts issuing
    /// `workspace/configuration`-style requests — currently unused.)
    pub fn register_server_request(
        &self,
        session_id: SessionId,
        request_id: lsp_server::RequestId,
        responder: ServerRequestResponder,
    ) {
        self.server_requests
            .lock()
            .insert((session_id, request_id), responder);
    }

    pub fn close_session(&self, session_id: SessionId) {
        let _delivery = self.delivery_gate.lock();
        self.close_session_under_delivery_gate(session_id);
    }

    fn close_session_under_delivery_gate(&self, session_id: SessionId) {
        if let Some(termination) = self.scheduler.lock().close_session(session_id) {
            self.finish_termination(&termination);
        }
    }

    /// Transport-side teardown of a terminated session, then the owner-side
    /// teardown as one posted continuation: documents the session owned are
    /// first handed to a surviving overlay of the same physical file (browser
    /// takeover of a file the editor still has open), then the protocol
    /// session is dropped, which closes every document it still holds —
    /// including one whose `didOpen` applied after the scheduler had already
    /// forgotten the session.
    fn finish_termination(&self, termination: &SessionTermination) {
        self.pending_responses
            .lock()
            .retain(|pending| pending.token.session_id() != termination.session_id);
        self.server_requests
            .lock()
            .retain(|(session_id, _), _| *session_id != termination.session_id);
        let endpoint = self.endpoints.lock().remove(&termination.session_id);
        let Some(endpoint) = endpoint else {
            return;
        };
        endpoint.outbound.revoke();
        let identities: Vec<PathBuf> = termination
            .owned_documents
            .iter()
            .filter_map(|document| canonical_document_identity_str(&document.uri))
            .collect();
        {
            let mut overlays = self.document_overlays.lock();
            for identity in &identities {
                overlays.remove(&(termination.session_id, identity.clone()));
            }
        }
        (endpoint.close)();

        let key = session_key(termination.session_id);
        let weak = self.weak_self.clone();
        self.owner.post(OwnerEvent::Call(Box::new(move |state| {
            if let Some(runtime) = weak.upgrade() {
                for identity in &identities {
                    runtime.restore_surviving_overlay(state, identity);
                }
            }
            let closed = state.close_session(key);
            if !closed.is_empty() {
                tracing::debug!(
                    session = key.0,
                    documents = closed.len(),
                    "closed a terminated session's documents"
                );
            }
        })));
    }

    /// Replay the newest surviving overlay for `identity` (a physical path)
    /// through the protocol layer, so ownership of a file two sessions had
    /// open moves to the survivor instead of the file being closed.
    fn restore_surviving_overlay(&self, state: &mut GlobalState, identity: &Path) -> bool {
        let survivor = {
            let overlays = self.document_overlays.lock();
            overlays
                .iter()
                .filter(|((_, candidate_identity), _)| candidate_identity == identity)
                .max_by_key(|((session_id, _), _)| session_id.get())
                .map(|((session_id, _), document)| (*session_id, document.clone()))
        };
        let Some((session_id, document)) = survivor else {
            return false;
        };
        let Some(endpoint) = self.endpoint(session_id) else {
            return false;
        };
        let key = ensure_state_session(state, session_id, &endpoint);
        let open = lsp_server::Notification::new(
            "textDocument/didOpen".to_string(),
            lsp_types::DidOpenTextDocumentParams {
                text_document: document,
            },
        );
        match state.dispatch_notification(key, open) {
            Ok(()) => true,
            Err(error) => {
                tracing::warn!(session = key.0, %error, "replaying a surviving document overlay");
                false
            }
        }
    }

    fn forward_cancel_to_owner(&self, session_id: SessionId, request_id: lsp_server::RequestId) {
        let key = session_key(session_id);
        self.owner.post(OwnerEvent::Call(Box::new(move |state| {
            if state.session(key).is_err() {
                return;
            }
            // `RequestId` serializes as the wire id (number or string), which
            // is exactly `CancelParams { id }`.
            let cancel = lsp_server::Notification::new(
                "$/cancelRequest".to_string(),
                serde_json::json!({ "id": request_id }),
            );
            if let Err(error) = state.dispatch_notification(key, cancel) {
                tracing::debug!(session = key.0, %error, "cancel not applied by the protocol layer");
            }
        })));
    }

    fn wake(&self) {
        // Bounded(1): a token already in the slot means the owner will drain
        // after this admission anyway, so a full slot is not a lost wakeup.
        let _ = self.wake_tx.try_send(());
    }

    fn drain_controls(&self) {
        loop {
            let effect = self.scheduler.lock().next_control();
            match effect {
                Some(CancelEffect::Respond {
                    response_token,
                    error,
                    ..
                }) => self.send_owned_error(&response_token, error),
                Some(CancelEffect::Noop) => {}
                None => return,
            }
        }
    }

    fn endpoint(&self, session_id: SessionId) -> Option<Endpoint> {
        self.endpoints.lock().get(&session_id).cloned()
    }

    /// Owner-thread entry: flush parked responses, then dispatch every
    /// scheduled control and message in FIFO order until the scheduler is
    /// empty.
    pub fn drain(self: &Arc<Self>, state: &mut GlobalState) {
        self.drain_pending_responses();
        loop {
            let event = self.scheduler.lock().next_event();
            let Some(event) = event else {
                return;
            };
            match event {
                SchedulerEvent::Control(CancelEffect::Respond {
                    response_token,
                    error,
                    ..
                }) => self.send_owned_error(&response_token, error),
                SchedulerEvent::Control(CancelEffect::Noop) => {}
                SchedulerEvent::Dispatch(item) => self.dispatch(state, item),
            }
        }
    }

    fn dispatch(self: &Arc<Self>, state: &mut GlobalState, item: DispatchItem) {
        let Some(endpoint) = self.endpoint(item.session_id) else {
            return;
        };
        let session = ensure_state_session(state, item.session_id, &endpoint);
        match item.message {
            lsp_server::Message::Request(request) => {
                let Some(token) = item.response_token else {
                    return;
                };
                let runtime = Arc::clone(self);
                let respond: Responder =
                    Box::new(move |result| runtime.send_handler_result(&token, result));
                let method = request.method.clone();
                guarded(&method, || {
                    state.dispatch_request(session, request, respond);
                });
            }
            lsp_server::Message::Notification(notification) => {
                let tracking = notification.clone();
                let Some(applied) = guarded(&tracking.method, || {
                    state.dispatch_notification(session, notification)
                }) else {
                    return;
                };
                match applied {
                    Ok(()) => {}
                    Err(LspError::NotificationNotSupported(method)) => {
                        // Unknown notifications are a client-capability
                        // mismatch, not an error condition; nothing goes to
                        // the client.
                        tracing::warn!(session = session.0, %method, "ignoring unsupported notification");
                        return;
                    }
                    Err(error) => {
                        tracing::warn!(session = session.0, method = %tracking.method, %error, "notification was not applied");
                        return;
                    }
                }
                if self.record_applied_document(item.session_id, &tracking) {
                    if let Some(hook) = &endpoint.after_notification {
                        hook(state, session, &tracking);
                    }
                    if tracking.method == "textDocument/didClose"
                        && let Some(uri) = tracking
                            .params
                            .pointer("/textDocument/uri")
                            .and_then(serde_json::Value::as_str)
                        && let Some(identity) = canonical_document_identity_str(uri)
                    {
                        self.restore_surviving_overlay(state, &identity);
                    }
                } else {
                    // Browser takeover/transport close won while this
                    // mutation was applying. Hand the just-applied document
                    // to a surviving overlay now; the termination's posted
                    // continuation closes whatever the dead session still
                    // holds.
                    self.restore_orphaned_document(state, &tracking);
                }
            }
            lsp_server::Message::Response(response) => {
                // Client response to a server-initiated request: route
                // through the correlation seam; without a registration it is
                // logged, never silently lost.
                let responder = self
                    .server_requests
                    .lock()
                    .remove(&(item.session_id, response.id.clone()));
                match responder {
                    Some(respond) => respond(response),
                    None => tracing::warn!(
                        session = item.session_id.get(),
                        id = %response.id,
                        "dropping client response with no matching server-initiated request"
                    ),
                }
            }
        }
    }

    fn send_handler_result(
        &self,
        token: &ResponseToken,
        result: Result<serde_json::Value, LspError>,
    ) {
        let outcome = if result.is_ok() {
            ResponseOutcome::Success
        } else {
            ResponseOutcome::Error
        };
        let claim = self.scheduler.lock().claim_response(token, outcome);
        if claim != ResponseClaim::Claimed {
            return;
        }
        if let Ok(value) = &result {
            // One lock scope: read the lifecycle and record the negotiated
            // initialize result under the same guard.
            let mut scheduler = self.scheduler.lock();
            if scheduler.lifecycle(token.session_id()) == Some(LifecycleState::InitializeResponding)
            {
                let _ = scheduler.set_negotiated_capabilities(token.session_id(), value.clone());
            }
        }
        let response = match result {
            Ok(value) => lsp_server::Response {
                id: token.request_id().clone(),
                result: Some(value),
                error: None,
            },
            Err(error) => lsp_server::Response {
                id: token.request_id().clone(),
                result: None,
                // The one request error-code mapping boundary.
                error: Some(error.to_response_error()),
            },
        };
        self.order_response(token, response);
    }

    fn send_owned_error(&self, token: &ResponseToken, error: ProtocolError) {
        let response = lsp_server::Response {
            id: token.request_id().clone(),
            result: None,
            error: Some(lsp_server::ResponseError {
                code: error.kind.json_rpc_code(),
                message: error.message,
                data: None,
            }),
        };
        self.order_response(token, response);
    }

    fn order_response(&self, token: &ResponseToken, response: lsp_server::Response) {
        let _delivery = self.delivery_gate.lock();
        let mut message = lsp_server::Message::Response(response);
        let bytes = serde_json::to_vec(&message).map_or(usize::MAX, |value| value.len());
        match self.scheduler.lock().validate_response_size(token, bytes) {
            Ok(()) => {}
            Err(ResponseSizeError::UnknownOrSuperseded) => return,
            Err(ResponseSizeError::ExceedsReservation) => {
                // An oversized payload fails only its own request: the
                // replacement error is a few hundred bytes and always fits
                // the reservation, so the session survives instead of being
                // deterministically closed.
                tracing::warn!(
                    session = token.session_id().get(),
                    request_id = %token.request_id(),
                    bytes,
                    "replacing an LSP response that exceeds its reservation"
                );
                message = lsp_server::Message::Response(lsp_server::Response {
                    id: token.request_id().clone(),
                    result: None,
                    error: Some(lsp_server::ResponseError {
                        code: lsp_server::ErrorCode::RequestFailed as i32,
                        message: "response exceeds the transport's per-response reservation"
                            .to_string(),
                        data: None,
                    }),
                });
            }
        }
        let mut pending = self.pending_responses.lock();
        if pending
            .iter()
            .any(|response| response.token.session_id() == token.session_id())
        {
            pending.push_back(PendingResponse {
                token: token.clone(),
                message,
            });
            return;
        }
        drop(pending);
        let Some(endpoint) = self.endpoint(token.session_id()) else {
            return;
        };
        match (endpoint.sink)(message.clone()) {
            SinkDelivery::Sent => {
                let _ = self.scheduler.lock().response_ordered(token);
            }
            SinkDelivery::Saturated => {
                self.pending_responses.lock().push_back(PendingResponse {
                    token: token.clone(),
                    message,
                });
                // A parked response may have been produced off the owner
                // thread (admission rejection, cancellation); the owner arms
                // the retry timer once it wakes.
                self.wake();
            }
            SinkDelivery::Oversized | SinkDelivery::Closed => {
                self.close_session_under_delivery_gate(token.session_id());
            }
        }
    }

    fn has_pending_responses(&self) -> bool {
        !self.pending_responses.lock().is_empty()
    }

    /// The instant the owner should retry parked responses, or `None` when
    /// nothing is parked (no timer is armed).
    fn pending_response_retry_at(&self) -> Option<Instant> {
        self.has_pending_responses()
            .then(|| Instant::now() + PENDING_RESPONSE_RETRY)
    }

    /// Retry each session's oldest response once without allowing one slow
    /// transport to head-of-line block unrelated sessions. A response keeps
    /// its scheduler reservation until it is actually accepted by the sink.
    fn drain_pending_responses(&self) {
        let _delivery = self.delivery_gate.lock();
        let pending = std::mem::take(&mut *self.pending_responses.lock());
        if pending.is_empty() {
            return;
        }

        let mut blocked_sessions = HashSet::new();
        let mut retry = VecDeque::new();
        for pending in pending {
            let session_id = pending.token.session_id();
            if blocked_sessions.contains(&session_id) {
                retry.push_back(pending);
                continue;
            }
            let Some(endpoint) = self.endpoint(session_id) else {
                continue;
            };
            match (endpoint.sink)(pending.message.clone()) {
                SinkDelivery::Sent => {
                    let _ = self.scheduler.lock().response_ordered(&pending.token);
                }
                SinkDelivery::Saturated => {
                    blocked_sessions.insert(session_id);
                    retry.push_back(pending);
                }
                SinkDelivery::Oversized | SinkDelivery::Closed => {
                    self.close_session_under_delivery_gate(session_id);
                }
            }
        }
        self.pending_responses.lock().extend(retry);
    }

    fn record_applied_document(
        &self,
        session_id: SessionId,
        notification: &lsp_server::Notification,
    ) -> bool {
        let mut scheduler = self.scheduler.lock();
        if scheduler.lifecycle(session_id).is_none() {
            return false;
        }
        match notification.method.as_str() {
            "textDocument/didOpen" => {
                if let Ok(params) = serde_json::from_value::<lsp_types::DidOpenTextDocumentParams>(
                    notification.params.clone(),
                ) && let Some(identity) = canonical_document_identity(&params.text_document.uri)
                {
                    self.document_overlays
                        .lock()
                        .insert((session_id, identity), params.text_document.clone());
                    scheduler.record_document_open(
                        session_id,
                        params.text_document.uri.to_string(),
                        Some(params.text_document.version),
                    );
                }
            }
            "textDocument/didChange" => {
                if let Ok(params) = serde_json::from_value::<lsp_types::DidChangeTextDocumentParams>(
                    notification.params.clone(),
                ) && let Some(identity) = canonical_document_identity(&params.text_document.uri)
                {
                    if let [change] = params.content_changes.as_slice()
                        && change.range.is_none()
                        && let Some(document) = self
                            .document_overlays
                            .lock()
                            .get_mut(&(session_id, identity))
                    {
                        document.version = params.text_document.version;
                        document.text.clone_from(&change.text);
                    }
                    scheduler.record_document_version(
                        session_id,
                        params.text_document.uri.as_str(),
                        Some(params.text_document.version),
                    );
                }
            }
            "textDocument/didClose" => {
                if let Ok(params) = serde_json::from_value::<lsp_types::DidCloseTextDocumentParams>(
                    notification.params.clone(),
                ) && let Some(identity) = canonical_document_identity(&params.text_document.uri)
                {
                    self.document_overlays
                        .lock()
                        .remove(&(session_id, identity));
                    scheduler.record_document_close(session_id, params.text_document.uri.as_str());
                }
            }
            _ => {}
        }
        true
    }

    /// A `didOpen`/`didChange` applied for a session the scheduler has
    /// already terminated: give the physical file back to a surviving
    /// overlay if one exists. If none does, nothing happens here — the
    /// termination continuation closes the document when it drops the
    /// protocol session.
    fn restore_orphaned_document(
        &self,
        state: &mut GlobalState,
        notification: &lsp_server::Notification,
    ) {
        if !matches!(
            notification.method.as_str(),
            "textDocument/didOpen" | "textDocument/didChange"
        ) {
            return;
        }
        let Some(identity) = notification
            .params
            .pointer("/textDocument/uri")
            .and_then(serde_json::Value::as_str)
            .and_then(canonical_document_identity_str)
        else {
            return;
        };
        self.restore_surviving_overlay(state, &identity);
    }
}

/// The protocol session for a transport session, opened on first use.
///
/// Opening lazily on the owner thread, immediately before the first dispatch
/// for the session, means the protocol session exists exactly when the first
/// message can reach it — with no ordering assumption between the wake
/// channel and the owner's event queue.
fn ensure_state_session(
    state: &mut GlobalState,
    session_id: SessionId,
    endpoint: &Endpoint,
) -> SessionKey {
    let key = session_key(session_id);
    if state.session(key).is_err() {
        let sender: Arc<dyn ClientSender> = endpoint.outbound.clone();
        state.open_session(key, sender);
    }
    key
}

/// The owner thread's body: block until there is something to do, do it,
/// repeat. Returns when the runtime is gone (its wake sender dropped).
fn owner_loop(runtime: &Weak<LspRuntime>, mut state: GlobalState, wake_rx: &Receiver<()>) {
    enum Wake {
        Ingress,
        Event(OwnerEvent),
        Tick,
        Stop,
    }
    // A clone of the owner's event receiver so `select!` does not borrow the
    // state while an arm needs it mutably.
    let events = state.events().clone();
    loop {
        // The strong reference is dropped before blocking: an owner parked
        // on `select!` must not keep the runtime alive.
        let deadline = {
            let Some(runtime) = runtime.upgrade() else {
                return;
            };
            [state.next_deadline(), runtime.pending_response_retry_at()]
                .into_iter()
                .flatten()
                .min()
        };
        let timer = deadline.map_or_else(crossbeam_channel::never, crossbeam_channel::at);
        let wake = crossbeam_channel::select! {
            recv(wake_rx) -> wake => wake.map_or(Wake::Stop, |()| Wake::Ingress),
            recv(events) -> event => event.map_or(Wake::Stop, Wake::Event),
            recv(timer) -> _ => Wake::Tick,
        };
        match wake {
            Wake::Ingress => {
                let Some(runtime) = runtime.upgrade() else {
                    return;
                };
                runtime.drain(&mut state);
            }
            Wake::Event(event) => {
                guarded("owner event", || state.handle_event(event));
            }
            Wake::Tick => {
                let now = Instant::now();
                guarded("owner tick", || state.on_tick(now));
                if let Some(runtime) = runtime.upgrade() {
                    runtime.drain_pending_responses();
                }
            }
            Wake::Stop => return,
        }
    }
}

/// Run one owner-thread step under `catch_unwind`: a panic in a handler or
/// event is logged and the loop continues. Every database `set_*` is atomic,
/// so the state is never structurally corrupt after an unwind; the request
/// (if any) simply never answers through this path and its response token
/// is reclaimed when the session ends.
fn guarded<R>(what: &str, step: impl FnOnce() -> R) -> Option<R> {
    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(step)) {
        Ok(result) => Some(result),
        Err(payload) => {
            let message = payload
                .downcast_ref::<&str>()
                .map(|s| (*s).to_owned())
                .or_else(|| payload.downcast_ref::<String>().cloned())
                .unwrap_or_else(|| "<non-string panic payload>".to_owned());
            tracing::error!(%what, %message, "owner-thread step panicked; continuing");
            None
        }
    }
}

/// The physical identity of a document URI, for overlay ownership across
/// sessions: the same rule the protocol layer keys the database by, so two
/// sessions spelling one file differently (`/tmp` vs `/private/tmp`, a
/// symlinked directory) share one overlay slot. The client's exact URI is
/// kept in the `TextDocumentItem` for replay.
fn canonical_document_identity(uri: &lsp_types::Url) -> Option<PathBuf> {
    let path = uri.to_file_path().ok()?;
    Some(baml_lsp::paths::canonical_physical_path(&path))
}

fn canonical_document_identity_str(uri: &str) -> Option<PathBuf> {
    canonical_document_identity(&lsp_types::Url::parse(uri).ok()?)
}

#[cfg(test)]
mod tests {
    use baml_lsp::executor::Inline;
    use lsp_server::{Message, Notification, Request, RequestId};

    use super::*;
    use crate::lsp_ingress::{ProtocolErrorKind, ResponseOrder};

    fn capturing_sink(messages: Arc<Mutex<Vec<Message>>>) -> Sink {
        Arc::new(move |message| {
            messages.lock().push(message);
            SinkDelivery::Sent
        })
    }

    fn scripted_sink(
        deliveries: impl IntoIterator<Item = SinkDelivery>,
        messages: Arc<Mutex<Vec<Message>>>,
    ) -> Sink {
        let deliveries = Arc::new(Mutex::new(deliveries.into_iter().collect::<VecDeque<_>>()));
        Arc::new(move |message| {
            messages.lock().push(message);
            deliveries.lock().pop_front().unwrap_or(SinkDelivery::Sent)
        })
    }

    fn test_state() -> GlobalState {
        GlobalState::new(Box::new(Inline), None)
    }

    /// The runtime without its owner thread: tests drive `drain` and the
    /// owner's event queue by hand against `state`.
    fn runtime_without_worker() -> (Arc<LspRuntime>, GlobalState) {
        runtime_without_worker_with_limits(LspRuntime::default_limits())
    }

    fn runtime_without_worker_with_limits(limits: IngressLimits) -> (Arc<LspRuntime>, GlobalState) {
        let state = test_state();
        let (runtime, _wake_rx) = LspRuntime::with_limits(limits, state.handle()).unwrap();
        (runtime, state)
    }

    /// Apply every queued owner event (posted continuations, pool results).
    fn pump_owner_events(state: &mut GlobalState) {
        let events = state.events().clone();
        while let Ok(event) = events.try_recv() {
            state.handle_event(event);
        }
    }

    fn request(id: i32, method: &str) -> Message {
        Message::Request(Request {
            id: RequestId::from(id),
            method: method.to_string(),
            params: serde_json::Value::Null,
        })
    }

    fn initialize(runtime: &LspRuntime, session_id: SessionId) {
        let mut scheduler = runtime.scheduler.lock();
        let AdmissionResult::Admitted {
            response_token: Some(token),
            ..
        } = scheduler.admit_message(session_id, request(1, "initialize"), 20)
        else {
            panic!("initialize must be admitted");
        };
        let Some(SchedulerEvent::Dispatch(_)) = scheduler.next_event() else {
            panic!("initialize must dispatch");
        };
        assert_eq!(
            scheduler.claim_response(&token, ResponseOutcome::Success),
            ResponseClaim::Claimed
        );
        assert_eq!(scheduler.response_ordered(&token), ResponseOrder::Ordered);
        assert!(matches!(
            scheduler.admit_message(
                session_id,
                Message::Notification(Notification::new(
                    "initialized".to_string(),
                    serde_json::Value::Null,
                )),
                10,
            ),
            AdmissionResult::Admitted { .. }
        ));
        let Some(SchedulerEvent::Dispatch(_)) = scheduler.next_event() else {
            panic!("initialized must dispatch");
        };
    }

    fn admit_command(runtime: &LspRuntime, session_id: SessionId, id: i32) -> ResponseToken {
        let AdmissionResult::Admitted {
            response_token: Some(token),
            ..
        } = runtime.scheduler.lock().admit_message(
            session_id,
            request(id, "workspace/executeCommand"),
            20,
        )
        else {
            panic!("executeCommand must be admitted");
        };
        token
    }

    fn cancel(id: i32) -> Message {
        Message::Notification(Notification::new(
            "$/cancelRequest".to_string(),
            serde_json::json!({ "id": id }),
        ))
    }

    fn log_message(method: &str) -> serde_json::Value {
        serde_json::json!({ "type": 3, "message": method })
    }

    #[test]
    fn browser_takeover_tombstones_old_outbound_without_leaking_to_new_or_stdio() {
        let (runtime, mut state) = runtime_without_worker();
        let stdio_messages = Arc::new(Mutex::new(Vec::new()));
        let old_browser_messages = Arc::new(Mutex::new(Vec::new()));
        let new_browser_messages = Arc::new(Mutex::new(Vec::new()));
        let close: Close = Arc::new(|| {});

        let stdio = runtime.open_session(
            TransportKind::Stdio,
            capturing_sink(stdio_messages.clone()),
            close.clone(),
            None,
        );
        let old_browser = runtime.open_session(
            TransportKind::Browser,
            capturing_sink(old_browser_messages.clone()),
            close.clone(),
            None,
        );
        // A drive-less file URI fails `Url::to_file_path` on Windows.
        let shared_uri = if cfg!(windows) {
            "file:///C:/tmp/shared-overlay.baml".to_string()
        } else {
            "file:///tmp/shared-overlay.baml".to_string()
        };
        let shared_identity = canonical_document_identity_str(&shared_uri).unwrap();
        {
            let mut scheduler = runtime.scheduler.lock();
            scheduler.record_document_open(stdio.session_id, shared_uri.clone(), Some(3));
            scheduler.record_document_open(old_browser.session_id, shared_uri.clone(), Some(7));
        }
        runtime.document_overlays.lock().insert(
            (stdio.session_id, shared_identity.clone()),
            lsp_types::TextDocumentItem {
                uri: lsp_types::Url::parse(&shared_uri).unwrap(),
                language_id: "baml".to_string(),
                version: 3,
                text: "function stdio() -> int { 3 }".to_string(),
            },
        );
        runtime.document_overlays.lock().insert(
            (old_browser.session_id, shared_identity.clone()),
            lsp_types::TextDocumentItem {
                uri: lsp_types::Url::parse(&shared_uri).unwrap(),
                language_id: "baml".to_string(),
                version: 7,
                text: "function browser() -> int { 7 }".to_string(),
            },
        );
        let stale_sender = runtime.endpoint(old_browser.session_id).unwrap().outbound;
        let new_browser = runtime.open_session(
            TransportKind::Browser,
            capturing_sink(new_browser_messages.clone()),
            close,
            None,
        );
        // The takeover's owner-side teardown runs as a posted continuation.
        pump_owner_events(&mut state);
        let stdio_messages_after_restore = stdio_messages.lock().len();

        // The retained old sender models a late diagnostic/tail clone: it is
        // tombstoned, while the live sessions still deliver.
        assert!(matches!(
            stale_sender.send_notification("window/logMessage", log_message("late-old-browser")),
            Err(LspError::ClientClosed)
        ));
        runtime
            .endpoint(stdio.session_id)
            .unwrap()
            .outbound
            .send_notification("window/logMessage", log_message("stdio"))
            .unwrap();
        runtime
            .endpoint(new_browser.session_id)
            .unwrap()
            .outbound
            .send_notification("window/logMessage", log_message("new-browser"))
            .unwrap();

        assert!(old_browser_messages.lock().is_empty());
        assert_eq!(
            stdio_messages.lock().len(),
            stdio_messages_after_restore + 1
        );
        assert_eq!(new_browser_messages.lock().len(), 1);
        let overlays = runtime.document_overlays.lock();
        assert!(overlays.contains_key(&(stdio.session_id, shared_identity.clone())));
        assert!(!overlays.contains_key(&(old_browser.session_id, shared_identity)));
        assert!(
            state.session(session_key(old_browser.session_id)).is_err(),
            "the superseded protocol session must be gone"
        );
    }

    #[cfg(unix)]
    #[test]
    fn document_overlay_identity_resolves_symlink_uri_aliases() {
        let root = std::env::temp_dir().join(format!(
            "baml_lsp_overlay_alias_{}_{}",
            std::process::id(),
            std::thread::current().name().unwrap_or("test")
        ));
        let real = root.join("real");
        let alias = root.join("alias");
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&real).unwrap();
        std::os::unix::fs::symlink(&real, &alias).unwrap();
        let real_uri = lsp_types::Url::from_file_path(real.join("main.baml")).unwrap();
        let alias_uri = lsp_types::Url::from_file_path(alias.join("main.baml")).unwrap();

        assert_ne!(real_uri, alias_uri);
        assert_eq!(
            canonical_document_identity(&real_uri),
            canonical_document_identity(&alias_uri),
            "different client URIs for one physical path must share overlay ownership"
        );
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn transient_sink_saturation_does_not_revoke_session_sender() {
        let messages = Arc::new(Mutex::new(Vec::new()));
        let sender = RevocableSessionSender::new(scripted_sink(
            [SinkDelivery::Saturated, SinkDelivery::Sent],
            messages.clone(),
        ));

        let saturated = sender.send_notification("test/first", serde_json::Value::Null);
        assert!(matches!(saturated, Err(LspError::OutboundSaturated)));
        assert!(!sender.is_closed());
        assert!(
            sender
                .send_notification("test/second", serde_json::Value::Null)
                .is_ok(),
            "the live sink must remain installed after transient backpressure"
        );
        assert_eq!(messages.lock().len(), 2);
    }

    #[test]
    fn closed_sink_revokes_session_sender() {
        let messages = Arc::new(Mutex::new(Vec::new()));
        let sender = RevocableSessionSender::new(scripted_sink(
            [SinkDelivery::Closed, SinkDelivery::Sent],
            messages.clone(),
        ));

        assert!(matches!(
            sender.send_notification("test/closed", serde_json::Value::Null),
            Err(LspError::ClientClosed)
        ));
        assert!(sender.is_closed());
        assert!(matches!(
            sender.send_notification("test/late", serde_json::Value::Null),
            Err(LspError::ClientClosed)
        ));
        assert_eq!(
            messages.lock().len(),
            1,
            "a closed sink is tombstoned after the first failed delivery"
        );
    }

    #[test]
    fn saturated_response_remains_reserved_until_retry_is_ordered() {
        let (runtime, _state) = runtime_without_worker();
        let messages = Arc::new(Mutex::new(Vec::new()));
        let opened = runtime.open_session(
            TransportKind::Stdio,
            scripted_sink(
                [SinkDelivery::Saturated, SinkDelivery::Sent],
                messages.clone(),
            ),
            Arc::new(|| {}),
            None,
        );
        let token = {
            let mut scheduler = runtime.scheduler.lock();
            let AdmissionResult::Admitted {
                response_token: Some(token),
                ..
            } = scheduler.admit_message(opened.session_id, request(1, "initialize"), 20)
            else {
                panic!("initialize must be admitted");
            };
            assert!(matches!(
                scheduler.next_event(),
                Some(SchedulerEvent::Dispatch(_))
            ));
            assert_eq!(
                scheduler.claim_response(&token, ResponseOutcome::Success),
                ResponseClaim::Claimed
            );
            token
        };

        runtime.order_response(
            &token,
            lsp_server::Response {
                id: token.request_id().clone(),
                result: Some(serde_json::json!({ "capabilities": {} })),
                error: None,
            },
        );
        assert_eq!(runtime.pending_responses.lock().len(), 1);
        assert!(
            runtime.pending_response_retry_at().is_some(),
            "a parked response arms the owner's retry timer"
        );
        assert_eq!(
            runtime.scheduler.lock().lifecycle(opened.session_id),
            Some(LifecycleState::InitializeResponding),
            "lifecycle must not advance before the response reaches the sink"
        );
        assert!(runtime.endpoint(opened.session_id).is_some());

        runtime.drain_pending_responses();

        assert!(runtime.pending_responses.lock().is_empty());
        assert!(
            runtime.pending_response_retry_at().is_none(),
            "nothing parked, no timer"
        );
        assert_eq!(
            runtime.scheduler.lock().lifecycle(opened.session_id),
            Some(LifecycleState::InitializeResponded)
        );
        assert_eq!(messages.lock().len(), 2);
    }

    /// Defect containment: a response payload exceeding the per-response
    /// reservation is replaced by a `RequestFailed` error for that request
    /// only — the session stays open and the reservation is released.
    #[test]
    fn oversized_response_payload_fails_only_its_own_request() {
        let limits = IngressLimits {
            response_reservation_bytes: 256,
            ..IngressLimits::default()
        };
        let (runtime, _state) = runtime_without_worker_with_limits(limits);
        let messages = Arc::new(Mutex::new(Vec::new()));
        let opened = runtime.open_session(
            TransportKind::Stdio,
            capturing_sink(messages.clone()),
            Arc::new(|| {}),
            None,
        );
        initialize(&runtime, opened.session_id);
        let AdmissionResult::Admitted {
            response_token: Some(token),
            ..
        } = runtime.scheduler.lock().admit_message(
            opened.session_id,
            request(2, "textDocument/hover"),
            20,
        )
        else {
            panic!("hover must be admitted");
        };
        assert!(matches!(
            runtime.scheduler.lock().next_event(),
            Some(SchedulerEvent::Dispatch(_))
        ));

        runtime.send_handler_result(&token, Ok(serde_json::json!("x".repeat(1024))));

        let messages = messages.lock();
        let Some(Message::Response(response)) = messages.last() else {
            panic!("the replacement error response must reach the sink");
        };
        assert_eq!(response.id, RequestId::from(2));
        let error = response.error.as_ref().expect("must be an error response");
        assert_eq!(error.code, ProtocolErrorKind::RequestFailed.json_rpc_code());
        // Session alive, reservation released (request no longer outstanding).
        assert!(runtime.endpoint(opened.session_id).is_some());
        assert_eq!(runtime.scheduler.lock().outstanding_state(&token), None);
    }

    /// Defect containment: client responses to server-initiated requests
    /// route through the correlation seam instead of being admitted and then
    /// silently dropped.
    #[test]
    fn client_response_routes_through_the_server_request_seam() {
        let (runtime, mut state) = runtime_without_worker();
        let messages = Arc::new(Mutex::new(Vec::new()));
        let opened = runtime.open_session(
            TransportKind::Stdio,
            capturing_sink(messages),
            Arc::new(|| {}),
            None,
        );
        initialize(&runtime, opened.session_id);

        let observed = Arc::new(Mutex::new(None));
        let observed_in_responder = observed.clone();
        runtime.register_server_request(
            opened.session_id,
            RequestId::from(41),
            Box::new(move |response| {
                *observed_in_responder.lock() = Some(response);
            }),
        );

        // An uncorrelated response is logged and dropped, not fatal.
        assert!(matches!(
            runtime.submit(
                opened.session_id,
                Message::Response(lsp_server::Response {
                    id: RequestId::from(99),
                    result: Some(serde_json::Value::Null),
                    error: None,
                }),
            ),
            SubmitResult::Accepted
        ));
        // The registered response reaches its responder via dispatch.
        assert!(matches!(
            runtime.submit(
                opened.session_id,
                Message::Response(lsp_server::Response {
                    id: RequestId::from(41),
                    result: Some(serde_json::json!({ "answer": 42 })),
                    error: None,
                }),
            ),
            SubmitResult::Accepted
        ));
        runtime.drain(&mut state);

        let observed = observed.lock();
        let response = observed.as_ref().expect("responder must run");
        assert_eq!(response.id, RequestId::from(41));
        assert_eq!(response.result, Some(serde_json::json!({ "answer": 42 })),);
        assert!(runtime.server_requests.lock().is_empty());
    }

    #[test]
    fn execute_command_cancel_before_dispatch_owns_response() {
        let (runtime, _state) = runtime_without_worker();
        let session_id = runtime
            .scheduler
            .lock()
            .open_session(TransportKind::Stdio)
            .session_id;
        initialize(&runtime, session_id);
        let token = admit_command(&runtime, session_id, 2);

        assert!(matches!(
            runtime.submit(session_id, cancel(2)),
            SubmitResult::Accepted
        ));
        assert_eq!(
            runtime
                .scheduler
                .lock()
                .claim_response(&token, ResponseOutcome::Success),
            ResponseClaim::AlreadyClaimed
        );
    }

    #[test]
    fn execute_command_cancel_after_dispatch_leaves_normal_response_owner() {
        let (runtime, _state) = runtime_without_worker();
        let session_id = runtime
            .scheduler
            .lock()
            .open_session(TransportKind::Stdio)
            .session_id;
        initialize(&runtime, session_id);
        let token = admit_command(&runtime, session_id, 2);
        let Some(SchedulerEvent::Dispatch(dispatch)) = runtime.scheduler.lock().next_event() else {
            panic!("executeCommand must transition to running");
        };
        assert!(!dispatch.cancellation.unwrap().is_cancelled());

        assert!(matches!(
            runtime.submit(session_id, cancel(2)),
            SubmitResult::Accepted
        ));
        assert_eq!(
            runtime
                .scheduler
                .lock()
                .claim_response(&token, ResponseOutcome::Success),
            ResponseClaim::Claimed,
            "a running side effect must never be reported as canceled"
        );
    }

    /// The protocol session is opened on the owner thread at the first
    /// dispatch for its transport session and dropped by the termination
    /// continuation — no cross-channel ordering is involved.
    #[test]
    fn protocol_session_opens_on_first_dispatch_and_closes_on_termination() {
        let (runtime, mut state) = runtime_without_worker();
        let messages = Arc::new(Mutex::new(Vec::new()));
        let opened = runtime.open_session(
            TransportKind::Stdio,
            capturing_sink(messages),
            Arc::new(|| {}),
            None,
        );
        let key = session_key(opened.session_id);
        assert!(state.session(key).is_err());

        assert!(matches!(
            runtime.submit(opened.session_id, request(1, "initialize")),
            SubmitResult::Accepted
        ));
        runtime.drain(&mut state);
        assert!(
            state.session(key).is_ok(),
            "first dispatch opens the session"
        );

        runtime.close_session(opened.session_id);
        pump_owner_events(&mut state);
        assert!(
            state.session(key).is_err(),
            "termination closes the session"
        );
        assert!(runtime.endpoint(opened.session_id).is_none());
    }

    /// The owner thread is bounded by the runtime: dropping the last runtime
    /// handle ends the loop instead of leaking a parked thread.
    #[test]
    fn owner_thread_exits_when_the_runtime_is_dropped() {
        let state = test_state();
        let (runtime, wake_rx) =
            LspRuntime::with_limits(LspRuntime::default_limits(), state.handle()).unwrap();
        let weak = Arc::downgrade(&runtime);
        let owner = std::thread::Builder::new()
            .name("baml-lsp-owner-test".to_string())
            .spawn(move || owner_loop(&weak, state, &wake_rx))
            .unwrap();
        drop(runtime);
        owner.join().expect("owner loop must return cleanly");
    }
}
