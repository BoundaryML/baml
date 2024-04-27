use anyhow::Result;
mod chat;
mod completion;

use internal_baml_core::ir::{repr::IntermediateRepr, RetryPolicyWalker};
use internal_baml_jinja::{RenderContext_Client, RenderedPrompt};

use crate::{runtime::prompt_renderer::PromptRenderer, RuntimeContext};

pub use self::{
    chat::{WithChat, WithNoChat},
    completion::{WithCompletion, WithNoCompletion},
};

use super::{retry_policy::CallablePolicy, LLMResponse, ModelFeatures, RetryLLMResponse};

pub trait WithRetryPolicy {
    fn retry_policy<'a>(&self, ir: &'a IntermediateRepr) -> Option<RetryPolicyWalker<'a>>;
}

pub trait WithCallable {
    async fn call(
        &mut self,
        ir: &IntermediateRepr,
        ctx: &RuntimeContext,
        prompt: &RenderedPrompt,
    ) -> LLMResponse;
}

pub trait WithSingleCallable {
    async fn single_call(
        &mut self,
        ctx: &RuntimeContext,
        prompt: &RenderedPrompt,
    ) -> Result<LLMResponse>;
}

pub trait WithClient {
    fn context(&mut self) -> &RenderContext_Client;

    fn model_features(&mut self, ctx: &RuntimeContext) -> Result<&ModelFeatures>;
}

pub trait WithPrompt {
    fn render_prompt(
        &mut self,
        renderer: &PromptRenderer<'_>,
        ctx: &RuntimeContext,
        params: &serde_json::Value,
    ) -> Result<RenderedPrompt>;
}

impl<T> WithCallable for T
where
    T: WithRetryPolicy + WithSingleCallable,
{
    async fn call<'a>(
        &mut self,
        ir: &'a IntermediateRepr,
        ctx: &RuntimeContext,
        prompt: &RenderedPrompt,
    ) -> LLMResponse {
        if let Some(policy) = &self.retry_policy(ir) {
            let retry_strategy = CallablePolicy::new(&policy);
            // TODO: @sxlijin collect all errors.
            let mut err = vec![];

            let to_final_response = |res: LLMResponse, err: Vec<LLMResponse>| {
                if err.is_empty() {
                    res
                } else {
                    LLMResponse::Retry(RetryLLMResponse {
                        client: None,
                        passed: Some(Box::new(res)),
                        failed: err,
                    })
                }
            };

            for delay in retry_strategy {
                match self.single_call(ctx, prompt).await {
                    Ok(LLMResponse::Success(content)) => {
                        return to_final_response(LLMResponse::Success(content), err);
                    }
                    Ok(LLMResponse::Retry(retry)) if retry.passed.is_some() => {
                        return to_final_response(LLMResponse::Retry(retry), err);
                    }
                    Ok(x) => {
                        err.push(x);
                    }
                    Err(e) => {
                        err.push(LLMResponse::OtherFailures(e.to_string()));
                    }
                }
                tokio::time::sleep(delay).await;
            }

            if err.is_empty() {
                LLMResponse::OtherFailures("No calls were made".to_string())
            } else {
                LLMResponse::Retry(RetryLLMResponse {
                    client: None,
                    passed: None,
                    failed: err,
                })
            }
        } else {
            match self.single_call(ctx, prompt).await {
                Ok(x) => x,
                Err(e) => LLMResponse::OtherFailures(e.to_string()),
            }
        }
    }
}

impl<T> WithSingleCallable for T
where
    T: WithClient + WithChat + WithCompletion,
{
    async fn single_call(
        &mut self,
        ctx: &RuntimeContext,
        prompt: &RenderedPrompt,
    ) -> Result<LLMResponse> {
        match self.model_features(ctx)? {
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

impl<T> WithPrompt for T
where
    T: WithClient + WithChat + WithCompletion,
{
    fn render_prompt(
        &mut self,
        renderer: &PromptRenderer<'_>,
        ctx: &RuntimeContext,
        params: &serde_json::Value,
    ) -> Result<RenderedPrompt> {
        let prompt = renderer.render_prompt(ctx, params, self.context())?;

        match self.model_features(ctx)? {
            ModelFeatures {
                completion: true,
                chat: false,
            } => Ok(prompt.as_completion(&self.completion_options(ctx)?)),
            ModelFeatures {
                completion: false,
                chat: true,
            } => Ok(prompt.as_chat(&self.chat_options(ctx)?)),
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
