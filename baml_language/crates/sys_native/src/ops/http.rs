//! HTTP operations.

use std::{collections::HashMap, sync::LazyLock};

use bex_external_types::BexExternalValue;
use indexmap::IndexMap;
use sys_types::{OpError, SysOpResult};

use crate::registry::REGISTRY;

/// Shared HTTP client with connection pooling.
pub(crate) static HTTP_CLIENT: LazyLock<reqwest::Client> = LazyLock::new(reqwest::Client::new);

/// Fetches a URL and returns a Response resource.
///
/// Signature: `fn fetch(url: String) -> Response`
pub(crate) fn fetch(args: Vec<BexExternalValue>) -> SysOpResult {
    SysOpResult::Async(Box::pin(fetch_async(args)))
}

async fn fetch_async(args: Vec<BexExternalValue>) -> Result<BexExternalValue, OpError> {
    let url = match args.into_iter().next() {
        Some(BexExternalValue::String(s)) => s,
        other => {
            return Err(OpError::TypeError {
                expected: "string URL",
                actual: format!("{other:?}"),
            });
        }
    };

    let response = HTTP_CLIENT
        .get(&url)
        .send()
        .await
        .map_err(|e| OpError::Other(format!("HTTP request failed for '{url}': {e}")))?;

    // Capture metadata before storing
    let status = response.status().as_u16();
    let headers: HashMap<String, String> = response
        .headers()
        .iter()
        .map(|(k, v)| (k.as_str().to_string(), v.to_str().unwrap_or("").to_string()))
        .collect();
    let final_url = response.url().to_string();

    let handle = REGISTRY.register_http_response(response, status, headers, final_url);
    Ok(BexExternalValue::Resource(handle))
}

/// Gets the response body as text (consumes the body).
///
/// Signature: `fn text(self: Response) -> String`
pub(crate) fn text(args: Vec<BexExternalValue>) -> SysOpResult {
    SysOpResult::Async(Box::pin(text_async(args)))
}

async fn text_async(args: Vec<BexExternalValue>) -> Result<BexExternalValue, OpError> {
    let handle = match args.into_iter().next() {
        Some(BexExternalValue::Resource(h)) => h,
        other => {
            return Err(OpError::TypeError {
                expected: "Response resource",
                actual: format!("{other:?}"),
            });
        }
    };

    let response_mutex = REGISTRY
        .get_http_response_body(handle.key())
        .ok_or_else(|| OpError::Other("Response handle is invalid".into()))?;

    let mut guard = response_mutex.lock().await;
    let response = guard
        .take()
        .ok_or_else(|| OpError::Other("Response body has already been consumed".into()))?;

    let text = response
        .text()
        .await
        .map_err(|e| OpError::Other(format!("Failed to read response body: {e}")))?;

    Ok(BexExternalValue::String(text))
}

/// Gets the response status code.
///
/// Signature: `fn status(self: Response) -> i64`
pub(crate) fn status(args: Vec<BexExternalValue>) -> SysOpResult {
    let result = status_sync(args);
    SysOpResult::Ready(result)
}

fn status_sync(args: Vec<BexExternalValue>) -> Result<BexExternalValue, OpError> {
    let handle = match args.into_iter().next() {
        Some(BexExternalValue::Resource(h)) => h,
        other => {
            return Err(OpError::TypeError {
                expected: "Response resource",
                actual: format!("{other:?}"),
            });
        }
    };

    let (status, _, _) = REGISTRY
        .get_http_response_metadata(handle.key())
        .ok_or_else(|| OpError::Other("Response handle is invalid".into()))?;

    Ok(BexExternalValue::Int(i64::from(status)))
}

/// Checks if the response status is OK (2xx).
///
/// Signature: `fn ok(self: Response) -> bool`
pub(crate) fn ok(args: Vec<BexExternalValue>) -> SysOpResult {
    let result = ok_sync(args);
    SysOpResult::Ready(result)
}

fn ok_sync(args: Vec<BexExternalValue>) -> Result<BexExternalValue, OpError> {
    let handle = match args.into_iter().next() {
        Some(BexExternalValue::Resource(h)) => h,
        other => {
            return Err(OpError::TypeError {
                expected: "Response resource",
                actual: format!("{other:?}"),
            });
        }
    };

    let (status, _, _) = REGISTRY
        .get_http_response_metadata(handle.key())
        .ok_or_else(|| OpError::Other("Response handle is invalid".into()))?;

    Ok(BexExternalValue::Bool((200..300).contains(&status)))
}

/// Gets the request URL (may differ from original if redirected).
///
/// Signature: `fn url(self: Response) -> String`
pub(crate) fn url(args: Vec<BexExternalValue>) -> SysOpResult {
    let result = url_sync(args);
    SysOpResult::Ready(result)
}

fn url_sync(args: Vec<BexExternalValue>) -> Result<BexExternalValue, OpError> {
    let handle = match args.into_iter().next() {
        Some(BexExternalValue::Resource(h)) => h,
        other => {
            return Err(OpError::TypeError {
                expected: "Response resource",
                actual: format!("{other:?}"),
            });
        }
    };

    let (_, _, url) = REGISTRY
        .get_http_response_metadata(handle.key())
        .ok_or_else(|| OpError::Other("Response handle is invalid".into()))?;

    Ok(BexExternalValue::String(url))
}

/// Gets the response headers as a map.
///
/// Signature: `fn headers(self: Response) -> Map<String, String>`
pub(crate) fn headers(args: Vec<BexExternalValue>) -> SysOpResult {
    let result = headers_sync(args);
    SysOpResult::Ready(result)
}

fn headers_sync(args: Vec<BexExternalValue>) -> Result<BexExternalValue, OpError> {
    let handle = match args.into_iter().next() {
        Some(BexExternalValue::Resource(h)) => h,
        other => {
            return Err(OpError::TypeError {
                expected: "Response resource",
                actual: format!("{other:?}"),
            });
        }
    };

    let (_, headers, _) = REGISTRY
        .get_http_response_metadata(handle.key())
        .ok_or_else(|| OpError::Other("Response handle is invalid".into()))?;

    let entries: IndexMap<String, BexExternalValue> = headers
        .into_iter()
        .map(|(k, v)| (k, BexExternalValue::String(v)))
        .collect();

    Ok(BexExternalValue::Map {
        key_type: bex_external_types::Ty::String,
        value_type: bex_external_types::Ty::String,
        entries,
    })
}
