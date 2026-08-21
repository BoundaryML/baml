//! WASM HTTP implementation.
//!
//! `WasmHttp` holds the JS fetch function for regular HTTP requests and uses
//! reqwest directly for SSE streaming. Each `BamlWasmRuntime` gets its own
//! `WasmHttp` instance, so there are no globals.

use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
};

use bex_events::run::{HeaderObservation, InMemoryRunStore};
use js_sys::{Function, Object, Promise, Reflect};
use sys_ops::io::{self, IoClassHttpResponse, IoNamespaceHttp};
use sys_types::{BexHeap, CallId, SysOpContext, SysOpOutput, VmBamlError, VmPanic, VmRustFnError};
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
    run_store: Arc<InMemoryRunStore>,
    notification_callback: crate::send_wrapper::SendWrapper<Function>,
    next_fetch_id: AtomicU64,
}

impl WasmHttp {
    pub(crate) fn new(
        fetch_fn: Function,
        run_store: Arc<InMemoryRunStore>,
        notification_callback: crate::send_wrapper::SendWrapper<Function>,
    ) -> Self {
        Self {
            fetch_fn: crate::send_wrapper::SendWrapper::new(fetch_fn),
            registry: Arc::new(WasmRegistry::new()),
            run_store,
            notification_callback,
            next_fetch_id: AtomicU64::new(1),
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
        let run_store = self.run_store.clone();
        let notification_callback = self.notification_callback.clone();
        let fetch_id = self.next_fetch_id.fetch_add(1, Ordering::Relaxed);
        let host_call_id = crate::runs::wasm_host_call_id(call_id);
        if let Some(host_call_id) = &host_call_id
            && let Some(patch) = self.run_store.ingest_fetch_started(
                host_call_id,
                fetch_id,
                request.method.clone(),
                request.url.clone(),
                header_observations(&request.headers),
                Some(request.body.len()),
            )
        {
            crate::runs::send_run_patch(&self.notification_callback, &patch);
        }

        SysOpOutput::async_op(SendFuture(async move {
            let headers_json =
                serde_json::to_string(&request.headers).map_err(|e| VmBamlError::DevOther {
                    message: format!("Failed to serialize headers: {e}"),
                })?;

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
                    VmBamlError::Io {
                        message: format!("Failed to call fetch function: {msg}"),
                    }
                })?;

            let promise: Promise = promise.dyn_into().map_err(|_| VmBamlError::Io {
                message: "Fetch function did not return a Promise".into(),
            })?;

            let result = match JsFuture::from(promise).await {
                Ok(result) => result,
                Err(e) => {
                    let msg = e
                        .as_string()
                        .or_else(|| {
                            e.dyn_ref::<js_sys::Error>()
                                .map(|err| String::from(err.message()))
                        })
                        .unwrap_or_else(|| format!("{e:?}"));
                    if let Some(host_call_id) = &host_call_id
                        && let Some(patch) = run_store.ingest_fetch_updated(
                            host_call_id,
                            fetch_id,
                            None,
                            None,
                            Vec::new(),
                            None,
                            Some(msg.clone()),
                        )
                    {
                        crate::runs::send_run_patch(&notification_callback, &patch);
                    }
                    return Err(VmRustFnError::from(VmBamlError::Io {
                        message: format!("HTTP request failed: {msg}"),
                    }));
                }
            };

            let obj: Object = result.dyn_into().map_err(|_| VmBamlError::Io {
                message: "Fetch response is not an object".into(),
            })?;

            let status_f64 = Reflect::get(&obj, &"status".into())
                .map_err(|_| VmBamlError::Io {
                    message: "Response missing 'status' field".into(),
                })?
                .as_f64()
                .ok_or_else(|| VmBamlError::Io {
                    message: "Response 'status' is not a number".into(),
                })?;
            // `as i64` for f64 is saturating: NaN → 0, +inf → i64::MAX,
            // -inf → i64::MIN, fractionals → truncated toward zero. None
            // of those make sense for an HTTP status code, and downstream
            // consumers (the stdlib's 2xx success check, auth's
            // `u16::try_from`) would misclassify them as success / 0.
            // `FromPrimitive::from_f64` returns `None` exactly when the
            // value is non-finite, out of `i64` range, or non-integer —
            // the precise set we want to reject.
            let status =
                <i64 as num_traits::FromPrimitive>::from_f64(status_f64).ok_or_else(|| {
                    VmBamlError::Io {
                        message: format!(
                            "Response 'status' must be a finite integer, got {status_f64}"
                        ),
                    }
                })?;

            let headers_str = Reflect::get(&obj, &"headersJson".into())
                .map_err(|_| VmBamlError::Io {
                    message: "Response missing 'headersJson' field".into(),
                })?
                .as_string()
                .ok_or_else(|| VmBamlError::Io {
                    message: "Response 'headersJson' is not a string".into(),
                })?;

            let final_url = Reflect::get(&obj, &"url".into())
                .map_err(|_| VmBamlError::Io {
                    message: "Response missing 'url' field".into(),
                })?
                .as_string()
                .ok_or_else(|| VmBamlError::Io {
                    message: "Response 'url' is not a string".into(),
                })?;

            let body_promise = Reflect::get(&obj, &"bodyPromise".into())
                .map_err(|_| VmBamlError::Io {
                    message: "Response missing 'bodyPromise' field".into(),
                })?
                .dyn_into::<Promise>()
                .map_err(|_| VmBamlError::Io {
                    message: "Response 'bodyPromise' is not a Promise".into(),
                })?;

            let headers: indexmap::IndexMap<String, String> = serde_json::from_str(&headers_str)
                .map_err(|e| VmBamlError::ParseError {
                    message: format!("Failed to parse headersJson: {e}"),
                })?;
            if let Some(host_call_id) = &host_call_id
                && let Some(patch) = run_store.ingest_fetch_updated(
                    host_call_id,
                    fetch_id,
                    Some(status),
                    None,
                    header_observations(&headers),
                    None,
                    None,
                )
            {
                crate::runs::send_run_patch(&notification_callback, &patch);
            }

            let key = registry.store_body_promise(body_promise);
            let body: Arc<dyn std::any::Any + Send + Sync> =
                Arc::new(WasmResponseBody { registry, key });

            Ok(io::owned::http::Response {
                status_code: status,
                headers,
                url: final_url,
                _body: body,
            })
        }))
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
            return SysOpOutput::err(VmBamlError::DevOther {
                message: "Response body handle is not a WasmResponseBody".into(),
            });
        };

        SysOpOutput::async_op(SendFuture(async move {
            let promise =
                registry
                    .take_body_promise(key)
                    .ok_or_else(|| VmBamlError::InvalidArgument {
                        message: "Response body has already been consumed or handle is invalid"
                            .into(),
                    })?;
            let value = JsFuture::from(promise).await.map_err(|e| {
                let msg = e
                    .as_string()
                    .or_else(|| {
                        e.dyn_ref::<js_sys::Error>()
                            .map(|err| String::from(err.message()))
                    })
                    .unwrap_or_else(|| format!("{e:?}"));
                VmBamlError::Io {
                    message: format!("Failed to read response body: {msg}"),
                }
            })?;
            value
                .as_string()
                .ok_or_else(|| VmBamlError::Io {
                    message: "Response body did not resolve to a string".into(),
                })
                .map_err(VmRustFnError::from)
        }))
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
            return SysOpOutput::err(VmBamlError::DevOther {
                message: "Response body handle is not a WasmResponseBody".into(),
            });
        };

        SysOpOutput::async_op(SendFuture(async move {
            let promise =
                registry
                    .take_body_promise(key)
                    .ok_or_else(|| VmBamlError::InvalidArgument {
                        message: "Response body has already been consumed or handle is invalid"
                            .into(),
                    })?;
            let value = JsFuture::from(promise).await.map_err(|e| {
                let msg = e
                    .as_string()
                    .or_else(|| {
                        e.dyn_ref::<js_sys::Error>()
                            .map(|err| String::from(err.message()))
                    })
                    .unwrap_or_else(|| format!("{e:?}"));
                VmBamlError::Io {
                    message: format!("Failed to read response body: {msg}"),
                }
            })?;
            if let Some(arr) = value.dyn_ref::<js_sys::Uint8Array>() {
                Ok(arr.to_vec())
            } else if let Some(buf) = value.dyn_ref::<js_sys::ArrayBuffer>() {
                Ok(js_sys::Uint8Array::new(buf).to_vec())
            } else {
                Err(VmBamlError::Io {
                    message: "Response body did not resolve to a Uint8Array or ArrayBuffer".into(),
                })
            }
            .map_err(VmRustFnError::from)
        }))
    }

    fn new(
        &self,
        _heap: &Arc<BexHeap>,
        _call_id: CallId,
        _status_code: i64,
        _headers: indexmap::IndexMap<String, String>,
        _body: Vec<u8>,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<io::owned::http::Response> {
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
    ) -> SysOpOutput<io::owned::http::Response> {
        SysOpOutput::err(VmPanic::HostUnavailable {
            resource: "http".to_string(),
            message: "Operation not supported on this platform".to_string(),
        })
    }

    fn write(
        &self,
        _heap: &Arc<BexHeap>,
        _call_id: CallId,
        _response: io::owned::http::Response,
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
        _response: io::owned::http::Response,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<()> {
        SysOpOutput::err(VmPanic::HostUnavailable {
            resource: "http".to_string(),
            message: "Operation not supported on this platform".to_string(),
        })
    }
}

// The HTTP server primitives are native-only; a browser cannot bind a listener.
impl io::IoClassHttpTlsConfig for WasmHttp {
    fn _new(
        &self,
        _heap: &Arc<BexHeap>,
        _call_id: CallId,
        _cert_pem: Vec<u8>,
        _key_pem: Vec<u8>,
        _allow_tls1_2: bool,
        _handshake_timeout_nanos: Arc<num_bigint::BigInt>,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<io::owned::http::TlsConfig> {
        SysOpOutput::err(VmPanic::HostUnavailable {
            resource: "http".to_string(),
            message: "Operation not supported on this platform".to_string(),
        })
    }
}

// The HTTP server primitives are native-only; a browser cannot bind a listener.
impl io::IoClassHttpServer for WasmHttp {
    fn bind(
        &self,
        _heap: &Arc<BexHeap>,
        _call_id: CallId,
        _addr: String,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<io::owned::http::Server> {
        SysOpOutput::err(VmPanic::HostUnavailable {
            resource: "http".to_string(),
            message: "Operation not supported on this platform".to_string(),
        })
    }

    fn _serve(
        &self,
        _heap: &Arc<BexHeap>,
        _call_id: CallId,
        _server: io::owned::http::Server,
        _handler: sys_types::Handle,
        _tls_config: Option<io::owned::http::TlsConfig>,
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
            return SysOpOutput::err(VmBamlError::DevOther {
                message: "SSE stream handle is not a WasmSseStreamHandle".into(),
            });
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
                return SysOpOutput::err(VmBamlError::Io {
                    message: format!("SSE stream error: {e}"),
                });
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
        SysOpOutput::async_op(SendFuture(async move {
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
                            return Err(VmRustFnError::from(VmBamlError::Io {
                                message: format!("SSE stream error: {e}"),
                            }));
                        }
                        DrainResult::Done | DrainResult::Empty => {}
                    }
                    Ok(Some(serialize_sse_events(events)?))
                }
                Some(Err(e)) => {
                    handle.mark_done();
                    Err(VmBamlError::Io {
                        message: format!("SSE stream error: {e}"),
                    })
                }
                None => {
                    handle.mark_done();
                    Ok(None)
                }
            }
            .map_err(VmRustFnError::from)
        }))
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
fn serialize_sse_events(events: Vec<sys_types::sse::SseEvent>) -> Result<String, VmBamlError> {
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
    serde_json::to_string(&json_events).map_err(|e| VmBamlError::DevOther {
        message: format!("Failed to serialize SSE events: {e}"),
    })
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
    // `timeout_nanos` is accepted for parity with the native ops but not yet
    // honored: the browser `fetch` backend behind reqwest's wasm client has no
    // straightforward per-request timeout hook. A `null` BAML timeout (the
    // default) is unbounded regardless, so omitting it only affects explicit
    // deadlines on the playground/wasm path.
    fn _fetch(
        &self,
        _heap: &Arc<BexHeap>,
        call_id: CallId,
        url: String,
        _timeout_nanos: Arc<num_bigint::BigInt>,
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

    fn _send(
        &self,
        _heap: &Arc<BexHeap>,
        call_id: CallId,
        request: io::owned::http::Request,
        _timeout_nanos: Arc<num_bigint::BigInt>,
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
        SysOpOutput::async_op(SendFuture(async move {
            let method = reqwest::Method::from_bytes(request.method.as_bytes()).map_err(|e| {
                VmBamlError::InvalidArgument {
                    message: format!("Invalid HTTP method '{}': {e}", request.method),
                }
            })?;

            let client = reqwest::Client::new();
            let mut builder = client.request(method, &request.url);

            for (key, value) in &request.headers {
                builder = builder.header(key.as_str(), value.as_str());
            }

            if !request.body.is_empty() {
                builder = builder.body(request.body.clone());
            }

            let response = builder.send().await.map_err(|e| VmBamlError::Io {
                message: format!("SSE connection failed: {e}"),
            })?;

            if !response.status().is_success() {
                let status = response.status().as_u16();
                let body = response
                    .text()
                    .await
                    .unwrap_or_else(|_| "<could not read body>".to_string());
                return Err(VmRustFnError::from(VmBamlError::Io {
                    message: format!("SSE request failed with status {status}: {body}"),
                }));
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
        }))
    }
}

fn header_observations(headers: &indexmap::IndexMap<String, String>) -> Vec<HeaderObservation> {
    headers
        .iter()
        .map(|(name, value)| HeaderObservation::observe(name, value))
        .collect()
}
