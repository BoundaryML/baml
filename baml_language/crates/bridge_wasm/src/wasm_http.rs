//! WASM HTTP implementation via JS callback.
//!
//! The JS side passes a function at init time that performs HTTP requests.
//! This module wraps that function to implement the `SysOpHttp` trait.

use std::sync::OnceLock;

use bex_heap::builtin_types;
use js_sys::{Function, Object, Promise, Reflect};
use sys_types::{OpErrorKind, SysOpHttp, SysOpOutput};
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::JsFuture;

use crate::{registry::REGISTRY, send_wrapper::SendFuture};

/// The stored JS fetch function.
static HTTP_PROVIDER: OnceLock<WasmHttpProvider> = OnceLock::new();

/// Wrapper around the JS fetch function.
struct WasmHttpProvider {
    /// The JS function to call for HTTP requests.
    /// Signature: (method: string, url: string, headersJson: string, body: string)
    ///            => Promise<{status: number, headers: string, url: string, body: string}>
    fetch_fn: crate::send_wrapper::SendWrapper<Function>,
}

impl WasmHttpProvider {
    /// Create a new HTTP provider with the given JS fetch function.
    fn new(fetch_fn: Function) -> Self {
        Self {
            fetch_fn: crate::send_wrapper::SendWrapper::new(fetch_fn),
        }
    }

    /// Get a reference to the fetch function.
    fn fetch_fn(&self) -> &Function {
        self.fetch_fn.inner()
    }
}

/// Initialize the HTTP provider with a JS fetch function.
///
/// Must be called before any HTTP operations are performed.
/// Returns `Err` if already initialized.
pub(crate) fn init_http_provider(fetch_fn: Function) -> Result<(), &'static str> {
    HTTP_PROVIDER
        .set(WasmHttpProvider::new(fetch_fn))
        .map_err(|_| "HTTP provider already initialized")
}

/// The WASM HTTP implementation.
pub(crate) struct WasmHttp;

impl SysOpHttp for WasmHttp {
    fn baml_http_fetch(url: String) -> SysOpOutput<builtin_types::owned::HttpResponse> {
        let req = builtin_types::owned::HttpRequest {
            method: "GET".to_string(),
            url,
            headers: indexmap::IndexMap::new(),
            body: String::new(),
        };
        Self::baml_http_send(req)
    }

    fn baml_http_send(
        request: builtin_types::owned::HttpRequest,
    ) -> SysOpOutput<builtin_types::owned::HttpResponse> {
        SysOpOutput::Async(Box::pin(SendFuture(async move {
            let provider = HTTP_PROVIDER.get().ok_or_else(|| {
                OpErrorKind::Other("HTTP provider not initialized. Call init() first.".into())
            })?;

            // Serialize headers to JSON
            let headers_json = serde_json::to_string(&request.headers)
                .map_err(|e| OpErrorKind::Other(format!("Failed to serialize headers: {e}")))?;

            // Call the JS fetch function
            let fetch_fn = provider.fetch_fn();
            let promise = fetch_fn
                .call4(
                    &wasm_bindgen::JsValue::NULL,
                    &request.method.into(),
                    &request.url.clone().into(),
                    &headers_json.into(),
                    &request.body.into(),
                )
                .map_err(|e| {
                    let msg = if let Some(s) = e.as_string() {
                        s
                    } else {
                        format!("{:?}", e)
                    };
                    OpErrorKind::Other(format!("Failed to call fetch function: {msg}"))
                })?;

            // Await the promise
            let promise: Promise = promise.dyn_into().map_err(|_| {
                OpErrorKind::Other("Fetch function did not return a Promise".into())
            })?;

            let result = JsFuture::from(promise).await.map_err(|e| {
                let msg = if let Some(s) = e.as_string() {
                    s
                } else if let Some(err) = e.dyn_ref::<js_sys::Error>() {
                    String::from(err.message())
                } else {
                    format!("{:?}", e)
                };
                OpErrorKind::Other(format!("HTTP request failed: {msg}"))
            })?;

            // Parse the response object
            let obj: Object = result
                .dyn_into()
                .map_err(|_| OpErrorKind::Other("Fetch response is not an object".into()))?;

            let status = Reflect::get(&obj, &"status".into())
                .map_err(|_| OpErrorKind::Other("Response missing 'status' field".into()))?
                .as_f64()
                .ok_or_else(|| OpErrorKind::Other("Response 'status' is not a number".into()))?
                as i64;

            let headers_str = Reflect::get(&obj, &"headers".into())
                .map_err(|_| OpErrorKind::Other("Response missing 'headers' field".into()))?
                .as_string()
                .ok_or_else(|| OpErrorKind::Other("Response 'headers' is not a string".into()))?;

            let final_url = Reflect::get(&obj, &"url".into())
                .map_err(|_| OpErrorKind::Other("Response missing 'url' field".into()))?
                .as_string()
                .ok_or_else(|| OpErrorKind::Other("Response 'url' is not a string".into()))?;

            let body = Reflect::get(&obj, &"body".into())
                .map_err(|_| OpErrorKind::Other("Response missing 'body' field".into()))?
                .as_string()
                .ok_or_else(|| OpErrorKind::Other("Response 'body' is not a string".into()))?;

            // Parse headers from JSON
            let headers: indexmap::IndexMap<String, String> =
                serde_json::from_str(&headers_str).unwrap_or_default();

            // Store body in registry and create handle
            let handle = REGISTRY.register_http_response(body, final_url.clone());

            Ok(builtin_types::owned::HttpResponse {
                status_code: status,
                headers,
                url: final_url,
                _handle: handle,
            })
        })))
    }

    fn baml_http_response_text(
        response: builtin_types::owned::HttpResponse,
    ) -> SysOpOutput<String> {
        // For WASM, the body is already stored in the registry - just retrieve it
        let body = REGISTRY
            .consume_http_response_body(response._handle.key())
            .ok_or_else(|| {
                OpErrorKind::Other(
                    "Response body has already been consumed or handle is invalid".into(),
                )
            });

        match body {
            Ok(text) => SysOpOutput::ok(text),
            Err(e) => SysOpOutput::err(e),
        }
    }

    fn baml_http_response_ok(response: builtin_types::owned::HttpResponse) -> SysOpOutput<bool> {
        // Pure Rust check - no async needed
        SysOpOutput::ok((200..300).contains(&response.status_code))
    }
}
