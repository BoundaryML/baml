use anyhow::Result;
use std::collections::HashMap;

use crate::{
  baml_span,
  otel::{
    self,
    span_events::{Exception, IOEvent, SpanEvent},
  },
};

pub struct ScopeGuard {
  span: tracing::Span,
  function: FunctionCtx,
}

impl ScopeGuard {
  pub fn new_with_parent(&self, function: &FunctionCtx) -> Self {
    let _guard = self.span.enter();
    let function_name = &function.function_name;
    Self {
      span: baml_span!(function_name),
      function: function.clone(),
    }
  }

  pub fn new(function: &FunctionCtx) -> Self {
    let function_name = &function.function_name;
    Self {
      span: baml_span!(function_name),
      function: function.clone(),
    }
  }

  pub fn log_inputs(&self, args: serde_json::Value) -> Result<()> {
    let args = self.function.trace_parameters(args)?;
    let _guard = self.span.enter();
    IOEvent::input_event(&args)
  }

  pub fn log_output(&self, value: &String) -> Result<()> {
    let _guard = self.span.enter();
    IOEvent::output_event(value, &self.function.return_type)
  }

  pub fn log_error(
    &self,
    error_code: i32,
    message: Option<&str>,
    traceback: Option<&str>,
  ) -> Result<()> {
    let _guard = self.span.enter();
    Exception::event(error_code, message, traceback)
  }

  pub fn log_llm_event(&self, name: SpanEvent, event: serde_json::Value) -> Result<()> {
    let _guard = self.span.enter();
    otel::log_event(name, event)
  }
}

#[derive(Clone)]
pub struct FunctionCtx {
  function_name: String,
  // TODO: Store actual type information here
  return_type: String,
  // List of (name, type) pairs
  parameters: Vec<(String, String)>,
  as_kwarg: bool,
}

impl FunctionCtx {
  pub fn new(
    function_name: String,
    return_type: String,
    parameters: Vec<(String, String)>,
    as_kwarg: bool,
  ) -> Self {
    Self {
      function_name,
      return_type,
      parameters,
      as_kwarg,
    }
  }

  fn trace_parameters(
    &self,
    args: serde_json::Value,
  ) -> Result<(
    Vec<(Option<String>, (String, String))>,
    HashMap<String, (String, String)>,
  )> {
    if self.as_kwarg {
      match args.as_object() {
        Some(obj) => {
          let kwargs = self
            .parameters
            .iter()
            .map(|(name, r#type)| {
              let value = obj
                .get(name)
                .map(|v| v.as_str().unwrap_or("<unserializable>"))
                .unwrap_or("null");
              (name.clone(), (value.to_string(), r#type.clone()))
            })
            .collect();

          Ok((Default::default(), kwargs))
        }
        None => Err(anyhow::anyhow!(
          "Expected a key-value object for keyword arguments"
        )),
      }
    } else {
      match args.as_array() {
        Some(arr) => {
          if arr.len() > self.parameters.len() {
            return Err(anyhow::anyhow!(
              "Too many arguements to function: {}. Expected {}, got {}",
              self.function_name,
              self.parameters.len(),
              arr.len()
            ));
          }

          let positional_args = arr
            .iter()
            .enumerate()
            .zip(self.parameters.iter())
            .map(|((i, arg), (name, r#type))| {
              (
                Some(name.clone()),
                (
                  arg.as_str().unwrap_or("<unserializable>").to_string(),
                  r#type.clone(),
                ),
              )
            })
            .collect();

          Ok((positional_args, Default::default()))
        }
        None => Err(anyhow::anyhow!("Expected array for positional arguments")),
      }
    }
  }
}
