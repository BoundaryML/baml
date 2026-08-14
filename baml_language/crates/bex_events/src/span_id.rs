/// Unique identifier for a span (function invocation or arbitrary region).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct SpanId(pub uuid::Uuid);

impl SpanId {
    pub fn new() -> Self {
        Self(uuid::Uuid::new_v4())
    }
}

impl Default for SpanId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for SpanId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Host-side span context passed from the bridge to the engine.
pub struct HostSpanContext;

/// Span context carried by every runtime event.
///
/// Encodes parent-child relationships so the full span tree can be
/// reconstructed from a flat event stream.
#[derive(Clone, Debug)]
pub struct SpanContext {
    /// This span's unique ID.
    pub span_id: SpanId,
    /// Parent span (None for root spans).
    pub parent_span_id: Option<SpanId>,
    /// Root span of the entire call tree.
    pub root_span_id: SpanId,
}
