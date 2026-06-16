//! Collector — no-op shim.
//!
//! Tracing has been removed. These types keep the public field/method shapes
//! that the PyO3/napi bindings read, but produce no events: collectors return
//! empty logs and zero usage.

use std::collections::HashMap;

use bex_external_types::BexExternalValue;

use crate::SpanId;

/// No-op collector. Retained for binding compatibility; never produces logs.
pub struct Collector {
    name: String,
}

impl Collector {
    pub fn new(name: String) -> Self {
        Self { name }
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    /// No-op: span tracking has been removed.
    pub fn track(&self, _engine_span_id: &SpanId) {}

    /// Always empty; no events are produced.
    pub fn logs(&self) -> Vec<FunctionLog> {
        vec![]
    }

    /// Always `None`; no events are produced.
    pub fn last(&self) -> Option<FunctionLog> {
        None
    }

    /// Always zero usage.
    pub fn usage(&self) -> Usage {
        Usage::default()
    }

    /// Nothing to clear.
    pub fn clear(&self) -> usize {
        0
    }

    /// Always `None`; no events are produced.
    pub fn id(&self, _span_id_str: &str) -> Option<FunctionLog> {
        None
    }
}

/// Read-only view of a single BAML function invocation.
#[derive(Clone, Debug)]
pub struct FunctionLog {
    pub id: SpanId,
    pub function_name: String,
    pub timing: Timing,
    pub usage: Usage,
    pub calls: Vec<LLMCall>,
    pub tags: HashMap<String, String>,
    pub result: Option<BexExternalValue>,
}

/// Timing information for a span.
#[derive(Clone, Debug, Default)]
pub struct Timing {
    pub start_time_utc_ms: i64,
    pub duration_ms: Option<i64>,
}

/// Token usage from an LLM call.
#[derive(Clone, Debug, Default)]
pub struct Usage {
    pub input_tokens: Option<i64>,
    pub output_tokens: Option<i64>,
    pub cached_input_tokens: Option<i64>,
}

/// A single LLM call within a function invocation.
#[derive(Clone, Debug)]
pub struct LLMCall {
    pub function_name: String,
    pub provider: Option<String>,
    pub timing: Timing,
    pub usage: Usage,
}
