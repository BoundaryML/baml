use std::sync::Arc;

use anyhow::Result;
use async_std::stream;
use baml_types::BamlValue;
use internal_baml_core::ir::{repr::IntermediateRepr, ClientWalker};

use crate::{
    internal::prompt_renderer::PromptRenderer, runtime_interface::InternalClientLookup,
    RuntimeContext,
};

use self::{
    anthropic::AnthropicClient, aws::AwsClient, google::GoogleClient, openai::OpenAIClient,
    request::RequestBuilder,
};

use super::{
    orchestrator::{
        ExecutionScope, IterOrchestrator, OrchestrationScope, OrchestrationState, OrchestratorNode,
        OrchestratorNodeIterator,
    },
    retry_policy::CallablePolicy,
    traits::{
        WithClient, WithPrompt, WithRenderRawCurl, WithRetryPolicy, WithSingleCallable,
        WithStreamable,
    },
    LLMResponse,
};

mod anthropic;
mod aws;
mod google;
mod openai;
pub(super) mod request;

// use crate::internal::llm_client::traits::ambassador_impl_WithRenderRawCurl;
// use crate::internal::llm_client::traits::ambassador_impl_WithRetryPolicy;
use ambassador::Delegate;
use enum_dispatch::enum_dispatch;

#[enum_dispatch(WithRetryPolicy)]
pub enum LLMPrimitive2 {
    OpenAIClient,
    AnthropicClient,
    GoogleClient,
    AwsClient,
}

// #[derive(Delegate)]
// #[delegate(WithRetryPolicy, WithRenderRawCurl)]
pub enum LLMPrimitiveProvider {
    OpenAI(OpenAIClient),
    Anthropic(AnthropicClient),
    Google(GoogleClient),
    Aws(aws::AwsClient),
}

// impl WithRetryPolicy for LLMPrimitiveProvider {
//     fn retry_policy_name(&self) -> Option<&str> {
//         match self {
//             LLMPrimitiveProvider::OpenAI(client) => {
//                 LLMPrimitive2::OpenAIClient(client).retry_policy_name()
//             }
//             LLMPrimitiveProvider::Anthropic(client) => {
//                 LLMPrimitive2::AnthropicClient(client).retry_policy_name()
//             }
//             LLMPrimitiveProvider::Google(client) => {
//                 LLMPrimitive2::GoogleClient(client).retry_policy_name()
//             }
//             LLMPrimitiveProvider::Aws(client) => {
//                 LLMPrimitive2::AwsClient(client).retry_policy_name()
//             }
//         }
//     }
// }

macro_rules! match_llm_provider {
    // Define the variants inside the macro
    ($self:expr, $method:ident, async $(, $args:tt)*) => {
        match $self {
            LLMPrimitiveProvider::OpenAI(client) => client.$method($($args),*).await,
            LLMPrimitiveProvider::Anthropic(client) => client.$method($($args),*).await,
            LLMPrimitiveProvider::Google(client) => client.$method($($args),*).await,
            LLMPrimitiveProvider::Aws(client) => client.$method($($args),*).await,
        }
    };

    ($self:expr, $method:ident $(, $args:tt)*) => {
        match $self {
            LLMPrimitiveProvider::OpenAI(client) => client.$method($($args),*),
            LLMPrimitiveProvider::Anthropic(client) => client.$method($($args),*),
            LLMPrimitiveProvider::Google(client) => client.$method($($args),*),
            LLMPrimitiveProvider::Aws(client) => client.$method($($args),*),
        }
    };
}

impl WithRetryPolicy for LLMPrimitiveProvider {
    fn retry_policy_name(&self) -> Option<&str> {
        match_llm_provider!(self, retry_policy_name)
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
            "google-ai" => GoogleClient::new(client, ctx).map(LLMPrimitiveProvider::Google),
            "aws-bedrock" => aws::AwsClient::new(client, ctx).map(LLMPrimitiveProvider::Aws),
            other => {
                let options = [
                    "openai",
                    "anthropic",
                    "ollama",
                    "google-ai",
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

impl WithRenderRawCurl for LLMPrimitiveProvider {
    async fn render_raw_curl(
        &self,
        ctx: &RuntimeContext,
        prompt: &Vec<internal_baml_jinja::RenderedChatMessage>,
        stream: bool,
    ) -> Result<String> {
        match_llm_provider!(self, render_raw_curl, async, ctx, prompt, stream)
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
            LLMPrimitiveProvider::Aws(_) => write!(f, "AWS"),
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
