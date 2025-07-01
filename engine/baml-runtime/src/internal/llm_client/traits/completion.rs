use anyhow::Result;
use internal_baml_jinja::CompletionOptions;

use super::{HttpContext, StreamResponse};
use crate::{internal::llm_client::LLMResponse, RuntimeContext};

pub trait WithCompletion: Sync + Send {
    fn completion_options(&self, ctx: &RuntimeContext) -> Result<CompletionOptions>;

    #[allow(async_fn_in_trait)]
    async fn completion(&self, ctx: &impl HttpContext, prompt: &str) -> LLMResponse;
}

pub trait WithStreamCompletion: Sync + Send {
    #[allow(async_fn_in_trait)]
    async fn stream_completion(&self, ctx: &impl HttpContext, prompt: &str) -> StreamResponse;
}

pub trait WithNoCompletion {}

impl<T> WithCompletion for T
where
    T: WithNoCompletion + Send + Sync,
{
    fn completion_options(&self, _ctx: &RuntimeContext) -> Result<CompletionOptions> {
        anyhow::bail!("Completion prompts are not supported by this provider")
    }

    #[allow(async_fn_in_trait)]
    async fn completion(&self, _: &impl HttpContext, _: &str) -> LLMResponse {
        LLMResponse::InternalFailure("Completion prompts are not supported by this provider".into())
    }
}

impl<T> WithStreamCompletion for T
where
    T: WithNoCompletion + Send + Sync,
{
    #[allow(async_fn_in_trait)]
    async fn stream_completion(&self, _: &impl HttpContext, _: &str) -> StreamResponse {
        Err(LLMResponse::InternalFailure(
            "Completion prompts are not supported by this provider".to_string(),
        ))
    }
}
