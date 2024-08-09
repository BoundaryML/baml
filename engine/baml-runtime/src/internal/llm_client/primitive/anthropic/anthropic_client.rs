use crate::internal::llm_client::ResolveMedia;
use std::collections::HashMap;

use anyhow::{Context, Result};
use baml_types::{BamlMedia, BamlMediaContent};
use eventsource_stream::Eventsource;
use futures::StreamExt;
use internal_baml_core::ir::ClientWalker;
use internal_baml_jinja::{
    ChatMessagePart, RenderContext_Client, RenderedChatMessage, RenderedPrompt,
};

use crate::{
    client_registry::ClientProperty,
    internal::llm_client::{
        primitive::{
            anthropic::types::{AnthropicMessageResponse, StopReason},
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

// stores properties required for making a post request to the API
struct PostRequestProperities {
    default_role: String,
    base_url: String,
    api_key: Option<String>,
    headers: HashMap<String, String>,
    proxy_url: Option<String>,
    // These are passed directly to the Anthropic API.
    properties: HashMap<String, serde_json::Value>,
}

// represents client that interacts with the Anthropic API
pub struct AnthropicClient {
    pub name: String,
    retry_policy: Option<String>,
    context: RenderContext_Client,
    features: ModelFeatures,
    properties: PostRequestProperities,

    // clients
    client: reqwest::Client,
}

// resolves/constructs PostRequestProperties from the client's options and runtime context, fleshing out the needed headers and parameters
// basically just reads the client's options and matches them to needed properties or defaults them
fn resolve_properties(
    mut properties: HashMap<String, serde_json::Value>,
    ctx: &RuntimeContext,
) -> Result<PostRequestProperities> {
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

    let mut headers = match properties.remove("headers") {
        Some(headers) => headers
            .as_object()
            .context("headers must be a map of strings to strings")?
            .iter()
            .map(|(k, v)| {
                Ok((
                    k.to_string(),
                    v.as_str()
                        .context(format!("Header '{}' must be a string", k))?
                        .to_string(),
                ))
            })
            .collect::<Result<HashMap<_, _>>>()?,
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
        proxy_url: ctx.env.get("BOUNDARY_PROXY_URL").map(|s| s.to_string()),
    })
}

// getters for client info
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

// Manages processing response chunks from streaming response, and converting it into a structured response format
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
                        request_options: params.clone(),
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
                                        request_options: params.clone(),
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
                                        request_options: params.clone(),
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

// handles streamign chat interactions, when sending prompt to API and processing response stream
impl WithStreamChat for AnthropicClient {
    async fn stream_chat(
        &self,
        _ctx: &RuntimeContext,
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

// constructs base client and resolves properties based on context
impl AnthropicClient {
    pub fn dynamic_new(client: &ClientProperty, ctx: &RuntimeContext) -> Result<Self> {
        let properties = resolve_properties(
            client
                .options
                .iter()
                .map(|(k, v)| Ok((k.clone(), json!(v))))
                .collect::<Result<HashMap<_, _>>>()?,
            ctx,
        )?;
        let default_role = properties.default_role.clone();
        Ok(Self {
            name: client.name.clone(),
            properties,
            context: RenderContext_Client {
                name: client.name.clone(),
                provider: client.provider.clone(),
                default_role,
            },
            features: ModelFeatures {
                chat: true,
                completion: false,
                anthropic_system_constraints: true,
                resolve_media_urls: ResolveMedia::Always,
            },
            retry_policy: client.retry_policy.clone(),
            client: create_client()?,
        })
    }

    pub fn new(client: &ClientWalker, ctx: &RuntimeContext) -> Result<AnthropicClient> {
        let properties = super::super::resolve_properties_walker(client, ctx)?;
        let properties = resolve_properties(properties, ctx)?;
        let default_role = properties.default_role.clone();
        Ok(Self {
            name: client.name().into(),
            properties,
            context: RenderContext_Client {
                name: client.name().into(),
                provider: client.elem().provider.clone(),
                default_role,
            },
            features: ModelFeatures {
                chat: true,
                completion: false,
                anthropic_system_constraints: true,
                resolve_media_urls: ResolveMedia::Always,
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

// how to build the HTTP request for requests
impl RequestBuilder for AnthropicClient {
    fn http_client(&self) -> &reqwest::Client {
        &self.client
    }

    async fn build_request(
        &self,
        prompt: either::Either<&String, &Vec<RenderedChatMessage>>,
        allow_proxy: bool,
        stream: bool,
    ) -> Result<reqwest::RequestBuilder> {
        let destination_url = if allow_proxy {
            self.properties
                .proxy_url
                .as_ref()
                .unwrap_or(&self.properties.base_url)
        } else {
            &self.properties.base_url
        };

        let mut req = self.client.post(if prompt.is_left() {
            format!("{}/v1/complete", destination_url)
        } else {
            format!("{}/v1/messages", destination_url)
        });

        for (key, value) in &self.properties.headers {
            req = req.header(key, value);
        }
        if let Some(key) = &self.properties.api_key {
            req = req.header("x-api-key", key);
        }

        if allow_proxy {
            req = req.header("baml-original-url", self.properties.base_url.as_str());
        }
        let mut body = json!(self.properties.properties);
        let body_obj = body.as_object_mut().unwrap();
        match prompt {
            either::Either::Left(prompt) => {
                body_obj.extend(convert_completion_prompt_to_body(prompt))
            }
            either::Either::Right(messages) => {
                body_obj.extend(convert_chat_prompt_to_body(messages)?);
            }
        }

        if stream {
            body_obj.insert("stream".into(), true.into());
        }

        Ok(req.json(&body))
    }

    fn request_options(&self) -> &HashMap<String, serde_json::Value> {
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
                request_options: self.properties.properties.clone(),
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
            request_options: self.properties.properties.clone(),
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

// converts completion prompt into JSON body for request
fn convert_completion_prompt_to_body(prompt: &String) -> HashMap<String, serde_json::Value> {
    let mut map = HashMap::new();
    map.insert("prompt".into(), json!(prompt));
    map
}

// converts chat prompt into JSON body for request
fn convert_chat_prompt_to_body(
    prompt: &Vec<RenderedChatMessage>,
) -> Result<HashMap<String, serde_json::Value>> {
    let mut map = HashMap::new();

    if let Some(first) = prompt.get(0) {
        if first.role == "system" {
            map.insert(
                "system".into(),
                convert_message_parts_to_content(&first.parts)?,
            );
            map.insert(
                "messages".into(),
                prompt
                    .iter()
                    .skip(1)
                    .map(|m| {
                        Ok(json!({
                            "role": m.role,
                            "content": convert_message_parts_to_content(&m.parts)?
                        }))
                    })
                    .collect::<Result<serde_json::Value>>()?,
            );
        } else {
            map.insert(
                "messages".into(),
                prompt
                    .iter()
                    .map(|m| {
                        Ok(json!({
                            "role": m.role,
                            "content": convert_message_parts_to_content(&m.parts)?
                        }))
                    })
                    .collect::<Result<serde_json::Value>>()?,
            );
        }
    } else {
        map.insert(
            "messages".into(),
            prompt
                .iter()
                .map(|m| {
                    Ok(json!({
                        "role": m.role,
                        "content": convert_message_parts_to_content(&m.parts)?
                    }))
                })
                .collect::<Result<serde_json::Value>>()?,
        );
    }
    log::debug!("converted chat prompt to body: {:#?}", map);

    Ok(map)
}

// converts chat message parts into JSON content
fn convert_message_parts_to_content(parts: &Vec<ChatMessagePart>) -> Result<serde_json::Value> {
    if parts.len() == 1 {
        if let ChatMessagePart::Text(text) = &parts[0] {
            return Ok(json!(text));
        }
    }

    parts
        .iter()
        .map(|part| {
            Ok(match part {
                ChatMessagePart::Text(text) => json!({
                    "type": "text",
                    "text": text
                }),

                ChatMessagePart::Media(media) => match &media.content {
                    BamlMediaContent::Base64(data) => json!({
                        "type":  media.media_type.to_string(),

                        "source": {
                            "type": "base64",
                            "media_type": data.mime_type,
                            "data": data.base64
                        }
                    }),
                    BamlMediaContent::File(_) => {
                        anyhow::bail!(
                            "BAML internal error (Anthropic): file should have been resolved to base64"
                        )
                    }
                    BamlMediaContent::Url(_) => {
                        anyhow::bail!(
                            "BAML internal error (Anthropic): media URL should have been resolved to base64"
                        )
                    }
                    //never executes, keep for future if anthropic supports urls
                    // BamlMedia::Url(media_type, data) => json!({
                    //     "type": "image",

                    //     "source": {
                    //         "type": "url",
                    //         "url": data.url
                    //     }
                    // }),
                },
            })
        })
        .collect()
}
