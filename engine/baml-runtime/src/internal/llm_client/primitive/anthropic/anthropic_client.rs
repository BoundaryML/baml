use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use anyhow::{anyhow, Context, Result};
use baml_types::BamlImage;
use eventsource_stream::Eventsource;
use futures::{Stream, StreamExt};
use internal_baml_core::ir::ClientWalker;
use internal_baml_jinja::{
    ChatMessagePart, RenderContext_Client, RenderedChatMessage, RenderedPrompt,
};

use crate::internal::llm_client::{
    primitive::anthropic::types::{AnthropicErrorResponse, AnthropicMessageResponse, StopReason},
    state::LlmClientState,
    traits::{WithChat, WithClient, WithNoCompletion, WithRetryPolicy},
    LLMCompleteResponse, LLMResponse, ModelFeatures, SseResponseTrait,
};
use serde_json::{json, Value};

use crate::RuntimeContext;

use super::types::MessageChunk;

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

impl SseResponseTrait for AnthropicClient {
    fn build_request_for_stream(
        &self,
        _ctx: &RuntimeContext,
        prompt: &internal_baml_jinja::RenderedPrompt,
    ) -> Result<reqwest::RequestBuilder> {
        log::trace!("stream chat starting");
        let RenderedPrompt::Chat(prompt) = prompt else {
            anyhow::bail!("Expected a chat prompt, got: {:#?}", prompt);
        };

        let mut body = json!(self.properties.properties);
        body.as_object_mut()
            .unwrap()
            .extend(convert_chat_prompt_to_body(prompt));
        body.as_object_mut()
            .unwrap()
            .insert("stream".into(), json!(true));
        log::trace!("anthropic stream body {:#?}", body);

        let mut headers: HashMap<String, String> = HashMap::default();
        match &self.properties.api_key {
            Some(key) => {
                headers.insert("x-api-key".to_string(), key.to_string());
            }
            None => {}
        }
        for (k, v) in &self.properties.headers {
            headers.insert(k.to_string(), v.to_string());
        }

        let mut request = reqwest::Client::new()
            .post(format!("{}/v1/messages", self.properties.base_url))
            .json(&body);
        for (key, value) in headers {
            request = request.header(key, value);
        }

        match self.internal_state.clone().lock() {
            Ok(mut state) => {
                state.call_count += 1;
            }
            Err(e) => {
                log::warn!(
                    "Failed to increment call count for AnthropicClient: {:#?}",
                    e
                );
            }
        }
        log::trace!("stream chat successfully built request {:#?}", request);
        Ok(request)
    }

    fn response_stream(
        &self,
        resp: reqwest::Response,
    ) -> impl Stream<Item = Result<LLMCompleteResponse>> {
        log::info!("response object {:#?}", resp);
        resp.bytes_stream()
            .inspect(|event| log::trace!("anthropic event bytes: {:#?}", event))
            .eventsource()
            .map(|event| -> Result<MessageChunk> { Ok(serde_json::from_str(&event?.data)?) })
            .inspect(|event| log::trace!("anthropic eventsource: {:#?}", event))
            .scan(
                Ok(LLMCompleteResponse {
                    client: "".to_string(),
                    prompt: internal_baml_jinja::RenderedPrompt::Chat(vec![]),
                    content: "".to_string(),
                    // TODO: compute start_time_unix_ms
                    start_time_unix_ms: 0,
                    // TODO: compute latency_ms
                    latency_ms: 0,
                    // TODO: figure out how to extract this from the response
                    model: "".to_string(),
                    metadata: json!({
                        //"baml_is_complete": false,
                        //"finish_reason": "".to_string(),
                        // TODO: define behavior for these (openai doesn't)
                        // "prompt_tokens": usage.map(|u| u.prompt_tokens),
                        // "output_tokens": usage.map(|u| u.completion_tokens),
                        // "total_tokens": usage.map(|u| u.total_tokens),
                    }),
                }),
                |accumulated: &mut Result<LLMCompleteResponse>, event| {
                    let Ok(ref mut inner) = accumulated else {
                        // halt the stream: the last stream event failed to parse
                        return std::future::ready(None);
                    };
                    let event = match event {
                        Ok(event) => event,
                        Err(e) => {
                            *accumulated = Err(anyhow::anyhow!(
                                "Failed to accumulate response (failed to parse previous event from EventSource)"
                            ));
                            return std::future::ready(Some(Err(e.context("Failed to parse event from EventSource"))));
                        }
                    };
                    match event {
                        MessageChunk::MessageStart(_) => (),
                        MessageChunk::ContentBlockDelta(event) => {
                            inner.content += &event.delta.text;
                        }
                        MessageChunk::ContentBlockStart(_) => (),
                        MessageChunk::ContentBlockStop(_) => (),
                        MessageChunk::Ping => (),
                        MessageChunk::MessageDelta(_) => (),
                        MessageChunk::MessageStop => (),
                        MessageChunk::Error(err) => {
                            return std::future::ready(Some(Err(anyhow::anyhow!("Anthropic API Error: {:#?}", err))));
                        }
                    };
                    //if let Some(content) = event.choices[0].delta.content.as_ref() {
                    //    inner.content += content.as_str();
                    //    inner.model = event.model;
                    //    // TODO: set inner.metadata.finish_reason
                    //}

                    std::future::ready(Some(Ok(inner.clone())))
                },
            )
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

    async fn chat(&self, _ctx: &RuntimeContext, prompt: &Vec<RenderedChatMessage>) -> LLMResponse {
        use crate::{
            internal::llm_client::{ErrorCode, LLMCompleteResponse, LLMErrorResponse},
            request::{self, RequestError},
        };

        let mut body = json!(self.properties.properties);
        body.as_object_mut()
            .unwrap()
            .extend(convert_chat_prompt_to_body(prompt));

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
                    return LLMResponse::LLMFailure(LLMErrorResponse {
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
                    });
                }

                let usage = body.usage;

                LLMResponse::Success(LLMCompleteResponse {
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
                            Some(StopReason::StopSequence) => true,
                            Some(StopReason::EndTurn)  => true,
                            _ => false,
                        },
                        "finish_reason": body.stop_reason,
                        "prompt_tokens": usage.input_tokens,
                        "output_tokens": usage.output_tokens,
                        "total_tokens": usage.input_tokens + usage.output_tokens,
                    }),
                })
            }
            Err(e) => match e {
                RequestError::BuildError(e)
                | RequestError::FetchError(e)
                | RequestError::JsonError(e)
                | RequestError::SerdeError(e) => LLMResponse::LLMFailure(LLMErrorResponse {
                    client: self.context.name.to_string(),
                    model: None,
                    prompt: internal_baml_jinja::RenderedPrompt::Chat(prompt.clone()),
                    start_time_unix_ms: now
                        .duration_since(web_time::UNIX_EPOCH)
                        .unwrap()
                        .as_millis() as u64,
                    latency_ms: now.elapsed().unwrap().as_millis() as u64,
                    message: format!("Failed to make request: {:#?}", e),
                    code: ErrorCode::Other(2),
                }),
                RequestError::ResponseError(status, res) => {
                    match request::response_json::<AnthropicErrorResponse>(res).await {
                        Ok(err) => {
                            let err_message =
                                format!("API Error ({}): {}", err.error.r#type, err.error.message);
                            LLMResponse::LLMFailure(LLMErrorResponse {
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
                            })
                        }
                        Err(e) => LLMResponse::LLMFailure(LLMErrorResponse {
                            client: self.context.name.to_string(),
                            model: None,
                            prompt: internal_baml_jinja::RenderedPrompt::Chat(prompt.clone()),
                            start_time_unix_ms: now
                                .duration_since(web_time::UNIX_EPOCH)
                                .unwrap()
                                .as_millis() as u64,
                            latency_ms: now.elapsed().unwrap().as_millis() as u64,
                            message: format!("Failed to parse error response: {:#?}", e),
                            code: ErrorCode::from_u16(status),
                        }),
                    }
                }
            },
        }
    }
}

fn convert_chat_prompt_to_body(
    prompt: &Vec<RenderedChatMessage>,
) -> HashMap<String, serde_json::Value> {
    let mut map = HashMap::new();

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
