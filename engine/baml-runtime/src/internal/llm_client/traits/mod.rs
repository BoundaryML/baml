use anyhow::Result;
mod chat;
mod completion;

use baml_types::BamlValue;
use internal_baml_jinja::{RenderContext_Client, RenderedPrompt};
use std::sync::{Arc, Mutex};

use crate::{internal::prompt_renderer::PromptRenderer, RuntimeContext};

pub use self::{
    chat::{WithChat, WithStreamChat},
    completion::{WithCompletion, WithNoCompletion, WithStreamCompletion},
};

use super::{
    retry_policy::CallablePolicy, FunctionResultStream, LLMResponse, ModelFeatures,
    RetryLLMResponse,
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
        prompt_renderer: &PromptRenderer,
        baml_args: &BamlArgType,
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
    ) -> FunctionResultStream;
}

pub trait WithSingleCallable {
    #[allow(async_fn_in_trait)]
    async fn single_call(
        &self,
        ctx: &RuntimeContext,
        prompt_renderer: &PromptRenderer,
        baml_args: &BamlArgType,
    ) -> ResponseType;
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

impl<T> WithCallable for T
where
    T: WithSingleCallable + Send,
{
    #[allow(async_fn_in_trait)]
    async fn call(
        &self,
        retry_policy: Option<CallablePolicy>,
        ctx: &RuntimeContext,
        prompt_renderer: &PromptRenderer,
        baml_args: &BamlArgType,
    ) -> LLMResponse {
        let mut errors = Vec::new(); // Vector to store errors
        let mut success_response = None;

        if let Some(mut retry_strategy) = retry_policy {
            for delay in retry_strategy {
                match self.single_call(ctx, prompt_renderer, baml_args).await {
                    Ok(response) => match response {
                        LLMResponse::Success(_) => {
                            success_response = Some(Box::new(response));
                            // Found a successful response
                            break;
                        }
                        _ => errors.push(response),
                    },
                    Err(e) => errors.push(LLMResponse::OtherFailures(e.to_string())),
                }

                // Delay logic for retry
                #[cfg(not(feature = "no_wasm"))]
                wasm_sleep(delay.as_millis() as i32).await;
                #[cfg(feature = "no_wasm")]
                tokio::time::sleep(delay).await;
            }

            // Always return Retry when retry_policy is set, containing both successes and errors
            return LLMResponse::Retry(RetryLLMResponse {
                client: None,
                passed: success_response, // Pass the last successful response, if any
                failed: errors,
            });
        }

        // Handle no-retry-policy scenario
        match self.single_call(ctx, prompt_renderer, baml_args).await {
            Ok(x) => x,
            Err(e) => LLMResponse::OtherFailures(e.to_string()),
        }
    }
}

impl<T> WithSingleCallable for T
where
    T: WithClient + WithChat + WithCompletion,
{
    #[allow(async_fn_in_trait)]
    async fn single_call(
        &self,
        ctx: &RuntimeContext,
        prompt_renderer: &PromptRenderer,
        baml_args: &BamlArgType,
    ) -> ResponseType {
        let rendered_prompt = self.render_prompt(prompt_renderer, ctx, baml_args)?;
        match self.model_features() {
            ModelFeatures {
                completion: true,
                chat: false,
            } => {
                let prompt = match rendered_prompt {
                    RenderedPrompt::Completion(p) => p,
                    _ => anyhow::bail!("Expected completion prompt"),
                };
                self.completion(ctx, &prompt).await
            }
            ModelFeatures {
                completion: false,
                chat: true,
            } => {
                let prompt = match rendered_prompt {
                    RenderedPrompt::Chat(p) => p,
                    _ => anyhow::bail!("Expected chat prompt"),
                };
                self.chat(ctx, &prompt).await
            }
            ModelFeatures {
                completion: true,
                chat: true,
            } => match rendered_prompt {
                RenderedPrompt::Chat(p) => self.chat(ctx, &p).await,
                RenderedPrompt::Completion(p) => self.completion(ctx, &p).await,
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

#[cfg(not(feature = "no_wasm"))]
pub async fn wasm_sleep(delay: i32) {
    let mut cb = |resolve: js_sys::Function, reject: js_sys::Function| {
        web_sys::window()
            .unwrap()
            .set_timeout_with_callback_and_timeout_and_arguments_0(&resolve, delay);
    };

    let p = js_sys::Promise::new(&mut cb);

    wasm_bindgen_futures::JsFuture::from(p).await.unwrap();
}
