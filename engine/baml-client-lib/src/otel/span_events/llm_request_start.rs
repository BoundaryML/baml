use anyhow::Result;
use serde::Deserialize;
use tracing::field::Visit;

use crate::{
    api_wrapper::core_types::{EventType, LLMEventInput, LLMEventInputPrompt, Template},
    baml_event_def,
};

use super::partial_types::{Apply, PartialLogSchema};

#[derive(Default, Deserialize)]
pub(crate) struct LlmRequestStart {
    prompt: Option<Template>,
    provider: Option<String>,
}

impl LlmRequestStart {
    pub fn self_event(&self) -> Result<()> {
        match (&self.prompt, &self.provider) {
            (Some(prompt), Some(provider)) => Self::event(prompt, provider),
            (Some(_), None) => Err(anyhow::anyhow!("provider is missing")),
            (None, Some(_)) => Err(anyhow::anyhow!("prompt is missing")),
            (None, None) => Err(anyhow::anyhow!("prompt and provider are missing")),
        }
    }

    pub fn event(prompt: &Template, provider: &str) -> Result<()> {
        let prompt = serde_json::to_string(prompt)?;
        baml_event_def!(LlmRequestStart, prompt, provider);
        Ok(())
    }
}

impl Visit for LlmRequestStart {
    fn record_debug(&mut self, field: &tracing::field::Field, _value: &dyn std::fmt::Debug) {
        // By defaul invalid
        panic!("unexpected field name: {}", field.name());
    }

    fn record_str(&mut self, field: &tracing::field::Field, value: &str) {
        match field.name() {
            "prompt" => self.prompt = serde_json::from_str(value).ok(),
            "provider" => self.provider = Some(value.to_string()),
            name => {
                panic!("unexpected field name: {}", name);
            }
        }
    }
}

impl<'a, S> Apply<'a, LlmRequestStart, S> for PartialLogSchema
where
    S: tracing::Subscriber,
    S: for<'lookup> tracing_subscriber::registry::LookupSpan<'lookup>,
{
    fn apply(
        &mut self,
        event: LlmRequestStart,
        _span: &tracing_subscriber::registry::SpanRef<'a, S>,
    ) {
        self.event_type = EventType::FuncLlm;
        let meta = self.get_meta_data_mut(true).unwrap();
        match (&mut meta.input, event.prompt) {
            (_, None) => {
                return;
            }
            (Some(input), Some(prompt)) => {
                input.prompt.template = prompt;
            }
            (None, Some(prompt)) => {
                meta.input = Some(LLMEventInput {
                    prompt: LLMEventInputPrompt {
                        template: prompt,
                        ..Default::default()
                    },
                    ..Default::default()
                });
            }
        }
        meta.provider = event.provider;
    }
}
