//! Shared HTTP helpers used by the `SysOpHttp` trait impl.

use std::{error::Error, fmt::Write};

use bex_heap::builtin_types;
use sys_types::OpErrorKind;

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
pub(crate) fn format_error_chain(mut err: &dyn Error) -> String {
    let mut s = err.to_string();
    while let Some(src) = err.source() {
        let _ = write!(s, "\n  Caused by: {src}");
        err = src;
    }
    s
}

/// Send an HTTP request and return a Response resource.
///
/// Shared by both `fetch` (which creates a GET request) and `send` (which takes a Request).
pub(crate) async fn send_async(
    req: builtin_types::owned::HttpRequest,
) -> Result<builtin_types::owned::HttpResponse, OpErrorKind> {
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

    let response = match builder.send().await {
        Ok(resp) => resp,
        Err(e) => {
            // Network error: return a synthetic response with status_code=0
            // so BAML orchestration code can check ok() and fall back.
            // The error message is available via text() for debugging.
            let error_msg = format_error_chain(&e);
            let handle = REGISTRY.register_error_http_response(req.url.clone(), error_msg.clone());
            let mut headers = indexmap::IndexMap::new();
            headers.insert("x-baml-error".to_string(), error_msg);
            return Ok(builtin_types::owned::HttpResponse {
                status_code: 0,
                headers,
                url: req.url,
                _handle: handle,
            });
        }
    };

    // Capture metadata before storing
    let status = response.status().as_u16();
    let headers = response
        .headers()
        .iter()
        .map(|(k, v)| (k.as_str().to_string(), v.to_str().unwrap_or("").to_string()))
        .collect();
    let final_url = response.url().to_string();

    let handle = REGISTRY.register_http_response(response, final_url.clone());
    Ok(builtin_types::owned::HttpResponse {
        status_code: i64::from(status),
        headers,
        url: final_url,
        _handle: handle,
    })
}
