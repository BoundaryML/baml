#![deny(clippy::all)]

use anyhow::Result;
mod api_wrapper;
mod baml_function_ctx;
mod otel;

use baml_function_ctx::{FunctionCtx, ScopeGuard};
use napi_derive::napi;
use otel::span_events::SpanEvent;

#[napi(constructor)]
pub struct BamlTracer {}

#[napi]
impl BamlTracer {
  #[napi]
  pub fn start(&self) {
    otel::init_tracer();
  }

  #[napi]
  pub fn stop(&self) -> Result<()> {
    otel::stop_tracer()
  }

  #[napi]
  pub fn flush(&self) -> Result<()> {
    otel::flush_tracer()
  }
}

#[napi(js_name = "BamlScopeGuard")]
pub struct JsScopeGuard {
  guard: ScopeGuard,
}

#[napi]
impl JsScopeGuard {
  #[napi(factory)]
  pub fn create(
    function_name: String,
    return_type: String,
    parameters: Vec<(String, String)>,
    as_kwarg: bool,
  ) -> Self {
    Self {
      guard: ScopeGuard::new(&FunctionCtx::new(
        function_name,
        return_type,
        parameters,
        as_kwarg,
      )),
    }
  }

  #[napi]
  pub fn child(
    &self,
    function_name: String,
    return_type: String,
    parameters: Vec<(String, String)>,
    as_kwarg: bool,
  ) -> Self {
    Self {
      guard: self.guard.new_with_parent(&FunctionCtx::new(
        function_name,
        return_type,
        parameters,
        as_kwarg,
      )),
    }
  }

  #[napi]
  pub fn log_inputs(
    &self,
    #[napi(ts_arg_type = "{[key: string]: any} | any[]")] args: serde_json::Value,
  ) -> Result<()> {
    self.guard.log_inputs(args)
  }

  #[napi]
  pub fn log_output(&self, result: String) -> Result<()> {
    self.guard.log_output(&result)
  }

  #[napi]
  pub fn log_error(
    &self,
    error_code: i32,
    message: Option<String>,
    stack: Option<String>,
  ) -> Result<()> {
    self
      .guard
      .log_error(error_code, message.as_deref(), stack.as_deref())
  }

  #[napi]
  pub fn log_llm_start(
    &self,
    #[napi(ts_arg_type = "{
      prompt: string | {
        role: string,
        content: string,
      }[],
      provider: string
    }")]
    event: serde_json::Value,
  ) -> Result<()> {
    self.guard.log_llm_event(SpanEvent::LlmRequestStart, event)
  }

  #[napi]
  pub fn log_llm_end(
    &self,
    #[napi(ts_arg_type = "{
      model_name: string,
      generated: string,
      metadata: any
    }")]
    event: serde_json::Value,
  ) -> Result<()> {
    self.guard.log_llm_event(SpanEvent::LlmRequestEnd, event)
  }

  #[napi]
  pub fn log_llm_error(
    &self,
    #[napi(ts_arg_type = "{
      error_code: number,
      message?: string,
      traceback?: string,
    }")]
    event: serde_json::Value,
  ) -> Result<()> {
    self.guard.log_llm_event(SpanEvent::LlmRequestError, event)
  }

  #[napi]
  pub fn log_llm_cache_hit(
    &self,
    #[napi(ts_arg_type = "number")] event: serde_json::Value,
  ) -> Result<()> {
    self
      .guard
      .log_llm_event(SpanEvent::LlmRequestCacheHit, event)
  }

  #[napi]
  pub fn log_llm_args(
    &self,
    #[napi(ts_arg_type = "{[key: string]: any}")] args: serde_json::Value,
  ) -> Result<()> {
    self.guard.log_llm_event(SpanEvent::LlmRequestArgs, args)
  }

  #[napi]
  pub fn log_llm_template_args(
    &self,
    #[napi(ts_arg_type = "{
      template: string | {
        role: string,
        content: string,
      }[],
      template_args: {
        [key: string]: string,
      }
    }")]
    args: serde_json::Value,
  ) -> Result<()> {
    self.guard.log_llm_event(SpanEvent::LlmPromptTemplate, args)
  }

  #[napi]
  pub fn log_variant(
    &self,
    #[napi(ts_arg_type = "string")] event: serde_json::Value,
  ) -> Result<()> {
    self.guard.log_llm_event(SpanEvent::Variant, event)
  }

  #[napi]
  pub fn set_tags(
    &self,
    #[napi(ts_arg_type = "{
      [key: string]: string | null,
    }")]
    event: serde_json::Value,
  ) -> Result<()> {
    self.guard.log_llm_event(SpanEvent::SetTags, event)
  }
}
