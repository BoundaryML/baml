use anyhow::Result;
use baml_types::BamlValue;
use internal_baml_core::ir::ClientWalker;

use crate::RuntimeContext;

use super::{
    super::prompt_renderer::PromptRenderer,
    anthropic::AnthropicClient,
    openai::OpenAIClient,
    retry_policy::CallablePolicy,
    roundrobin::{roundrobin_client::FnGetClientConfig, RoundRobinClient},
    traits::{WithCallable, WithPrompt, WithRetryPolicy, WithStreamable},
    FunctionResultStream, LLMResponse, LLMResponse,
};

pub enum LLMProvider {
    OpenAI(OpenAIClient),
    Anthropic(AnthropicClient),
    RoundRobin(RoundRobinClient),
    // Fallback(FallbackClient),
}

impl LLMProvider {
    pub fn from_ir(
        client: &ClientWalker,
        ctx: &RuntimeContext,
        get_client_config_cb: FnGetClientConfig,
    ) -> Result<LLMProvider> {
        // figure out if its roundrobin, later figure out fallback client
        match client.elem().provider.as_str() {
            "baml-openai-chat" | "openai" => {
                OpenAIClient::new(client, ctx).map(LLMProvider::OpenAI)
            }
            "baml-anthropic-chat" | "anthropic" => {
                AnthropicClient::new(client, ctx).map(LLMProvider::Anthropic)
            }
            "baml-ollama-chat" | "ollama" => {
                OpenAIClient::new(client, ctx).map(LLMProvider::OpenAI)
            }
            "baml-roundrobin" | "roundrobin" => {
                RoundRobinClient::new(client, ctx, get_client_config_cb)
                    .map(LLMProvider::RoundRobin)
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

impl<'ir> WithPrompt<'ir> for LLMProvider {
    fn render_prompt(
        &'ir self,
        renderer: &PromptRenderer,
        ctx: &RuntimeContext,
        params: &BamlValue,
    ) -> Result<internal_baml_jinja::RenderedPrompt> {
        match self {
            LLMProvider::OpenAI(client) => client.render_prompt(renderer, ctx, params),
            LLMProvider::Anthropic(client) => client.render_prompt(renderer, ctx, params),
            LLMProvider::RoundRobin(client) => client.render_prompt(renderer, ctx, params),
        }
    }
}

impl WithRetryPolicy for LLMProvider {
    fn retry_policy_name(&self) -> Option<&str> {
        match self {
            LLMProvider::OpenAI(client) => client.retry_policy_name(),
            LLMProvider::Anthropic(client) => client.retry_policy_name(),
            LLMProvider::RoundRobin(client) => client.retry_policy_name(),
        }
    }
}

impl WithCallable for LLMProvider {
    async fn call(
        &self,
        retry_policy: Option<CallablePolicy>,
        ctx: &RuntimeContext,
        prompt: &PromptRenderer,
        baml_args: &BamlArgType,
    ) -> LLMResponse {
        match self {
            LLMProvider::OpenAI(client) => client.call(retry_policy, ctx, prompt, baml_args).await,
            LLMProvider::Anthropic(client) => {
                client.call(retry_policy, ctx, prompt, baml_args).await
            }
            LLMProvider::RoundRobin(client) => {
                client.call(retry_policy, ctx, prompt, baml_args).await
            }
        }
    }
}

impl WithStreamable for LLMProvider {
    async fn stream(
        &self,
        _retry_policy: Option<CallablePolicy>,
        _ctx: &RuntimeContext,
        _prompt: &internal_baml_jinja::RenderedPrompt,
    ) -> FunctionResultStream {
        todo!()
    }
}
