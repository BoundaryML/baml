use anyhow::Result;
use internal_baml_jinja::CompletionOptions;

use crate::{internal::llm_client::LLMResponse, RuntimeContext};

use super::StreamResponse;

pub trait WithCompletion: Sync + Send {
    fn completion_options(&self, ctx: &RuntimeContext) -> Result<CompletionOptions>;

    #[allow(async_fn_in_trait)]
    async fn completion(&self, ctx: &RuntimeContext, prompt: &String) -> LLMResponse;
}

pub trait WithStreamCompletion: Sync + Send {
    #[allow(async_fn_in_trait)]
    async fn stream_completion(&self, ctx: &RuntimeContext, prompt: &String) -> StreamResponse;
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
    async fn completion(&self, _: &RuntimeContext, _: &String) -> LLMResponse {
        LLMResponse::InternalFailure("Completion prompts are not supported by this provider".into())
    }
}

impl<T> WithStreamCompletion for T
where
    T: WithNoCompletion + Send + Sync,
{
    #[allow(async_fn_in_trait)]
    async fn stream_completion(&self, _: &RuntimeContext, _: &String) -> StreamResponse {
        Err(LLMResponse::InternalFailure(
            "Completion prompts are not supported by this provider".to_string(),
        ))
    }
}
