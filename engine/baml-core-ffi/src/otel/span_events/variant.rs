use core::panic;

use anyhow::Result;
use tracing::field::Visit;

use crate::baml_event_def;

use super::partial_types::{Apply, PartialLogSchema};

#[derive(Default)]
pub(crate) struct Variant {
  variant_name: Option<String>,
}

impl Variant {
  pub fn variant_name(&self) -> Option<&String> {
    self.variant_name.as_ref()
  }

  pub fn event(variant_name: &str) -> Result<()> {
    baml_event_def!(Variant, variant_name);
    Ok(())
  }
}

impl Visit for Variant {
  fn record_debug(&mut self, field: &tracing::field::Field, _value: &dyn std::fmt::Debug) {
    panic!("unexpected field name: {}", field.name());
  }

  fn record_str(&mut self, field: &tracing::field::Field, value: &str) {
    match field.name() {
      "variant_name" => self.variant_name = Some(value.to_string()),
      name => {
        panic!("unexpected field name: {}", name);
      }
    }
  }
}

impl<'a, S> Apply<'a, Variant, S> for PartialLogSchema
where
  S: tracing::Subscriber,
  S: for<'lookup> tracing_subscriber::registry::LookupSpan<'lookup>,
{
  fn apply(&mut self, event: Variant, _span: &tracing_subscriber::registry::SpanRef<'a, S>) {
    if let Some(e) = self.context.event_chain.last_mut() {
      e.variant_name = event.variant_name;
    }
  }
}
