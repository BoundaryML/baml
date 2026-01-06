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
        self.raw
            .call_method_for_object("body", ())
            .unwrap_or_else(|e| baml_unreachable!("Failed to get body: {e}"))
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
        self.raw
            .call_method_for_object("body", ())
            .unwrap_or_else(|e| baml_unreachable!("Failed to get body: {e}"))
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
