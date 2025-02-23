use anyhow::Result;
use baml_types::tracing::events::HttpRequestId;
use internal_baml_jinja::{ChatOptions, RenderedChatMessage};

use crate::{internal::llm_client::LLMResponse, RuntimeContext};

use super::StreamResponse;

pub trait WithChatOptions {
    fn chat_options(&self, ctx: &RuntimeContext) -> Result<ChatOptions>;
}

impl<T> WithChatOptions for T
where
    T: super::WithClientProperties,
{
    fn chat_options(&self, ctx: &RuntimeContext) -> Result<ChatOptions> {
        Ok(ChatOptions::new(
            self.default_role(),
            Some(self.allowed_roles()),
        ))
    }
}

pub trait WithChat: Sync + Send + WithChatOptions {
    #[allow(async_fn_in_trait)]
    async fn chat(
        &self,
        ctx: &RuntimeContext,
        prompt: &[RenderedChatMessage],
        http_request_id: HttpRequestId,
    ) -> LLMResponse;
}

pub trait WithStreamChat: Sync + Send {
    #[allow(async_fn_in_trait)]
    async fn stream_chat(
        &self,
        ctx: &RuntimeContext,
        prompt: &[RenderedChatMessage],
        http_request_id: HttpRequestId,
    ) -> StreamResponse;
}

pub trait WithNoChat {}

impl<T> WithChat for T
where
    T: WithNoChat + Send + Sync + WithChatOptions,
{
    #[allow(async_fn_in_trait)]
    async fn chat(
        &self,
        _: &RuntimeContext,
        _: &[RenderedChatMessage],
        _: HttpRequestId,
    ) -> LLMResponse {
        LLMResponse::InternalFailure("Chat prompts are not supported by this provider".to_string())
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
        _: &[RenderedChatMessage],
        _: HttpRequestId,
    ) -> StreamResponse {
        Err(LLMResponse::InternalFailure(
            "Chat prompts are not supported by this provider".to_string(),
        ))
    }
}
