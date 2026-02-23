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

use tokio::sync::broadcast;

use crate::playground_ws::WsOutMessage;

/// Shared state for the HTTP interceptor.
pub struct PlaygroundHttpState {
    broadcast_tx: broadcast::Sender<WsOutMessage>,
    next_fetch_id: AtomicU64,
    /// Maps response handle key -> (call_id, fetch_id) for response_text tracking.
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

impl sys_types::SysOpHttp for PlaygroundHttp {
    fn baml_http_send(
        &self,
        call_id: sys_types::CallId,
        request: bex_heap::builtin_types::owned::HttpRequest,
    ) -> sys_types::SysOpOutput<bex_heap::builtin_types::owned::HttpResponse> {
        let state = self.0.clone();
        let cid = call_id.0;
        let fetch_id = state.next_fetch_id.fetch_add(1, Ordering::Relaxed);
        let start = std::time::Instant::now();

        let _ = state.broadcast_tx.send(WsOutMessage::FetchLogNew {
            call_id: cid,
            id: fetch_id,
            method: request.method.clone(),
            url: request.url.clone(),
            request_headers: request
                .headers
                .iter()
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect(),
            request_body: request.body.clone(),
        });

        let native_result = sys_native::NativeSysOps.baml_http_send(call_id, request);

        match native_result {
            sys_types::SysOpOutput::Async(fut) => sys_types::SysOpOutput::async_op(async move {
                let result = fut.await;
                let elapsed = start.elapsed().as_millis() as u64;
                match &result {
                    Ok(resp) => {
                        state
                            .response_to_fetch
                            .lock()
                            .unwrap()
                            .insert(resp._handle.key(), (cid, fetch_id));
                        let _ = state.broadcast_tx.send(WsOutMessage::FetchLogUpdate {
                            call_id: cid,
                            log_id: fetch_id,
                            status: Some(resp.status_code),
                            duration_ms: Some(elapsed),
                            response_body: None,
                            error: None,
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
                        });
                    }
                }
                result
            }),
            sys_types::SysOpOutput::Ready(result) => {
                let elapsed = start.elapsed().as_millis() as u64;
                match &result {
                    Ok(resp) => {
                        state
                            .response_to_fetch
                            .lock()
                            .unwrap()
                            .insert(resp._handle.key(), (cid, fetch_id));
                        let _ = state.broadcast_tx.send(WsOutMessage::FetchLogUpdate {
                            call_id: cid,
                            log_id: fetch_id,
                            status: Some(resp.status_code),
                            duration_ms: Some(elapsed),
                            response_body: None,
                            error: None,
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
                        });
                    }
                }
                sys_types::SysOpOutput::Ready(result)
            }
        }
    }

    fn baml_http_fetch(
        &self,
        call_id: sys_types::CallId,
        url: String,
    ) -> sys_types::SysOpOutput<bex_heap::builtin_types::owned::HttpResponse> {
        let req = bex_heap::builtin_types::owned::HttpRequest {
            method: "GET".to_string(),
            url,
            headers: indexmap::IndexMap::new(),
            body: String::new(),
        };
        self.baml_http_send(call_id, req)
    }

    fn baml_http_response_text(
        &self,
        call_id: sys_types::CallId,
        response: bex_heap::builtin_types::owned::HttpResponse,
    ) -> sys_types::SysOpOutput<String> {
        let state = self.0.clone();
        let key = response._handle.key();
        let fetch_info = state.response_to_fetch.lock().unwrap().remove(&key);

        let native_result = sys_native::NativeSysOps.baml_http_response_text(call_id, response);

        match fetch_info {
            Some((cid, fetch_id)) => match native_result {
                sys_types::SysOpOutput::Async(fut) => {
                    sys_types::SysOpOutput::async_op(async move {
                        let text = fut.await?;
                        let _ = state.broadcast_tx.send(WsOutMessage::FetchLogUpdate {
                            call_id: cid,
                            log_id: fetch_id,
                            status: None,
                            duration_ms: None,
                            response_body: Some(text.clone()),
                            error: None,
                        });
                        Ok(text)
                    })
                }
                other => other,
            },
            None => native_result,
        }
    }

    fn baml_http_response_ok(
        &self,
        call_id: sys_types::CallId,
        response: bex_heap::builtin_types::owned::HttpResponse,
    ) -> sys_types::SysOpOutput<bool> {
        sys_native::NativeSysOps.baml_http_response_ok(call_id, response)
    }
}
