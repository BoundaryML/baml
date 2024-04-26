use std::collections::HashMap;

use anyhow::Result;
use async_trait::async_trait;
use internal_baml_core::ir::repr::IntermediateRepr;
use internal_baml_core::ir::{ClientWalker, RetryPolicyWalker};
use internal_baml_jinja::{CompletionOptions, RenderContext_Client, RenderedChatMessage};
use serde_json::json;

use crate::runtime::llm_client::expression_helper::to_value;
use crate::runtime::llm_client::traits::{
    WithChat, WithClient, WithCompletion, WithNoCompletion, WithPrompt, WithRetryPolicy,
};
use crate::runtime::llm_client::{openai::types::FinishReason, LLMResponse, ModelFeatures};
use crate::RuntimeContext;

use super::types::ChatCompletionResponse;

struct PostRequestProperities<'ir> {
    default_role: String,
    base_url: String,
    api_key: Option<String>,
    headers: HashMap<String, String>,

    // These are passed directly to the OpenAI API.
    properties: HashMap<&'ir str, serde_json::Value>,
}

pub struct OpenAIClient<'ir> {
    client: &'ir ClientWalker<'ir>,
    properties: Option<PostRequestProperities<'ir>>,
    context: RenderContext_Client,
    features: Option<ModelFeatures>,
}

impl WithRetryPolicy for OpenAIClient<'_> {
    fn retry_policy<'ir>(&self, ir: &'ir IntermediateRepr) -> Option<RetryPolicyWalker<'ir>> {
        self.client
            .retry_policy()
            .as_ref()
            .and_then(|policy| ir.walk_retry_policies().find(|r| r.name() == policy))
    }
}

impl WithClient for OpenAIClient<'_> {
    fn context(&mut self) -> &RenderContext_Client {
        &self.context
    }

    fn model_features(&mut self, ctx: &RuntimeContext) -> Result<&ModelFeatures> {
        match self.features {
            Some(ref f) => Ok(f),
            None => {
                let properties = self.load_properties(&self.client, ctx)?;
                self.features = Some(ModelFeatures {
                    chat: true,
                    completion: false,
                });

                Ok(self.features.as_ref().unwrap())
            }
        }
    }
}

impl WithNoCompletion for OpenAIClient<'_> {}

impl WithChat for OpenAIClient<'_> {
    fn chat_options(&mut self) -> Result<internal_baml_jinja::ChatOptions> {
        Ok(internal_baml_jinja::ChatOptions::new("system".into(), None))
    }

    async fn chat(
        &mut self,
        ctx: &RuntimeContext,
        prompt: &Vec<RenderedChatMessage>,
    ) -> Result<LLMResponse> {
        let req = self.client_http_request(ctx, "/chat/completions", prompt)?;

        let now = std::time::SystemTime::now();
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

impl<'ir> OpenAIClient<'ir> {
    pub fn load_properties(
        &mut self,
        client: &'ir ClientWalker<'ir>,
        ctx: &RuntimeContext,
    ) -> Result<&PostRequestProperities<'ir>> {
        match self.properties {
            Some(ref p) => Ok(p),
            None => {
                let mut properties = (&client.item.elem.options)
                    .iter()
                    .map(|(k, v)| Ok((k.as_str(), to_value(ctx, v)?)))
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

                let properties = PostRequestProperities {
                    default_role,
                    base_url,
                    api_key,
                    headers,
                    properties,
                };

                self.properties = Some(properties);

                Ok(self.properties.as_ref().unwrap())
            }
        }
    }

    pub fn new(client: &'ir ClientWalker<'ir>, _: &RuntimeContext) -> Result<OpenAIClient<'ir>> {
        Ok(Self {
            client,
            properties: None,
            context: RenderContext_Client {
                name: client.name().into(),
                provider: client.elem().provider.clone(),
            },
            features: None,
        })
    }

    fn client_http_request(
        &mut self,
        ctx: &RuntimeContext,
        path: &str,
        prompt: &Vec<RenderedChatMessage>,
    ) -> Result<reqwest::RequestBuilder> {
        let properties = self.load_properties(&self.client, ctx)?;

        // TODO: ideally like to keep this alive longer.
        let client = reqwest::Client::new();

        let mut body = json!(properties.properties);
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
            .post(format!("{}{}", properties.base_url, path))
            .json(&body);
        match properties.api_key {
            Some(ref key) => {
                req = req.header("Authorization", format!("Bearer {}", key));
            }
            None => {}
        }
        for (k, v) in &properties.headers {
            req = req.header(k, v);
        }

        // Add all the properties as data parameters.
        Ok(req)
    }
}
