mod client_anthropic;
mod client_openai;

use anyhow::Result;
use serde::Serialize;

pub use client_anthropic::AnthropicClient;
pub use client_openai::OpenaiClient;

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Message {
    pub role: String,
    pub content: String,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Prompt {
    Completion(String),
    Chat(Vec<Message>),
}

pub trait LlmClient {
    #[allow(async_fn_in_trait)]
    async fn chat(&self, prompt: Prompt) -> Result<String>;
}
