// mod anthropic;
mod expression_helper;
mod llm_provider;
mod openai;
mod retry_policy;
mod traits;

pub use llm_provider::LLMProvider;
pub use traits::{WithCallable, WithPrompt};

#[derive(Debug)]
pub struct LLMResponse {
    pub model: String,
    pub content: String,
    pub start_time_unix_ms: u64,
    pub latency_ms: u64,
    pub metadata: serde_json::Value,
}

pub struct LLMStreamResponse {
    pub delta: String,
    pub start_time_unix_ms: u64,
    pub latency_ms: u64,
    pub metadata: serde_json::Value,
}

#[derive(Clone, Copy)]
pub struct ModelFeatures {
    pub completion: bool,
    pub chat: bool,
}
