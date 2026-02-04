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

use std::{
    collections::HashMap,
    sync::{Arc, LazyLock},
};

use bex_heap::BexHeap;
use bex_vm_types::{Object, Value};
use sys_resource_types::ResourceHandle;
use sys_types::{BexExternalValue, BexValue, OpError, SysOpResult};

use crate::registry::REGISTRY;

/// Shared HTTP client with connection pooling.
pub(crate) static HTTP_CLIENT: LazyLock<reqwest::Client> = LazyLock::new(reqwest::Client::new);

// ============================================================================
// Wrapper Types for GC-Safe Instance Access
// ============================================================================

/// Wrapper for Response instance that provides GC-safe access to fields.
struct ResponseRef {
    /// The resource handle extracted from the `_handle` field.
    resource_handle: ResourceHandle,
}

impl ResponseRef {
    /// Extract a `ResponseRef` from a `BexValue` that should be a Response instance.
    ///
    /// For opaque handles (heap objects), uses GC-protected access.
    /// For external values (already copied), extracts directly.
    fn from_value(heap: &Arc<BexHeap>, value: &BexValue) -> Result<Self, OpError> {
        match value {
            BexValue::Opaque(handle) => {
                // Access heap object with GC protection
                heap.with_gc_protection(|protected| {
                    let ptr = protected
                        .resolve_handle(handle.slab_key())
                        .ok_or_else(|| OpError::Other("invalid handle".into()))?;

                    // SAFETY: GC protection held, pointer is valid
                    let obj = unsafe { ptr.get() };
                    let Object::Instance(inst) = obj else {
                        return Err(OpError::TypeError {
                            expected: "Response instance",
                            actual: format!("{obj:?}"),
                        });
                    };

                    // Get class to verify type and find field index
                    let Object::Class(class) = (unsafe { inst.class.get() }) else {
                        return Err(OpError::Other("bad class ptr".into()));
                    };

                    if class.name != "baml.http.Response" {
                        return Err(OpError::TypeError {
                            expected: "Response instance",
                            actual: format!("Instance of {}", class.name),
                        });
                    }

                    // Find _handle field
                    let idx = class
                        .fields
                        .iter()
                        .position(|f| f.name == "_handle")
                        .ok_or_else(|| OpError::Other("missing _handle field".into()))?;

                    // Extract resource handle from _handle field
                    match inst.fields.get(idx) {
                        Some(Value::Object(h)) => match unsafe { h.get() } {
                            Object::Resource(rh) => Ok(Self {
                                resource_handle: rh.clone(),
                            }),
                            other => Err(OpError::TypeError {
                                expected: "Resource",
                                actual: format!("{other:?}"),
                            }),
                        },
                        other => Err(OpError::Other(format!("invalid _handle field: {other:?}"))),
                    }
                })
            }
            BexValue::External(BexExternalValue::Instance { class_name, fields }) => {
                // Already copied out - extract directly
                if class_name != "baml.http.Response" {
                    return Err(OpError::TypeError {
                        expected: "Response instance",
                        actual: format!("Instance of {class_name}"),
                    });
                }
                match fields.get("_handle") {
                    Some(BexExternalValue::Resource(h)) => Ok(Self {
                        resource_handle: h.clone(),
                    }),
                    _ => Err(OpError::TypeError {
                        expected: "Resource in _handle field",
                        actual: "missing or invalid _handle".to_string(),
                    }),
                }
            }
            other => Err(OpError::TypeError {
                expected: "Response instance",
                actual: format!("{other:?}"),
            }),
        }
    }

    /// Get the registry key for the underlying response.
    fn key(&self) -> usize {
        self.resource_handle.key()
    }
}

/// Wrapper for Request instance that provides GC-safe access to fields.
struct RequestRef {
    method: String,
    url: String,
    headers: Vec<(String, String)>,
    body: String,
}

impl RequestRef {
    /// Extract a `RequestRef` from a `BexValue` that should be a Request instance.
    fn from_value(heap: &Arc<BexHeap>, value: &BexValue) -> Result<Self, OpError> {
        match value {
            BexValue::Opaque(handle) => {
                heap.with_gc_protection(|protected| {
                    let ptr = protected
                        .resolve_handle(handle.slab_key())
                        .ok_or_else(|| OpError::Other("invalid handle".into()))?;

                    // SAFETY: GC protection held, pointer is valid
                    let obj = unsafe { ptr.get() };
                    let Object::Instance(inst) = obj else {
                        return Err(OpError::TypeError {
                            expected: "Request instance",
                            actual: format!("{obj:?}"),
                        });
                    };

                    let Object::Class(class) = (unsafe { inst.class.get() }) else {
                        return Err(OpError::Other("bad class ptr".into()));
                    };

                    if class.name != "baml.http.Request" {
                        return Err(OpError::TypeError {
                            expected: "Request instance",
                            actual: format!("Instance of {}", class.name),
                        });
                    }

                    // Extract fields by name
                    let field_idx = |name: &str| -> Result<usize, OpError> {
                        class
                            .fields
                            .iter()
                            .position(|f| f.name == name)
                            .ok_or_else(|| OpError::Other(format!("missing field '{name}'")))
                    };

                    let extract_string = |idx: usize| -> Result<String, OpError> {
                        match inst.fields.get(idx) {
                            Some(Value::Object(h)) => match unsafe { h.get() } {
                                Object::String(s) => Ok(s.clone()),
                                other => Err(OpError::TypeError {
                                    expected: "String",
                                    actual: format!("{other:?}"),
                                }),
                            },
                            other => Err(OpError::Other(format!("invalid field value: {other:?}"))),
                        }
                    };

                    let method = extract_string(field_idx("method")?)?;
                    let url = extract_string(field_idx("url")?)?;
                    let body = extract_string(field_idx("body")?)?;

                    // Extract headers map
                    let headers_idx = field_idx("headers")?;
                    let headers = match inst.fields.get(headers_idx) {
                        Some(Value::Object(h)) => match unsafe { h.get() } {
                            Object::Map(map) => map
                                .iter()
                                .map(|(k, v)| {
                                    let val = match v {
                                        Value::Object(ptr) => match unsafe { ptr.get() } {
                                            Object::String(s) => s.clone(),
                                            _ => String::new(),
                                        },
                                        _ => String::new(),
                                    };
                                    (k.clone(), val)
                                })
                                .collect(),
                            _ => Vec::new(),
                        },
                        _ => Vec::new(),
                    };

                    Ok(Self {
                        method,
                        url,
                        headers,
                        body,
                    })
                })
            }
            BexValue::External(BexExternalValue::Instance { class_name, fields }) => {
                if class_name != "baml.http.Request" {
                    return Err(OpError::TypeError {
                        expected: "Request instance",
                        actual: format!("Instance of {class_name}"),
                    });
                }

                let extract_string = |name: &str| -> Result<String, OpError> {
                    match fields.get(name) {
                        Some(BexExternalValue::String(s)) => Ok(s.clone()),
                        other => Err(OpError::Other(format!(
                            "expected String for '{name}', got {other:?}"
                        ))),
                    }
                };

                let method = extract_string("method")?;
                let url = extract_string("url")?;
                let body = extract_string("body")?;

                let headers = match fields.get("headers") {
                    Some(BexExternalValue::Map { entries, .. }) => entries
                        .iter()
                        .filter_map(|(k, v)| {
                            if let BexExternalValue::String(s) = v {
                                Some((k.clone(), s.clone()))
                            } else {
                                None
                            }
                        })
                        .collect(),
                    _ => Vec::new(),
                };

                Ok(Self {
                    method,
                    url,
                    headers,
                    body,
                })
            }
            other => Err(OpError::TypeError {
                expected: "Request instance",
                actual: format!("{other:?}"),
            }),
        }
    }
}

// ============================================================================
// HTTP Operations
// ============================================================================

/// Fetches a URL and returns a Response resource.
///
/// Signature: `fn fetch(url: String) -> Response`
///
/// Implemented as a GET request via `send_async`.
pub(crate) fn fetch(_heap: Arc<BexHeap>, args: &[BexValue]) -> SysOpResult {
    let Some(arg) = args.first() else {
        return SysOpResult::Ready(Err(OpError::TypeError {
            expected: "url argument",
            actual: "no arguments".to_string(),
        }));
    };
    let url = match extract_string(arg) {
        Ok(u) => u,
        Err(e) => return SysOpResult::Ready(Err(e)),
    };
    let request_ref = RequestRef {
        method: "GET".to_string(),
        url,
        headers: Vec::new(),
        body: String::new(),
    };
    SysOpResult::Async(Box::pin(send_async(request_ref)))
}

fn extract_string(value: &BexValue) -> Result<String, OpError> {
    match value {
        BexValue::External(BexExternalValue::String(s)) => Ok(s.clone()),
        other => Err(OpError::TypeError {
            expected: "string URL",
            actual: format!("{other:?}"),
        }),
    }
}

/// Gets the response body as text (consumes the body).
///
/// Signature: `fn text(self: Response) -> String`
pub(crate) fn text(heap: Arc<BexHeap>, args: &[BexValue]) -> SysOpResult {
    // Extract key synchronously with GC protection, then run async
    let response_ref = match args.first() {
        Some(value) => match ResponseRef::from_value(&heap, value) {
            Ok(r) => r,
            Err(e) => return SysOpResult::Ready(Err(e)),
        },
        None => {
            return SysOpResult::Ready(Err(OpError::TypeError {
                expected: "Response instance",
                actual: "no arguments".to_string(),
            }));
        }
    };

    let key = response_ref.key();
    SysOpResult::Async(Box::pin(text_async(key)))
}

async fn text_async(key: usize) -> Result<BexExternalValue, OpError> {
    let response_mutex = REGISTRY
        .get_http_response_body(key)
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

/// Checks if the response status is OK (2xx).
///
/// Signature: `fn ok(self: Response) -> bool`
pub(crate) fn ok(heap: Arc<BexHeap>, args: &[BexValue]) -> SysOpResult {
    let result = ok_sync(&heap, args);
    SysOpResult::Ready(result)
}

fn ok_sync(heap: &Arc<BexHeap>, args: &[BexValue]) -> Result<BexExternalValue, OpError> {
    let response_ref = match args.first() {
        Some(value) => ResponseRef::from_value(heap, value)?,
        None => {
            return Err(OpError::TypeError {
                expected: "Response instance",
                actual: "no arguments".to_string(),
            });
        }
    };

    let (status, _, _) = REGISTRY
        .get_http_response_metadata(response_ref.key())
        .ok_or_else(|| OpError::Other("Response handle is invalid".into()))?;

    Ok(BexExternalValue::Bool((200..300).contains(&status)))
}

/// Sends an HTTP request and returns a Response.
///
/// Signature: `fn send(request: Request) -> Response`
pub(crate) fn send(heap: Arc<BexHeap>, args: &[BexValue]) -> SysOpResult {
    let request_ref = match args.first() {
        Some(value) => match RequestRef::from_value(&heap, value) {
            Ok(r) => r,
            Err(e) => return SysOpResult::Ready(Err(e)),
        },
        None => {
            return SysOpResult::Ready(Err(OpError::TypeError {
                expected: "Request instance",
                actual: "no arguments".to_string(),
            }));
        }
    };

    SysOpResult::Async(Box::pin(send_async(request_ref)))
}

async fn send_async(req: RequestRef) -> Result<BexExternalValue, OpError> {
    let method = reqwest::Method::from_bytes(req.method.as_bytes())
        .map_err(|e| OpError::Other(format!("Invalid HTTP method '{}': {e}", req.method)))?;

    let mut builder = HTTP_CLIENT.request(method, &req.url);

    for (key, value) in &req.headers {
        builder = builder.header(key.as_str(), value.as_str());
    }

    if !req.body.is_empty() {
        builder = builder.body(req.body);
    }

    let response = builder
        .send()
        .await
        .map_err(|e| OpError::Other(format!("HTTP request failed for '{}': {e}", req.url)))?;

    // Capture metadata before storing
    let status = response.status().as_u16();
    let headers: HashMap<String, String> = response
        .headers()
        .iter()
        .map(|(k, v)| (k.as_str().to_string(), v.to_str().unwrap_or("").to_string()))
        .collect();
    let final_url = response.url().to_string();

    let handle =
        REGISTRY.register_http_response(response, status, headers.clone(), final_url.clone());
    Ok(bex_external_types::builtins::new_http_response(
        handle, status, headers, final_url,
    ))
}
