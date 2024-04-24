mod openai_client;

use anyhow::Result;
use async_stream::stream;
use futures::Stream;
use internal_baml_jinja::{
    render_prompt, RenderContext, RenderContext_Client, RenderedChatMessage, RenderedPrompt,
};

use super::{PromptRenderer, RuntimeContext};

struct LLMResponse {
    content: String,
    start_time_unix_ms: u64,
    latency_ms: u64,
    metadata: serde_json::Value,
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
    Completion,
}

pub trait LLMClient {
    fn context(&self) -> RenderContext_Client;

    fn model_type(&self) -> ModelType;

    fn render_prompt(
        &self,
        renderer: &PromptRenderer<'_>,
        ctx: &RuntimeContext,
        params: &serde_json::Value,
    ) -> Result<RenderedPrompt>;

    async fn call(&self, prompt: RenderedPrompt) -> Result<LLMResponse>;
}

pub trait LLMChatClient: LLMClient {
    fn default_role(&self) -> &str;

    fn render_chat_prompt(
        &self,
        renderer: &PromptRenderer<'_>,
        ctx: &RuntimeContext,
        params: &serde_json::Value,
    ) -> Result<Vec<RenderedChatMessage>> {
        let response = render_prompt(
            renderer.prompt_template(),
            params,
            &RenderContext {
                client: self.context(),
                output_format: "".into(),
                env: ctx.env.clone(),
            },
            renderer.template_macros(),
        )?;

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
        let response = render_prompt(
            renderer.prompt_template(),
            params,
            &RenderContext {
                client: self.context(),
                output_format: "".into(),
                env: ctx.env.clone(),
            },
            renderer.template_macros(),
        )?;

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
