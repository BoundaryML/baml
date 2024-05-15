use anyhow::Result;
use internal_baml_jinja::CompletionOptions;

use crate::{
    internal::llm_client::{LLMResponse, LLMResponseStream},
    RuntimeContext,
};

pub trait WithCompletion: Sync + Send {
    fn completion_options(&self, ctx: &RuntimeContext) -> Result<CompletionOptions>;

    #[allow(async_fn_in_trait)]
    async fn completion(&self, ctx: &RuntimeContext, prompt: &String) -> Result<LLMResponse>;
}

pub trait WithStreamCompletion: Sync + Send {
    #[allow(async_fn_in_trait)]
    async fn stream_completion(&self, ctx: &RuntimeContext, prompt: &String) -> LLMResponseStream;
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
    async fn completion(&self, _: &RuntimeContext, _: &String) -> Result<LLMResponse> {
        anyhow::bail!("Completion prompts are not supported by this provider")
    }
}
