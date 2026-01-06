//! HTTP types for request/response introspection
//!
//! These wrap FFI pointers to HTTP objects managed by the BAML runtime.

use std::{collections::HashMap, ffi::c_void};

use super::{RawObject, RawObjectTrait};
use crate::{baml_unreachable, error::BamlError, proto::baml_cffi_v1::BamlObjectType};

// =============================================================================
// HTTPBody
// =============================================================================

define_raw_object_wrapper! {
    /// HTTP request or response body
    HTTPBody => ObjectHttpBody
}

impl HTTPBody {
    /// Create from an object handle
    pub(crate) fn from_handle(
        handle: crate::proto::baml_cffi_v1::BamlObjectHandle,
        runtime: *const c_void,
    ) -> Self {
        let ptr = super::extract_ptr_from_handle(&handle)
            .unwrap_or_else(|e| baml_unreachable!("Failed to extract HTTPBody handle: {e}"));
        Self {
            raw: RawObject::from_pointer(ptr, runtime, BamlObjectType::ObjectHttpBody),
        }
    }

    /// Get body as text (UTF-8 decoded)
    pub fn text(&self) -> Result<String, BamlError> {
        self.raw.try_call_method("text", ())
    }

    /// Get body as JSON value
    ///
    /// Parses the body text as JSON. Returns an error if the body is not valid
    /// JSON.
    pub fn json(&self) -> Result<serde_json::Value, BamlError> {
        let text: String = self.raw.try_call_method("text", ())?;
        serde_json::from_str(&text)
            .map_err(|e| BamlError::internal(format!("failed to parse JSON: {e}")))
    }
}

// =============================================================================
// HTTPRequest
// =============================================================================

define_raw_object_wrapper! {
    /// HTTP request details
    HTTPRequest => ObjectHttpRequest
}

impl HTTPRequest {
    /// Create from an object handle
    pub(crate) fn from_handle(
        handle: crate::proto::baml_cffi_v1::BamlObjectHandle,
        runtime: *const c_void,
    ) -> Self {
        let ptr = super::extract_ptr_from_handle(&handle)
            .unwrap_or_else(|e| baml_unreachable!("Failed to extract HTTPRequest handle: {e}"));
        Self {
            raw: RawObject::from_pointer(ptr, runtime, BamlObjectType::ObjectHttpRequest),
        }
    }

    /// Get the request ID
    pub fn id(&self) -> String {
        self.raw.call_method("id", ())
    }

    /// Get the request URL
    pub fn url(&self) -> String {
        self.raw.call_method("url", ())
    }

    /// Get the HTTP method (GET, POST, etc.)
    pub fn method(&self) -> String {
        self.raw.call_method("method", ())
    }

    /// Get request headers
    pub fn headers(&self) -> HashMap<String, String> {
        self.raw.call_method("headers", ())
    }

    /// Get the request body
    pub fn body(&self) -> HTTPBody {
        let handle = self
            .raw
            .call_method_for_object("body", ())
            .unwrap_or_else(|e| baml_unreachable!("Failed to get body: {e}"));
        HTTPBody::from_handle(handle, self.raw.runtime())
    }
}

// =============================================================================
// HTTPResponse
// =============================================================================

define_raw_object_wrapper! {
    /// HTTP response details
    HTTPResponse => ObjectHttpResponse
}

impl HTTPResponse {
    /// Create from an object handle
    pub(crate) fn from_handle(
        handle: crate::proto::baml_cffi_v1::BamlObjectHandle,
        runtime: *const c_void,
    ) -> Self {
        let ptr = super::extract_ptr_from_handle(&handle)
            .unwrap_or_else(|e| baml_unreachable!("Failed to extract HTTPResponse handle: {e}"));
        Self {
            raw: RawObject::from_pointer(ptr, runtime, BamlObjectType::ObjectHttpResponse),
        }
    }

    /// Get the request ID this response corresponds to
    pub fn id(&self) -> String {
        self.raw.call_method("id", ())
    }

    /// Get the HTTP status code
    pub fn status(&self) -> i64 {
        self.raw.call_method("status", ())
    }

    /// Get response headers
    pub fn headers(&self) -> HashMap<String, String> {
        self.raw.call_method("headers", ())
    }

    /// Get the response body
    pub fn body(&self) -> HTTPBody {
        let handle = self
            .raw
            .call_method_for_object("body", ())
            .unwrap_or_else(|e| baml_unreachable!("Failed to get body: {e}"));
        HTTPBody::from_handle(handle, self.raw.runtime())
    }
}

// =============================================================================
// SSEResponse
// =============================================================================

define_raw_object_wrapper! {
    /// Server-Sent Event response chunk
    SSEResponse => ObjectSseResponse
}

impl SSEResponse {
    /// Create from an object handle
    pub(crate) fn from_handle(
        handle: crate::proto::baml_cffi_v1::BamlObjectHandle,
        runtime: *const c_void,
    ) -> Self {
        let ptr = super::extract_ptr_from_handle(&handle)
            .unwrap_or_else(|e| baml_unreachable!("Failed to extract SSEResponse handle: {e}"));
        Self {
            raw: RawObject::from_pointer(ptr, runtime, BamlObjectType::ObjectSseResponse),
        }
    }

    /// Get the SSE data as text
    pub fn text(&self) -> String {
        self.raw.call_method("text", ())
    }

    /// Try to parse the SSE data as JSON
    pub fn json(&self) -> Option<serde_json::Value> {
        let text: String = self.raw.call_method("text", ());
        serde_json::from_str(&text).ok()
    }
}
