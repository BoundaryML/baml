use std::collections::HashMap;

use anyhow::Result;
use serde::Deserialize;
use tracing::field::Visit;

use crate::{
    api_wrapper::core_types::{LLMEventInputPrompt, Template},
    baml_event_def,
};

use super::partial_types::{Apply, PartialLogSchema};

#[derive(Default, Deserialize)]
pub(crate) struct LlmPromptTemplate {
    template: Template,
    template_args: HashMap<String, String>,
}

impl LlmPromptTemplate {
    pub fn self_event(&self) -> Result<()> {
        Self::event(&self.template, self.template_args.clone())
    }

    pub fn event(template: &Template, template_args: HashMap<String, String>) -> Result<()> {
        let template = serde_json::to_string(template)?;
        let template_args = serde_json::to_string(&template_args)?;
        baml_event_def!(LlmPromptTemplate, template, template_args);
        Ok(())
    }
}

impl Visit for LlmPromptTemplate {
    fn record_debug(&mut self, field: &tracing::field::Field, _value: &dyn std::fmt::Debug) {
        // By defaul invalid
        panic!("unexpected field name: {}", field.name());
    }

    fn record_str(&mut self, field: &tracing::field::Field, value: &str) {
        match field.name() {
            "template" => self.template = serde_json::from_str(value).unwrap(),
            "template_args" => {
                self.template_args = serde_json::from_str(value).unwrap_or_default();
            }
            name => {
                panic!("unexpected field name: {}", name);
            }
        }
    }
}

impl<'a, S> Apply<'a, LlmPromptTemplate, S> for PartialLogSchema
where
    S: tracing::Subscriber,
    S: for<'lookup> tracing_subscriber::registry::LookupSpan<'lookup>,
{
    fn apply(
        &mut self,
        event: LlmPromptTemplate,
        _span: &tracing_subscriber::registry::SpanRef<'a, S>,
    ) {
        if let Some(meta) = self.get_meta_data_mut(false) {
            if let Some(input) = &mut meta.input {
                input.prompt = LLMEventInputPrompt {
                    template: event.template,
                    template_args: event.template_args,
                    ..Default::default()
                }
            }
        } else {
            println!("No metadata found for llm event");
        }
    }
}
