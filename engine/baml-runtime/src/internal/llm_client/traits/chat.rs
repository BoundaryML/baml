use anyhow::Result;
use internal_baml_jinja::{ChatOptions, RenderedChatMessage};

use crate::{
    internal::llm_client::{LLMResponse, LLMResponseStream},
    RuntimeContext,
};

pub trait WithChat: Sync + Send {
    fn chat_options(&self, ctx: &RuntimeContext) -> Result<ChatOptions>;

    #[allow(async_fn_in_trait)]
    async fn chat(
        &self,
        ctx: &RuntimeContext,
        prompt: &Vec<RenderedChatMessage>,
    ) -> Result<LLMResponse>;
}

pub trait WithStreamChat: Sync + Send {
    #[allow(async_fn_in_trait)]
    async fn stream_chat(
        &self,
        ctx: &RuntimeContext,
        prompt: &Vec<RenderedChatMessage>,
    ) -> Result<LLMResponseStream>;
}

pub trait WithNoChat {}

impl<T> WithChat for T
where
    T: WithNoChat + Send + Sync,
{
    fn chat_options(&self, _ctx: &RuntimeContext) -> Result<ChatOptions> {
        anyhow::bail!("Chat prompts are not supported by this provider")
    }

    #[cfg(not(feature = "no_wasm"))]
    #[allow(async_fn_in_trait)]
    async fn chat(&self, _: &RuntimeContext, _: &Vec<RenderedChatMessage>) -> Result<LLMResponse> {
        anyhow::bail!("Chat prompts are not supported by this provider")
    }

    #[cfg(feature = "no_wasm")]
    #[allow(async_fn_in_trait)]
    async fn chat(&self, _: &RuntimeContext, _: &Vec<RenderedChatMessage>) -> Result<LLMResponse> {
        anyhow::bail!("Chat prompts are not supported by this provider")
    }
}
