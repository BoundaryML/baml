use anyhow::Result;
mod chat;
mod completion;

use baml_types::BamlValue;
use internal_baml_jinja::{RenderContext_Client, RenderedPrompt};


use crate::{internal::prompt_renderer::PromptRenderer, RuntimeContext};

pub use self::{
    chat::{WithChat, WithStreamChat},
    completion::{WithCompletion, WithNoCompletion, WithStreamCompletion},
};

use super::{
    retry_policy::CallablePolicy, LLMResponse, LLMResponseStream, ModelFeatures,
};


pub trait WithRetryPolicy {
    fn retry_policy_name(&self) -> Option<&str>;
}

type ResponseType = Result<LLMResponse>;

pub trait WithStreamable: Send {
    /// Retries are not supported for streaming calls.
    #[allow(async_fn_in_trait)]
    async fn stream(
        &self,
        retry_policy: Option<CallablePolicy>,
        ctx: &RuntimeContext,
        prompt: &RenderedPrompt,
    ) -> LLMResponseStream;
}

pub trait WithSingleCallable {
    #[allow(async_fn_in_trait)]
    async fn single_call(
        &self,
        ctx: &RuntimeContext,
        prompt: &RenderedPrompt,
    ) -> Result<LLMResponse>;
}

pub trait WithClient {
    fn context(&self) -> &RenderContext_Client;

    fn model_features(&self) -> &ModelFeatures;
}

pub trait WithPrompt<'ir> {
    fn render_prompt(
        &'ir self,
        renderer: &PromptRenderer,
        ctx: &RuntimeContext,
        params: &BamlValue,
    ) -> Result<RenderedPrompt>;
}

impl<T> WithSingleCallable for T
where
    T: WithClient + WithChat + WithCompletion,
{
    #[allow(async_fn_in_trait)]
    async fn single_call(
        &self,
        ctx: &RuntimeContext,
        prompt: &RenderedPrompt,
    ) -> Result<LLMResponse> {
        match self.model_features() {
            ModelFeatures {
                completion: true,
                chat: false,
            } => {
                let prompt = match prompt {
                    RenderedPrompt::Completion(p) => p,
                    _ => anyhow::bail!("Expected completion prompt"),
                };
                self.completion(ctx, prompt).await
            }
            ModelFeatures {
                completion: false,
                chat: true,
            } => {
                let prompt = match prompt {
                    RenderedPrompt::Chat(p) => p,
                    _ => anyhow::bail!("Expected chat prompt"),
                };
                self.chat(ctx, prompt).await
            }
            ModelFeatures {
                completion: true,
                chat: true,
            } => match prompt {
                RenderedPrompt::Chat(p) => self.chat(ctx, p).await,
                RenderedPrompt::Completion(p) => self.completion(ctx, p).await,
            },
            ModelFeatures {
                completion: false,
                chat: false,
            } => anyhow::bail!("No model type supported"),
        }
    }
}

impl<'ir, T> WithPrompt<'ir> for T
where
    T: WithClient + WithChat + WithCompletion,
{
    fn render_prompt(
        &'ir self,
        renderer: &PromptRenderer,
        ctx: &RuntimeContext,
        params: &BamlValue,
    ) -> Result<RenderedPrompt> {
        let prompt = renderer.render_prompt(ctx, params, self.context())?;
        log::debug!("WithPrompt.render_prompt => {:#?}", prompt);

        match self.model_features() {
            ModelFeatures {
                completion: true,
                chat: false,
            } => {
                let options = self.completion_options(ctx)?;
                Ok(prompt.as_completion(&options))
            }
            ModelFeatures {
                completion: false,
                chat: true,
            } => {
                let options = self.chat_options(ctx)?;
                Ok(prompt.as_chat(&options))
            }
            ModelFeatures {
                completion: true,
                chat: true,
            } => Ok(prompt),
            ModelFeatures {
                completion: false,
                chat: false,
            } => anyhow::bail!("No model type supported"),
        }
    }
}
