//! WASM HTTP implementation via JS callback.
//!
//! `WasmHttp` holds the JS fetch function and implements the HTTP `sys_ops`.
//! Each `BamlWasmRuntime` gets its own `WasmHttp` instance, so there are no globals.

use std::sync::Arc;

use bex_factory::builtin_types;
use js_sys::{Function, Object, Promise, Reflect};
use sys_types::{OpErrorKind, SysOpHttp, SysOpOutput};
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::JsFuture;

use crate::{registry::WasmRegistry, send_wrapper::SendFuture};

/// WASM HTTP implementation that holds the JS fetch function and response registry.
///
/// Each runtime creates its own `WasmHttp` with the fetch callback;
/// the instance is captured in the `SysOps` closures so no global state is needed.
pub(crate) struct WasmHttp {
    /// The JS function to call for HTTP requests.
    /// Signature: (method, url, headersJson, body) =>
    ///   Promise<{ status: number, headersJson: string, url: string, bodyPromise: Promise<string> }>
    /// The body is only awaited when `response_text()` is called.
    fetch_fn: crate::send_wrapper::SendWrapper<Function>,
    /// Registry for HTTP response bodies (and other resources) for this instance.
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
}

impl SysOpHttp for WasmHttp {
    fn baml_http_fetch(&self, url: String) -> SysOpOutput<builtin_types::owned::HttpResponse> {
        let req = builtin_types::owned::HttpRequest {
            method: "GET".to_string(),
            url,
            headers: indexmap::IndexMap::new(),
            body: String::new(),
        };
        self.baml_http_send(req)
    }

    fn baml_http_send(
        &self,
        request: builtin_types::owned::HttpRequest,
    ) -> SysOpOutput<builtin_types::owned::HttpResponse> {
        let fetch_fn = self.fetch_fn().clone();
        let registry = Arc::clone(&self.registry);
        SysOpOutput::Async(Box::pin(SendFuture(async move {
            let headers_json = serde_json::to_string(&request.headers)
                .map_err(|e| OpErrorKind::Other(format!("Failed to serialize headers: {e}")))?;

            let promise = fetch_fn
                .call4(
                    &wasm_bindgen::JsValue::NULL,
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

            let handle = registry.register_http_response(body_promise, final_url.clone());

            Ok(builtin_types::owned::HttpResponse {
                status_code: status,
                headers,
                url: final_url,
                _handle: handle,
            })
        })))
    }

    fn baml_http_response_text(
        &self,
        response: builtin_types::owned::HttpResponse,
    ) -> SysOpOutput<String> {
        let registry = Arc::clone(&self.registry);
        let key = response._handle.key();
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

    fn baml_http_response_ok(
        &self,
        response: builtin_types::owned::HttpResponse,
    ) -> SysOpOutput<bool> {
        SysOpOutput::ok((200..300).contains(&response.status_code))
    }
}
