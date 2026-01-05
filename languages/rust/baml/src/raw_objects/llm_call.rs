//! LLM call types for detailed call introspection
//!
//! These wrap FFI pointers to LLM call objects managed by the BAML runtime.

use std::ffi::c_void;

use super::{
    RawObject, RawObjectTrait,
    collector::{StreamTiming, Timing, Usage},
    define_raw_object_wrapper,
    http::{HTTPRequest, HTTPResponse, SSEResponse},
};
use crate::{baml_unreachable, proto::baml_cffi_v1::BamlObjectType};

// =============================================================================
// LLMCall
// =============================================================================

define_raw_object_wrapper! {
    /// Details of a single LLM API call
    LLMCall => ObjectLlmCall
}

impl LLMCall {
    /// Create from an object handle
    pub(crate) fn from_handle(
        handle: crate::proto::baml_cffi_v1::BamlObjectHandle,
        runtime: *const c_void,
    ) -> Self {
        let ptr = super::extract_ptr_from_handle(&handle)
            .unwrap_or_else(|e| baml_unreachable!("Failed to extract LLMCall handle: {e}"));
        Self {
            raw: RawObject::from_pointer(ptr, runtime, BamlObjectType::ObjectLlmCall),
        }
    }

    /// Get the request ID
    pub fn request_id(&self) -> String {
        self.raw.call_method("http_request_id", ())
    }

    /// Get the client name (e.g., "GPT4oMini")
    pub fn client_name(&self) -> String {
        self.raw.call_method("client_name", ())
    }

    /// Get the provider (e.g., "openai", "anthropic")
    pub fn provider(&self) -> String {
        self.raw.call_method("provider", ())
    }

    /// Get the HTTP request details
    pub fn http_request(&self) -> Option<HTTPRequest> {
        self.raw
            .call_method_for_object("http_request", ())
            .ok()
            .map(|handle| HTTPRequest::from_handle(handle, self.raw.runtime()))
    }

    /// Get the HTTP response details (None for streaming calls)
    pub fn http_response(&self) -> Option<HTTPResponse> {
        self.raw
            .call_method_for_object("http_response", ())
            .ok()
            .map(|handle| HTTPResponse::from_handle(handle, self.raw.runtime()))
    }

    /// Get token usage for this call
    pub fn usage(&self) -> Option<Usage> {
        self.raw
            .call_method_for_object("usage", ())
            .ok()
            .map(|handle| Usage::from_handle(handle, self.raw.runtime()))
    }

    /// Whether this call was selected for parsing
    pub fn selected(&self) -> bool {
        self.raw.call_method("selected", ())
    }

    /// Get timing information for this call
    pub fn timing(&self) -> Timing {
        let handle = self
            .raw
            .call_method_for_object("timing", ())
            .unwrap_or_else(|e| baml_unreachable!("Failed to get timing: {e}"));
        Timing::from_handle(handle, self.raw.runtime())
    }
}

// =============================================================================
// LLMStreamCall
// =============================================================================

define_raw_object_wrapper! {
    /// Details of a streaming LLM API call
    LLMStreamCall => ObjectLlmStreamCall
}

impl LLMStreamCall {
    /// Create from an object handle
    pub(crate) fn from_handle(
        handle: crate::proto::baml_cffi_v1::BamlObjectHandle,
        runtime: *const c_void,
    ) -> Self {
        let ptr = super::extract_ptr_from_handle(&handle)
            .unwrap_or_else(|e| baml_unreachable!("Failed to extract LLMStreamCall handle: {e}"));
        Self {
            raw: RawObject::from_pointer(ptr, runtime, BamlObjectType::ObjectLlmStreamCall),
        }
    }

    /// Get the request ID
    pub fn request_id(&self) -> String {
        self.raw.call_method("http_request_id", ())
    }

    /// Get the client name (e.g., "GPT4oMini")
    pub fn client_name(&self) -> String {
        self.raw.call_method("client_name", ())
    }

    /// Get the provider (e.g., "openai", "anthropic")
    pub fn provider(&self) -> String {
        self.raw.call_method("provider", ())
    }

    /// Get the HTTP request details
    pub fn http_request(&self) -> Option<HTTPRequest> {
        self.raw
            .call_method_for_object("http_request", ())
            .ok()
            .map(|handle| HTTPRequest::from_handle(handle, self.raw.runtime()))
    }

    /// Get the HTTP response details (usually None for streaming)
    pub fn http_response(&self) -> Option<HTTPResponse> {
        self.raw
            .call_method_for_object("http_response", ())
            .ok()
            .map(|handle| HTTPResponse::from_handle(handle, self.raw.runtime()))
    }

    /// Get token usage for this call
    pub fn usage(&self) -> Option<Usage> {
        self.raw
            .call_method_for_object("usage", ())
            .ok()
            .map(|handle| Usage::from_handle(handle, self.raw.runtime()))
    }

    /// Whether this call was selected for parsing
    pub fn selected(&self) -> bool {
        self.raw.call_method("selected", ())
    }

    /// Get timing information for this streaming call
    pub fn timing(&self) -> StreamTiming {
        let handle = self
            .raw
            .call_method_for_object("timing", ())
            .unwrap_or_else(|e| baml_unreachable!("Failed to get timing: {e}"));
        StreamTiming::from_handle(handle, self.raw.runtime())
    }

    /// Get the SSE chunks from the streaming response
    pub fn sse_chunks(&self) -> Option<Vec<SSEResponse>> {
        self.raw
            .call_method_for_objects("sse_chunks", ())
            .ok()
            .map(|handles| {
                handles
                    .into_iter()
                    .map(|h| SSEResponse::from_handle(h, self.raw.runtime()))
                    .collect()
            })
    }
}

// =============================================================================
// LLMCallKind - Union type for call or stream call
// =============================================================================

/// Either an LLMCall or LLMStreamCall
///
/// The CFFI layer returns different object types (OBJECT_LLM_CALL vs OBJECT_LLM_STREAM_CALL)
/// based on the actual underlying type. We dispatch on the protobuf oneof discriminator
/// to construct the appropriate variant.
///
/// This mirrors how Python uses `Either<LLMCall, LLMStreamCall>` and Go uses interface
/// embedding where `LLMStreamCall` extends `LLMCall`.
#[derive(Clone)]
pub enum LLMCallKind {
    /// Regular (non-streaming) LLM call
    Call(LLMCall),
    /// Streaming LLM call
    Stream(LLMStreamCall),
}

impl LLMCallKind {
    /// Create from an object handle by dispatching on the object type
    ///
    /// The CFFI layer encodes each object with its actual type:
    /// - `Either::Left(LLMCall)` -> `BamlObjectHandle.llm_call` (OBJECT_LLM_CALL = 6)
    /// - `Either::Right(LLMStreamCall)` -> `BamlObjectHandle.llm_stream_call` (OBJECT_LLM_STREAM_CALL = 7)
    pub(crate) fn from_handle(
        handle: crate::proto::baml_cffi_v1::BamlObjectHandle,
        runtime: *const std::ffi::c_void,
    ) -> Self {
        use crate::proto::baml_cffi_v1::BamlObjectType;

        let object_type =
            super::object_type_from_handle(&handle).unwrap_or(BamlObjectType::ObjectLlmCall);

        match object_type {
            BamlObjectType::ObjectLlmStreamCall => {
                LLMCallKind::Stream(LLMStreamCall::from_handle(handle, runtime))
            }
            _ => LLMCallKind::Call(LLMCall::from_handle(handle, runtime)),
        }
    }

    /// Get the client name
    pub fn client_name(&self) -> String {
        match self {
            LLMCallKind::Call(c) => c.client_name(),
            LLMCallKind::Stream(s) => s.client_name(),
        }
    }

    /// Get the provider
    pub fn provider(&self) -> String {
        match self {
            LLMCallKind::Call(c) => c.provider(),
            LLMCallKind::Stream(s) => s.provider(),
        }
    }

    /// Whether this call was selected for parsing
    pub fn selected(&self) -> bool {
        match self {
            LLMCallKind::Call(c) => c.selected(),
            LLMCallKind::Stream(s) => s.selected(),
        }
    }

    /// Get HTTP request details
    pub fn http_request(&self) -> Option<HTTPRequest> {
        match self {
            LLMCallKind::Call(c) => c.http_request(),
            LLMCallKind::Stream(s) => s.http_request(),
        }
    }

    /// Get usage information
    pub fn usage(&self) -> Option<Usage> {
        match self {
            LLMCallKind::Call(c) => c.usage(),
            LLMCallKind::Stream(s) => s.usage(),
        }
    }

    /// Try to get as a regular LLMCall (returns None for streaming calls)
    pub fn as_call(&self) -> Option<&LLMCall> {
        match self {
            LLMCallKind::Call(c) => Some(c),
            LLMCallKind::Stream(_) => None,
        }
    }

    /// Try to get as a streaming LLMStreamCall (returns None for regular calls)
    pub fn as_stream(&self) -> Option<&LLMStreamCall> {
        match self {
            LLMCallKind::Call(_) => None,
            LLMCallKind::Stream(s) => Some(s),
        }
    }
}
