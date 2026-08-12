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
use parking_lot::Mutex;
use sys_ops::io::{self, owned};
use sys_types::{CallId, SysOpContext, SysOpOutput, VmPanic};
use tokio::sync::broadcast;

use crate::{playground_runs::broadcast_run_patch, playground_ws::WsOutMessage};

/// Shared state for the HTTP interceptor.
pub struct PlaygroundHttpState {
    broadcast_tx: broadcast::Sender<WsOutMessage>,
    run_store: Arc<InMemoryRunStore>,
    next_fetch_id: AtomicU64,
    /// Maps response body pointer → (host_call_id, fetch_id) for response body tracking.
    response_to_fetch: Mutex<HashMap<usize, (CallId, u64)>>,
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
            response_to_fetch: Mutex::new(HashMap::new()),
        }
    }
}

pub struct PlaygroundHttp(pub Arc<PlaygroundHttpState>);

fn response_body_key(resp: &owned::http::Response) -> usize {
    Arc::as_ptr(&resp._body) as *const () as usize
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
    fn text(
        &self,
        heap: &Arc<BexHeap>,
        call_id: CallId,
        response: owned::http::Response,
        ctx: &SysOpContext,
    ) -> SysOpOutput<String> {
        let state = self.0.clone();
        let key = response_body_key(&response);
        let fetch_info = state.response_to_fetch.lock().remove(&key);

        let native_result = <sys_native::NativeSysOps as io::IoClassHttpResponse>::text(
            &sys_native::NativeSysOps,
            heap,
            call_id,
            response,
            ctx,
        );

        match fetch_info {
            Some((host_call_id, fetch_id)) => match native_result {
                SysOpOutput::Async(fut) => SysOpOutput::async_op_with_throw(async move {
                    let text = fut.await?;
                    if let Some(patch) = state.run_store.ingest_fetch_updated(
                        &HostCallId::Native(host_call_id),
                        fetch_id,
                        None,
                        None,
                        Vec::new(),
                        Some(text.len()),
                        None,
                    ) {
                        broadcast_run_patch(&state.broadcast_tx, &patch);
                    }
                    let _ = state.broadcast_tx.send(WsOutMessage::FetchLogUpdate {
                        call_id: host_call_id.0,
                        log_id: fetch_id,
                        status: None,
                        duration_ms: None,
                        response_headers: None,
                        response_body: Some(text.clone()),
                        error: None,
                    });
                    Ok(text)
                }),
                other => other,
            },
            None => native_result,
        }
    }

    fn bytes(
        &self,
        heap: &Arc<BexHeap>,
        call_id: CallId,
        response: owned::http::Response,
        ctx: &SysOpContext,
    ) -> SysOpOutput<Vec<u8>> {
        let state = self.0.clone();
        let key = response_body_key(&response);
        let fetch_info = state.response_to_fetch.lock().remove(&key);

        let native_result = <sys_native::NativeSysOps as io::IoClassHttpResponse>::bytes(
            &sys_native::NativeSysOps,
            heap,
            call_id,
            response,
            ctx,
        );

        match fetch_info {
            Some((host_call_id, fetch_id)) => match native_result {
                SysOpOutput::Async(fut) => SysOpOutput::async_op_with_throw(async move {
                    let bytes = fut.await?;
                    if let Some(patch) = state.run_store.ingest_fetch_updated(
                        &HostCallId::Native(host_call_id),
                        fetch_id,
                        None,
                        None,
                        Vec::new(),
                        Some(bytes.len()),
                        None,
                    ) {
                        broadcast_run_patch(&state.broadcast_tx, &patch);
                    }
                    let _ = state.broadcast_tx.send(WsOutMessage::FetchLogUpdate {
                        call_id: host_call_id.0,
                        log_id: fetch_id,
                        status: None,
                        duration_ms: None,
                        response_headers: None,
                        response_body: Some(format!("<binary data: {} bytes>", bytes.len())),
                        error: None,
                    });
                    Ok(bytes)
                }),
                other => other,
            },
            None => native_result,
        }
    }

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

    fn write(
        &self,
        _heap: &Arc<BexHeap>,
        _call_id: CallId,
        _response: owned::http::Response,
        _data: Vec<u8>,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<()> {
        SysOpOutput::err(VmPanic::HostUnavailable {
            resource: "http".to_string(),
            message: "Operation not supported on this platform".to_string(),
        })
    }

    fn end(
        &self,
        _heap: &Arc<BexHeap>,
        _call_id: CallId,
        _response: owned::http::Response,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<()> {
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
        _server: owned::http::Server,
        _handler: sys_types::Handle,
        _tls_config: Option<owned::http::TlsConfig>,
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

impl io::IoClassHttpSseStream for PlaygroundHttp {
    fn next(
        &self,
        heap: &Arc<BexHeap>,
        call_id: CallId,
        sse_stream: owned::http::SseStream,
        ctx: &SysOpContext,
    ) -> SysOpOutput<Option<String>> {
        <sys_native::NativeSysOps as io::IoClassHttpSseStream>::next(
            &sys_native::NativeSysOps,
            heap,
            call_id,
            sse_stream,
            ctx,
        )
    }

    fn close(
        &self,
        heap: &Arc<BexHeap>,
        call_id: CallId,
        sse_stream: owned::http::SseStream,
        ctx: &SysOpContext,
    ) -> SysOpOutput<()> {
        <sys_native::NativeSysOps as io::IoClassHttpSseStream>::close(
            &sys_native::NativeSysOps,
            heap,
            call_id,
            sse_stream,
            ctx,
        )
    }
}

impl io::IoNamespaceHttp for PlaygroundHttp {
    fn _send(
        &self,
        heap: &Arc<BexHeap>,
        call_id: CallId,
        request: owned::http::Request,
        timeout_nanos: Arc<num_bigint::BigInt>,
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
            &sys_native::NativeSysOps,
            heap,
            call_id,
            request,
            timeout_nanos,
            ctx,
        );

        match native_result {
            SysOpOutput::Async(fut) => SysOpOutput::async_op_with_throw(async move {
                let result = fut.await;
                let elapsed = start.elapsed().as_millis() as u64;
                match &result {
                    Ok(resp) => {
                        state
                            .response_to_fetch
                            .lock()
                            .insert(response_body_key(resp), (cid, fetch_id));
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
                result
            }),
            SysOpOutput::Ready(result) => {
                let elapsed = start.elapsed().as_millis() as u64;
                match &result {
                    Ok(resp) => {
                        state
                            .response_to_fetch
                            .lock()
                            .insert(response_body_key(resp), (cid, fetch_id));
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
                SysOpOutput::Ready(result)
            }
        }
    }

    fn _fetch(
        &self,
        heap: &Arc<BexHeap>,
        call_id: CallId,
        url: String,
        timeout_nanos: Arc<num_bigint::BigInt>,
        ctx: &SysOpContext,
    ) -> SysOpOutput<owned::http::Response> {
        let req = owned::http::Request {
            method: "GET".to_string(),
            url,
            headers: indexmap::IndexMap::new(),
            body: String::new(),
        };
        self._send(heap, call_id, req, timeout_nanos, ctx)
    }

    fn fetch_sse(
        &self,
        heap: &Arc<BexHeap>,
        call_id: CallId,
        request: owned::http::Request,
        ctx: &SysOpContext,
    ) -> SysOpOutput<owned::http::SseStream> {
        // Delegate to native implementation — playground doesn't need SSE logging yet.
        <sys_native::NativeSysOps as io::IoNamespaceHttp>::fetch_sse(
            &sys_native::NativeSysOps,
            heap,
            call_id,
            request,
            ctx,
        )
    }
}
