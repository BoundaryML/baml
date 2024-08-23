use std::sync::Arc;

use anyhow::Result;
use baml_types::BamlValue;
use internal_baml_core::ir::{repr::IntermediateRepr, ClientWalker};

use crate::{
    client_registry::ClientProperty, internal::prompt_renderer::PromptRenderer,
    runtime_interface::InternalClientLookup, RenderCurlSettings, RuntimeContext,
};

use self::{
    anthropic::AnthropicClient, aws::AwsClient, google::GoogleAIClient, openai::OpenAIClient,
    request::RequestBuilder, vertex::VertexClient,
};

use super::{
    orchestrator::{
        ExecutionScope, IterOrchestrator, OrchestrationScope, OrchestrationState, OrchestratorNode,
        OrchestratorNodeIterator,
    },
    traits::{
        WithClient, WithClientProperties, WithPrompt, WithRenderRawCurl, WithRetryPolicy,
        WithSingleCallable, WithStreamable,
    },
    LLMResponse,
};

mod anthropic;
mod aws;
mod google;
mod openai;
pub(super) mod request;
mod vertex;

// use crate::internal::llm_client::traits::ambassador_impl_WithRenderRawCurl;
// use crate::internal::llm_client::traits::ambassador_impl_WithRetryPolicy;
use enum_dispatch::enum_dispatch;

#[enum_dispatch(WithRetryPolicy)]
pub enum LLMPrimitive2 {
    OpenAIClient,
    AnthropicClient,
    GoogleAIClient,
    VertexClient,
    AwsClient,
}

// #[derive(Delegate)]
// #[delegate(WithRetryPolicy, WithRenderRawCurl)]
pub enum LLMPrimitiveProvider {
    OpenAI(OpenAIClient),
    Anthropic(AnthropicClient),
    Google(GoogleAIClient),
    Vertex(VertexClient),
    Aws(aws::AwsClient),
}

macro_rules! match_llm_provider {
    // Define the variants inside the macro
    ($self:expr, $method:ident, async $(, $args:tt)*) => {
        match $self {
            LLMPrimitiveProvider::OpenAI(client) => client.$method($($args),*).await,
            LLMPrimitiveProvider::Anthropic(client) => client.$method($($args),*).await,
            LLMPrimitiveProvider::Google(client) => client.$method($($args),*).await,
            LLMPrimitiveProvider::Aws(client) => client.$method($($args),*).await,
            LLMPrimitiveProvider::Vertex(client) => client.$method($($args),*).await,
        }
    };

    ($self:expr, $method:ident $(, $args:tt)*) => {
        match $self {
            LLMPrimitiveProvider::OpenAI(client) => client.$method($($args),*),
            LLMPrimitiveProvider::Anthropic(client) => client.$method($($args),*),
            LLMPrimitiveProvider::Google(client) => client.$method($($args),*),
            LLMPrimitiveProvider::Aws(client) => client.$method($($args),*),
            LLMPrimitiveProvider::Vertex(client) => client.$method($($args),*),
        }
    };
}

impl WithRetryPolicy for LLMPrimitiveProvider {
    fn retry_policy_name(&self) -> Option<&str> {
        match_llm_provider!(self, retry_policy_name)
    }
}

impl WithClientProperties for LLMPrimitiveProvider {
    fn client_properties(&self) -> &std::collections::HashMap<String, serde_json::Value> {
        match_llm_provider!(self, client_properties)
    }
    fn allowed_metadata(&self) -> &super::AllowedMetadata {
        match_llm_provider!(self, allowed_metadata)
    }
}

impl TryFrom<(&ClientProperty, &RuntimeContext)> for LLMPrimitiveProvider {
    type Error = anyhow::Error;

    fn try_from((value, ctx): (&ClientProperty, &RuntimeContext)) -> Result<Self> {
        match value.provider.as_str() {
            "openai" => OpenAIClient::dynamic_new(value, ctx).map(LLMPrimitiveProvider::OpenAI),
            "azure-openai" => {
                OpenAIClient::dynamic_new_azure(value, ctx).map(LLMPrimitiveProvider::OpenAI)
            }
            "ollama" => {
                OpenAIClient::dynamic_new_ollama(value, ctx).map(LLMPrimitiveProvider::OpenAI)
            }
            "anthropic" => {
                AnthropicClient::dynamic_new(value, ctx).map(LLMPrimitiveProvider::Anthropic)
            }
            "google-ai" => {
                GoogleAIClient::dynamic_new(value, ctx).map(LLMPrimitiveProvider::Google)
            }
            "vertex-ai" => VertexClient::dynamic_new(value, ctx).map(LLMPrimitiveProvider::Vertex),
            // dynamic_new is not implemented for aws::AwsClient
            other => {
                let options = [
                    "openai",
                    "anthropic",
                    "ollama",
                    "google-ai",
                    "vertex-ai",
                    "azure-openai",
                    "fallback",
                    "round-robin",
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
            "google-ai" => GoogleAIClient::new(client, ctx).map(LLMPrimitiveProvider::Google),
            "aws-bedrock" => aws::AwsClient::new(client, ctx).map(LLMPrimitiveProvider::Aws),
            "vertex-ai" => VertexClient::new(client, ctx).map(LLMPrimitiveProvider::Vertex),
            other => {
                let options = [
                    "openai",
                    "anthropic",
                    "ollama",
                    "google-ai",
                    "vertex-ai",
                    "azure-openai",
                    "fallback",
                    "round-robin",
                    "aws-bedrock",
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
    async fn render_prompt(
        &'ir self,
        ir: &'ir IntermediateRepr,
        renderer: &PromptRenderer,
        ctx: &RuntimeContext,
        params: &BamlValue,
    ) -> Result<internal_baml_jinja::RenderedPrompt> {
        match_llm_provider!(self, render_prompt, async, ir, renderer, ctx, params)
    }
}

impl WithRenderRawCurl for LLMPrimitiveProvider {
    async fn render_raw_curl(
        &self,
        ctx: &RuntimeContext,
        prompt: &Vec<internal_baml_jinja::RenderedChatMessage>,
        render_settings: RenderCurlSettings,
    ) -> Result<String> {
        match_llm_provider!(self, render_raw_curl, async, ctx, prompt, render_settings)
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
    ) -> Result<OrchestratorNodeIterator> {
        Ok(vec![OrchestratorNode::new(
            ExecutionScope::Direct(self.name().to_string()),
            self.clone(),
        )])
    }
}

impl std::fmt::Display for LLMPrimitiveProvider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LLMPrimitiveProvider::OpenAI(_) => write!(f, "OpenAI"),
            LLMPrimitiveProvider::Anthropic(_) => write!(f, "Anthropic"),
            LLMPrimitiveProvider::Google(_) => write!(f, "Google"),
            LLMPrimitiveProvider::Aws(_) => write!(f, "AWS"),
            LLMPrimitiveProvider::Vertex(_) => write!(f, "Vertex"),
        }
    }
}

impl LLMPrimitiveProvider {
    pub fn name(&self) -> &str {
        &match_llm_provider!(self, context).name
    }

    pub fn request_options(&self) -> &std::collections::HashMap<String, serde_json::Value> {
        match_llm_provider!(self, request_options)
    }
}

use super::resolve_properties_walker;
