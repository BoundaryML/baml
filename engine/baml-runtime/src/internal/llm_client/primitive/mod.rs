use std::sync::Arc;

use anyhow::Result;
use baml_types::{tracing::events::HttpRequestId, BamlMap, BamlValue};
use internal_baml_core::ir::{repr::IntermediateRepr, ClientWalker};
use internal_llm_client::{AllowedRoleMetadata, ClientProvider, OpenAIClientProviderVariant};

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
mod stream_request;
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
#[derive(derive_more::From)]
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
    fn allowed_metadata(&self) -> &AllowedRoleMetadata {
        match_llm_provider!(self, allowed_metadata)
    }
    fn supports_streaming(&self) -> bool {
        match_llm_provider!(self, supports_streaming)
    }
    fn finish_reason_filter(&self) -> &internal_llm_client::FinishReasonFilter {
        match_llm_provider!(self, finish_reason_filter)
    }
    fn default_role(&self) -> String {
        match_llm_provider!(self, default_role)
    }
    fn allowed_roles(&self) -> Vec<String> {
        match_llm_provider!(self, allowed_roles)
    }
}

impl TryFrom<(&ClientProperty, &RuntimeContext)> for LLMPrimitiveProvider {
    type Error = anyhow::Error;

    fn try_from((value, ctx): (&ClientProperty, &RuntimeContext)) -> Result<Self> {
        match &value.provider {
            ClientProvider::OpenAI(open_aiclient_provider_variant) => {
                match open_aiclient_provider_variant {
                    OpenAIClientProviderVariant::Base => {
                        OpenAIClient::dynamic_new(value, ctx).map(Into::into)
                    }
                    OpenAIClientProviderVariant::Ollama => {
                        OpenAIClient::dynamic_new_ollama(value, ctx).map(Into::into)
                    }
                    OpenAIClientProviderVariant::Azure => {
                        OpenAIClient::dynamic_new_azure(value, ctx).map(Into::into)
                    }
                    OpenAIClientProviderVariant::Generic => {
                        OpenAIClient::dynamic_new_generic(value, ctx).map(Into::into)
                    }
                }
            }
            ClientProvider::Anthropic => AnthropicClient::dynamic_new(value, ctx).map(Into::into),
            ClientProvider::AwsBedrock => AwsClient::dynamic_new(value, ctx).map(Into::into),
            ClientProvider::GoogleAi => GoogleAIClient::dynamic_new(value, ctx).map(Into::into),
            ClientProvider::Vertex => VertexClient::dynamic_new(value, ctx).map(Into::into),
            ClientProvider::Strategy(strategy_client_provider) => {
                unimplemented!(
                    "Strategy client providers are not supported yet in LLMPrimitiveProvider"
                )
            } // "openai" => OpenAIClient::dynamic_new(value, ctx).map(Into::into),
              // "openai-generic" => OpenAIClient::dynamic_new_generic(value, ctx).map(Into::into),
              // "azure-openai" => OpenAIClient::dynamic_new_azure(value, ctx).map(Into::into),
              // "ollama" => OpenAIClient::dynamic_new_ollama(value, ctx).map(Into::into),
              // "anthropic" => AnthropicClient::dynamic_new(value, ctx).map(Into::into),
              // "google-ai" => GoogleAIClient::dynamic_new(value, ctx).map(Into::into),
              // "vertex-ai" => VertexClient::dynamic_new(value, ctx).map(Into::into),
              // // dynamic_new is not implemented for aws::AwsClient
              // other => {
              //     let options = [
              //         "anthropic",
              //         "azure-openai",
              //         "google-ai",
              //         "openai",
              //         "openai-generic",
              //         "vertex-ai",
              //         "fallback",
              //         "round-robin",
              //     ];
              //     anyhow::bail!(
              //         "Unsupported provider: {}. Available ones are: {}",
              //         other,
              //         options.join(", ")
              //     )
              // }
        }
    }
}

impl TryFrom<(&ClientWalker<'_>, &RuntimeContext)> for LLMPrimitiveProvider {
    type Error = anyhow::Error;

    fn try_from((client, ctx): (&ClientWalker, &RuntimeContext)) -> Result<Self> {
        match &client.elem().provider {
            ClientProvider::OpenAI(open_aiclient_provider_variant) => {
                match open_aiclient_provider_variant {
                    OpenAIClientProviderVariant::Base => {
                        OpenAIClient::new(client, ctx).map(Into::into)
                    }
                    OpenAIClientProviderVariant::Ollama => {
                        OpenAIClient::new_ollama(client, ctx).map(Into::into)
                    }
                    OpenAIClientProviderVariant::Azure => {
                        OpenAIClient::new_azure(client, ctx).map(Into::into)
                    }
                    OpenAIClientProviderVariant::Generic => {
                        OpenAIClient::new_generic(client, ctx).map(Into::into)
                    }
                }
            }
            ClientProvider::Anthropic => AnthropicClient::new(client, ctx).map(Into::into),
            ClientProvider::AwsBedrock => AwsClient::new(client, ctx).map(Into::into),
            ClientProvider::GoogleAi => GoogleAIClient::new(client, ctx).map(Into::into),
            ClientProvider::Vertex => VertexClient::new(client, ctx).map(Into::into),
            ClientProvider::Strategy(strategy_client_provider) => {
                unimplemented!(
                    "Strategy client providers are not supported yet in LLMPrimitiveProvider"
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
        prompt: &[internal_baml_jinja::RenderedChatMessage],
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
        http_request_id: HttpRequestId,
    ) -> LLMResponse {
        match_llm_provider!(self, single_call, async, ctx, prompt, http_request_id)
    }
}

impl WithStreamable for LLMPrimitiveProvider {
    async fn stream(
        &self,
        ctx: &RuntimeContext,
        prompt: &internal_baml_jinja::RenderedPrompt,
        http_request_id: HttpRequestId,
    ) -> super::traits::StreamResponse {
        match_llm_provider!(self, stream, async, ctx, prompt, http_request_id)
    }
}

impl IterOrchestrator for Arc<LLMPrimitiveProvider> {
    fn iter_orchestrator(
        &self,
        _state: &mut OrchestrationState,
        _previous: OrchestrationScope,
        _ctx: &RuntimeContext,
        _client_lookup: &dyn InternalClientLookup,
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

    pub fn provider_name(&self) -> &str {
        &match_llm_provider!(self, context).provider
    }

    pub fn request_options(&self) -> &BamlMap<String, serde_json::Value> {
        match_llm_provider!(self, request_options)
    }
}

#[cfg(test)]
mod tests {
    use crate::internal::llm_client::traits::WithClient;

    use super::request::RequestBuilder;
    use anyhow::Result;

    pub struct MockClient {
        model_features: crate::internal::llm_client::ModelFeatures,
        context: internal_baml_jinja::RenderContext_Client,
        request_options: baml_types::BamlMap<String, serde_json::Value>,
    }

    impl MockClient {
        pub fn new() -> Self {
            Self {
                model_features: crate::internal::llm_client::ModelFeatures {
                    completion: false,
                    chat: false,
                    max_one_system_prompt: false,
                    resolve_media_urls: crate::internal::llm_client::ResolveMediaUrls::Always,
                    allowed_metadata: crate::internal::llm_client::AllowedRoleMetadata::All,
                },
                context: internal_baml_jinja::RenderContext_Client {
                    name: "mock".to_string(),
                    provider: "mock".to_string(),
                    default_role: "user".to_string(),
                    allowed_roles: vec![],
                },
                request_options: baml_types::BamlMap::new(),
            }
        }
    }

    impl WithClient for MockClient {
        fn model_features(&self) -> &crate::internal::llm_client::ModelFeatures {
            &self.model_features
        }

        fn context(&self) -> &internal_baml_jinja::RenderContext_Client {
            &self.context
        }
    }

    impl RequestBuilder for MockClient {
        async fn build_request(
            &self,
            prompt: either::Either<&String, &[internal_baml_jinja::RenderedChatMessage]>,
            allow_proxy: bool,
            stream: bool,
            expose_secrets: bool,
        ) -> Result<reqwest::RequestBuilder> {
            unimplemented!("Not used in tests")
        }
    
        fn request_options(&self) -> &baml_types::BamlMap<String, serde_json::Value> {
            &self.request_options
        }
    
        fn http_client(&self) -> &reqwest::Client {
            unimplemented!("Not used in tests")
        }
    }
}