use std::sync::Arc;

use baml_types::tracing::events::{HTTPRequest, HTTPResponse};


#[derive(Debug, Default, Clone, Hash, Eq, PartialEq)]
pub struct Usage {
    pub input_tokens: Option<i64>,
    pub output_tokens: Option<i64>,
}

impl Usage {
    pub fn accumulate(&mut self, other: Usage) {
        self.input_tokens = match (self.input_tokens, other.input_tokens) {
            (Some(a), Some(b)) => Some(a + b),
            (None, Some(b)) => Some(b),
            (Some(a), None) => Some(a),
            (None, None) => None,
        };
        self.output_tokens = match (self.output_tokens, other.output_tokens) {
            (Some(a), Some(b)) => Some(a + b),
            (None, Some(b)) => Some(b),
            (Some(a), None) => Some(a),
            (None, None) => None,
        };
    }
}

#[derive(Debug, Default, Clone, Hash, Eq, PartialEq)]
pub struct Timing {
    pub start_time_utc_ms: i64,
    pub duration_ms: Option<i64>,
    pub time_to_first_parsed_ms: Option<i64>,
}

#[derive(Debug, Default, Clone, Hash, Eq, PartialEq)]
pub struct StreamTiming {
    pub start_time_utc_ms: i64,
    pub duration_ms: Option<i64>,
    pub time_to_first_parsed_ms: Option<i64>,
    pub time_to_first_token_ms: Option<i64>,
}

#[derive(Debug, Clone)]
pub enum LLMCallKind {
    Basic(LLMCall),
    Stream(LLMStreamCall),
}

impl LLMCallKind {
    /// Returns whether this call is selected.
    pub fn selected(&self) -> bool {
        match self {
            LLMCallKind::Basic(c) => c.selected,
            LLMCallKind::Stream(c) => c.selected,
        }
    }
}

#[derive(Debug, Default, Clone)]
pub struct LLMCall {
    pub client_name: String,
    pub provider: String,
    pub timing: Timing,
    pub request: Option<Arc<HTTPRequest>>,
    pub response: Option<Arc<HTTPResponse>>,
    pub usage: Option<Usage>,
    pub selected: bool,
}

#[derive(Debug, Default, Clone)]
pub struct LLMStreamCall {
    pub client_name: String,
    pub provider: String,
    pub timing: StreamTiming,
    pub request: Option<Arc<HTTPRequest>>,
    pub response: Option<Arc<HTTPResponse>>,
    pub usage: Option<Usage>,
    pub selected: bool,
    pub chunks: Vec<serde_json::Value>,
}
