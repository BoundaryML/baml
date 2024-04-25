mod llm_provider;
mod openai;
mod retry_policy;

use anyhow::Result;
use async_stream::stream;
use futures::Stream;
use internal_baml_core::ir::RetryPolicyWalker;
use internal_baml_jinja::{RenderContext_Client, RenderedChatMessage, RenderedPrompt};

use self::retry_policy::CallablePolicy;

use super::{PromptRenderer};
use crate::RuntimeContext;

pub use llm_provider::LLMProvider;

#[derive(Debug)]
pub struct LLMResponse {
    pub model: String,
    pub content: String,
    pub start_time_unix_ms: u64,
    pub latency_ms: u64,
    pub metadata: serde_json::Value,
}

struct LLMStreamResponse {
    delta: String,
    start_time_unix_ms: u64,
    latency_ms: u64,
    metadata: serde_json::Value,
}

#[derive(Clone, Copy)]
pub enum ModelType {
    Chat,
}

pub trait LLMClientExt {
    fn retry_policy(&self) -> Option<RetryPolicyWalker>;

    fn render_prompt(
        &self,
        renderer: &PromptRenderer<'_>,
        ctx: &RuntimeContext,
        params: &serde_json::Value,
    ) -> Result<RenderedPrompt>;

    async fn call(&mut self, prompt: &RenderedPrompt) -> Result<LLMResponse> {
        if let Some(policy) = self.retry_policy() {
            let retry_strategy = CallablePolicy::new(&policy);
            let mut err = None;
            for delay in retry_strategy {
                match self.single_call(prompt).await {
                    Ok(response) => return Ok(response),
                    Err(e) => {
                        err = Some(e);
                    }
                }
                tokio::time::sleep(delay).await;
            }
            if let Some(e) = err {
                return Err(e);
            } else {
                anyhow::bail!("No response from client");
            }
        } else {
            return self.single_call(prompt).await;
        }
    }

    async fn single_call(&self, prompt: &RenderedPrompt) -> Result<LLMResponse>;
}

trait LLMClient: LLMClientExt {
    fn context(&self) -> RenderContext_Client;

    fn model_type(&self) -> ModelType;
}

trait LLMChatClient: LLMClient {
    fn default_role(&self) -> &str;

    fn render_chat_prompt(
        &self,
        renderer: &PromptRenderer<'_>,
        ctx: &RuntimeContext,
        params: &serde_json::Value,
    ) -> Result<Vec<RenderedChatMessage>> {
        let response = renderer.render_prompt(ctx, params, self.context())?;

        match response {
            RenderedPrompt::Completion(message) => Ok(vec![RenderedChatMessage {
                role: self.default_role().into(),
                message,
            }]),
            RenderedPrompt::Chat(messages) => Ok(messages),
        }
    }

    async fn chat(&self, messages: &Vec<RenderedChatMessage>) -> Result<LLMResponse>;

    async fn stream_chat(
        &self,
        messages: &Vec<RenderedChatMessage>,
    ) -> impl Stream<Item = Result<LLMStreamResponse>> {
        stream! {
            let response = self.chat(messages).await?;
            yield Ok(LLMStreamResponse {
                delta: response.content,
                start_time_unix_ms: response.start_time_unix_ms,
                latency_ms: response.latency_ms,
                metadata: response.metadata,
            });
        }
    }
}

trait LLMCompletionClient: LLMClient {
    fn default_join(&self) -> &str {
        "\n"
    }

    fn render_completion_prompt(
        &self,
        renderer: &PromptRenderer<'_>,
        ctx: &RuntimeContext,
        params: &serde_json::Value,
    ) -> Result<String> {
        let response = renderer.render_prompt(ctx, params, self.context())?;

        match response {
            RenderedPrompt::Completion(response) => Ok(response),
            RenderedPrompt::Chat(messages) => {
                let message = messages
                    .iter()
                    .map(|m| m.message.as_str())
                    .collect::<Vec<_>>()
                    .join(self.default_join());
                Ok(message)
            }
        }
    }

    async fn completion(&self, prompt: String) -> Result<LLMResponse>;
    async fn stream_completion(
        &self,
        prompt: String,
    ) -> impl Stream<Item = Result<LLMStreamResponse>> {
        stream! {
            let response = self.completion(prompt).await?;
            yield Ok(LLMStreamResponse {
                delta: response.content,
                start_time_unix_ms: response.start_time_unix_ms,
                latency_ms: response.latency_ms,
                metadata: response.metadata,
            });
        }
    }
}
