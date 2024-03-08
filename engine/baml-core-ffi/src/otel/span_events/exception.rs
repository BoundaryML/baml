use anyhow::Result;
use tracing::field::Visit;
use tracing_subscriber::registry::SpanRef;

use crate::{api_wrapper::core_types::Error, baml_event_def};

use super::partial_types::{Apply, PartialLogSchema};

#[derive(Default)]
pub(crate) struct Exception {
  error_code: i32,
  message: Option<String>,
  traceback: Option<String>,
}

impl Exception {
  pub fn event(error_code: i32, message: Option<&str>, traceback: Option<&str>) -> Result<()> {
    baml_event_def!(Exception, error_code, message, traceback);
    Ok(())
  }
}

impl Visit for Exception {
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

impl<'a, S> Apply<'a, Exception, S> for PartialLogSchema
where
  S: tracing::Subscriber,
  S: for<'lookup> tracing_subscriber::registry::LookupSpan<'lookup>,
{
  fn apply(&mut self, event: Exception, _span: &SpanRef<'a, S>) {
    self.error = Some(Error {
      code: event.error_code,
      message: event.message.unwrap_or("Unknown error".to_string()),
      traceback: event.traceback,
      ..Default::default()
    });
  }
}
