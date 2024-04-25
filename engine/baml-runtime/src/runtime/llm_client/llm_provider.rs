use anyhow::Result;
use internal_baml_core::ir::{ClientWalker, RetryPolicyWalker};
use internal_baml_jinja::RenderedPrompt;

use crate::runtime::{prompt_renderer::PromptRenderer};
use crate::RuntimeContext;

use super::{openai::OpenAIClient, LLMClientExt, LLMResponse};

pub enum LLMProvider<'ir> {
    OpenAI(OpenAIClient<'ir>),
}

impl LLMClientExt for LLMProvider<'_> {
    fn retry_policy(&self) -> Option<RetryPolicyWalker> {
        match self {
            LLMProvider::OpenAI(client) => client.retry_policy(),
        }
    }

    fn render_prompt(
        &self,
        renderer: &PromptRenderer<'_>,
        ctx: &RuntimeContext,
        params: &serde_json::Value,
    ) -> Result<RenderedPrompt> {
        match self {
            LLMProvider::OpenAI(client) => client.render_prompt(renderer, ctx, params),
        }
    }

    async fn single_call(&self, prompt: &RenderedPrompt) -> Result<LLMResponse> {
        match self {
            LLMProvider::OpenAI(client) => client.single_call(prompt).await,
        }
    }

    async fn call(&mut self, prompt: &RenderedPrompt) -> Result<LLMResponse> {
        if let Some(policy) = self.retry_policy() {
            let retry_strategy = super::retry_policy::CallablePolicy::new(&policy);
            let mut err = None;
            for delay in retry_strategy {
                match self.single_call(prompt).await {
                    Ok(response) => return Ok(response),
                    Err(e) => {
                        err = Some(e);
                    }
                }
                tokio::time::sleep(delay).await;
            }
            if let Some(e) = err {
                return Err(e);
            } else {
                anyhow::bail!("No response from client");
            }
        } else {
            return self.single_call(prompt).await;
        }
    }
}

impl LLMProvider<'_> {
    pub fn from_ir<'ir>(
        client: &'ir ClientWalker,
        ctx: &RuntimeContext,
    ) -> Result<LLMProvider<'ir>> {
        match client.elem().provider.as_str() {
            "baml-openai-chat" | "openai" => {
                OpenAIClient::new(client, ctx).map(LLMProvider::OpenAI)
            }
            _ => anyhow::bail!("Unsupported provider"),
        }
    }
}
