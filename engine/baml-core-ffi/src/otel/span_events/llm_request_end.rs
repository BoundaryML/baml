use anyhow::Result;
use serde::Deserialize;
use tracing::field::Visit;

use crate::{
  api_wrapper::core_types::{LLMOutputModel, LLMOutputModelMetadata},
  baml_event_def,
};

use super::partial_types::{Apply, PartialLogSchema};

#[derive(Default, Deserialize)]
pub(crate) struct LlmRequestEnd {
  model_name: Option<String>,
  generated: Option<String>,
  metadata: Option<LLMOutputModelMetadata>,
}

impl LlmRequestEnd {
  pub fn self_event(&self) -> Result<()> {
    match (
      self.model_name.as_ref(),
      self.generated.as_ref(),
      self.metadata.as_ref(),
    ) {
      (Some(model_name), Some(generated), Some(metadata)) => {
        Self::event(model_name, generated, metadata)
      }
      _ => Ok(()),
    }
  }

  pub fn event(model_name: &str, generated: &str, metadata: &LLMOutputModelMetadata) -> Result<()> {
    let metadata = serde_json::to_string(metadata)?;
    baml_event_def!(LlmRequestEnd, model_name, generated, metadata);

    Ok(())
  }
}

impl Visit for LlmRequestEnd {
  fn record_debug(&mut self, field: &tracing::field::Field, _value: &dyn std::fmt::Debug) {
    // By defaul invalid
    panic!("unexpected field name: {}", field.name());
  }

  fn record_str(&mut self, field: &tracing::field::Field, value: &str) {
    match field.name() {
      "model_name" => self.model_name = Some(value.to_string()),
      "generated" => self.generated = Some(value.to_string()),
      "metadata" => self.metadata = serde_json::from_str(value).ok(),
      name => {
        panic!("unexpected field name: {}", name);
      }
    }
  }
}

impl<'a, S> Apply<'a, LlmRequestEnd, S> for PartialLogSchema
where
  S: tracing::Subscriber,
  S: for<'lookup> tracing_subscriber::registry::LookupSpan<'lookup>,
{
  fn apply(&mut self, event: LlmRequestEnd, _span: &tracing_subscriber::registry::SpanRef<'a, S>) {
    if let Some(meta) = self.get_meta_data_mut(false) {
      meta.model_name = event.model_name;
      match (event.generated, event.metadata) {
        (None, _) => {}
        (Some(generated), Some(metadata)) => {
          meta.output = Some(LLMOutputModel {
            raw_text: generated,
            metadata,
            ..Default::default()
          });
        }
        (Some(generated), None) => {
          meta.output = Some(LLMOutputModel {
            raw_text: generated,
            ..Default::default()
          });
        }
      }
    } else {
      println!("No metadata found for llm event");
    }
  }
}
