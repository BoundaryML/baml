use std::time::Duration;

use bex_external_types::BexExternalValue;
use web_time::SystemTime;

use crate::{SpanContext, SpanId};

/// A single runtime event emitted during BAML execution.
#[derive(Clone, Debug)]
pub struct RuntimeEvent {
    pub ctx: SpanContext,
    /// Full ancestor chain from root to current span, populated at emission time.
    pub call_stack: Vec<SpanId>,
    pub timestamp: SystemTime,
    pub event: EventKind,
}

/// Arbitrary metadata tags attached to a span.
pub type TraceTags = Vec<(String, String)>;

/// The kind of event.
#[derive(Clone, Debug)]
#[allow(clippy::large_enum_variant)]
pub enum EventKind {
    Function(FunctionEvent),
    /// Metadata/tag updates on the current span.
    SetTags(TraceTags),
    /// A structured log event emitted via `log.info()`, `log.debug()`, etc.
    Log(LogEvent),
    /// A custom user-defined event emitted via `baml.events.send()`.
    Custom(CustomEvent),
}

/// A log event emitted via `log.info()`, `log.debug()`, etc.
#[derive(Debug, Clone)]
pub struct LogEvent {
    /// Log level: "info", "debug", "warn", "error"
    pub level: String,
    /// Structured data
    pub data: BexExternalValue,
}

/// A custom user-defined event emitted via `baml.events.send()`.
#[derive(Debug, Clone)]
pub struct CustomEvent {
    /// Event name (e.g., "user_clicked", "request_started")
    pub name: String,
    /// Event payload
    pub data: BexExternalValue,
}

/// Function lifecycle events.
#[derive(Clone, Debug)]
pub enum FunctionEvent {
    Start(FunctionStart),
    End(Box<FunctionEnd>),
}

/// Emitted when a traced function begins execution.
#[derive(Clone, Debug)]
pub struct FunctionStart {
    pub name: String,
    pub args: Vec<BexExternalValue>,
    /// Tags inherited from the parent span at the time this function was entered.
    pub tags: TraceTags,
}

/// Emitted when a traced function finishes execution.
#[derive(Clone, Debug)]
pub struct FunctionEnd {
    pub name: String,
    pub result: BexExternalValue,
    pub duration: Duration,
}
