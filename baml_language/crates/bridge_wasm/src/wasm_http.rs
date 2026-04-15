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

        let Some((stream, parser)) = handle.take_stream() else {
            return SysOpOutput::ok(None);
        };

        SysOpOutput::Async(Box::pin(SendFuture(async move {
            use futures::StreamExt;

            let mut stream = stream;
            let mut parser = parser;

            loop {
                let polled = stream.next().await;
                // close() / mark_done() may have fired while we were awaiting.
                // Discard whatever this poll produced and report end-of-stream —
                // returning buffered events after done==true would violate the
                // "close means no more events" invariant.
                if handle.is_done() {
                    return Ok(None);
                }
                match polled {
                    Some(Ok(bytes)) => {
                        let events = parser.feed(&bytes);
                        if !events.is_empty() {
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
                            let json = serde_json::to_string(&json_events).map_err(|e| {
                                OpErrorKind::Other(format!("Failed to serialize SSE events: {e}"))
                            })?;
                            // Return the stream + parser for future next() calls.
                            handle.return_stream(stream, parser);
                            return Ok(Some(json));
                        }
                        // No complete events yet — continue reading.
                    }
                    Some(Err(e)) => {
                        handle.mark_done();
                        return Err(OpErrorKind::Other(format!("SSE stream error: {e}")));
                    }
                    None => {
                        // Stream ended. Flush any event buffered without a
                        // trailing blank line.
                        let final_events = parser.finish();
                        handle.mark_done();
                        if final_events.is_empty() {
                            return Ok(None);
                        }
                        let json_events: Vec<serde_json::Value> = final_events
                            .into_iter()
                            .map(|e| {
                                serde_json::json!({
                                    "event": e.event,
                                    "data": e.data,
                                    "id": e.id,
                                })
                            })
                            .collect();
                        let json = serde_json::to_string(&json_events).map_err(|e| {
                            OpErrorKind::Other(format!("Failed to serialize SSE events: {e}"))
                        })?;
                        return Ok(Some(json));
                    }
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
        // Mark done and drop the stream, which aborts the underlying fetch.
        if let Some(handle) = sse_stream._handle.downcast_ref::<WasmSseStreamHandle>() {
            handle.mark_done();
        }
        // sse_stream drops here, which drops the Arc<WasmSseStreamHandle>.
        SysOpOutput::ok(())
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
            let handle: Arc<dyn std::any::Any + Send + Sync> =
                Arc::new(WasmSseStreamHandle::new(byte_stream));

            Ok(io::owned::http::SseStream {
                url,
                _handle: handle,
            })
        })))
    }
}
