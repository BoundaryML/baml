//! HTTP operations.
//!
//! # Safety
//! This module uses `unsafe` for GC-protected heap access. All unsafe blocks
//! are guarded by `with_gc_protection` which ensures heap stability.
#![allow(
    unsafe_code,
    clippy::needless_pass_by_value,
    clippy::match_wildcard_for_single_variants
)]

use std::{error::Error, fmt::Write, sync::Arc};

use bex_heap::{BexHeap, builtin_types};
use sys_types::{BexExternalValue, OpError, OpErrorKind, SysOp, SysOpResult};

use crate::registry::REGISTRY;

/// Create an HTTP client for use within the current async context.
///
/// We intentionally do NOT use a global `LazyLock<reqwest::Client>` because
/// `reqwest::Client` spawns background connection-pool tasks on the Tokio
/// runtime that is active at creation time. In tests each `#[tokio::test]`
/// creates its own runtime, so a client created on runtime A will fail with
/// "dispatch task is gone" when used on runtime B after A shuts down.
///
/// Creating a client per request is cheap (`reqwest::Client::new()` is just
/// an `Arc` allocation) and avoids the cross-runtime lifetime issue.
fn new_http_client() -> reqwest::Client {
    reqwest::Client::new()
}

/// Format an error and its full `source()` chain so the real cause is visible
/// (e.g. reqwest's top-level "error sending request" often hides the actual reason).
fn format_error_chain(mut err: &dyn Error) -> String {
    let mut s = err.to_string();
    while let Some(src) = err.source() {
        let _ = write!(s, "\n  Caused by: {src}");
        err = src;
    }
    s
}

// ============================================================================
// HTTP Operations
// ============================================================================

/// Fetches a URL and returns a Response resource.
///
/// Signature: `fn fetch(url: String) -> Response`
///
/// Implemented as a GET request via `send_async`.
pub(crate) fn fetch(heap: &Arc<BexHeap>, mut args: Vec<bex_heap::BexValue<'_>>) -> SysOpResult {
    let err = |kind: OpErrorKind| SysOpResult::Ready(Err(OpError::new(SysOp::HttpFetch, kind)));

    if args.len() != 1 {
        return err(OpErrorKind::InvalidArgumentCount {
            expected: 1,
            actual: args.len(),
        });
    }

    let arg0 = args.remove(0);
    let url = match heap.with_gc_protection(move |protected| arg0.as_string(&protected).cloned()) {
        Ok(url) => url,
        Err(e) => return err(e.into()),
    };

    let req = builtin_types::owned::HttpRequest {
        method: "GET".to_string(),
        url,
        headers: indexmap::IndexMap::new(),
        body: String::new(),
    };
    SysOpResult::Async(Box::pin(async move {
        send_async(req)
            .await
            .map_err(|e| OpError::new(SysOp::HttpFetch, e))
    }))
}

/// Gets the response body as text (consumes the body).
///
/// Signature: `fn text(self: Response) -> String`
pub(crate) fn text(heap: &Arc<BexHeap>, mut args: Vec<bex_heap::BexValue<'_>>) -> SysOpResult {
    let err = |kind: OpErrorKind| SysOpResult::Ready(Err(OpError::new(SysOp::ResponseText, kind)));

    if args.len() != 1 {
        return err(OpErrorKind::InvalidArgumentCount {
            expected: 1,
            actual: args.len(),
        });
    }

    let arg0 = args.remove(0);
    let response = match heap.with_gc_protection(move |protected| {
        arg0.as_builtin_class::<builtin_types::HttpResponse>(&protected)
            .and_then(|r| r.into_owned(&protected))
    }) {
        Ok(response) => response,
        Err(e) => return err(e.into()),
    };

    SysOpResult::Async(Box::pin(async move {
        text_async(response._handle)
            .await
            .map_err(|e| OpError::new(SysOp::ResponseText, e))
    }))
}

async fn text_async(
    response: sys_resource_types::ResourceHandle,
) -> Result<BexExternalValue, OpErrorKind> {
    let response_mutex = REGISTRY
        .get_http_response_body(response.key())
        .ok_or_else(|| OpErrorKind::Other("Response handle is invalid".into()))?;

    let response = {
        let mut guard = response_mutex.lock().await;
        guard
            .take()
            .ok_or_else(|| OpErrorKind::Other("Response body has already been consumed".into()))?
    };

    let text = response.text().await.map_err(|e| {
        OpErrorKind::Other(format!(
            "Failed to read response body: {}",
            format_error_chain(&e)
        ))
    })?;

    Ok(BexExternalValue::String(text))
}

/// Checks if the response status is OK (2xx).
///
/// Signature: `fn ok(self: Response) -> bool`
pub(crate) fn ok(heap: &Arc<BexHeap>, mut args: Vec<bex_heap::BexValue<'_>>) -> SysOpResult {
    let err = |kind: OpErrorKind| SysOpResult::Ready(Err(OpError::new(SysOp::ResponseOk, kind)));

    if args.len() != 1 {
        return err(OpErrorKind::InvalidArgumentCount {
            expected: 1,
            actual: args.len(),
        });
    }

    let arg0 = args.remove(0);
    let response = match heap.with_gc_protection(move |protected| {
        arg0.as_builtin_class::<builtin_types::HttpResponse>(&protected)
            .and_then(|r| r.into_owned(&protected))
    }) {
        Ok(response) => response,
        Err(e) => return err(e.into()),
    };

    let result = ok_sync(response);
    SysOpResult::Ready(Ok(result))
}

fn ok_sync(response: builtin_types::owned::HttpResponse) -> BexExternalValue {
    BexExternalValue::Bool((200..300).contains(&response.status_code))
}

/// Sends an HTTP request and returns a Response.
///
/// Signature: `fn send(request: Request) -> Response`
pub(crate) fn send(heap: &Arc<BexHeap>, mut args: Vec<bex_heap::BexValue<'_>>) -> SysOpResult {
    let err = |kind: OpErrorKind| SysOpResult::Ready(Err(OpError::new(SysOp::HttpSend, kind)));

    if args.len() != 1 {
        return err(OpErrorKind::InvalidArgumentCount {
            expected: 1,
            actual: args.len(),
        });
    }

    let arg0 = args.remove(0);
    let request = match heap.with_gc_protection(move |protected| {
        arg0.as_builtin_class::<builtin_types::HttpRequest>(&protected)
            .and_then(|r| r.into_owned(&protected))
    }) {
        Ok(request) => request,
        Err(e) => return err(e.into()),
    };

    SysOpResult::Async(Box::pin(async move {
        send_async(request)
            .await
            .map_err(|e| OpError::new(SysOp::HttpSend, e))
    }))
}

async fn send_async(
    req: builtin_types::owned::HttpRequest,
) -> Result<BexExternalValue, OpErrorKind> {
    let method = reqwest::Method::from_bytes(req.method.as_bytes())
        .map_err(|e| OpErrorKind::Other(format!("Invalid HTTP method '{}': {e}", req.method)))?;

    let client = new_http_client();
    let mut builder = client.request(method, &req.url);

    for (key, value) in &req.headers {
        builder = builder.header(key.as_str(), value.as_str());
    }

    if !req.body.is_empty() {
        builder = builder.body(req.body);
    }

    let response = builder.send().await.map_err(|e| {
        OpErrorKind::Other(format!(
            "HTTP request failed for '{}': {}",
            req.url,
            format_error_chain(&e)
        ))
    })?;

    // Capture metadata before storing
    let status = response.status().as_u16();
    let headers = response
        .headers()
        .iter()
        .map(|(k, v)| (k.as_str().to_string(), v.to_str().unwrap_or("").to_string()))
        .collect();
    let final_url = response.url().to_string();

    let handle = REGISTRY.register_http_response(response, final_url.clone());
    let owned = builtin_types::owned::HttpResponse {
        status_code: i64::from(status),
        headers,
        url: final_url,
        _handle: handle,
    };
    Ok(owned.as_bex_external_value())
}
