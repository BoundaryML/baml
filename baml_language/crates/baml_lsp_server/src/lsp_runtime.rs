//! Process-owned, transport-neutral LSP ingress runtime.
//!
//! One [`IngressScheduler`] behind one process-owned mutex serves every LSP
//! transport: the stdio loop and each `/api/lsp` browser socket submit the
//! same [`lsp_server::Message`] representation and share admission, lifecycle
//! state, cancellation, and outbound-response accounting. Dispatch runs on one
//! dedicated worker thread in FIFO order; `$/cancelRequest` is processed on
//! the *submitting transport thread* so it can claim a response while the
//! worker is inside a slow synchronous handler.
//!
//! Cancellation tokens handed to handlers are operation-owned and observed
//! only at safe boundaries — they are never connected to Salsa's unwind-based
//! cancellation (abort-profile invariant).

use std::{
    collections::{HashMap, HashSet, VecDeque},
    path::{Component, Path, PathBuf},
    sync::{Arc, Mutex, mpsc},
    time::Duration,
};

use crate::lsp_ingress::{
    AdmissionResult, CancelEffect, ControlAdmission, IngressLimits, IngressScheduler,
    LifecycleState, ProtocolError, ResponseClaim, ResponseOutcome, ResponseSizeError,
    ResponseToken, SchedulerEvent, SessionId, SessionTermination, TerminationReason, TransportKind,
};

/// Result of a non-blocking transport enqueue attempt.
///
/// Saturation is deliberately distinct from closure: a full bounded queue is
/// a transient backpressure signal and must not tombstone the session sink.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum SinkDelivery {
    Sent,
    Saturated,
    Oversized,
    Closed,
}

pub(crate) type Sink = Arc<dyn Fn(lsp_server::Message) -> SinkDelivery + Send + Sync>;
pub(crate) type Close = Arc<dyn Fn() + Send + Sync>;
pub(crate) type NotificationHook = Arc<dyn Fn(&lsp_server::Notification) + Send + Sync>;

/// Responder for one server-initiated request's client response (see
/// [`LspRuntime::register_server_request`]).
pub(crate) type ServerRequestResponder = Box<dyn FnOnce(lsp_server::Response) + Send>;

/// Connection-owned LSP output. The sink lives behind a tombstone so an
/// asynchronous tail retaining an old `BexLsp` session clone cannot write
/// after browser takeover or transport close.
pub(crate) struct RevocableSessionSender {
    sink: Mutex<Option<Sink>>,
}

impl RevocableSessionSender {
    fn new(sink: Sink) -> Self {
        Self {
            sink: Mutex::new(Some(sink)),
        }
    }

    fn revoke(&self) {
        if let Ok(mut sink) = self.sink.lock() {
            sink.take();
        }
    }

    #[cfg(test)]
    fn is_closed(&self) -> bool {
        self.sink.lock().map_or(true, |sink| sink.is_none())
    }

    fn send(&self, message: lsp_server::Message) -> Result<(), bex_project::LspError> {
        let mut sink = self
            .sink
            .lock()
            .map_err(|_| bex_project::LspError::ClientClosed)?;
        let Some(active) = sink.as_ref() else {
            return Err(bex_project::LspError::ClientClosed);
        };
        match active(message) {
            SinkDelivery::Sent => Ok(()),
            SinkDelivery::Saturated => Err(bex_project::LspError::OutboundSaturated),
            SinkDelivery::Oversized => Err(bex_project::LspError::OutboundOversized),
            SinkDelivery::Closed => {
                sink.take();
                Err(bex_project::LspError::ClientClosed)
            }
        }
    }
}

impl bex_project::LspClientSenderTrait for RevocableSessionSender {
    fn send_notification(
        &self,
        notification: lsp_server::Notification,
    ) -> Result<(), bex_project::LspError> {
        self.send(lsp_server::Message::Notification(notification))
    }

    fn send_response_impl(
        &self,
        response: lsp_server::Response,
    ) -> Result<(), bex_project::LspError> {
        self.send(lsp_server::Message::Response(response))
    }

    fn make_request(&self, request: lsp_server::Request) -> Result<(), bex_project::LspError> {
        self.send(lsp_server::Message::Request(request))
    }
}

#[derive(Clone)]
struct Endpoint {
    bex: Arc<dyn bex_project::BexLsp>,
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
pub(crate) enum SubmitResult {
    Accepted,
    Dropped,
    Backpressure,
    Exited { normal: bool },
    Closed,
}

pub(crate) struct OpenedSession {
    pub session_id: SessionId,
}

/// One scheduler and one dispatcher thread for every LSP transport in the
/// process. Endpoints are bounded sinks owned by the transport adapters.
pub(crate) struct LspRuntime {
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
    wake_tx: mpsc::SyncSender<()>,
}

impl LspRuntime {
    pub(crate) fn new() -> anyhow::Result<Arc<Self>> {
        let (runtime, wake_rx) = Self::with_limits(Self::default_limits())?;
        let worker = runtime.clone();
        std::thread::Builder::new()
            .name("baml-lsp-ingress".to_string())
            .spawn(move || {
                loop {
                    match wake_rx.recv_timeout(Duration::from_millis(5)) {
                        Ok(()) => worker.drain(),
                        Err(mpsc::RecvTimeoutError::Timeout) => {
                            if worker.has_pending_responses() {
                                worker.drain_pending_responses();
                            }
                        }
                        Err(mpsc::RecvTimeoutError::Disconnected) => return,
                    }
                }
            })?;
        Ok(runtime)
    }

    fn default_limits() -> IngressLimits {
        IngressLimits {
            outbound_response_bytes: 64 * 1024 * 1024,
            response_reservation_bytes: 4 * 1024 * 1024,
            ..IngressLimits::default()
        }
    }

    fn with_limits(limits: IngressLimits) -> anyhow::Result<(Arc<Self>, mpsc::Receiver<()>)> {
        let scheduler = IngressScheduler::new(limits)?;
        let (wake_tx, wake_rx) = mpsc::sync_channel(1);
        let runtime = Arc::new(Self {
            scheduler: Mutex::new(scheduler),
            endpoints: Mutex::new(HashMap::new()),
            document_overlays: Mutex::new(HashMap::new()),
            pending_responses: Mutex::new(VecDeque::new()),
            server_requests: Mutex::new(HashMap::new()),
            delivery_gate: Mutex::new(()),
            wake_tx,
        });
        Ok((runtime, wake_rx))
    }

    pub(crate) fn open_session(
        &self,
        transport: TransportKind,
        bex: Arc<dyn bex_project::BexLsp>,
        sink: Sink,
        close: Close,
        after_notification: Option<NotificationHook>,
    ) -> OpenedSession {
        let _delivery = self.delivery_gate.lock().unwrap();
        let opened = self.scheduler.lock().unwrap().open_session(transport);
        if let Some(takeover) = opened.takeover.clone() {
            // Revoke the old sink and reconcile its overlays before exposing
            // the replacement endpoint. A surviving stdio owner is
            // restored instead of being erased by the synthetic didClose.
            self.finish_termination(takeover);
        }
        let outbound = Arc::new(RevocableSessionSender::new(sink.clone()));
        let bex = bex.new_lsp_session(outbound.clone());
        self.endpoints.lock().unwrap().insert(
            opened.session_id,
            Endpoint {
                bex,
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

    pub(crate) fn submit(
        &self,
        session_id: SessionId,
        message: lsp_server::Message,
    ) -> SubmitResult {
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
            let result = self.scheduler.lock().unwrap().admit_cancel(
                session_id,
                request_id,
                serialized_bytes,
            );
            return match result {
                ControlAdmission::Admitted => {
                    // Cancellation is a true control path: process it on the
                    // submitting transport thread so it can claim a response
                    // while the sole FIFO dispatch worker is inside a slow
                    // synchronous handler.
                    self.drain_controls();
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

        let admission =
            self.scheduler
                .lock()
                .unwrap()
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
                let _delivery = self.delivery_gate.lock().unwrap();
                self.finish_termination(termination);
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
    /// the responder runs on the dispatch worker when the response arrives.
    /// (Correlation seam for a server that starts issuing
    /// `workspace/configuration`-style requests — currently unused.)
    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn register_server_request(
        &self,
        session_id: SessionId,
        request_id: lsp_server::RequestId,
        responder: ServerRequestResponder,
    ) {
        self.server_requests
            .lock()
            .unwrap()
            .insert((session_id, request_id), responder);
    }

    pub(crate) fn close_session(&self, session_id: SessionId) {
        let _delivery = self.delivery_gate.lock().unwrap();
        self.close_session_under_delivery_gate(session_id);
    }

    fn close_session_under_delivery_gate(&self, session_id: SessionId) {
        if let Some(termination) = self.scheduler.lock().unwrap().close_session(session_id) {
            self.finish_termination(termination);
        }
    }

    fn finish_termination(&self, termination: SessionTermination) {
        self.pending_responses
            .lock()
            .unwrap()
            .retain(|pending| pending.token.session_id() != termination.session_id);
        self.server_requests
            .lock()
            .unwrap()
            .retain(|(session_id, _), _| *session_id != termination.session_id);
        // Drop the endpoint-map guard before replaying a surviving overlay:
        // `restore_surviving_overlay` resolves that survivor through the same
        // map. An `if let` scrutinee temporary otherwise lives through the
        // whole body and deadlocks on browser takeover.
        let endpoint = {
            self.endpoints
                .lock()
                .unwrap()
                .remove(&termination.session_id)
        };
        if let Some(endpoint) = endpoint {
            endpoint.outbound.revoke();
            for document in termination.owned_documents {
                let identity = canonical_document_identity_str(&document.uri);
                if let Some(identity) = identity.as_ref() {
                    self.document_overlays
                        .lock()
                        .unwrap()
                        .remove(&(termination.session_id, identity.clone()));
                }
                if !identity
                    .as_ref()
                    .is_some_and(|identity| self.restore_surviving_overlay(identity))
                    && let Ok(uri) = lsp_types::Url::parse(&document.uri)
                {
                    endpoint
                        .bex
                        .handle_notification(lsp_server::Notification::new(
                            "textDocument/didClose".to_string(),
                            lsp_types::DidCloseTextDocumentParams {
                                text_document: lsp_types::TextDocumentIdentifier { uri },
                            },
                        ));
                }
            }
            (endpoint.close)();
        }
    }

    fn restore_surviving_overlay(&self, identity: &Path) -> bool {
        let survivor = self.document_overlays.lock().ok().and_then(|overlays| {
            overlays
                .iter()
                .filter(|((_, candidate_identity), _)| candidate_identity == identity)
                .max_by_key(|((session_id, _), _)| session_id.get())
                .map(|((session_id, _), document)| (*session_id, document.clone()))
        });
        let Some((session_id, document)) = survivor else {
            return false;
        };
        let Some(endpoint) = self.endpoint(session_id) else {
            return false;
        };
        endpoint
            .bex
            .handle_notification(lsp_server::Notification::new(
                "textDocument/didOpen".to_string(),
                lsp_types::DidOpenTextDocumentParams {
                    text_document: document,
                },
            ));
        true
    }

    fn wake(&self) {
        let _ = self.wake_tx.try_send(());
    }

    fn drain_controls(&self) {
        loop {
            let effect = self.scheduler.lock().unwrap().next_control();
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
        self.endpoints.lock().ok()?.get(&session_id).cloned()
    }

    fn drain(&self) {
        self.drain_pending_responses();
        loop {
            let event = self.scheduler.lock().unwrap().next_event();
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
                SchedulerEvent::Dispatch(item) => {
                    let Some(endpoint) = self.endpoint(item.session_id) else {
                        continue;
                    };
                    match item.message {
                        lsp_server::Message::Request(request) => {
                            let Some(token) = item.response_token else {
                                continue;
                            };
                            let runtime = self;
                            endpoint.bex.handle_request_with_cancellation(
                                request,
                                item.cancellation.as_ref(),
                                &mut move |_request_id, result| {
                                    runtime.send_handler_result(&token, result);
                                },
                            );
                        }
                        lsp_server::Message::Notification(notification) => {
                            let tracking = notification.clone();
                            let applied =
                                endpoint.bex.handle_notification_with_status(notification);
                            if !applied {
                                continue;
                            }
                            if self.record_applied_document(item.session_id, &tracking) {
                                if let Some(hook) = &endpoint.after_notification {
                                    hook(&tracking);
                                }
                                if tracking.method == "textDocument/didClose"
                                    && let Some(uri) = tracking
                                        .params
                                        .pointer("/textDocument/uri")
                                        .and_then(serde_json::Value::as_str)
                                    && let Some(identity) = canonical_document_identity_str(uri)
                                {
                                    self.restore_surviving_overlay(&identity);
                                }
                            } else {
                                // Browser takeover/transport close can win
                                // while a synchronous mutation is running.
                                // Close the just-applied old-session overlay
                                // before the FIFO worker can dispatch a new
                                // session's didOpen for the same URI.
                                self.close_orphaned_document(&endpoint, &tracking);
                            }
                        }
                        lsp_server::Message::Response(response) => {
                            // Client response to a server-initiated request:
                            // route through the correlation seam; without a
                            // registration it is logged, never silently lost.
                            let responder = self
                                .server_requests
                                .lock()
                                .unwrap()
                                .remove(&(item.session_id, response.id.clone()));
                            match responder {
                                Some(respond) => respond(response),
                                None => tracing::warn!(
                                    session = item.session_id.get(),
                                    id = %response.id,
                                    "dropping client response with no matching \
                                     server-initiated request"
                                ),
                            }
                        }
                    }
                }
            }
        }
    }

    fn send_handler_result(
        &self,
        token: &ResponseToken,
        result: Result<serde_json::Value, bex_project::LspError>,
    ) {
        let outcome = if result.is_ok() {
            ResponseOutcome::Success
        } else {
            ResponseOutcome::Error
        };
        let claim = self
            .scheduler
            .lock()
            .unwrap()
            .claim_response(token, outcome);
        if claim != ResponseClaim::Claimed {
            return;
        }
        if let Ok(value) = &result {
            // One lock scope: read the lifecycle and record the negotiated
            // initialize result under the same guard.
            let mut scheduler = self.scheduler.lock().unwrap();
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
        let _delivery = self.delivery_gate.lock().unwrap();
        let mut message = lsp_server::Message::Response(response);
        let bytes = serde_json::to_vec(&message).map_or(usize::MAX, |value| value.len());
        match self
            .scheduler
            .lock()
            .unwrap()
            .validate_response_size(token, bytes)
        {
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
        let mut pending = self.pending_responses.lock().unwrap();
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
                let _ = self.scheduler.lock().unwrap().response_ordered(token);
            }
            SinkDelivery::Saturated => {
                self.pending_responses
                    .lock()
                    .unwrap()
                    .push_back(PendingResponse {
                        token: token.clone(),
                        message,
                    });
            }
            SinkDelivery::Oversized | SinkDelivery::Closed => {
                self.close_session_under_delivery_gate(token.session_id());
            }
        }
    }

    fn has_pending_responses(&self) -> bool {
        !self.pending_responses.lock().unwrap().is_empty()
    }

    /// Retry each session's oldest response once without allowing one slow
    /// transport to head-of-line block unrelated sessions. A response keeps
    /// its scheduler reservation until it is actually accepted by the sink.
    fn drain_pending_responses(&self) {
        let _delivery = self.delivery_gate.lock().unwrap();
        let pending = {
            let mut queue = self.pending_responses.lock().unwrap();
            std::mem::take(&mut *queue)
        };
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
                    let _ = self
                        .scheduler
                        .lock()
                        .unwrap()
                        .response_ordered(&pending.token);
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
        self.pending_responses.lock().unwrap().extend(retry);
    }

    fn record_applied_document(
        &self,
        session_id: SessionId,
        notification: &lsp_server::Notification,
    ) -> bool {
        let mut scheduler = self.scheduler.lock().unwrap();
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
                        .unwrap()
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
                            .unwrap()
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
                        .unwrap()
                        .remove(&(session_id, identity));
                    scheduler.record_document_close(session_id, params.text_document.uri.as_str());
                }
            }
            _ => {}
        }
        true
    }

    fn close_orphaned_document(
        &self,
        endpoint: &Endpoint,
        notification: &lsp_server::Notification,
    ) {
        if !matches!(
            notification.method.as_str(),
            "textDocument/didOpen" | "textDocument/didChange"
        ) {
            return;
        }
        let Some(uri) = notification
            .params
            .pointer("/textDocument/uri")
            .and_then(serde_json::Value::as_str)
            .and_then(|uri| lsp_types::Url::parse(uri).ok())
        else {
            return;
        };
        if canonical_document_identity(&uri)
            .as_deref()
            .is_some_and(|identity| self.restore_surviving_overlay(identity))
        {
            return;
        }
        endpoint
            .bex
            .handle_notification(lsp_server::Notification::new(
                "textDocument/didClose".to_string(),
                lsp_types::DidCloseTextDocumentParams {
                    text_document: lsp_types::TextDocumentIdentifier { uri },
                },
            ));
    }
}

fn canonical_document_identity(uri: &lsp_types::Url) -> Option<PathBuf> {
    let path = uri.to_file_path().ok()?;
    Some(canonical_physical_path(&path))
}

fn canonical_document_identity_str(uri: &str) -> Option<PathBuf> {
    canonical_document_identity(&lsp_types::Url::parse(uri).ok()?)
}

/// Resolve URI aliases to one physical identity while still retaining the
/// client's exact URI in `TextDocumentItem` for replay and publications.
fn canonical_physical_path(path: &Path) -> PathBuf {
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir().unwrap_or_default().join(path)
    };
    let mut normalized = PathBuf::new();
    for component in absolute.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                normalized.pop();
            }
            other => normalized.push(other.as_os_str()),
        }
    }
    let mut existing = normalized.as_path();
    let mut missing = Vec::new();
    while !existing.exists() {
        let Some(name) = existing.file_name() else {
            break;
        };
        missing.push(name.to_os_string());
        let Some(parent) = existing.parent() else {
            break;
        };
        existing = parent;
    }
    let mut canonical = std::fs::canonicalize(existing).unwrap_or_else(|_| existing.to_path_buf());
    for component in missing.into_iter().rev() {
        canonical.push(component);
    }
    #[cfg(windows)]
    {
        canonical = PathBuf::from(canonical.to_string_lossy().to_lowercase());
    }
    canonical
}

#[cfg(test)]
mod tests {
    use bex_project::LspClientSenderTrait;
    use lsp_server::{Message, Notification, Request, RequestId};

    use super::*;
    use crate::lsp_ingress::{ProtocolErrorKind, ResponseOrder};

    #[derive(Default)]
    struct NoopPlaygroundSender;

    impl bex_project::PlaygroundSender for NoopPlaygroundSender {
        fn send_playground_notification(&self, _notification: bex_project::PlaygroundNotification) {
        }
    }

    fn capturing_sink(messages: Arc<Mutex<Vec<Message>>>) -> Sink {
        Arc::new(move |message| {
            messages.lock().unwrap().push(message);
            SinkDelivery::Sent
        })
    }

    fn scripted_sink(
        deliveries: impl IntoIterator<Item = SinkDelivery>,
        messages: Arc<Mutex<Vec<Message>>>,
    ) -> Sink {
        let deliveries = Arc::new(Mutex::new(deliveries.into_iter().collect::<VecDeque<_>>()));
        Arc::new(move |message| {
            messages.lock().unwrap().push(message);
            deliveries
                .lock()
                .unwrap()
                .pop_front()
                .unwrap_or(SinkDelivery::Sent)
        })
    }

    fn test_bex(global_messages: Arc<Mutex<Vec<Message>>>) -> Arc<dyn bex_project::BexLsp> {
        let global_sender = Arc::new(RevocableSessionSender::new(capturing_sink(global_messages)));
        let fs: Arc<Box<dyn bex_project::BulkReadFileSystem>> =
            Arc::new(Box::new(crate::native_vfs::NativeVfs::new()));
        Arc::new(bex_project::new_lsp(
            Arc::new(|_| Arc::new(sys_ops::SysOpsBuilder::new().build())),
            global_sender,
            Arc::new(NoopPlaygroundSender),
            bex_project::BamlVFS::new(fs),
            bex_project::BackgroundSpawner::new(),
        ))
    }

    fn runtime_without_worker() -> Arc<LspRuntime> {
        runtime_without_worker_with_limits(LspRuntime::default_limits())
    }

    fn runtime_without_worker_with_limits(limits: IngressLimits) -> Arc<LspRuntime> {
        LspRuntime::with_limits(limits).unwrap().0
    }

    fn request(id: i32, method: &str) -> Message {
        Message::Request(Request {
            id: RequestId::from(id),
            method: method.to_string(),
            params: serde_json::Value::Null,
        })
    }

    fn initialize(runtime: &LspRuntime, session_id: SessionId) {
        let mut scheduler = runtime.scheduler.lock().unwrap();
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
        } = runtime.scheduler.lock().unwrap().admit_message(
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

    #[test]
    fn browser_takeover_tombstones_old_outbound_without_leaking_to_new_or_stdio() {
        let runtime = runtime_without_worker();
        let global_messages = Arc::new(Mutex::new(Vec::new()));
        let bex = test_bex(global_messages.clone());
        let stdio_messages = Arc::new(Mutex::new(Vec::new()));
        let old_browser_messages = Arc::new(Mutex::new(Vec::new()));
        let new_browser_messages = Arc::new(Mutex::new(Vec::new()));
        let close: Close = Arc::new(|| {});

        let stdio = runtime.open_session(
            TransportKind::Stdio,
            bex.clone(),
            capturing_sink(stdio_messages.clone()),
            close.clone(),
            None,
        );
        let old_browser = runtime.open_session(
            TransportKind::Browser,
            bex.clone(),
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
            let mut scheduler = runtime.scheduler.lock().unwrap();
            scheduler.record_document_open(stdio.session_id, shared_uri.clone(), Some(3));
            scheduler.record_document_open(old_browser.session_id, shared_uri.clone(), Some(7));
        }
        runtime.document_overlays.lock().unwrap().insert(
            (stdio.session_id, shared_identity.clone()),
            lsp_types::TextDocumentItem {
                uri: lsp_types::Url::parse(&shared_uri).unwrap(),
                language_id: "baml".to_string(),
                version: 3,
                text: "function stdio() -> int { 3 }".to_string(),
            },
        );
        runtime.document_overlays.lock().unwrap().insert(
            (old_browser.session_id, shared_identity.clone()),
            lsp_types::TextDocumentItem {
                uri: lsp_types::Url::parse(&shared_uri).unwrap(),
                language_id: "baml".to_string(),
                version: 7,
                text: "function browser() -> int { 7 }".to_string(),
            },
        );
        let stale_dispatcher = runtime.endpoint(old_browser.session_id).unwrap().bex;
        let new_browser = runtime.open_session(
            TransportKind::Browser,
            bex,
            capturing_sink(new_browser_messages.clone()),
            close,
            None,
        );
        let stdio_messages_after_restore = stdio_messages.lock().unwrap().len();

        // Unsupported incoming notifications cause BexLsp to emit a
        // window/logMessage notification through the dispatcher's sender.
        // The retained old dispatcher models a late diagnostic/tail clone.
        stale_dispatcher.handle_notification(Notification::new(
            "test/late-old-browser".to_string(),
            serde_json::Value::Null,
        ));
        runtime
            .endpoint(stdio.session_id)
            .unwrap()
            .bex
            .handle_notification(Notification::new(
                "test/stdio".to_string(),
                serde_json::Value::Null,
            ));
        runtime
            .endpoint(new_browser.session_id)
            .unwrap()
            .bex
            .handle_notification(Notification::new(
                "test/new-browser".to_string(),
                serde_json::Value::Null,
            ));

        assert!(old_browser_messages.lock().unwrap().is_empty());
        assert_eq!(
            stdio_messages.lock().unwrap().len(),
            stdio_messages_after_restore + 1
        );
        assert_eq!(new_browser_messages.lock().unwrap().len(), 1);
        assert!(global_messages.lock().unwrap().is_empty());
        let overlays = runtime.document_overlays.lock().unwrap();
        assert!(overlays.contains_key(&(stdio.session_id, shared_identity.clone())));
        assert!(!overlays.contains_key(&(old_browser.session_id, shared_identity)));
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

        let saturated = sender.send_notification(Notification::new(
            "test/first".to_string(),
            serde_json::Value::Null,
        ));
        assert!(matches!(
            saturated,
            Err(bex_project::LspError::OutboundSaturated)
        ));
        assert!(!sender.is_closed());
        assert!(
            sender
                .send_notification(Notification::new(
                    "test/second".to_string(),
                    serde_json::Value::Null,
                ))
                .is_ok(),
            "the live sink must remain installed after transient backpressure"
        );
        assert_eq!(messages.lock().unwrap().len(), 2);
    }

    #[test]
    fn closed_sink_revokes_session_sender() {
        let messages = Arc::new(Mutex::new(Vec::new()));
        let sender = RevocableSessionSender::new(scripted_sink(
            [SinkDelivery::Closed, SinkDelivery::Sent],
            messages.clone(),
        ));

        assert!(matches!(
            sender.send_notification(Notification::new(
                "test/closed".to_string(),
                serde_json::Value::Null,
            )),
            Err(bex_project::LspError::ClientClosed)
        ));
        assert!(sender.is_closed());
        assert!(matches!(
            sender.send_notification(Notification::new(
                "test/late".to_string(),
                serde_json::Value::Null,
            )),
            Err(bex_project::LspError::ClientClosed)
        ));
        assert_eq!(
            messages.lock().unwrap().len(),
            1,
            "a closed sink is tombstoned after the first failed delivery"
        );
    }

    #[test]
    fn saturated_response_remains_reserved_until_retry_is_ordered() {
        let runtime = runtime_without_worker();
        let global_messages = Arc::new(Mutex::new(Vec::new()));
        let bex = test_bex(global_messages);
        let messages = Arc::new(Mutex::new(Vec::new()));
        let opened = runtime.open_session(
            TransportKind::Stdio,
            bex,
            scripted_sink(
                [SinkDelivery::Saturated, SinkDelivery::Sent],
                messages.clone(),
            ),
            Arc::new(|| {}),
            None,
        );
        let token = {
            let mut scheduler = runtime.scheduler.lock().unwrap();
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
        assert_eq!(runtime.pending_responses.lock().unwrap().len(), 1);
        assert_eq!(
            runtime
                .scheduler
                .lock()
                .unwrap()
                .lifecycle(opened.session_id),
            Some(LifecycleState::InitializeResponding),
            "lifecycle must not advance before the response reaches the sink"
        );
        assert!(runtime.endpoint(opened.session_id).is_some());

        runtime.drain_pending_responses();

        assert!(runtime.pending_responses.lock().unwrap().is_empty());
        assert_eq!(
            runtime
                .scheduler
                .lock()
                .unwrap()
                .lifecycle(opened.session_id),
            Some(LifecycleState::InitializeResponded)
        );
        assert_eq!(messages.lock().unwrap().len(), 2);
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
        let runtime = runtime_without_worker_with_limits(limits);
        let global_messages = Arc::new(Mutex::new(Vec::new()));
        let bex = test_bex(global_messages);
        let messages = Arc::new(Mutex::new(Vec::new()));
        let opened = runtime.open_session(
            TransportKind::Stdio,
            bex,
            capturing_sink(messages.clone()),
            Arc::new(|| {}),
            None,
        );
        initialize(&runtime, opened.session_id);
        let AdmissionResult::Admitted {
            response_token: Some(token),
            ..
        } = runtime.scheduler.lock().unwrap().admit_message(
            opened.session_id,
            request(2, "textDocument/hover"),
            20,
        )
        else {
            panic!("hover must be admitted");
        };
        assert!(matches!(
            runtime.scheduler.lock().unwrap().next_event(),
            Some(SchedulerEvent::Dispatch(_))
        ));

        runtime.send_handler_result(&token, Ok(serde_json::json!("x".repeat(1024))));

        let messages = messages.lock().unwrap();
        let Some(Message::Response(response)) = messages.last() else {
            panic!("the replacement error response must reach the sink");
        };
        assert_eq!(response.id, RequestId::from(2));
        let error = response.error.as_ref().expect("must be an error response");
        assert_eq!(error.code, ProtocolErrorKind::RequestFailed.json_rpc_code());
        // Session alive, reservation released (request no longer outstanding).
        assert!(runtime.endpoint(opened.session_id).is_some());
        assert_eq!(
            runtime.scheduler.lock().unwrap().outstanding_state(&token),
            None
        );
    }

    /// Defect containment: client responses to server-initiated requests
    /// route through the correlation seam instead of being admitted and then
    /// silently dropped.
    #[test]
    fn client_response_routes_through_the_server_request_seam() {
        let runtime = runtime_without_worker();
        let global_messages = Arc::new(Mutex::new(Vec::new()));
        let bex = test_bex(global_messages);
        let messages = Arc::new(Mutex::new(Vec::new()));
        let opened = runtime.open_session(
            TransportKind::Stdio,
            bex,
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
                *observed_in_responder.lock().unwrap() = Some(response);
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
        runtime.drain();

        let observed = observed.lock().unwrap();
        let response = observed.as_ref().expect("responder must run");
        assert_eq!(response.id, RequestId::from(41));
        assert_eq!(response.result, Some(serde_json::json!({ "answer": 42 })),);
        assert!(runtime.server_requests.lock().unwrap().is_empty());
    }

    #[test]
    fn execute_command_cancel_before_dispatch_owns_response() {
        let runtime = runtime_without_worker();
        let session_id = runtime
            .scheduler
            .lock()
            .unwrap()
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
                .unwrap()
                .claim_response(&token, ResponseOutcome::Success),
            ResponseClaim::AlreadyClaimed
        );
    }

    #[test]
    fn execute_command_cancel_after_dispatch_leaves_normal_response_owner() {
        let runtime = runtime_without_worker();
        let session_id = runtime
            .scheduler
            .lock()
            .unwrap()
            .open_session(TransportKind::Stdio)
            .session_id;
        initialize(&runtime, session_id);
        let token = admit_command(&runtime, session_id, 2);
        let Some(SchedulerEvent::Dispatch(dispatch)) =
            runtime.scheduler.lock().unwrap().next_event()
        else {
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
                .unwrap()
                .claim_response(&token, ResponseOutcome::Success),
            ResponseClaim::Claimed,
            "a running side effect must never be reported as canceled"
        );
    }
}
