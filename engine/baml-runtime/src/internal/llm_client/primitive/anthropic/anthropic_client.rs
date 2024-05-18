use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use anyhow::{anyhow, Context, Result};
use baml_types::BamlImage;
use internal_baml_core::ir::ClientWalker;
use internal_baml_jinja::{ChatMessagePart, RenderContext_Client, RenderedChatMessage};

use crate::internal::llm_client::{
    primitive::anthropic::types::{AnthropicErrorResponse, AnthropicMessageResponse, StopReason},
    state::LlmClientState,
    traits::{WithChat, WithClient, WithNoCompletion, WithRetryPolicy},
    LLMResponse, ModelFeatures,
};
use serde_json::{json, Value};

use crate::RuntimeContext;

struct PostRequestProperities {
    default_role: String,
    base_url: String,
    api_key: Option<String>,
    headers: HashMap<String, String>,

    // These are passed directly to the Anthropic API.
    properties: HashMap<String, serde_json::Value>,
}

pub struct AnthropicClient {
    pub name: String,
    retry_policy: Option<String>,
    context: RenderContext_Client,
    features: ModelFeatures,
    properties: PostRequestProperities,

    internal_state: Arc<Mutex<LlmClientState>>,
}

fn resolve_properties(
    client: &ClientWalker,
    ctx: &RuntimeContext,
) -> Result<PostRequestProperities> {
    let mut properties = (&client.item.elem.options)
        .iter()
        .map(|(k, v)| {
            Ok((
                k.into(),
                ctx.resolve_expression::<serde_json::Value>(v)
                    .context(format!(
                        "client {} could not resolve options.{}",
                        client.name(),
                        k
                    ))?,
            ))
        })
        .collect::<Result<HashMap<_, _>>>()?;
    // this is a required field
    properties
        .entry("max_tokens".into())
        .or_insert_with(|| 4096.into());

    let default_role = properties
        .remove("default_role")
        .and_then(|v| v.as_str().map(|s| s.to_string()))
        .unwrap_or_else(|| "system".to_string());

    let base_url = properties
        .remove("base_url")
        .and_then(|v| v.as_str().map(|s| s.to_string()))
        .or_else(|| {
            ctx.env
                .get("BOUNDARY_ANTHROPIC_PROXY_URL")
                .map(|s| s.to_string())
        })
        .unwrap_or_else(|| "https://api.anthropic.com".to_string());

    let api_key = properties
        .remove("api_key")
        .and_then(|v| v.as_str().map(|s| s.to_string()))
        .or_else(|| ctx.env.get("ANTHROPIC_API_KEY").map(|s| s.to_string()));

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

    let mut headers = match headers {
        Some(h) => h?,
        None => Default::default(),
    };

    headers
        .entry("anthropic-version".to_string())
        .or_insert("2023-06-01".to_string());

    Ok(PostRequestProperities {
        default_role,
        base_url,
        api_key,
        headers,
        properties,
    })
}

impl WithRetryPolicy for AnthropicClient {
    fn retry_policy_name(&self) -> Option<&str> {
        self.retry_policy.as_deref()
    }
}

impl WithClient for AnthropicClient {
    fn context(&self) -> &RenderContext_Client {
        &self.context
    }

    fn model_features(&self) -> &ModelFeatures {
        &self.features
    }
}

impl WithNoCompletion for AnthropicClient {}

impl AnthropicClient {
    pub fn new(client: &ClientWalker, ctx: &RuntimeContext) -> Result<AnthropicClient> {
        Ok(Self {
            name: client.name().into(),
            properties: resolve_properties(client, ctx)?,
            context: RenderContext_Client {
                name: client.name().into(),
                provider: client.elem().provider.clone(),
            },
            features: ModelFeatures {
                chat: true,
                completion: false,
            },
            retry_policy: client
                .elem()
                .retry_policy_id
                .as_ref()
                .map(|s| s.to_string()),
            internal_state: Arc::new(Mutex::new(LlmClientState::new())),
        })
    }
}

impl WithChat for AnthropicClient {
    fn chat_options(&self, _ctx: &RuntimeContext) -> Result<internal_baml_jinja::ChatOptions> {
        Ok(internal_baml_jinja::ChatOptions::new(
            self.properties.default_role.clone(),
            None,
        ))
    }

    async fn chat(
        &self,
        _ctx: &RuntimeContext,
        prompt: &Vec<RenderedChatMessage>,
    ) -> Result<LLMResponse> {
        use crate::{
            internal::llm_client::{ErrorCode, LLMCompleteResponse, LLMErrorResponse},
            request::{self, RequestError},
        };

        let mut body = json!(self.properties.properties);
        body.as_object_mut()
            .unwrap()
            .extend(build_anthropic_chat_request(prompt)?.into_iter());

        let mut headers = HashMap::default();
        match &self.properties.api_key {
            Some(key) => {
                headers.insert("x-api-key".to_string(), key.to_string());
            }
            None => {}
        }
        for (k, v) in &self.properties.headers {
            headers.insert(k.to_string(), v.to_string());
        }

        let now = web_time::SystemTime::now();
        match request::call_request_with_json::<AnthropicMessageResponse, _>(
            &format!("{}{}", self.properties.base_url, "/v1/messages"),
            &body,
            Some(headers),
        )
        .await
        {
            Ok(body) => {
                if body.content.len() < 1 {
                    return Ok(LLMResponse::LLMFailure(LLMErrorResponse {
                        client: self.context.name.to_string(),
                        model: None,
                        prompt: internal_baml_jinja::RenderedPrompt::Chat(prompt.clone()),
                        start_time_unix_ms: now
                            .duration_since(web_time::UNIX_EPOCH)
                            .unwrap()
                            .as_millis() as u64,
                        latency_ms: now.elapsed().unwrap().as_millis() as u64,
                        message: format!("No content in response:\n{:#?}", body),
                        code: ErrorCode::Other(200),
                    }));
                }

                let usage = body.usage;

                Ok(LLMResponse::Success(LLMCompleteResponse {
                    client: self.context.name.to_string(),
                    prompt: internal_baml_jinja::RenderedPrompt::Chat(prompt.clone()),
                    content: body.content[0].text.clone(),
                    start_time_unix_ms: now
                        .duration_since(web_time::UNIX_EPOCH)
                        .unwrap()
                        .as_millis() as u64,
                    latency_ms: now.elapsed().unwrap().as_millis() as u64,
                    model: body.model,
                    metadata: json!({
                        "baml_is_complete": match body.stop_reason {
                            None => true,
                            Some(StopReason::STOP_SEQUENCE) => true,
                            Some(StopReason::END_TURN)  => true,
                            _ => false,
                        },
                        "finish_reason": body.stop_reason,
                        "prompt_tokens": usage.input_tokens,
                        "output_tokens": usage.output_tokens,
                        "total_tokens": usage.input_tokens + usage.output_tokens,
                    }),
                }))
            }
            Err(e) => match e {
                RequestError::BuildError(e)
                | RequestError::FetchError(e)
                | RequestError::JsonError(e)
                | RequestError::SerdeError(e) => {
                    Err(anyhow::anyhow!("Failed to make request: {:#?}", e))
                }
                RequestError::ResponseError(status, res) => {
                    match request::response_json::<AnthropicErrorResponse>(res).await {
                        Ok(err) => {
                            let err_message =
                                format!("API Error ({}): {}", err.error.r#type, err.error.message);
                            Ok(LLMResponse::LLMFailure(LLMErrorResponse {
                                client: self.context.name.to_string(),
                                model: None,
                                prompt: internal_baml_jinja::RenderedPrompt::Chat(prompt.clone()),
                                start_time_unix_ms: now
                                    .duration_since(web_time::UNIX_EPOCH)
                                    .unwrap()
                                    .as_millis()
                                    as u64,
                                latency_ms: now.elapsed().unwrap().as_millis() as u64,
                                message: err_message,
                                code: ErrorCode::from_u16(status),
                            }))
                        }
                        Err(e) => {
                            anyhow::bail!(
                                "Does this support the Anthropic Response type?\n{:#?}",
                                e
                            )
                        }
                    }
                }
            },
        }
    }
}

fn build_anthropic_chat_request(
    prompt: &Vec<RenderedChatMessage>,
) -> Result<serde_json::Map<String, Value>> {
    // ensure the number of system messages is <= 1.
    // if there is a system message in the prompt, extract it out of the list and put it in the top-level "system" property of the request.
    let (system, messages): (Vec<&RenderedChatMessage>, Vec<&RenderedChatMessage>) =
        prompt.iter().partition(|m| m.role == "system");

    if system.len() > 1 {
        return Err(anyhow!("Too many system messages in prompt"));
    }

    let mut request = json!({
        "messages": messages
            .iter()
            .map(|m| {
                // Use `convert_message_parts_to_content` with `?` to handle errors
                match convert_message_parts_to_content(&m.parts) {
                    Ok(content) => Ok(json!({
                        "role": m.role,
                        "content": content,
                    })),
                    Err(err) => return Err(err),
                }
            })
            .collect::<Result<Vec<Value>>>()?, // Collect results, propagate error if any
    });

    if let Some(system_message) = system.first() {
        let system_content = convert_system_message_parts_to_text(&system_message.parts)?;
        request["system"] = json!(system_content);
    }

    match request {
        Value::Object(obj) => Ok(obj),
        _ => unreachable!(),
    }
}

fn convert_message_parts_to_content(parts: &Vec<ChatMessagePart>) -> Result<Vec<Value>> {
    let content: Vec<Value> = parts
        .iter()
        .map(|part| match part {
            ChatMessagePart::Text(text) => Ok(json!({
                "type": "text",
                "text": text
            })),
            ChatMessagePart::Image(image) => match image {
                // internal_baml_jinja::BamlImage::Url(image_url) => {
                //     let rt = tokio::runtime::Builder::new_current_thread()
                //         .enable_all()
                //         .build()?;

                //     let (media_type, base64_data) = rt
                //         .block_on(download_image_as_base64(&image_url.url))
                //         .map_err(|e| anyhow!("Failed to fetch image: {}", e))?;
                //     Ok(json!({
                //         "type": "image",
                //         "source": {
                //             "type": "base64",
                //             "media_type": media_type,
                //             "data": base64_data
                //         }
                //     }))
                // }
                BamlImage::Base64(image) => Ok(json!({
                    "type": "image",
                    "source": {
                        "type": "base64",
                        "media_type": image.media_type,
                        "data": image.base64
                    }
                })),
                _ => Err(anyhow!("Unsupported image type")),
            },
        })
        .collect::<Result<Vec<Value>>>()?;

    Ok(content)
}

fn convert_system_message_parts_to_text(parts: &Vec<ChatMessagePart>) -> Result<String> {
    if parts.len() != 1 {
        return Err(anyhow!("System message must contain exactly one text part, but we detected a file-type in the system message."));
    }

    match &parts[0] {
        ChatMessagePart::Text(text) => Ok(text.clone()),
        _ => Err(anyhow!("System message contains non-text parts")),
    }
}
