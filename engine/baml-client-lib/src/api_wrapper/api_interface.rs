use anyhow::Result;
use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::core_types::{LLMOutputModel, LogSchema, Template};

pub(crate) trait BoundaryAPI {
    async fn check_cache(&self, payload: &CacheRequest) -> Result<Option<CacheResponse>>;
    async fn log_schema(&self, payload: &LogSchema) -> Result<()>;
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
