use std::collections::HashMap;

use crate::ir_helpers::{ClientWalker, RuntimeContext};

use super::{LLMChatClient, LLMClient};
use anyhow::Result;
use internal_baml_core::ir::repr::Expression;
use internal_baml_jinja::{RenderContext_Client, RenderedChatMessage, RenderedPrompt};
use serde_json::json;

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

struct OpenAIClient<'ir> {
    client: ClientWalker<'ir>,
    default_role: String,
    base_url: String,
    api_key: Option<String>,

    // These are passed directly to the OpenAI API.
    properties: HashMap<&'ir str, serde_json::Value>,
}

impl<'ir> OpenAIClient<'ir> {
    pub fn new(client: ClientWalker<'ir>, ctx: &RuntimeContext) -> Result<OpenAIClient<'ir>> {
        // TODO: These properties should be validated before going into the IR.
        let properties = &client.item.elem;
        let mut properties = (&properties.options)
            .iter()
            .map(|(k, v)| Ok((k.as_str(), serialize(ctx, v)?)))
            .collect::<Result<HashMap<&str, serde_json::Value>>>()?;

        let default_role = properties
            .remove("default_role")
            .and_then(|v| v.as_str().map(|s| s.to_string()))
            .unwrap_or_else(|| "system".to_string());

        let base_url = properties
            .remove("base_url")
            .and_then(|v| v.as_str().map(|s| s.to_string()))
            .unwrap_or_else(|| "https://api.openai.com/v1".to_string());

        let api_key = properties
            .remove("api_key")
            .and_then(|v| v.as_str().map(|s| s.to_string()))
            .or_else(|| ctx.env.get("OPENAI_API_KEY").map(|s| s.to_string()));

        Ok(Self {
            client,
            base_url,
            api_key,
            default_role,
            properties,
        })
    }

    fn http_request(&self) -> Result<reqwest::Request> {
        todo!()
    }
}

impl LLMClient for OpenAIClient<'_> {
    fn context(&self) -> RenderContext_Client {
        RenderContext_Client {
            name: self.client.elem().name.clone(),
            provider: self.client.elem().provider.clone(),
        }
    }

    fn model_type(&self) -> super::ModelType {
        // Openai is always a chat model
        super::ModelType::Chat
    }

    fn render_prompt(
        &self,
        renderer: &crate::ir_helpers::PromptRenderer<'_>,
        ctx: &crate::ir_helpers::RuntimeContext,
        params: &serde_json::Value,
    ) -> Result<RenderedPrompt> {
        self.render_chat_prompt(renderer, ctx, params)
            .map(|v| RenderedPrompt::Chat(v))
    }

    async fn call(&self, prompt: RenderedPrompt) -> Result<super::LLMResponse> {
        match prompt {
            RenderedPrompt::Chat(messages) => self.chat(&messages).await,
            RenderedPrompt::Completion(_) => {
                anyhow::bail!("Completion prompts are not supported by this client")
            }
        }
    }
}

impl LLMChatClient for OpenAIClient<'_> {
    fn default_role(&self) -> &str {
        &self.default_role
    }

    async fn chat(
        &self,
        messages: &Vec<RenderedChatMessage>,
    ) -> anyhow::Result<super::LLMResponse> {
        todo!()
    }
}
