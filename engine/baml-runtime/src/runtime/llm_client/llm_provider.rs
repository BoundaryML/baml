use std::sync::Arc;

use anyhow::Result;
use internal_baml_core::ir::ClientWalker;

use crate::{runtime::prompt_renderer::PromptRenderer, RuntimeContext};

use super::{
    openai::OpenAIClient,
    traits::{WithCallable, WithPrompt},
    LLMResponse,
};

pub enum LLMProvider<'ir> {
    OpenAI(OpenAIClient<'ir>),
    // Anthropic(AnthropicClient<'ir>),
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

impl WithPrompt for LLMProvider<'_> {
    fn render_prompt(
        &mut self,
        renderer: &PromptRenderer<'_>,
        ctx: &RuntimeContext,
        params: &serde_json::Value,
    ) -> Result<internal_baml_jinja::RenderedPrompt> {
        match self {
            LLMProvider::OpenAI(client) => client.render_prompt(renderer, ctx, params),
        }
    }
}

impl WithCallable for LLMProvider<'_> {
    async fn call(
        &mut self,
        ir: &internal_baml_core::ir::repr::IntermediateRepr,
        ctx: &RuntimeContext,
        prompt: &internal_baml_jinja::RenderedPrompt,
    ) -> Result<LLMResponse> {
        match self {
            LLMProvider::OpenAI(client) => client.call(ir, ctx, prompt).await,
        }
    }
}
