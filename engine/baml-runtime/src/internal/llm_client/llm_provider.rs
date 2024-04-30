use anyhow::Result;
use internal_baml_core::ir::ClientWalker;

use crate::RuntimeContext;

use super::{
    super::prompt_renderer::PromptRenderer,
    openai::OpenAIClient,
    retry_policy::CallablePolicy,
    traits::{WithCallable, WithPrompt, WithRetryPolicy},
    LLMResponse,
};

pub enum LLMProvider {
    OpenAI(OpenAIClient),
    // Anthropic(AnthropicClient<'ir>),
}

impl LLMProvider {
    pub fn from_ir(client: &ClientWalker, ctx: &RuntimeContext) -> Result<LLMProvider> {
        match client.elem().provider.as_str() {
            "baml-openai-chat" | "openai" => {
                OpenAIClient::new(client, ctx).map(LLMProvider::OpenAI)
            }
            // "baml-anthropic-chat" | "anthropic" => {
            //     AnthropicClient::new(client, ctx).map(LLMProvider::Anthropic)
            // }
            other => {
                let options = ["openai"];
                anyhow::bail!(
                    "Unsupported provider: {}. Available ones are: {}",
                    other,
                    options.join(", ")
                )
            }
        }
    }
}

impl<'ir> WithPrompt<'ir> for LLMProvider {
    fn render_prompt(
        &'ir mut self,
        renderer: &PromptRenderer,
        ctx: &RuntimeContext,
        params: &serde_json::Value,
    ) -> Result<internal_baml_jinja::RenderedPrompt> {
        match self {
            LLMProvider::OpenAI(client) => client.render_prompt(renderer, ctx, params),
        }
    }
}

impl WithRetryPolicy for LLMProvider {
    fn retry_policy_name(&self) -> Option<&str> {
        match self {
            LLMProvider::OpenAI(client) => client.retry_policy_name(),
        }
    }
}

impl WithCallable for LLMProvider {
    async fn call(
        &mut self,
        retry_policy: Option<CallablePolicy>,
        ctx: &RuntimeContext,
        prompt: &internal_baml_jinja::RenderedPrompt,
    ) -> LLMResponse {
        match self {
            LLMProvider::OpenAI(client) => client.call(retry_policy, ctx, prompt).await,
        }
    }
}
