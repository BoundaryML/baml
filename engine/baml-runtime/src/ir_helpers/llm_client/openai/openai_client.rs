use std::collections::HashMap;

use crate::ir_helpers::{
    llm_client::{
        openai::types::{ChatCompletionResponse, FinishReason},
        LLMChatClient, LLMClient, LLMResponse, ModelType,
    },
    ClientWalker, LLMClientExt, RetryPolicyWalker, RuntimeContext,
};

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

pub struct OpenAIClient<'ir> {
    client: &'ir ClientWalker<'ir>,
    default_role: String,
    base_url: String,
    api_key: Option<String>,
    headers: HashMap<String, String>,

    // These are passed directly to the OpenAI API.
    properties: HashMap<&'ir str, serde_json::Value>,
}

impl<'ir> OpenAIClient<'ir> {
    pub fn new(client: &'ir ClientWalker<'ir>, ctx: &RuntimeContext) -> Result<OpenAIClient<'ir>> {
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
        // Remove trailing slashes.
        let base_url = base_url.trim_end_matches('/').to_string();

        let api_key = properties
            .remove("api_key")
            .and_then(|v| v.as_str().map(|s| s.to_string()))
            .or_else(|| ctx.env.get("OPENAI_API_KEY").map(|s| s.to_string()));

        let headers = properties.remove("headers").map(|v| {
            if let Some(v) = v.as_object() {
                v.iter()
                    .map(|(k, v)| {
                        Ok((
                            k.to_string(),
                            match v {
                                serde_json::Value::String(s) => s.to_string(),
                                _ => anyhow::bail!("Header '{k}' must be a string"),
                            },
                        ))
                    })
                    .collect::<Result<HashMap<String, String>>>()
            } else {
                Ok(Default::default())
            }
        });
        let headers = match headers {
            Some(h) => h?,
            None => Default::default(),
        };

        Ok(Self {
            client,
            base_url,
            api_key,
            headers,
            default_role,
            properties,
        })
    }

    fn client_http_request(
        &self,
        path: &str,
        prompt: &Vec<RenderedChatMessage>,
    ) -> Result<reqwest::RequestBuilder> {
        // TODO: ideally like to keep this alive longer.
        let client = reqwest::Client::new();

        let mut body = json!(self.properties);
        body.as_object_mut().unwrap().insert(
            "messages".into(),
            prompt
                .iter()
                .map(|m| {
                    json!({
                        "role": m.role,
                        "content": m.message,
                    })
                })
                .collect::<serde_json::Value>(),
        );

        let mut req = client
            .post(format!("{}{}", self.base_url, path))
            .json(&body);
        match self.api_key {
            Some(ref key) => {
                req = req.header("Authorization", format!("Bearer {}", key));
            }
            None => {}
        }
        for (k, v) in &self.headers {
            req = req.header(k, v);
        }

        // Add all the properties as data parameters.
        Ok(req)
    }
}

impl LLMClientExt for OpenAIClient<'_> {
    fn render_prompt(
        &self,
        renderer: &crate::ir_helpers::PromptRenderer<'_>,
        ctx: &crate::ir_helpers::RuntimeContext,
        params: &serde_json::Value,
    ) -> Result<RenderedPrompt> {
        self.render_chat_prompt(renderer, ctx, params)
            .map(|v| RenderedPrompt::Chat(v))
    }

    async fn single_call(&self, prompt: &RenderedPrompt) -> Result<LLMResponse> {
        match prompt {
            RenderedPrompt::Chat(messages) => self.chat(messages).await,
            RenderedPrompt::Completion(_) => {
                anyhow::bail!("Completion prompts are not supported by this client")
            }
        }
    }

    fn retry_policy(&self) -> Option<RetryPolicyWalker> {
        self.client.retry_policy()
    }
}

impl LLMClient for OpenAIClient<'_> {
    fn context(&self) -> RenderContext_Client {
        RenderContext_Client {
            name: self.client.elem().name.clone(),
            provider: self.client.elem().provider.clone(),
        }
    }

    fn model_type(&self) -> ModelType {
        // Openai is always a chat model
        ModelType::Chat
    }
}

impl LLMChatClient for OpenAIClient<'_> {
    fn default_role(&self) -> &str {
        &self.default_role
    }

    async fn chat(&self, messages: &Vec<RenderedChatMessage>) -> anyhow::Result<LLMResponse> {
        let now = std::time::SystemTime::now();
        let req = self.client_http_request("/chat/completions", messages)?;
        let res = req.send().await?;

        // Raise for status.
        let res = res.error_for_status()?;

        let body = res.json::<ChatCompletionResponse>().await?;

        if body.choices.len() < 1 {
            anyhow::bail!(
                "Expected exactly one response from OpenAI, got 0.\n{:?}",
                body
            );
        }

        let usage = body.usage.as_ref();

        Ok(LLMResponse {
            content: body.choices[0]
                .message
                .content
                .as_ref()
                .map_or("", |s| s.as_str())
                .to_string(),
            start_time_unix_ms: now
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_millis() as u64,
            latency_ms: now.elapsed().unwrap().as_millis() as u64,
            model: body.model,
            metadata: json!({
                "baml_is_complete": match body.choices[0].finish_reason {
                    None => true,
                    Some(FinishReason::Stop) => true,
                    _ => false,
                },
                "finish_reason": body.choices[0].finish_reason,
                "prompt_tokens": usage.map(|u| u.prompt_tokens),
                "output_tokens": usage.map(|u| u.completion_tokens),
                "total_tokens": usage.map(|u| u.total_tokens),
            }),
        })
    }
}
