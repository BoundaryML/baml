//! Bridge-agnostic Collector shim.
//!
//! Thin wrapper around `bex_events::Collector` so that `bridge_python`
//! (and future `bridge_typescript`) share the same abstraction.

pub use bex_events::{FunctionLog, LLMCall, Timing, Usage};

/// Language-agnostic collector wrapping the core Rust implementation.
pub struct Collector {
    inner: bex_events::Collector,
}

impl Collector {
    pub fn new(name: String) -> Self {
        Self {
            inner: bex_events::Collector::new(name),
        }
    }

    pub fn name(&self) -> &str {
        self.inner.name()
    }

    pub fn inner(&self) -> &bex_events::Collector {
        &self.inner
    }

    pub fn track(&self, root_span_id: &bex_events::SpanId) {
        self.inner.track(root_span_id);
    }

    pub fn logs(&self) -> Vec<FunctionLog> {
        self.inner.logs()
    }

    pub fn last(&self) -> Option<FunctionLog> {
        self.inner.last()
    }

    pub fn usage(&self) -> Usage {
        self.inner.usage()
    }

    pub fn clear(&self) -> usize {
        self.inner.clear()
    }

    pub fn id(&self, span_id_str: &str) -> Option<FunctionLog> {
        self.inner.id(span_id_str)
    }
}
