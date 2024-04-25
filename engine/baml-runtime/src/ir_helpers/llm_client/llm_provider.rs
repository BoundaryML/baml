use anyhow::Result;
use internal_baml_jinja::RenderedPrompt;

use crate::ir_helpers::{ClientWalker, PromptRenderer, RetryPolicyWalker, RuntimeContext};

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
