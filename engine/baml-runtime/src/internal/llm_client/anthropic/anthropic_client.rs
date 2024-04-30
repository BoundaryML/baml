use std::collections::HashMap;

use anyhow::Result;
use internal_baml_core::ir::{repr::Expression, ClientWalker, RetryPolicyWalker};
use internal_baml_jinja::{RenderContext_Client, RenderedChatMessage, RenderedPrompt};
use serde_json::json;

use crate::runtime::{
    llm_client::{LLMResponse, ModelFeatures},
    prompt_renderer::PromptRenderer,
};
use crate::RuntimeContext;

fn serialize(ctx: &RuntimeContext, expr: &Expression) -> Result<serde_json::Value> {
    // serde_json::Value::serialize(&self.into(), serializer)
    Ok(match expr {
        Expression::Identifier(idn) => match idn {
            internal_baml_core::ir::repr::Identifier::ENV(key) => {
                ctx.env.get(key).map_or(serde_json::Value::Null, |val| {
                    serde_json::Value::String(val.to_string())
                })
            }
            _ => serde_json::Value::String(idn.name()),
        },
        Expression::Bool(b) => serde_json::Value::Bool(*b),
        Expression::Numeric(n) => serde_json::Value::Number(n.parse().unwrap()),
        Expression::String(s) => serde_json::Value::String(s.clone()),
        Expression::RawString(s) => serde_json::Value::String(s.to_string()),
        Expression::List(items) => serde_json::Value::Array(
            items
                .iter()
                .map(|item| serialize(ctx, item))
                .collect::<Result<Vec<_>>>()?,
        ),
        Expression::Map(kv) => {
            let res = kv
                .iter()
                .map(|(k, v)| {
                    let k = match k {
                        Expression::String(s) => s.clone(),
                        _ => todo!(),
                    };
                    let v = serialize(ctx, v)?;
                    Ok((k, v))
                })
                .collect::<Result<HashMap<_, _>>>()?;
            json!(res)
        }
    })
}

pub struct AnthropicClient<'ir> {
    client: &'ir ClientWalker<'ir>,
    default_role: String,
    base_url: String,
    api_key: Option<String>,
    headers: HashMap<String, String>,

    // These are passed directly to the Anthropic API.
    properties: HashMap<&'ir str, serde_json::Value>,
}

impl<'ir> AnthropicClient<'ir> {
    pub fn new(
        client: &'ir ClientWalker<'ir>,
        ctx: &RuntimeContext,
    ) -> Result<AnthropicClient<'ir>> {
        todo!()
    }

    fn client_http_request(
        &self,
        path: &str,
        prompt: &Vec<RenderedChatMessage>,
    ) -> Result<reqwest::RequestBuilder> {
        todo!()
    }
}

impl LLMClientOrStrategy for AnthropicClient<'_> {
    fn render_prompt(
        &self,
        renderer: &PromptRenderer<'_>,
        ctx: &RuntimeContext,
        params: &serde_json::Value,
    ) -> Result<RenderedPrompt> {
        self.render_chat_prompt(renderer, ctx, params)
            .map(|v| RenderedPrompt::Chat(v))
    }

    async fn single_call(&self, prompt: &RenderedPrompt) -> Result<LLMResponse> {
        todo!()
    }

    fn retry_policy(&self) -> Option<RetryPolicyWalker> {
        self.client.retry_policy()
    }
}

impl LLMClient for AnthropicClient<'_> {
    fn context(&self) -> RenderContext_Client {
        RenderContext_Client {
            name: self.client.elem().name.clone(),
            provider: self.client.elem().provider.clone(),
        }
    }

    /// https://docs.anthropic.com/claude/docs/models-overview
    fn model_features(&self) -> ModelFeatures {
        if let Some(model) = self.properties.get("model") {
            if let serde_json::Value::String(model) = model {
                if model.contains("claude-2.1")
                    || model.contains("claude-2.0")
                    || model.contains("claude-instant-1.2")
                {
                    // The old Claude models support both chat and completion, but it's unclear how to model this in model_type
                    return ModelFeatures::Chat;
                }
            }
        }
        // newer models only support chat
        ModelFeatures::Chat
    }
}

impl LLMChatClient for AnthropicClient<'_> {
    fn default_role(&self) -> &str {
        &self.default_role
    }

    async fn chat(&self, messages: &Vec<RenderedChatMessage>) -> anyhow::Result<LLMResponse> {
        todo!()
    }
}
