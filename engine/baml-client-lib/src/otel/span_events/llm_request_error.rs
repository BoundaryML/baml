use anyhow::Result;
use serde::Deserialize;
use tracing::field::Visit;

use crate::{api_wrapper::core_types::Error, baml_event_def};

use super::partial_types::{Apply, PartialLogSchema};

#[derive(Default, Deserialize)]
pub(crate) struct LlmRequestError {
    error_code: i32,
    message: Option<String>,
    traceback: Option<String>,
}

impl LlmRequestError {
    pub fn self_event(&self) -> Result<()> {
        Self::event(
            self.error_code,
            self.message.as_deref(),
            self.traceback.as_deref(),
        )
    }

    pub fn event(error_code: i32, message: Option<&str>, traceback: Option<&str>) -> Result<()> {
        baml_event_def!(LlmRequestError, error_code, message, traceback);
        Ok(())
    }
}

impl Visit for LlmRequestError {
    fn record_debug(&mut self, field: &tracing::field::Field, _value: &dyn std::fmt::Debug) {
        // By defaul invalid
        panic!("unexpected field name: {}", field.name());
    }

    fn record_i64(&mut self, field: &tracing::field::Field, value: i64) {
        match field.name() {
            "error_code" => self.error_code = value as i32,
            name => {
                panic!("unexpected field name: {}", name);
            }
        }
    }

    fn record_str(&mut self, field: &tracing::field::Field, value: &str) {
        match field.name() {
            "message" => self.message = Some(value.to_string()),
            "traceback" => self.traceback = Some(value.to_string()),
            name => {
                panic!("unexpected field name: {}", name);
            }
        }
    }
}

impl<'a, S> Apply<'a, LlmRequestError, S> for PartialLogSchema
where
    S: tracing::Subscriber,
    S: for<'lookup> tracing_subscriber::registry::LookupSpan<'lookup>,
{
    fn apply(
        &mut self,
        event: LlmRequestError,
        _span: &tracing_subscriber::registry::SpanRef<'a, S>,
    ) {
        if let Some(meta) = self.get_meta_data_mut(false) {
            meta.error = Some(Error {
                code: event.error_code,
                message: event.message.unwrap_or("Unknown error".to_string()),
                traceback: event.traceback,
                ..Default::default()
            });
        }
    }
}
