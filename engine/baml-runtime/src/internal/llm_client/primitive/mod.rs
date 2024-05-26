use std::sync::Arc;

use anyhow::Result;
use async_std::stream;
use baml_types::BamlValue;
use internal_baml_core::ir::ClientWalker;

use crate::{
    internal::prompt_renderer::PromptRenderer, runtime_interface::InternalClientLookup,
    RuntimeContext,
};

use self::{anthropic::AnthropicClient, openai::OpenAIClient, request::RequestBuilder};

use super::{
    orchestrator::{
        ExecutionScope, IterOrchestrator, OrchestrationScope, OrchestrationState, OrchestratorNode,
        OrchestratorNodeIterator,
    },
    retry_policy::CallablePolicy,
    traits::{WithPrompt, WithRetryPolicy, WithSingleCallable, WithStreamable},
    LLMResponse,
};

mod anthropic;
mod openai;
pub(super) mod request;

pub enum LLMPrimitiveProvider {
    OpenAI(OpenAIClient),
    Anthropic(AnthropicClient),
}

impl TryFrom<(&ClientWalker<'_>, &RuntimeContext)> for LLMPrimitiveProvider {
    type Error = anyhow::Error;

    fn try_from((client, ctx): (&ClientWalker, &RuntimeContext)) -> Result<Self> {
        match client.elem().provider.as_str() {
            "baml-openai-chat" | "openai" => {
                OpenAIClient::new(client, ctx).map(LLMPrimitiveProvider::OpenAI)
            }
            "baml-anthropic-chat" | "anthropic" => {
                AnthropicClient::new(client, ctx).map(LLMPrimitiveProvider::Anthropic)
            }
            "baml-ollama-chat" | "ollama" => {
                OpenAIClient::new(client, ctx).map(LLMPrimitiveProvider::OpenAI)
            }
            other => {
                let options = ["openai", "anthropic", "ollama"];
                anyhow::bail!(
                    "Unsupported provider: {}. Available ones are: {}",
                    other,
                    options.join(", ")
                )
            }
        }
    }
}

impl<'ir> WithPrompt<'ir> for LLMPrimitiveProvider {
    fn render_prompt(
        &'ir self,
        renderer: &PromptRenderer,
        ctx: &RuntimeContext,
        params: &BamlValue,
    ) -> Result<internal_baml_jinja::RenderedPrompt> {
        match self {
            LLMPrimitiveProvider::OpenAI(client) => client.render_prompt(renderer, ctx, params),
            LLMPrimitiveProvider::Anthropic(client) => client.render_prompt(renderer, ctx, params),
        }
    }
}

impl WithRetryPolicy for LLMPrimitiveProvider {
    fn retry_policy_name(&self) -> Option<&str> {
        match self {
            LLMPrimitiveProvider::OpenAI(client) => client.retry_policy_name(),
            LLMPrimitiveProvider::Anthropic(client) => client.retry_policy_name(),
        }
    }
}

impl WithSingleCallable for LLMPrimitiveProvider {
    async fn single_call(
        &self,
        ctx: &RuntimeContext,
        prompt: &internal_baml_jinja::RenderedPrompt,
    ) -> LLMResponse {
        match self {
            LLMPrimitiveProvider::OpenAI(client) => client.single_call(ctx, prompt).await,
            LLMPrimitiveProvider::Anthropic(client) => client.single_call(ctx, prompt).await,
        }
    }
}

impl WithStreamable for LLMPrimitiveProvider {
    async fn stream(
        &self,
        ctx: &RuntimeContext,
        prompt: &internal_baml_jinja::RenderedPrompt,
    ) -> super::traits::StreamResponse {
        match self {
            LLMPrimitiveProvider::OpenAI(client) => client.stream(ctx, prompt).await,
            LLMPrimitiveProvider::Anthropic(client) => client.stream(ctx, prompt).await,
        }
    }
}

impl IterOrchestrator for Arc<LLMPrimitiveProvider> {
    fn iter_orchestrator<'a>(
        &self,
        _state: &mut OrchestrationState,
        _previous: OrchestrationScope,
        _ctx: &RuntimeContext,
        _client_lookup: &'a dyn InternalClientLookup,
    ) -> OrchestratorNodeIterator {
        vec![OrchestratorNode::new(
            ExecutionScope::Direct(self.name().to_string()),
            self.clone(),
        )]
    }
}

impl std::fmt::Display for LLMPrimitiveProvider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LLMPrimitiveProvider::OpenAI(_) => write!(f, "OpenAI"),
            LLMPrimitiveProvider::Anthropic(_) => write!(f, "Anthropic"),
        }
    }
}

impl LLMPrimitiveProvider {
    pub fn name(&self) -> &str {
        match self {
            LLMPrimitiveProvider::OpenAI(o) => o.name.as_str(),
            LLMPrimitiveProvider::Anthropic(a) => a.name.as_str(),
        }
    }
}

impl RequestBuilder for LLMPrimitiveProvider {
    fn http_client(&self) -> &reqwest::Client {
        match self {
            LLMPrimitiveProvider::OpenAI(client) => client.http_client(),
            LLMPrimitiveProvider::Anthropic(client) => client.http_client(),
        }
    }
    fn invocation_params(&self) -> &std::collections::HashMap<String, serde_json::Value> {
        match self {
            LLMPrimitiveProvider::OpenAI(client) => client.invocation_params(),
            LLMPrimitiveProvider::Anthropic(client) => client.invocation_params(),
        }
    }

    fn build_request(
        &self,
        prompt: either::Either<&String, &Vec<internal_baml_jinja::RenderedChatMessage>>,
        stream: bool,
    ) -> reqwest::RequestBuilder {
        match self {
            LLMPrimitiveProvider::OpenAI(client) => client.build_request(prompt, stream),
            LLMPrimitiveProvider::Anthropic(client) => client.build_request(prompt, stream),
        }
    }
}
