//! HTTP interceptor that wraps native HTTP and broadcasts fetch logs.
//!
//! Every outgoing HTTP request is logged to connected playground UIs via
//! the broadcast channel, enabling the fetch log panel in the playground.

use std::{
    collections::HashMap,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
};

use bex_events::run::{HeaderObservation, HostCallId, InMemoryRunStore};
use bex_heap::BexHeap;
use sys_ops::io::{self, owned};
use sys_types::{
    CallId, OpOutput, SysOpContext, SysOpOutput, VmPanic, network::NetworkContext,
    resource::HttpBodyResource,
};
use tokio::sync::broadcast;

use crate::{playground_runs::broadcast_run_patch, playground_ws::WsOutMessage};

/// Shared state for the HTTP interceptor.
pub struct PlaygroundHttpState {
    broadcast_tx: broadcast::Sender<WsOutMessage>,
    run_store: Arc<InMemoryRunStore>,
    next_fetch_id: AtomicU64,
}

impl PlaygroundHttpState {
    pub fn new(
        broadcast_tx: broadcast::Sender<WsOutMessage>,
        run_store: Arc<InMemoryRunStore>,
    ) -> Self {
        Self {
            broadcast_tx,
            run_store,
            next_fetch_id: AtomicU64::new(1),
        }
    }
}

pub struct PlaygroundHttp(pub Arc<PlaygroundHttpState>);

/// A fetched response's body, which reports its first read to the fetch log
/// panel and otherwise is the native body it wraps.
struct PlaygroundBody {
    inner: owned::http::ResponseBody,
    state: Arc<PlaygroundHttpState>,
    host_call_id: CallId,
    fetch_id: u64,
}

impl PlaygroundBody {
    /// Report that the body was read: `size` bytes, shown as `shown`.
    fn report(&self, size: usize, shown: String) {
        let state = &self.state;
        if let Some(patch) = state.run_store.ingest_fetch_updated(
            &HostCallId::Native(self.host_call_id),
            self.fetch_id,
            None,
            None,
            Vec::new(),
            Some(size),
            None,
        ) {
            broadcast_run_patch(&state.broadcast_tx, &patch);
        }
        let _ = state.broadcast_tx.send(WsOutMessage::FetchLogUpdate {
            call_id: self.host_call_id.0,
            log_id: self.fetch_id,
            status: None,
            duration_ms: None,
            response_headers: None,
            response_body: Some(shown),
            error: None,
        });
    }
}

impl HttpBodyResource for PlaygroundBody {
    fn text(self: Arc<Self>) -> OpOutput<String> {
        OpOutput::async_op(async move {
            let text = Arc::clone(&self.inner).text().resolve().await?;
            self.report(text.len(), text.clone());
            Ok(text)
        })
    }

    fn bytes(self: Arc<Self>) -> OpOutput<Vec<u8>> {
        OpOutput::async_op(async move {
            let bytes = Arc::clone(&self.inner).bytes().resolve().await?;
            self.report(bytes.len(), format!("<binary data: {} bytes>", bytes.len()));
            Ok(bytes)
        })
    }

    fn write(self: Arc<Self>, data: Vec<u8>) -> OpOutput<()> {
        Arc::clone(&self.inner).write(data)
    }

    fn end(self: Arc<Self>) -> OpOutput<()> {
        Arc::clone(&self.inner).end()
    }

    fn take_stream(&self) -> sys_types::http::StreamTake {
        self.inner.take_stream()
    }

    fn measure(&self, meter: &mut sys_types::Meter) {
        meter.shared(&self.inner, |meter| self.inner.measure(meter));
    }

    fn network(&self) -> Option<&NetworkContext> {
        self.inner.network()
    }
}

/// `response` with its body reporting to the fetch log as `fetch_id`.
fn logged(
    mut response: owned::http::Response,
    state: &Arc<PlaygroundHttpState>,
    host_call_id: CallId,
    fetch_id: u64,
) -> owned::http::Response {
    response._body = Arc::new(PlaygroundBody {
        inner: response._body,
        state: Arc::clone(state),
        host_call_id,
        fetch_id,
    });
    response
}

fn extract_headers_as_hashmap(
    headers: &indexmap::IndexMap<String, String>,
) -> HashMap<String, String> {
    headers
        .iter()
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect()
}

fn redacted_headers_from_indexmap(
    headers: &indexmap::IndexMap<String, String>,
) -> Vec<HeaderObservation> {
    headers
        .iter()
        .map(|(name, value)| HeaderObservation::observe(name, value))
        .collect()
}

impl io::IoClassHttpResponse for PlaygroundHttp {
    fn new(
        &self,
        _heap: &Arc<BexHeap>,
        _call_id: CallId,
        _status_code: i64,
        _headers: indexmap::IndexMap<String, String>,
        _body: Vec<u8>,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<owned::http::Response> {
        SysOpOutput::err(VmPanic::HostUnavailable {
            resource: "http".to_string(),
            message: "Operation not supported on this platform".to_string(),
        })
    }

    fn new_streaming(
        &self,
        _heap: &Arc<BexHeap>,
        _call_id: CallId,
        _status_code: i64,
        _headers: indexmap::IndexMap<String, String>,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<owned::http::Response> {
        SysOpOutput::err(VmPanic::HostUnavailable {
            resource: "http".to_string(),
            message: "Operation not supported on this platform".to_string(),
        })
    }
}

// The HTTP server primitives are not available in the playground proxy.
impl io::IoClassHttpTlsConfig for PlaygroundHttp {
    fn _new(
        &self,
        _heap: &Arc<BexHeap>,
        _call_id: CallId,
        _cert_pem: Vec<u8>,
        _key_pem: Vec<u8>,
        _allow_tls1_2: bool,
        _handshake_timeout_nanos: Arc<num_bigint::BigInt>,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<owned::http::TlsConfig> {
        SysOpOutput::err(VmPanic::HostUnavailable {
            resource: "http".to_string(),
            message: "Operation not supported on this platform".to_string(),
        })
    }
}

// The HTTP server primitives are not available in the playground proxy.
impl io::IoClassHttpServer for PlaygroundHttp {
    fn bind(
        &self,
        _heap: &Arc<BexHeap>,
        _call_id: CallId,
        _addr: String,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<owned::http::Server> {
        SysOpOutput::err(VmPanic::HostUnavailable {
            resource: "http".to_string(),
            message: "Operation not supported on this platform".to_string(),
        })
    }

    fn _serve(
        &self,
        _heap: &Arc<BexHeap>,
        _call_id: CallId,
        _state: owned::http::ServerState,
        _handler: sys_types::Handle,
        _websocket: sys_types::Handle,
        _tls_keys: Option<owned::http::TlsKeys>,
        _allow_tls1_2: bool,
        _tls_handshake_timeout_nanos: Arc<num_bigint::BigInt>,
        _allow_http1: bool,
        _allow_http2: bool,
        _max_body_size: i64,
        _max_connections: i64,
        _header_read_timeout_nanos: Arc<num_bigint::BigInt>,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<()> {
        SysOpOutput::err(VmPanic::HostUnavailable {
            resource: "http".to_string(),
            message: "Operation not supported on this platform".to_string(),
        })
    }
}

impl io::IoNamespaceHttp for PlaygroundHttp {
    fn _send(
        &self,
        heap: &Arc<BexHeap>,
        call_id: CallId,
        request: owned::http::Request,
        timeout_nanos: Arc<num_bigint::BigInt>,
        connect_timeout_nanos: Arc<num_bigint::BigInt>,
        ctx: &SysOpContext,
    ) -> SysOpOutput<owned::http::Response> {
        let state = self.0.clone();
        let cid = call_id;
        let fetch_id = state.next_fetch_id.fetch_add(1, Ordering::Relaxed);
        let start = std::time::Instant::now();

        if let Some(patch) = state.run_store.ingest_fetch_started(
            &HostCallId::Native(cid),
            fetch_id,
            request.method.clone(),
            request.url.clone(),
            redacted_headers_from_indexmap(&request.headers),
            Some(request.body.len()),
        ) {
            broadcast_run_patch(&state.broadcast_tx, &patch);
        }
        let _ = state.broadcast_tx.send(WsOutMessage::FetchLogNew {
            call_id: cid.0,
            id: fetch_id,
            method: request.method.clone(),
            url: request.url.clone(),
            request_headers: extract_headers_as_hashmap(&request.headers),
            request_body: request.body.clone(),
        });

        let native_result = <sys_native::NativeSysOps as io::IoNamespaceHttp>::_send(
            &sys_native::NativeSysOps::default(),
            heap,
            call_id,
            request,
            timeout_nanos,
            connect_timeout_nanos,
            ctx,
        );

        match native_result {
            SysOpOutput::Async(fut) => SysOpOutput::async_op_with_throw(async move {
                let result = fut.await;
                let elapsed = u64::try_from(start.elapsed().as_millis()).unwrap_or(u64::MAX);
                match &result {
                    Ok(resp) => {
                        let headers: HashMap<String, String> = resp
                            .headers
                            .iter()
                            .map(|(k, v)| (k.clone(), v.clone()))
                            .collect();
                        if let Some(patch) = state.run_store.ingest_fetch_updated(
                            &HostCallId::Native(cid),
                            fetch_id,
                            Some(resp.status_code),
                            Some(elapsed),
                            redacted_headers_from_indexmap(&resp.headers),
                            None,
                            None,
                        ) {
                            broadcast_run_patch(&state.broadcast_tx, &patch);
                        }
                        let _ = state.broadcast_tx.send(WsOutMessage::FetchLogUpdate {
                            call_id: cid.0,
                            log_id: fetch_id,
                            status: Some(resp.status_code),
                            duration_ms: Some(elapsed),
                            response_body: None,
                            error: None,
                            response_headers: Some(headers),
                        });
                    }
                    Err(e) => {
                        if let Some(patch) = state.run_store.ingest_fetch_updated(
                            &HostCallId::Native(cid),
                            fetch_id,
                            Some(0),
                            Some(elapsed),
                            Vec::new(),
                            None,
                            Some(format!("{e}")),
                        ) {
                            broadcast_run_patch(&state.broadcast_tx, &patch);
                        }
                        let _ = state.broadcast_tx.send(WsOutMessage::FetchLogUpdate {
                            call_id: cid.0,
                            log_id: fetch_id,
                            status: Some(0),
                            duration_ms: Some(elapsed),
                            response_body: None,
                            error: Some(format!("{e}")),
                            response_headers: None,
                        });
                    }
                }
                result.map(|resp| logged(resp, &state, cid, fetch_id))
            }),
            SysOpOutput::Ready(result) => {
                let elapsed = u64::try_from(start.elapsed().as_millis()).unwrap_or(u64::MAX);
                match &result {
                    Ok(resp) => {
                        let headers: HashMap<String, String> = resp
                            .headers
                            .iter()
                            .map(|(k, v)| (k.clone(), v.clone()))
                            .collect();
                        if let Some(patch) = state.run_store.ingest_fetch_updated(
                            &HostCallId::Native(cid),
                            fetch_id,
                            Some(resp.status_code),
                            Some(elapsed),
                            redacted_headers_from_indexmap(&resp.headers),
                            None,
                            None,
                        ) {
                            broadcast_run_patch(&state.broadcast_tx, &patch);
                        }
                        let _ = state.broadcast_tx.send(WsOutMessage::FetchLogUpdate {
                            call_id: cid.0,
                            log_id: fetch_id,
                            status: Some(resp.status_code),
                            duration_ms: Some(elapsed),
                            response_body: None,
                            error: None,
                            response_headers: Some(headers),
                        });
                    }
                    Err(e) => {
                        if let Some(patch) = state.run_store.ingest_fetch_updated(
                            &HostCallId::Native(cid),
                            fetch_id,
                            Some(0),
                            Some(elapsed),
                            Vec::new(),
                            None,
                            Some(format!("{e}")),
                        ) {
                            broadcast_run_patch(&state.broadcast_tx, &patch);
                        }
                        let _ = state.broadcast_tx.send(WsOutMessage::FetchLogUpdate {
                            call_id: cid.0,
                            log_id: fetch_id,
                            status: Some(0),
                            duration_ms: Some(elapsed),
                            response_body: None,
                            error: Some(format!("{e}")),
                            response_headers: None,
                        });
                    }
                }
                SysOpOutput::Ready(result.map(|resp| logged(resp, &state, cid, fetch_id)))
            }
        }
    }

    fn _fetch(
        &self,
        heap: &Arc<BexHeap>,
        call_id: CallId,
        url: String,
        timeout_nanos: Arc<num_bigint::BigInt>,
        connect_timeout_nanos: Arc<num_bigint::BigInt>,
        ctx: &SysOpContext,
    ) -> SysOpOutput<owned::http::Response> {
        let req = owned::http::Request {
            method: "GET".to_string(),
            url,
            headers: indexmap::IndexMap::new(),
            body: String::new(),
            timeout: None,
            connect_timeout: None,
        };
        self._send(
            heap,
            call_id,
            req,
            timeout_nanos,
            connect_timeout_nanos,
            ctx,
        )
    }

    fn _send_sse(
        &self,
        heap: &Arc<BexHeap>,
        call_id: CallId,
        request: owned::http::Request,
        timeout_nanos: Arc<num_bigint::BigInt>,
        connect_timeout_nanos: Arc<num_bigint::BigInt>,
        ctx: &SysOpContext,
    ) -> SysOpOutput<owned::http::SseStream> {
        // Delegate to native implementation — playground doesn't need SSE logging yet.
        <sys_native::NativeSysOps as io::IoNamespaceHttp>::_send_sse(
            &sys_native::NativeSysOps::default(),
            heap,
            call_id,
            request,
            timeout_nanos,
            connect_timeout_nanos,
            ctx,
        )
    }
}
