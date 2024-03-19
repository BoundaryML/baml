use anyhow::Result;
use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::core_types::{LLMOutputModel, LogSchema, Template, TestCaseStatus};

pub(crate) trait BoundaryAPI {
  async fn check_cache(&self, payload: &CacheRequest) -> Result<Option<CacheResponse>>;
  async fn log_schema(&self, payload: &LogSchema) -> Result<()>;
  async fn create_session(&self) -> Result<CreateSessionResponse>;
  async fn finish_session(&self) -> Result<()>;
}

// This is a trait that is used to define the API for the BoundaryTestAPI
// It assumes the implementor with manage state
pub(crate) trait BoundaryTestAPI {
  async fn register_test_cases<T: IntoIterator<Item = (String, String)>>(
    &self,
    payload: T,
  ) -> Result<()>;
  async fn update_test_case_batch(&self, payload: &[UpdateTestCaseRequest]) -> Result<()>;
  async fn update_test_case(
    &self,
    test_suite: &str,
    test_case: &str,
    status: TestCaseStatus,
    error_data: Option<Value>,
  ) -> Result<()>;
}

#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct CreateSessionResponse {
  #[serde(rename(deserialize = "test_cycle_id"))]
  pub session_id: String,
  pub dashboard_url: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct UpdateTestCaseRequest {
  pub test_suite: String,
  pub test_case: String,
  pub status: TestCaseStatus,
}

#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct CacheRequest {
  provider: String,
  prompt: Template,
  prompt_vars: HashMap<String, String>,
  invovation_params: HashMap<String, Value>,
}

#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct CacheResponse {
  model_name: String,
  llm_output: LLMOutputModel,
  latency_ms: i32,
}
