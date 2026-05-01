//! WASM HTTP implementation.
//!
//! `WasmHttp` holds the JS fetch function for regular HTTP requests and uses
//! reqwest directly for SSE streaming. Each `BamlWasmRuntime` gets its own
//! `WasmHttp` instance, so there are no globals.

use std::sync::Arc;

use js_sys::{Function, Object, Promise, Reflect};
use sys_ops::io::{self, IoClassHttpResponse, IoNamespaceHttp};
use sys_types::{BexHeap, CallId, OpErrorKind, SysOpContext, SysOpOutput};
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::JsFuture;

use crate::{
    registry::{WasmRegistry, WasmResponseBody, WasmSseStreamHandle},
    send_wrapper::SendFuture,
};

/// WASM HTTP implementation that holds the JS fetch function and response registry.
///
/// Regular HTTP uses the JS fetch callback. SSE streaming uses reqwest directly
/// (its WASM backend calls browser fetch under the hood) with `SseParser` for
/// parsing, matching the native implementation.
pub(crate) struct WasmHttp {
    /// The JS function to call for HTTP requests.
    fetch_fn: crate::send_wrapper::SendWrapper<Function>,
    /// Registry for HTTP response bodies for this instance.
    registry: Arc<WasmRegistry>,
}

impl WasmHttp {
    pub(crate) fn new(fetch_fn: Function) -> Self {
        Self {
            fetch_fn: crate::send_wrapper::SendWrapper::new(fetch_fn),
            registry: Arc::new(WasmRegistry::new()),
        }
    }

    fn fetch_fn(&self) -> &Function {
        self.fetch_fn.inner()
    }

    /// Shared implementation for both `fetch` (GET) and `send` (arbitrary method).
    fn do_send(
        &self,
        call_id: CallId,
        request: io::owned::http::Request,
    ) -> SysOpOutput<io::owned::http::Response> {
        let fetch_fn = self.fetch_fn().clone();
        let registry = Arc::clone(&self.registry);

        SysOpOutput::Async(Box::pin(SendFuture(async move {
            let headers_json = serde_json::to_string(&request.headers)
                .map_err(|e| OpErrorKind::Other(format!("Failed to serialize headers: {e}")))?;

            let promise = fetch_fn
                .call5(
                    &wasm_bindgen::JsValue::NULL,
                    #[allow(clippy::cast_precision_loss)]
                    &wasm_bindgen::JsValue::from_f64(call_id.0 as f64),
                    &request.method.into(),
                    &request.url.clone().into(),
                    &headers_json.into(),
                    &request.body.into(),
                )
                .map_err(|e| {
                    let msg = e.as_string().unwrap_or_else(|| format!("{e:?}"));
                    OpErrorKind::Other(format!("Failed to call fetch function: {msg}"))
                })?;

            let promise: Promise = promise.dyn_into().map_err(|_| {
                OpErrorKind::Other("Fetch function did not return a Promise".into())
            })?;

            let result = JsFuture::from(promise).await.map_err(|e| {
                let msg = e
                    .as_string()
                    .or_else(|| {
                        e.dyn_ref::<js_sys::Error>()
                            .map(|err| String::from(err.message()))
                    })
                    .unwrap_or_else(|| format!("{e:?}"));
                OpErrorKind::Other(format!("HTTP request failed: {msg}"))
            })?;

            let obj: Object = result
                .dyn_into()
                .map_err(|_| OpErrorKind::Other("Fetch response is not an object".into()))?;

            #[allow(clippy::cast_possible_truncation)]
            let status = Reflect::get(&obj, &"status".into())
                .map_err(|_| OpErrorKind::Other("Response missing 'status' field".into()))?
                .as_f64()
                .ok_or_else(|| OpErrorKind::Other("Response 'status' is not a number".into()))?
                as i64;

            let headers_str = Reflect::get(&obj, &"headersJson".into())
                .map_err(|_| OpErrorKind::Other("Response missing 'headersJson' field".into()))?
                .as_string()
                .ok_or_else(|| {
                    OpErrorKind::Other("Response 'headersJson' is not a string".into())
                })?;

            let final_url = Reflect::get(&obj, &"url".into())
                .map_err(|_| OpErrorKind::Other("Response missing 'url' field".into()))?
                .as_string()
                .ok_or_else(|| OpErrorKind::Other("Response 'url' is not a string".into()))?;

            let body_promise = Reflect::get(&obj, &"bodyPromise".into())
                .map_err(|_| OpErrorKind::Other("Response missing 'bodyPromise' field".into()))?
                .dyn_into::<Promise>()
                .map_err(|_| {
                    OpErrorKind::Other("Response 'bodyPromise' is not a Promise".into())
                })?;

            let headers: indexmap::IndexMap<String, String> = serde_json::from_str(&headers_str)
                .map_err(|e| OpErrorKind::Other(format!("Failed to parse headersJson: {e}")))?;

            let key = registry.store_body_promise(body_promise);
            let body: Arc<dyn std::any::Any + Send + Sync> =
                Arc::new(WasmResponseBody { registry, key });

            Ok(io::owned::http::Response {
                status_code: status,
                headers,
                url: final_url,
                _body: body,
            })
        })))
    }
}

impl IoClassHttpResponse for WasmHttp {
    fn text(
        &self,
        _heap: &Arc<BexHeap>,
        _call_id: CallId,
        response: io::owned::http::Response,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<String> {
        let registry = Arc::clone(&self.registry);
        let body = response
            ._body
            .downcast_ref::<WasmResponseBody>()
            .map(|b| b.key);
        let Some(key) = body else {
            return SysOpOutput::err(OpErrorKind::Other(
                "Response body handle is not a WasmResponseBody".into(),
            ));
        };

        SysOpOutput::Async(Box::pin(SendFuture(async move {
            let promise = registry.take_body_promise(key).ok_or_else(|| {
                OpErrorKind::Other(
                    "Response body has already been consumed or handle is invalid".into(),
                )
            })?;
            let value = JsFuture::from(promise).await.map_err(|e| {
                let msg = e
                    .as_string()
                    .or_else(|| {
                        e.dyn_ref::<js_sys::Error>()
                            .map(|err| String::from(err.message()))
                    })
                    .unwrap_or_else(|| format!("{e:?}"));
                OpErrorKind::Other(format!("Failed to read response body: {msg}"))
            })?;
            value.as_string().ok_or_else(|| {
                OpErrorKind::Other("Response body did not resolve to a string".into())
            })
        })))
    }

    fn bytes(
        &self,
        _heap: &Arc<BexHeap>,
        _call_id: CallId,
        response: io::owned::http::Response,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<Vec<u8>> {
        let registry = Arc::clone(&self.registry);
        let body = response
            ._body
            .downcast_ref::<WasmResponseBody>()
            .map(|b| b.key);
        let Some(key) = body else {
            return SysOpOutput::err(OpErrorKind::Other(
                "Response body handle is not a WasmResponseBody".into(),
            ));
        };

        SysOpOutput::Async(Box::pin(SendFuture(async move {
            let promise = registry.take_body_promise(key).ok_or_else(|| {
                OpErrorKind::Other(
                    "Response body has already been consumed or handle is invalid".into(),
                )
            })?;
            let value = JsFuture::from(promise).await.map_err(|e| {
                let msg = e
                    .as_string()
                    .or_else(|| {
                        e.dyn_ref::<js_sys::Error>()
                            .map(|err| String::from(err.message()))
                    })
                    .unwrap_or_else(|| format!("{e:?}"));
                OpErrorKind::Other(format!("Failed to read response body: {msg}"))
            })?;
            if let Some(arr) = value.dyn_ref::<js_sys::Uint8Array>() {
                Ok(arr.to_vec())
            } else if let Some(buf) = value.dyn_ref::<js_sys::ArrayBuffer>() {
                Ok(js_sys::Uint8Array::new(buf).to_vec())
            } else {
                Err(OpErrorKind::Other(
                    "Response body did not resolve to a Uint8Array or ArrayBuffer".into(),
                ))
            }
        })))
    }
}

impl io::IoClassHttpSseStream for WasmHttp {
    fn next(
        &self,
        _heap: &Arc<BexHeap>,
        _call_id: CallId,
        sse_stream: io::owned::http::SseStream,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<Option<String>> {
        let handle = sse_stream
            ._handle
            .clone()
            .downcast::<WasmSseStreamHandle>()
            .ok();
        let Some(handle) = handle else {
            return SysOpOutput::err(OpErrorKind::Other(
                "SSE stream handle is not a WasmSseStreamHandle".into(),
            ));
        };

        if handle.is_done() {
            return SysOpOutput::ok(None);
        }

        // Try to drain available events synchronously first (no async needed).
        match drain_receiver(&handle) {
            DrainResult::Events(events) => {
                return match serialize_sse_events(events) {
                    Ok(json) => SysOpOutput::ok(Some(json)),
                    Err(e) => SysOpOutput::err(e),
                };
            }
            DrainResult::Error(e) => {
                return SysOpOutput::err(OpErrorKind::Other(format!("SSE stream error: {e}")));
            }
            DrainResult::Done => {
                return SysOpOutput::ok(None);
            }
            DrainResult::Empty => {
                // No events available — need to await.
            }
        }

        // No events ready — await the next one. We use poll_fn to borrow
        // the receiver only during poll (not across await points), so there
        // is no take/return pattern that could break on re-entrant calls.
        SysOpOutput::Async(Box::pin(SendFuture(async move {
            use futures::stream::StreamExt;

            let event = futures::future::poll_fn(|cx| {
                let mut guard = handle.receiver_ref().borrow_mut();
                let Some(receiver) = guard.as_mut() else {
                    return std::task::Poll::Ready(None);
                };
                receiver.poll_next_unpin(cx)
            })
            .await;

            match event {
                Some(Ok(first)) => {
                    let mut events = vec![first];
                    // Drain any additional events that arrived while we were awaiting.
                    match drain_receiver(&handle) {
                        DrainResult::Events(more) => events.extend(more),
                        DrainResult::Error(e) => {
                            return Err(OpErrorKind::Other(format!("SSE stream error: {e}")));
                        }
                        DrainResult::Done | DrainResult::Empty => {}
                    }
                    Ok(Some(serialize_sse_events(events)?))
                }
                Some(Err(e)) => {
                    handle.mark_done();
                    Err(OpErrorKind::Other(format!("SSE stream error: {e}")))
                }
                None => {
                    handle.mark_done();
                    Ok(None)
                }
            }
        })))
    }

    fn close(
        &self,
        _heap: &Arc<BexHeap>,
        _call_id: CallId,
        sse_stream: io::owned::http::SseStream,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<()> {
        // Drop the receiver, which signals the background task to exit.
        if let Some(handle) = sse_stream._handle.downcast_ref::<WasmSseStreamHandle>() {
            handle.mark_done();
        }
        SysOpOutput::ok(())
    }
}

/// Result of a synchronous drain attempt on the receiver.
enum DrainResult {
    /// Got one or more events.
    Events(Vec<sys_types::sse::SseEvent>),
    /// Background task sent an error.
    Error(String),
    /// Channel closed — stream is done.
    Done,
    /// No events available yet (channel still open).
    Empty,
}

/// Drain all currently available events from the handle's receiver without blocking.
fn drain_receiver(handle: &WasmSseStreamHandle) -> DrainResult {
    let mut guard = handle.receiver_ref().borrow_mut();
    let Some(receiver) = guard.as_mut() else {
        return DrainResult::Done;
    };

    let mut events = Vec::new();
    loop {
        match receiver.try_recv() {
            Ok(Ok(event)) => events.push(event),
            Ok(Err(e)) => {
                // Drop receiver to signal background task.
                guard.take();
                return DrainResult::Error(e);
            }
            Err(futures::channel::mpsc::TryRecvError::Closed) => {
                guard.take();
                if events.is_empty() {
                    return DrainResult::Done;
                }
                return DrainResult::Events(events);
            }
            Err(futures::channel::mpsc::TryRecvError::Empty) => {
                if events.is_empty() {
                    return DrainResult::Empty;
                }
                return DrainResult::Events(events);
            }
        }
    }
}

/// Serialize a batch of SSE events to JSON.
fn serialize_sse_events(events: Vec<sys_types::sse::SseEvent>) -> Result<String, OpErrorKind> {
    let json_events: Vec<serde_json::Value> = events
        .into_iter()
        .map(|e| {
            serde_json::json!({
                "event": e.event,
                "data": e.data,
                "id": e.id,
            })
        })
        .collect();
    serde_json::to_string(&json_events)
        .map_err(|e| OpErrorKind::Other(format!("Failed to serialize SSE events: {e}")))
}

/// Background task that reads from a byte stream, parses SSE events, and sends
/// them through the channel. Exits when the stream ends, errors, or the
/// receiver is dropped (channel closed).
async fn sse_background_task(
    byte_stream: crate::registry::ByteStream,
    sender: futures::channel::mpsc::UnboundedSender<Result<sys_types::sse::SseEvent, String>>,
) {
    use futures::StreamExt;

    let mut stream = byte_stream;
    let mut parser = sys_types::sse::SseParser::new();

    loop {
        match stream.next().await {
            Some(Ok(bytes)) => {
                let events = parser.feed(&bytes);
                for event in events {
                    if sender.unbounded_send(Ok(event)).is_err() {
                        // Receiver dropped — stream was closed.
                        return;
                    }
                }
            }
            Some(Err(e)) => {
                let _ = sender.unbounded_send(Err(format!("{e}")));
                return;
            }
            None => {
                // Stream ended. Flush any buffered event without a trailing blank line.
                let final_events = parser.finish();
                for event in final_events {
                    if sender.unbounded_send(Ok(event)).is_err() {
                        return;
                    }
                }
                return;
            }
        }
    }
}

impl IoNamespaceHttp for WasmHttp {
    fn fetch(
        &self,
        _heap: &Arc<BexHeap>,
        call_id: CallId,
        url: String,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<io::owned::http::Response> {
        let req = io::owned::http::Request {
            method: "GET".to_string(),
            url,
            headers: indexmap::IndexMap::new(),
            body: String::new(),
        };
        self.do_send(call_id, req)
    }

    fn send(
        &self,
        _heap: &Arc<BexHeap>,
        call_id: CallId,
        request: io::owned::http::Request,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<io::owned::http::Response> {
        self.do_send(call_id, request)
    }

    fn fetch_sse(
        &self,
        _heap: &Arc<BexHeap>,
        _call_id: CallId,
        request: io::owned::http::Request,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<io::owned::http::SseStream> {
        SysOpOutput::Async(Box::pin(SendFuture(async move {
            let method = reqwest::Method::from_bytes(request.method.as_bytes()).map_err(|e| {
                OpErrorKind::Other(format!("Invalid HTTP method '{}': {e}", request.method))
            })?;

            let client = reqwest::Client::new();
            let mut builder = client.request(method, &request.url);

            for (key, value) in &request.headers {
                builder = builder.header(key.as_str(), value.as_str());
            }

            if !request.body.is_empty() {
                builder = builder.body(request.body.clone());
            }

            let response = builder
                .send()
                .await
                .map_err(|e| OpErrorKind::Other(format!("SSE connection failed: {e}")))?;

            if !response.status().is_success() {
                let status = response.status().as_u16();
                let body = response
                    .text()
                    .await
                    .unwrap_or_else(|_| "<could not read body>".to_string());
                return Err(OpErrorKind::Other(format!(
                    "SSE request failed with status {status}: {body}"
                )));
            }

            let url = response.url().to_string();
            let byte_stream = Box::pin(response.bytes_stream());

            // Create channel and spawn background task to parse SSE events.
            let (sender, receiver) = futures::channel::mpsc::unbounded();
            wasm_bindgen_futures::spawn_local(sse_background_task(byte_stream, sender));

            let handle: Arc<dyn std::any::Any + Send + Sync> =
                Arc::new(WasmSseStreamHandle::new(receiver));

            Ok(io::owned::http::SseStream {
                url,
                _handle: handle,
            })
        })))
    }
}
