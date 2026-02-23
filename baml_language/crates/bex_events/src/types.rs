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
}

/// Function lifecycle events.
#[derive(Clone, Debug)]
pub enum FunctionEvent {
    Start(FunctionStart),
    End(FunctionEnd),
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
