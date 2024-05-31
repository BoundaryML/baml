use anyhow::Result;
use internal_baml_jinja::{ChatOptions, RenderedChatMessage};

use crate::{
    internal::llm_client::{LLMErrorResponse, LLMResponse},
    RuntimeContext,
};

use super::{SseResponseTrait, StreamResponse};

pub trait WithChat: Sync + Send {
    fn chat_options(&self, ctx: &RuntimeContext) -> Result<ChatOptions>;

    #[allow(async_fn_in_trait)]
    async fn chat(&self, ctx: &RuntimeContext, prompt: &Vec<RenderedChatMessage>) -> LLMResponse;
}

pub trait WithStreamChat: Sync + Send {
    #[allow(async_fn_in_trait)]
    async fn stream_chat(
        &self,
        ctx: &RuntimeContext,
        prompt: &Vec<RenderedChatMessage>,
    ) -> StreamResponse;
}

pub trait WithNoChat {}

impl<T> WithChat for T
where
    T: WithNoChat + Send + Sync,
{
    fn chat_options(&self, _ctx: &RuntimeContext) -> Result<ChatOptions> {
        anyhow::bail!("Chat prompts are not supported by this provider")
    }

    #[allow(async_fn_in_trait)]
    async fn chat(&self, _: &RuntimeContext, _: &Vec<RenderedChatMessage>) -> LLMResponse {
        LLMResponse::OtherFailure("Chat prompts are not supported by this provider".to_string())
    }
}

impl<T> WithStreamChat for T
where
    T: WithNoChat + Send + Sync,
{
    #[allow(async_fn_in_trait)]
    async fn stream_chat(
        &self,
        _: &RuntimeContext,
        _: &Vec<RenderedChatMessage>,
    ) -> StreamResponse {
        Err(LLMResponse::OtherFailure(
            "Chat prompts are not supported by this provider".to_string(),
        ))
    }
}
