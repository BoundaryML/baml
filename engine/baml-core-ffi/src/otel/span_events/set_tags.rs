use core::panic;
use std::collections::HashMap;

use anyhow::Result;
use tracing::field::Visit;

use crate::baml_event_def;

use super::partial_types::{Apply, PartialLogSchema};

#[derive(Default)]
pub(crate) struct SetTags {
  tags: HashMap<String, Option<String>>,
}

impl SetTags {
  #[allow(dead_code)]
  pub fn tags(&self) -> &HashMap<String, Option<String>> {
    &self.tags
  }

  #[allow(dead_code)]
  pub fn tags_mut(&mut self) -> &mut HashMap<String, Option<String>> {
    &mut self.tags
  }

  pub fn event(tags: &HashMap<String, Option<String>>) -> Result<()> {
    let tags = serde_json::to_string(tags)?;
    baml_event_def!(SetTags, tags);
    Ok(())
  }
}

impl Visit for SetTags {
  fn record_debug(&mut self, field: &tracing::field::Field, _value: &dyn std::fmt::Debug) {
    panic!("unexpected field name: {}", field.name());
  }

  fn record_str(&mut self, field: &tracing::field::Field, value: &str) {
    match field.name() {
      "tags" => {
        self.tags = serde_json::from_str(value).unwrap_or_default();
      }
      name => {
        panic!("unexpected field name: {}", name);
      }
    }
  }
}

impl<'a, S> Apply<'a, SetTags, S> for PartialLogSchema
where
  S: tracing::Subscriber,
  S: for<'lookup> tracing_subscriber::registry::LookupSpan<'lookup>,
{
  fn apply(&mut self, event: SetTags, _span: &tracing_subscriber::registry::SpanRef<'a, S>) {
    // First, we need get the parent of the current event
    event.tags.iter().for_each(|(k, v)| {
      match v {
        Some(v) => self.context.tags.insert(k.clone(), v.clone()),
        None => self.context.tags.remove(k),
      };
    });
  }
}
