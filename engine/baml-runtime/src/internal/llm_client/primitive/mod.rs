use std::sync::Arc;

use anyhow::Result;
use baml_types::BamlValue;
use internal_baml_core::ir::{repr::IntermediateRepr, ClientWalker};

use crate::{
    internal::prompt_renderer::PromptRenderer, runtime_interface::InternalClientLookup,
    RuntimeContext,
};

use self::{
    anthropic::AnthropicClient, google::GoogleClient, openai::OpenAIClient, request::RequestBuilder,
};

use super::{
    orchestrator::{
        ExecutionScope, IterOrchestrator, OrchestrationScope, OrchestrationState, OrchestratorNode,
        OrchestratorNodeIterator,
    },
    retry_policy::CallablePolicy,
    traits::{WithClient, WithPrompt, WithRetryPolicy, WithSingleCallable, WithStreamable},
    LLMResponse,
};

mod anthropic;
mod google;
mod openai;
pub(super) mod request;

pub enum LLMPrimitiveProvider {
    OpenAI(OpenAIClient),
    Anthropic(AnthropicClient),
    Google(GoogleClient),
}

macro_rules! match_llm_provider {
    // Define the variants inside the macro
    ($self:expr, $method:ident, async $(, $args:tt)*) => {
        match $self {
            LLMPrimitiveProvider::OpenAI(client) => client.$method($($args),*).await,
            LLMPrimitiveProvider::Anthropic(client) => client.$method($($args),*).await,
            LLMPrimitiveProvider::Google(client) => client.$method($($args),*).await,
        }
    };

    ($self:expr, $method:ident $(, $args:tt)*) => {
        match $self {
            LLMPrimitiveProvider::OpenAI(client) => client.$method($($args),*),
            LLMPrimitiveProvider::Anthropic(client) => client.$method($($args),*),
            LLMPrimitiveProvider::Google(client) => client.$method($($args),*),
        }
    };
}

impl TryFrom<(&ClientWalker<'_>, &RuntimeContext)> for LLMPrimitiveProvider {
    type Error = anyhow::Error;

    fn try_from((client, ctx): (&ClientWalker, &RuntimeContext)) -> Result<Self> {
        match client.elem().provider.as_str() {
            "baml-openai-chat" | "openai" => {
                OpenAIClient::new(client, ctx).map(LLMPrimitiveProvider::OpenAI)
            }
            "baml-azure-chat" | "azure-openai" => {
                OpenAIClient::new_azure(client, ctx).map(LLMPrimitiveProvider::OpenAI)
            }
            "baml-anthropic-chat" | "anthropic" => {
                AnthropicClient::new(client, ctx).map(LLMPrimitiveProvider::Anthropic)
            }
            "baml-ollama-chat" | "ollama" => {
                OpenAIClient::new_ollama(client, ctx).map(LLMPrimitiveProvider::OpenAI)
            }
            "baml-google-chat" | "google" => {
                GoogleClient::new(client, ctx).map(LLMPrimitiveProvider::Google)
            }
            other => {
                let options = [
                    "openai",
                    "anthropic",
                    "ollama",
                    "azure-openai",
                    "fallback",
                    "round-robin",
                    "google",
                ];
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
        ir: &'ir IntermediateRepr,
        renderer: &PromptRenderer,
        ctx: &RuntimeContext,
        params: &BamlValue,
    ) -> Result<internal_baml_jinja::RenderedPrompt> {
        match_llm_provider!(self, render_prompt, ir, renderer, ctx, params)
    }
}

impl WithRetryPolicy for LLMPrimitiveProvider {
    fn retry_policy_name(&self) -> Option<&str> {
        match_llm_provider!(self, retry_policy_name)
    }
}

impl WithSingleCallable for LLMPrimitiveProvider {
    async fn single_call(
        &self,
        ctx: &RuntimeContext,
        prompt: &internal_baml_jinja::RenderedPrompt,
    ) -> LLMResponse {
        match_llm_provider!(self, single_call, async, ctx, prompt)
    }
}

impl WithStreamable for LLMPrimitiveProvider {
    async fn stream(
        &self,
        ctx: &RuntimeContext,
        prompt: &internal_baml_jinja::RenderedPrompt,
    ) -> super::traits::StreamResponse {
        match_llm_provider!(self, stream, async, ctx, prompt)
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
            LLMPrimitiveProvider::Google(_) => write!(f, "Google"),
        }
    }
}

impl LLMPrimitiveProvider {
    pub fn name(&self) -> &str {
        &match_llm_provider!(self, context).name
    }
}

impl RequestBuilder for LLMPrimitiveProvider {
    fn http_client(&self) -> &reqwest::Client {
        match_llm_provider!(self, http_client)
    }

    fn invocation_params(&self) -> &std::collections::HashMap<String, serde_json::Value> {
        match_llm_provider!(self, invocation_params)
    }

    fn build_request(
        &self,
        prompt: either::Either<&String, &Vec<internal_baml_jinja::RenderedChatMessage>>,
        stream: bool,
    ) -> reqwest::RequestBuilder {
        match_llm_provider!(self, build_request, prompt, stream)
    }
}
