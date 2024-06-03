use std::{
    collections::HashMap,
    fmt::format,
    sync::{Arc, Mutex},
};

use anyhow::{Context, Result};
use baml_types::BamlImage;
use eventsource_stream::Eventsource;
use futures::{SinkExt, StreamExt};
use internal_baml_core::ir::ClientWalker;
use internal_baml_jinja::{
    ChatMessagePart, RenderContext_Client, RenderedChatMessage, RenderedPrompt,
};
use reqwest::Response;

use crate::{
    internal::llm_client::{
        primitive::{
            anthropic::types::{AnthropicErrorResponse, AnthropicMessageResponse, StopReason},
            request::{make_parsed_request, make_request, RequestBuilder},
        },
        traits::{
            SseResponseTrait, StreamResponse, WithChat, WithClient, WithNoCompletion,
            WithRetryPolicy, WithStreamChat,
        },
        ErrorCode, LLMCompleteResponse, LLMCompleteResponseMetadata, LLMErrorResponse, LLMResponse,
        ModelFeatures,
    },
    request::create_client,
};
use serde_json::json;

use crate::RuntimeContext;

use super::types::MessageChunk;

struct PostRequestProperities {
    default_role: String,
    base_url: String,
    api_key: Option<String>,
    headers: HashMap<String, String>,
    proxy_url: Option<String>,
    // These are passed directly to the Anthropic API.
    properties: HashMap<String, serde_json::Value>,
}

pub struct AnthropicClient {
    pub name: String,
    retry_policy: Option<String>,
    context: RenderContext_Client,
    features: ModelFeatures,
    properties: PostRequestProperities,

    // clients
    client: reqwest::Client,
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
        proxy_url: ctx
            .env
            .get("BOUNDARY_ANTHROPIC_PROXY_URL")
            .map(|s| s.to_string()),
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

impl SseResponseTrait for AnthropicClient {
    fn response_stream(
        &self,
        resp: reqwest::Response,
        prompt: &Vec<RenderedChatMessage>,
        system_start: web_time::SystemTime,
        instant_start: web_time::Instant,
    ) -> StreamResponse {
        let prompt = prompt.clone();
        let client_name = self.context.name.clone();
        let params = self.properties.properties.clone();

        Ok(Box::pin(
            resp.bytes_stream()
                .inspect(|event| log::trace!("anthropic event bytes: {:#?}", event))
                .eventsource()
                .map(|event| -> Result<MessageChunk> { Ok(serde_json::from_str(&event?.data)?) })
                .inspect(|event| log::trace!("anthropic eventsource: {:#?}", event))
                .scan(
                    Ok(LLMCompleteResponse {
                        client: client_name.clone(),
                        prompt: RenderedPrompt::Chat(prompt.clone()),
                        content: "".to_string(),
                        start_time: system_start,
                        latency: instant_start.elapsed(),
                        model: "".to_string(),
                        invocation_params: params.clone(),
                        metadata: LLMCompleteResponseMetadata {
                            baml_is_complete: false,
                            finish_reason: None,
                            prompt_tokens: None,
                            output_tokens: None,
                            total_tokens: None,
                        },
                    }),
                    move |accumulated: &mut Result<LLMCompleteResponse>, event| {
                        let Ok(ref mut inner) = accumulated else {
                            return std::future::ready(None);
                        };
                        let event = match event {
                            Ok(event) => event,
                            Err(e) => {
                                return std::future::ready(Some(LLMResponse::LLMFailure(
                                    LLMErrorResponse {
                                        client: client_name.clone(),
                                        model: if inner.model == "" {
                                            None
                                        } else {
                                            Some(inner.model.clone())
                                        },
                                        prompt: internal_baml_jinja::RenderedPrompt::Chat(
                                            prompt.clone(),
                                        ),
                                        invocation_params: params.clone(),
                                        start_time: system_start,
                                        latency: instant_start.elapsed(),
                                        message: format!("Failed to parse event: {:#?}", e),
                                        code: ErrorCode::Other(2),
                                    },
                                )));
                            }
                        };
                        match event {
                            MessageChunk::MessageStart(chunk) => {
                                let body = chunk.message;
                                inner.model = body.model;
                                let ref mut inner = inner.metadata;
                                inner.baml_is_complete = match body.stop_reason {
                                    Some(StopReason::StopSequence) | Some(StopReason::EndTurn) => {
                                        true
                                    }
                                    _ => false,
                                };
                                inner.finish_reason =
                                    body.stop_reason.as_ref().map(ToString::to_string);
                                inner.prompt_tokens = Some(body.usage.input_tokens);
                                inner.output_tokens = Some(body.usage.output_tokens);
                                inner.total_tokens =
                                    Some(body.usage.input_tokens + body.usage.output_tokens);
                            }
                            MessageChunk::ContentBlockDelta(event) => {
                                inner.content += &event.delta.text;
                            }
                            MessageChunk::ContentBlockStart(_) => (),
                            MessageChunk::ContentBlockStop(_) => (),
                            MessageChunk::Ping => (),
                            MessageChunk::MessageDelta(body) => {
                                let ref mut inner = inner.metadata;

                                inner.baml_is_complete = match body.delta.stop_reason {
                                    Some(StopReason::StopSequence) | Some(StopReason::EndTurn) => {
                                        true
                                    }
                                    _ => false,
                                };
                                inner.finish_reason = body
                                    .delta
                                    .stop_reason
                                    .as_ref()
                                    .map(|r| serde_json::to_string(r).unwrap_or("".into()));
                                inner.output_tokens = Some(body.usage.output_tokens);
                                inner.total_tokens = Some(
                                    inner.prompt_tokens.unwrap_or(0) + body.usage.output_tokens,
                                );
                            }
                            MessageChunk::MessageStop => (),
                            MessageChunk::Error(err) => {
                                return std::future::ready(Some(LLMResponse::LLMFailure(
                                    LLMErrorResponse {
                                        client: client_name.clone(),
                                        model: if inner.model == "" {
                                            None
                                        } else {
                                            Some(inner.model.clone())
                                        },
                                        prompt: internal_baml_jinja::RenderedPrompt::Chat(
                                            prompt.clone(),
                                        ),
                                        invocation_params: params.clone(),
                                        start_time: system_start,
                                        latency: instant_start.elapsed(),
                                        message: err.message,
                                        code: ErrorCode::Other(2),
                                    },
                                )));
                            }
                        };

                        inner.latency = instant_start.elapsed();
                        std::future::ready(Some(LLMResponse::Success(inner.clone())))
                    },
                ),
        ))
    }
}

impl WithStreamChat for AnthropicClient {
    async fn stream_chat(
        &self,
        ctx: &RuntimeContext,
        prompt: &Vec<RenderedChatMessage>,
    ) -> StreamResponse {
        let (response, system_now, instant_now) =
            match make_request(self, either::Either::Right(prompt), true).await {
                Ok(v) => v,
                Err(e) => return Err(e),
            };
        self.response_stream(response, prompt, system_now, instant_now)
    }
}

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
                anthropic_system_constraints: true,
            },
            retry_policy: client
                .elem()
                .retry_policy_id
                .as_ref()
                .map(|s| s.to_string()),
            client: create_client()?,
        })
    }
}

impl RequestBuilder for AnthropicClient {
    fn http_client(&self) -> &reqwest::Client {
        &self.client
    }

    fn build_request(
        &self,
        prompt: either::Either<&String, &Vec<RenderedChatMessage>>,
        stream: bool,
    ) -> reqwest::RequestBuilder {
        let mut req = self.client.post(if prompt.is_left() {
            format!(
                "{}/v1/complete",
                self.properties
                    .proxy_url
                    .as_ref()
                    .unwrap_or(&self.properties.base_url)
                    .clone()
            )
        } else {
            format!(
                "{}/v1/messages",
                self.properties
                    .proxy_url
                    .as_ref()
                    .unwrap_or(&self.properties.base_url)
                    .clone()
            )
        });

        for (key, value) in &self.properties.headers {
            req = req.header(key, value);
        }
        if let Some(key) = &self.properties.api_key {
            req = req.header("x-api-key", key);
        }

        req = req.header("original-url", self.properties.base_url.as_str());

        let mut body = json!(self.properties.properties);
        let body_obj = body.as_object_mut().unwrap();
        match prompt {
            either::Either::Left(prompt) => {
                body_obj.extend(convert_completion_prompt_to_body(prompt))
            }
            either::Either::Right(messages) => {
                body_obj.extend(convert_chat_prompt_to_body(messages));
            }
        }

        if stream {
            body_obj.insert("stream".into(), true.into());
        }

        req.json(&body)
    }

    fn invocation_params(&self) -> &HashMap<String, serde_json::Value> {
        &self.properties.properties
    }
}

impl WithChat for AnthropicClient {
    fn chat_options(&self, _ctx: &RuntimeContext) -> Result<internal_baml_jinja::ChatOptions> {
        Ok(internal_baml_jinja::ChatOptions::new(
            self.properties.default_role.clone(),
            None,
        ))
    }

    async fn chat(&self, _ctx: &RuntimeContext, prompt: &Vec<RenderedChatMessage>) -> LLMResponse {
        let (response, system_now, instant_now) = match make_parsed_request::<
            AnthropicMessageResponse,
        >(
            self, either::Either::Right(prompt), false
        )
        .await
        {
            Ok(v) => v,
            Err(e) => return e,
        };

        if response.content.len() != 1 {
            return LLMResponse::LLMFailure(LLMErrorResponse {
                client: self.context.name.to_string(),
                model: None,
                prompt: internal_baml_jinja::RenderedPrompt::Chat(prompt.clone()),
                start_time: system_now,
                invocation_params: self.properties.properties.clone(),
                latency: instant_now.elapsed(),
                message: format!(
                    "Expected exactly one content block, got {}",
                    response.content.len()
                ),
                code: ErrorCode::Other(200),
            });
        }

        LLMResponse::Success(LLMCompleteResponse {
            client: self.context.name.to_string(),
            prompt: internal_baml_jinja::RenderedPrompt::Chat(prompt.clone()),
            content: response.content[0].text.clone(),
            start_time: system_now,
            latency: instant_now.elapsed(),
            invocation_params: self.properties.properties.clone(),
            model: response.model,
            metadata: LLMCompleteResponseMetadata {
                baml_is_complete: match response.stop_reason {
                    Some(StopReason::StopSequence) | Some(StopReason::EndTurn) => true,
                    _ => false,
                },
                finish_reason: response
                    .stop_reason
                    .as_ref()
                    .map(|r| serde_json::to_string(r).unwrap_or("".into())),
                prompt_tokens: Some(response.usage.input_tokens),
                output_tokens: Some(response.usage.output_tokens),
                total_tokens: Some(response.usage.input_tokens + response.usage.output_tokens),
            },
        })
    }
}

fn convert_completion_prompt_to_body(prompt: &String) -> HashMap<String, serde_json::Value> {
    let mut map = HashMap::new();
    map.insert("prompt".into(), json!(prompt));
    map
}

fn convert_chat_prompt_to_body(
    prompt: &Vec<RenderedChatMessage>,
) -> HashMap<String, serde_json::Value> {
    let mut map = HashMap::new();
    log::debug!("converting chat prompt to body: {:#?}", prompt);

    if let Some(first) = prompt.get(0) {
        if first.role == "system" {
            map.insert(
                "system".into(),
                convert_message_parts_to_content(&first.parts),
            );
            map.insert(
                "messages".into(),
                prompt
                    .iter()
                    .skip(1)
                    .map(|m| {
                        json!({
                            "role": m.role,
                            "content": convert_message_parts_to_content(&m.parts)
                        })
                    })
                    .collect::<serde_json::Value>(),
            );
        } else {
            map.insert(
                "messages".into(),
                prompt
                    .iter()
                    .map(|m| {
                        json!({
                            "role": m.role,
                            "content": convert_message_parts_to_content(&m.parts)
                        })
                    })
                    .collect::<serde_json::Value>(),
            );
        }
    } else {
        map.insert(
            "messages".into(),
            prompt
                .iter()
                .map(|m| {
                    json!({
                        "role": m.role,
                        "content": convert_message_parts_to_content(&m.parts)
                    })
                })
                .collect::<serde_json::Value>(),
        );
    }
    log::debug!("converted chat prompt to body: {:#?}", map);

    return map;
}

fn convert_message_parts_to_content(parts: &Vec<ChatMessagePart>) -> serde_json::Value {
    if parts.len() == 1 {
        if let ChatMessagePart::Text(text) = &parts[0] {
            return json!(text);
        }
    }

    parts
        .iter()
        .map(|part| match part {
            ChatMessagePart::Text(text) => json!({
                "type": "text",
                "text": text
            }),
            ChatMessagePart::Image(image) => match image {
                BamlImage::Base64(image) => json!({
                    "type": "image",
                    "source": {
                        "type": "base64",
                        "media_type": image.media_type,
                        "data": image.base64
                    }
                }),
                BamlImage::Url(image) => json!({
                    "type": "image",
                    "source": {
                        "type": "url",
                        "url": image.url
                    }
                }),
            },
        })
        .collect()
}
