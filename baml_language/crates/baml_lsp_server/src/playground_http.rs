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

use bex_heap::BexHeap;
use sys_ops::io::{self, owned};
use sys_types::{CallId, SysOpContext, SysOpOutput};
use tokio::sync::broadcast;

use crate::playground_ws::WsOutMessage;

/// Shared state for the HTTP interceptor.
pub struct PlaygroundHttpState {
    broadcast_tx: broadcast::Sender<WsOutMessage>,
    next_fetch_id: AtomicU64,
    /// Maps response body pointer → (call_id, fetch_id) for response_text tracking.
    response_to_fetch: std::sync::Mutex<HashMap<usize, (u64, u64)>>,
}

impl PlaygroundHttpState {
    pub fn new(broadcast_tx: broadcast::Sender<WsOutMessage>) -> Self {
        Self {
            broadcast_tx,
            next_fetch_id: AtomicU64::new(1),
            response_to_fetch: std::sync::Mutex::new(HashMap::new()),
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
        let fetch_info = state.response_to_fetch.lock().unwrap().remove(&key);

        let native_result = <sys_native::NativeSysOps as io::IoClassHttpResponse>::text(
            &sys_native::NativeSysOps,
            heap,
            call_id,
            response,
            ctx,
        );

        match fetch_info {
            Some((cid, fetch_id)) => match native_result {
                SysOpOutput::Async(fut) => SysOpOutput::async_op(async move {
                    let text = fut.await?;
                    let _ = state.broadcast_tx.send(WsOutMessage::FetchLogUpdate {
                        call_id: cid,
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
        let fetch_info = state.response_to_fetch.lock().unwrap().remove(&key);

        let native_result = <sys_native::NativeSysOps as io::IoClassHttpResponse>::bytes(
            &sys_native::NativeSysOps,
            heap,
            call_id,
            response,
            ctx,
        );

        match fetch_info {
            Some((cid, fetch_id)) => match native_result {
                SysOpOutput::Async(fut) => SysOpOutput::async_op(async move {
                    let bytes = fut.await?;
                    let _ = state.broadcast_tx.send(WsOutMessage::FetchLogUpdate {
                        call_id: cid,
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
    fn send(
        &self,
        heap: &Arc<BexHeap>,
        call_id: CallId,
        request: owned::http::Request,
        ctx: &SysOpContext,
    ) -> SysOpOutput<owned::http::Response> {
        let state = self.0.clone();
        let cid = call_id.0;
        let fetch_id = state.next_fetch_id.fetch_add(1, Ordering::Relaxed);
        let start = std::time::Instant::now();

        let _ = state.broadcast_tx.send(WsOutMessage::FetchLogNew {
            call_id: cid,
            id: fetch_id,
            method: request.method.clone(),
            url: request.url.clone(),
            request_headers: extract_headers_as_hashmap(&request.headers),
            request_body: request.body.clone(),
        });

        let native_result = <sys_native::NativeSysOps as io::IoNamespaceHttp>::send(
            &sys_native::NativeSysOps,
            heap,
            call_id,
            request,
            ctx,
        );

        match native_result {
            SysOpOutput::Async(fut) => SysOpOutput::async_op(async move {
                let result = fut.await;
                let elapsed = start.elapsed().as_millis() as u64;
                match &result {
                    Ok(resp) => {
                        state
                            .response_to_fetch
                            .lock()
                            .unwrap()
                            .insert(response_body_key(resp), (cid, fetch_id));
                        let headers: HashMap<String, String> = resp
                            .headers
                            .iter()
                            .map(|(k, v)| (k.clone(), v.clone()))
                            .collect();
                        let _ = state.broadcast_tx.send(WsOutMessage::FetchLogUpdate {
                            call_id: cid,
                            log_id: fetch_id,
                            status: Some(resp.status_code),
                            duration_ms: Some(elapsed),
                            response_body: None,
                            error: None,
                            response_headers: Some(headers),
                        });
                    }
                    Err(e) => {
                        let _ = state.broadcast_tx.send(WsOutMessage::FetchLogUpdate {
                            call_id: cid,
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
                            .unwrap()
                            .insert(response_body_key(resp), (cid, fetch_id));
                        let headers: HashMap<String, String> = resp
                            .headers
                            .iter()
                            .map(|(k, v)| (k.clone(), v.clone()))
                            .collect();
                        let _ = state.broadcast_tx.send(WsOutMessage::FetchLogUpdate {
                            call_id: cid,
                            log_id: fetch_id,
                            status: Some(resp.status_code),
                            duration_ms: Some(elapsed),
                            response_body: None,
                            error: None,
                            response_headers: Some(headers),
                        });
                    }
                    Err(e) => {
                        let _ = state.broadcast_tx.send(WsOutMessage::FetchLogUpdate {
                            call_id: cid,
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

    fn fetch(
        &self,
        heap: &Arc<BexHeap>,
        call_id: CallId,
        url: String,
        ctx: &SysOpContext,
    ) -> SysOpOutput<owned::http::Response> {
        let req = owned::http::Request {
            method: "GET".to_string(),
            url,
            headers: indexmap::IndexMap::new(),
            body: String::new(),
        };
        self.send(heap, call_id, req, ctx)
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
