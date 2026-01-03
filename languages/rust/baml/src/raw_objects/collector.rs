//! Collector and related types (FunctionLog, Usage)
//!
//! These wrap FFI pointers to collector objects managed by the BAML runtime.

use std::collections::HashMap;
use std::ffi::c_void;

use crate::baml_unreachable;
use crate::codec::BamlDecode;
use crate::error::BamlError;
use crate::proto::baml_cffi_v1::{BamlObjectType, CffiValueHolder};

use super::llm_call::LLMCallKind;
use super::{define_raw_object_wrapper, RawObject, RawObjectTrait};

// =============================================================================
// LogType Enum
// =============================================================================

/// Type of function log entry
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogType {
    /// Regular function call
    Call,
    /// Streaming function call
    Stream,
}

impl BamlDecode for LogType {
    fn baml_decode(holder: &CffiValueHolder) -> Result<Self, BamlError> {
        let s = String::baml_decode(holder)?;
        match s.as_str() {
            "call" => Ok(LogType::Call),
            "stream" => Ok(LogType::Stream),
            other => Err(BamlError::internal(format!("unknown log type: {other}"))),
        }
    }
}

// =============================================================================
// Usage
// =============================================================================

define_raw_object_wrapper! {
    /// Token usage statistics
    Usage => ObjectUsage
}

impl Usage {
    /// Create from an object handle
    pub(crate) fn from_handle(
        handle: crate::proto::baml_cffi_v1::BamlObjectHandle,
        runtime: *const c_void,
    ) -> Self {
        let ptr = super::extract_ptr_from_handle(&handle)
            .unwrap_or_else(|e| baml_unreachable!("Failed to extract Usage handle: {e}"));
        Self {
            raw: RawObject::from_pointer(ptr, runtime, BamlObjectType::ObjectUsage),
        }
    }

    /// Input tokens used
    pub fn input_tokens(&self) -> i64 {
        self.raw.call_method("input_tokens", ())
    }

    /// Output tokens generated
    pub fn output_tokens(&self) -> i64 {
        self.raw.call_method("output_tokens", ())
    }

    /// Cached input tokens (if using prompt caching)
    ///
    /// Returns the number of tokens that were served from cache.
    /// Returns None if caching info is not available.
    pub fn cached_input_tokens(&self) -> Option<i64> {
        self.raw.call_method("cached_input_tokens", ())
    }
}

// =============================================================================
// Timing
// =============================================================================

define_raw_object_wrapper! {
    /// Timing information for a function or LLM call
    Timing => ObjectTiming
}

impl Timing {
    /// Create from an object handle
    pub(crate) fn from_handle(
        handle: crate::proto::baml_cffi_v1::BamlObjectHandle,
        runtime: *const c_void,
    ) -> Self {
        let ptr = super::extract_ptr_from_handle(&handle)
            .unwrap_or_else(|e| baml_unreachable!("Failed to extract Timing handle: {e}"));
        Self {
            raw: RawObject::from_pointer(ptr, runtime, BamlObjectType::ObjectTiming),
        }
    }

    /// Start time in UTC milliseconds since epoch
    pub fn start_time_utc_ms(&self) -> i64 {
        self.raw.call_method("start_time_utc_ms", ())
    }

    /// Duration in milliseconds (None if not yet completed)
    pub fn duration_ms(&self) -> Option<i64> {
        self.raw.call_method("duration_ms", ())
    }
}

// =============================================================================
// StreamTiming
// =============================================================================

define_raw_object_wrapper! {
    /// Timing information for a streaming call
    StreamTiming => ObjectStreamTiming
}

impl StreamTiming {
    /// Create from an object handle
    pub(crate) fn from_handle(
        handle: crate::proto::baml_cffi_v1::BamlObjectHandle,
        runtime: *const c_void,
    ) -> Self {
        let ptr = super::extract_ptr_from_handle(&handle)
            .unwrap_or_else(|e| baml_unreachable!("Failed to extract StreamTiming handle: {e}"));
        Self {
            raw: RawObject::from_pointer(ptr, runtime, BamlObjectType::ObjectStreamTiming),
        }
    }

    /// Start time in UTC milliseconds since epoch
    pub fn start_time_utc_ms(&self) -> i64 {
        self.raw.call_method("start_time_utc_ms", ())
    }

    /// Duration in milliseconds (None if not yet completed)
    pub fn duration_ms(&self) -> Option<i64> {
        self.raw.call_method("duration_ms", ())
    }
}

// =============================================================================
// FunctionLog
// =============================================================================

define_raw_object_wrapper! {
    /// Log entry for a function call
    FunctionLog => ObjectFunctionLog
}

impl FunctionLog {
    /// Create from an object handle
    pub(crate) fn from_handle(
        handle: crate::proto::baml_cffi_v1::BamlObjectHandle,
        runtime: *const c_void,
    ) -> Self {
        let ptr = super::extract_ptr_from_handle(&handle)
            .unwrap_or_else(|e| baml_unreachable!("Failed to extract FunctionLog handle: {e}"));
        Self {
            raw: RawObject::from_pointer(ptr, runtime, BamlObjectType::ObjectFunctionLog),
        }
    }

    /// Get the log ID
    pub fn id(&self) -> String {
        self.raw.call_method("id", ())
    }

    /// Get the function name
    pub fn function_name(&self) -> String {
        self.raw.call_method("function_name", ())
    }

    /// Get the log type (Call or Stream)
    pub fn log_type(&self) -> LogType {
        self.raw.call_method("log_type", ())
    }

    /// Get the raw LLM response (if available)
    pub fn raw_llm_response(&self) -> Option<String> {
        self.raw.call_method("raw_llm_response", ())
    }

    /// Get tags associated with this call
    pub fn tags(&self) -> HashMap<String, String> {
        self.raw.call_method("tags", ())
    }

    /// Get token usage for this call
    pub fn usage(&self) -> Usage {
        let handle = self
            .raw
            .call_method_for_object("usage", ())
            .unwrap_or_else(|e| baml_unreachable!("Failed to get usage: {e}"));
        Usage::from_handle(handle, self.raw.runtime())
    }

    /// Get timing information for this function call
    pub fn timing(&self) -> Timing {
        let handle = self
            .raw
            .call_method_for_object("timing", ())
            .unwrap_or_else(|e| baml_unreachable!("Failed to get timing: {e}"));
        Timing::from_handle(handle, self.raw.runtime())
    }

    /// Get all LLM calls made during this function call
    ///
    /// This returns all calls made (including retries). The call that
    /// was actually used for parsing can be identified with `selected()`.
    pub fn calls(&self) -> Vec<LLMCallKind> {
        let handles = self
            .raw
            .call_method_for_objects("calls", ())
            .unwrap_or_else(|e| baml_unreachable!("Failed to get calls: {e}"));
        handles
            .into_iter()
            .map(|h| LLMCallKind::from_handle(h, self.raw.runtime()))
            .collect()
    }

    /// Get the LLM call that was selected for parsing
    ///
    /// Returns None if no call was selected (e.g., all calls failed).
    pub fn selected_call(&self) -> Option<LLMCallKind> {
        self.raw
            .call_method_for_object("selected_call", ())
            .ok()
            .map(|handle| LLMCallKind::from_handle(handle, self.raw.runtime()))
    }
}

// =============================================================================
// Collector
// =============================================================================

define_raw_object_wrapper! {
    /// Collector for gathering telemetry from function calls
    Collector => ObjectCollector
}

impl std::fmt::Debug for Collector {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Collector(name={})", self.name())
    }
}

impl Collector {
    /// Create a new collector with the given name
    pub fn new(runtime: *const c_void, name: &str) -> Self {
        let raw = RawObject::new(runtime, BamlObjectType::ObjectCollector, ("name", name))
            .unwrap_or_else(|e| baml_unreachable!("Failed to create Collector: {e}"));
        Self { raw }
    }

    /// Get the collector name
    pub fn name(&self) -> String {
        self.raw.call_method("name", ())
    }

    /// Clear all logs and return count of cleared items
    pub fn clear(&self) -> i64 {
        self.raw.call_method("clear", ())
    }

    /// Get aggregated usage statistics
    pub fn usage(&self) -> Usage {
        let handle = self
            .raw
            .call_method_for_object("usage", ())
            .unwrap_or_else(|e| baml_unreachable!("Failed to get usage: {e}"));
        Usage::from_handle(handle, self.raw.runtime())
    }

    /// Get all collected logs
    pub fn logs(&self) -> Vec<FunctionLog> {
        let handles = self
            .raw
            .call_method_for_objects("logs", ())
            .unwrap_or_else(|e| baml_unreachable!("Failed to get logs: {e}"));
        handles
            .into_iter()
            .map(|h| FunctionLog::from_handle(h, self.raw.runtime()))
            .collect()
    }

    /// Get the most recent log (if any)
    pub fn last(&self) -> Option<FunctionLog> {
        let handle = self.raw.call_method_for_object("last", ()).ok()?;
        Some(FunctionLog::from_handle(handle, self.raw.runtime()))
    }

    /// Get a log by ID (if it exists)
    pub fn get_by_id(&self, id: &str) -> Option<FunctionLog> {
        let handle = self
            .raw
            .call_method_for_object("id", ("function_id", id))
            .ok()?;
        Some(FunctionLog::from_handle(handle, self.raw.runtime()))
    }

    /// Get the internal raw object (for use with FunctionArgs)
    pub(crate) fn raw(&self) -> &RawObject {
        &self.raw
    }
}
