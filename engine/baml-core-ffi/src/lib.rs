#![deny(clippy::all)]

use colored::*;
use std::{collections::HashMap, sync::Arc};

use anyhow::Result;
mod api_wrapper;
mod baml_function_ctx;
mod env_setup;
mod otel;

use api_wrapper::{
  api_interface::UpdateTestCaseRequest, core_types::TestCaseStatus, BoundaryAPI, BoundaryTestAPI,
};
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

#[napi]
pub struct BamlTester {
  // First key is the test suite name, second key is the test case name
  test_cases: Arc<napi::tokio::sync::Mutex<HashMap<(String, String), TestCaseStatus>>>,
}

#[napi]
impl BamlTester {
  #[napi(constructor)]
  pub fn new(test_cases: Vec<(String, String)>) -> Self {
    Self {
      test_cases: Arc::new(napi::tokio::sync::Mutex::new(
        test_cases
          .into_iter()
          .map(|(suite, case)| ((suite, case), TestCaseStatus::Queued))
          .collect(),
      )),
    }
  }

  #[napi]
  pub async fn start(&self) -> Result<(), napi::Error> {
    match otel::event_handler() {
      Some(handler) => {
        let dashboard_url = handler
          .api()
          .create_session()
          .await
          .map(|s| Some(s.session_id))?;

        let keys = self
          .test_cases
          .lock()
          .await
          .keys()
          .cloned()
          .collect::<Vec<_>>();
        handler.api().register_test_cases(keys).await?;

        if let Some(url) = dashboard_url {
          println!("Boundary Studio: {}", url.blue());
        }

        Ok(())
      }
      None => Ok(()),
    }
  }

  #[napi]
  pub async fn end(&self) -> Result<(), napi::Error> {
    match otel::event_handler() {
      Some(handler) => {
        let test_cases = self.test_cases.lock().await;
        // For any tests that are still queued, mark them as skipped
        let queued_tests = test_cases
          .iter()
          .filter(|(_, status)| **status == TestCaseStatus::Queued)
          .map(|((suite, case), _)| UpdateTestCaseRequest {
            test_suite: suite.clone(),
            test_case: case.clone(),
            status: TestCaseStatus::Cancelled,
          })
          .collect::<Vec<_>>();

        handler.api().update_test_case_batch(&queued_tests).await?;

        handler.api().finish_session().await?;

        Ok(())
      }
      None => Ok(()),
    }
  }

  #[napi]
  pub async fn update_test_case(
    &self,
    test_suite: String,
    test_case: String,
    status: TestCaseStatus,
    error_data: Option<serde_json::Value>,
  ) -> Result<(), napi::Error> {
    {
      let mut test_cases = self.test_cases.lock().await;
      // Update the status of the test case
      match test_cases.insert((test_suite.clone(), test_case.clone()), status) {
        Some(_) => {}
        None => {
          return Err(
            anyhow::Error::msg(format!(
              "Not registered test case - {}::{}\n{:?}",
              test_suite, test_case, self.test_cases
            ))
            .into(),
          );
        }
      }
    }

    match otel::event_handler() {
      Some(handler) => {
        handler
          .api()
          .update_test_case(&test_suite, &test_case, status, error_data)
          .await?;

        Ok(())
      }
      None => Ok(()),
    }
  }
}

#[napi(js_name = "BamlScopeGuard")]
pub struct JsScopeGuard {
  guard: Option<ScopeGuard>,
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
      guard: Some(ScopeGuard::new(&FunctionCtx::new(
        function_name,
        return_type,
        parameters,
        as_kwarg,
      ))),
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
    match &self.guard {
      Some(guard) => Self {
        guard: Some(guard.new_with_parent(&FunctionCtx::new(
          function_name,
          return_type,
          parameters,
          as_kwarg,
        ))),
      },
      None => panic!("Invalid scope guard is used"),
    }
  }

  #[napi]
  pub fn log_inputs(
    &self,
    #[napi(ts_arg_type = "{[key: string]: any} | any[]")] args: serde_json::Value,
  ) -> Result<()> {
    match &self.guard {
      Some(guard) => guard.log_inputs(args),
      None => Err(anyhow::anyhow!("Invalid scope guard is used")),
    }
  }

  #[napi]
  pub fn log_output(&self, result: Option<String>) -> Result<()> {
    match &self.guard {
      Some(guard) => guard.log_output(result.as_deref().unwrap_or("null")),
      None => Err(anyhow::anyhow!("Invalid scope guard is used")),
    }
  }

  #[napi]
  pub fn log_error(
    &self,
    error_code: i32,
    message: Option<String>,
    stack: Option<String>,
  ) -> Result<()> {
    match &self.guard {
      Some(guard) => guard.log_error(error_code, message.as_deref(), stack.as_deref()),
      None => Err(anyhow::anyhow!("Invalid scope guard is used")),
    }
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
    match &self.guard {
      Some(guard) => guard.log_llm_event(SpanEvent::LlmRequestStart, event),
      None => Err(anyhow::anyhow!("Invalid scope guard is used")),
    }
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
    match &self.guard {
      Some(guard) => guard.log_llm_event(SpanEvent::LlmRequestEnd, event),
      None => Err(anyhow::anyhow!("Invalid scope guard is used")),
    }
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
    match &self.guard {
      Some(guard) => guard.log_llm_event(SpanEvent::LlmRequestError, event),
      None => Err(anyhow::anyhow!("Invalid scope guard is used")),
    }
  }

  #[napi]
  pub fn log_llm_cache_hit(
    &self,
    #[napi(ts_arg_type = "number")] event: serde_json::Value,
  ) -> Result<()> {
    match &self.guard {
      Some(guard) => guard.log_llm_event(SpanEvent::LlmRequestCacheHit, event),
      None => Err(anyhow::anyhow!("Invalid scope guard is used")),
    }
  }

  #[napi]
  pub fn log_llm_args(
    &self,
    #[napi(ts_arg_type = "{[key: string]: any}")] args: serde_json::Value,
  ) -> Result<()> {
    match &self.guard {
      Some(guard) => guard.log_llm_event(SpanEvent::LlmRequestArgs, args),
      None => Err(anyhow::anyhow!("Invalid scope guard is used")),
    }
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
    match &self.guard {
      Some(guard) => guard.log_llm_event(SpanEvent::LlmPromptTemplate, args),
      None => Err(anyhow::anyhow!("Invalid scope guard is used")),
    }
  }

  #[napi]
  pub fn log_variant(
    &self,
    #[napi(ts_arg_type = "string")] event: serde_json::Value,
  ) -> Result<()> {
    match &self.guard {
      Some(guard) => guard.log_llm_event(SpanEvent::Variant, event),
      None => Err(anyhow::anyhow!("Invalid scope guard is used")),
    }
  }

  #[napi]
  pub fn set_tags(
    &self,
    #[napi(ts_arg_type = "{
      [key: string]: string | null,
    }")]
    event: serde_json::Value,
  ) -> Result<()> {
    match &self.guard {
      Some(guard) => guard.log_llm_event(SpanEvent::SetTags, event),
      None => Err(anyhow::anyhow!("Invalid scope guard is used")),
    }
  }

  #[napi]
  pub fn close(&mut self) {
    // In Rust, we need to manually drop the guard to release the lock
    self.guard = None;
  }
}

#[napi]
pub fn is_available() -> bool {
  true
}
