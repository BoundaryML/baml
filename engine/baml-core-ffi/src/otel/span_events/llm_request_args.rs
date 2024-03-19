use std::collections::HashMap;

use anyhow::Result;
use tracing::field::Visit;

use crate::baml_event_def;

use super::partial_types::{Apply, PartialLogSchema};

#[derive(Default)]
pub(crate) struct LlmRequestArgs {
  invocation_params: HashMap<String, serde_json::Value>,
}

impl LlmRequestArgs {
  pub fn event(invocation_params: &HashMap<String, serde_json::Value>) -> Result<()> {
    let invocation_params = serde_json::to_string(invocation_params)?;
    baml_event_def!(LlmRequestArgs, invocation_params);
    Ok(())
  }
}

impl Visit for LlmRequestArgs {
  fn record_debug(&mut self, field: &tracing::field::Field, _value: &dyn std::fmt::Debug) {
    // By defaul invalid
    panic!("unexpected field name: {}", field.name());
  }

  fn record_str(&mut self, field: &tracing::field::Field, value: &str) {
    match field.name() {
      "invocation_params" => {
        self.invocation_params = serde_json::from_str(value).unwrap_or_default();
      }
      name => {
        panic!("unexpected field name: {}", name);
      }
    }
  }
}

impl<'a, S> Apply<'a, LlmRequestArgs, S> for PartialLogSchema
where
  S: tracing::Subscriber,
  S: for<'lookup> tracing_subscriber::registry::LookupSpan<'lookup>,
{
  fn apply(&mut self, event: LlmRequestArgs, _span: &tracing_subscriber::registry::SpanRef<'a, S>) {
    if let Some(meta) = self.get_meta_data_mut(false) {
      if let Some(input) = &mut meta.input {
        input.invocation_params = event.invocation_params;
      }
    } else {
      println!("No metadata found for llm event");
    }
  }
}
