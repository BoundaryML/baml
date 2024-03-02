use anyhow::Result;
use serde::Deserialize;
use tracing::field::Visit;

use crate::baml_event_def;

use super::partial_types::{Apply, PartialLogSchema};

#[derive(Default, Deserialize)]
pub(crate) struct LlmRequestCacheHit {
    /// The latency of the original request
    latency_ms: u64,
}

impl LlmRequestCacheHit {
    pub fn latency_ms(&self) -> u64 {
        self.latency_ms
    }

    pub fn event(latency_ms: u64) -> Result<()> {
        baml_event_def!(LlmRequestCacheHit, latency_ms);
        Ok(())
    }
}

impl Visit for LlmRequestCacheHit {
    fn record_debug(&mut self, field: &tracing::field::Field, _value: &dyn std::fmt::Debug) {
        // By defaul invalid
        panic!("unexpected field name: {}", field.name());
    }

    fn record_u64(&mut self, field: &tracing::field::Field, value: u64) {
        match field.name() {
            "latency_ms" => self.latency_ms = value,
            name => {
                panic!("unexpected field name: {}", name);
            }
        }
    }
}

impl<'a, S> Apply<'a, LlmRequestCacheHit, S> for PartialLogSchema
where
    S: tracing::Subscriber,
    S: for<'lookup> tracing_subscriber::registry::LookupSpan<'lookup>,
{
    fn apply(
        &mut self,
        event: LlmRequestCacheHit,
        _span: &tracing_subscriber::registry::SpanRef<'a, S>,
    ) {
        self.context
            .tags
            .insert("latency_ms".to_string(), event.latency_ms.to_string());
    }
}
