use anyhow::Result;
mod chat;
mod completion;

use internal_baml_jinja::{BamlArgType, RenderContext_Client, RenderedPrompt};
use std::sync::{Arc, Mutex};

use crate::{internal::prompt_renderer::PromptRenderer, RuntimeContext};

pub use self::{
    chat::{WithChat, WithStreamChat},
    completion::{WithCompletion, WithNoCompletion, WithStreamCompletion},
};

use super::{
    retry_policy::CallablePolicy, LLMResponse, LLMCompleteResponseStream, ModelFeatures, RetryLLMResponse,
};

pub trait WithRetryPolicy {
    fn retry_policy_name(&self) -> Option<&str>;
}

// #[cfg(not(feature = "no_wasm"))]
// type ResponseType = Result<LLMResponse, wasm_bindgen::JsValue>;
// #[cfg(feature = "no_wasm")]
type ResponseType = Result<LLMResponse>;

pub trait WithCallable: Send {
    /// Call the model with the specified prompt, retrying as appropriate.
    ///
    /// retry_policy is a stateful iterator, so it's taken by value
    #[allow(async_fn_in_trait)]
    async fn call(
        &self,
        retry_policy: Option<CallablePolicy>,
        ctx: &RuntimeContext,
        prompt: &RenderedPrompt,
    ) -> LLMResponse;
}

pub trait WithStreamable: Send {
    /// Retries are not supported for streaming calls.
    #[allow(async_fn_in_trait)]
    async fn stream(
        &self,
        retry_policy: Option<CallablePolicy>,
        ctx: &RuntimeContext,
        prompt: &RenderedPrompt,
    ) -> LLMCompleteResponseStream;
}

pub trait WithSingleCallable {
    #[allow(async_fn_in_trait)]
    async fn single_call(&self, ctx: &RuntimeContext, prompt: &RenderedPrompt) -> ResponseType;
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
        params: &BamlArgType,
    ) -> Result<RenderedPrompt>;
}

impl<T> WithCallable for T
where
    T: WithSingleCallable + Send,
{
    #[allow(async_fn_in_trait)]
    async fn call(
        &self,
        retry_policy: Option<CallablePolicy>,
        ctx: &RuntimeContext,
        prompt: &RenderedPrompt,
    ) -> LLMResponse {
        if let Some(retry_strategy) = retry_policy {
            let retry_strategy = retry_strategy.clone();

            // TODO: @sxlijin collect all errors.
            let err = Arc::new(Mutex::new(vec![]));

            let to_final_response = |res: LLMResponse, err: Arc<Mutex<Vec<LLMResponse>>>| {
                let err = match Arc::try_unwrap(err) {
                    Ok(err) => match err.into_inner() {
                        Ok(err) => err,
                        Err(err) => {
                            log::error!("Failed to unwrap error: {:?}", err);
                            vec![]
                        }
                    },
                    Err(err) => {
                        log::error!("Failed to unwrap error: {:?}", err);
                        vec![]
                    }
                };

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
                        err.lock().unwrap().push(x);
                    }
                    Err(e) => {
                        err.lock()
                            .unwrap()
                            .push(LLMResponse::OtherFailures(e.to_string()));
                    }
                }
                tokio::time::sleep(delay).await;
            }

            let err = match std::sync::Arc::try_unwrap(err) {
                Ok(err) => match err.into_inner() {
                    Ok(err) => err,
                    Err(err) => {
                        log::error!("Failed to unwrap error: {:?}", err);
                        vec![]
                    }
                },
                Err(err) => {
                    log::error!("Failed to unwrap error: {:?}", err);
                    vec![]
                }
            };

            if err.is_empty() {
                LLMResponse::OtherFailures("No calls were made".into())
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
    #[allow(async_fn_in_trait)]
    async fn single_call(&self, ctx: &RuntimeContext, prompt: &RenderedPrompt) -> ResponseType {
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
        params: &BamlArgType,
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
