use crate::RuntimeContext;
use crate::{
    internal::llm_client::{
        primitive::{
            google::types::{FinishReason, GoogleResponse},
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
use anyhow::{Context, Result};
use baml_types::{BamlMedia, BamlMediaType};
use eventsource_stream::Eventsource;
use futures::StreamExt;
use internal_baml_core::ir::ClientWalker;
use internal_baml_jinja::{ChatMessagePart, RenderContext_Client, RenderedChatMessage};
use reqwest::Response;
use serde_json::json;
use std::collections::HashMap;
struct PostRequestProperities {
    default_role: String,
    api_key: Option<String>,
    headers: HashMap<String, String>,
    proxy_url: Option<String>,
    model_id: Option<String>,
    properties: HashMap<String, serde_json::Value>,
}

pub struct GoogleClient {
    pub name: String,
    pub client: reqwest::Client,
    pub retry_policy: Option<String>,
    pub context: RenderContext_Client,
    pub features: ModelFeatures,
    properties: PostRequestProperities,
}

fn resolve_properties(
    client: &ClientWalker,
    ctx: &RuntimeContext,
) -> Result<PostRequestProperities, anyhow::Error> {
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

    let default_role = properties
        .remove("default_role")
        .and_then(|v| v.as_str().map(|s| s.to_string()))
        .unwrap_or_else(|| "user".to_string());

    let api_key = properties
        .remove("api_key")
        .and_then(|v| v.as_str().map(|s| s.to_string()))
        .or_else(|| ctx.env.get("GOOGLE_API_KEY").map(|s| s.to_string()));

    let model_id = properties
        .remove("model")
        .and_then(|v| v.as_str().map(|s| s.to_string()))
        .or_else(|| Some("gemini-1.5-flash".to_string()));

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

    Ok(PostRequestProperities {
        default_role,
        api_key,
        headers,
        properties,
        model_id,
        proxy_url: ctx.env.get("BOUNDARY_PROXY_URL").map(|s| s.to_string()),
    })
}

impl WithRetryPolicy for GoogleClient {
    fn retry_policy_name(&self) -> Option<&str> {
        self.retry_policy.as_deref()
    }
}

impl WithClient for GoogleClient {
    fn context(&self) -> &RenderContext_Client {
        &self.context
    }

    fn model_features(&self) -> &ModelFeatures {
        &self.features
    }
}

impl WithNoCompletion for GoogleClient {}

impl SseResponseTrait for GoogleClient {
    fn response_stream(
        &self,
        resp: reqwest::Response,
        prompt: &Vec<RenderedChatMessage>,
        system_start: web_time::SystemTime,
        instant_start: web_time::Instant,
    ) -> StreamResponse {
        let prompt = prompt.clone();
        let client_name = self.context.name.clone();
        let model_id = self.properties.model_id.clone().unwrap_or_default();
        let params = self.properties.properties.clone();
        Ok(Box::pin(
            resp.bytes_stream()
                .eventsource()
                .inspect(|event| log::info!("Received event: {:?}", event))
                .take_while(|event| {
                    std::future::ready(event.as_ref().is_ok_and(|e| e.data != "data: \n"))
                })
                .map(|event| -> Result<GoogleResponse> {
                    Ok(serde_json::from_str::<GoogleResponse>(&event?.data)?)
                })
                .scan(
                    Ok(LLMCompleteResponse {
                        client: client_name.clone(),
                        prompt: internal_baml_jinja::RenderedPrompt::Chat(prompt.clone()),
                        content: "".to_string(),
                        start_time: system_start,
                        latency: instant_start.elapsed(),
                        model: model_id,
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
                            // halt the stream: the last stream event failed to parse
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
                                        start_time: system_start,
                                        invocation_params: params.clone(),
                                        latency: instant_start.elapsed(),
                                        message: format!("Failed to parse event: {:#?}", e),
                                        code: ErrorCode::Other(2),
                                    },
                                )));
                            }
                        };

                        if let Some(choice) = event.candidates.get(0) {
                            if let Some(content) = choice.content.parts.get(0) {
                                inner.content += &content.text;
                            }
                            match choice.finish_reason.as_ref() {
                                Some(FinishReason::Stop) => {
                                    inner.metadata.baml_is_complete = true;
                                    inner.metadata.finish_reason =
                                        Some(FinishReason::Stop.to_string());
                                }
                                _ => (),
                            }
                        }
                        inner.latency = instant_start.elapsed();

                        std::future::ready(Some(LLMResponse::Success(inner.clone())))
                    },
                ),
        ))
    }
}
// makes the request to the google client, on success it triggers the response_stream function to handle continuous rendering with the response object
impl WithStreamChat for GoogleClient {
    async fn stream_chat(
        &self,
        ctx: &RuntimeContext,
        prompt: &Vec<RenderedChatMessage>,
    ) -> StreamResponse {
        //incomplete, streaming response object is returned
        let (response, system_now, instant_now) =
            match make_request(self, either::Either::Right(prompt), true).await {
                Ok(v) => v,
                Err(e) => return Err(e),
            };
        self.response_stream(response, prompt, system_now, instant_now)
    }
}

impl GoogleClient {
    pub fn new(client: &ClientWalker, ctx: &RuntimeContext) -> Result<GoogleClient> {
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
                anthropic_system_constraints: false,
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

impl RequestBuilder for GoogleClient {
    fn http_client(&self) -> &reqwest::Client {
        &self.client
    }

    fn build_request(
        &self,
        prompt: either::Either<&String, &Vec<RenderedChatMessage>>,
        stream: bool,
    ) -> reqwest::RequestBuilder {
        let mut should_stream = "generateContent";
        if stream {
            should_stream = "streamGenerateContent?alt=sse";
        }

        let baml_original_url = format!(
            "https://generativelanguage.googleapis.com/v1/models/{}:{}",
            self.properties.model_id.as_ref().unwrap_or(&"".to_string()),
            should_stream
        );

        let mut req = self.client.post(
            self.properties
                .proxy_url
                .as_ref()
                .unwrap_or(&baml_original_url)
                .clone(),
        );

        for (key, value) in &self.properties.headers {
            req = req.header(key, value);
        }

        req = req.header("baml-original-url", baml_original_url);
        req = req.header(
            "x-goog-api-key",
            self.properties
                .api_key
                .clone()
                .unwrap_or_else(|| "".to_string()),
        );

        let mut body = json!(self.properties.properties);
        let body_obj = body.as_object_mut().unwrap();

        match prompt {
            either::Either::Left(prompt) => {
                body_obj.extend(convert_completion_prompt_to_body(prompt))
            }
            either::Either::Right(messages) => {
                body_obj.extend(convert_chat_prompt_to_body(messages))
            }
        }

        req.json(&body)
    }

    fn invocation_params(&self) -> &HashMap<String, serde_json::Value> {
        &self.properties.properties
    }
}

impl WithChat for GoogleClient {
    fn chat_options(&self, _ctx: &RuntimeContext) -> Result<internal_baml_jinja::ChatOptions> {
        Ok(internal_baml_jinja::ChatOptions::new(
            self.properties.default_role.clone(),
            None,
        ))
    }

    async fn chat(&self, _ctx: &RuntimeContext, prompt: &Vec<RenderedChatMessage>) -> LLMResponse {
        //non-streaming, complete response is returned
        let (response, system_now, instant_now) =
            match make_parsed_request::<GoogleResponse>(self, either::Either::Right(prompt), false)
                .await
            {
                Ok(v) => v,
                Err(e) => return e,
            };

        if response.candidates.len() != 1 {
            return LLMResponse::LLMFailure(LLMErrorResponse {
                client: self.context.name.to_string(),
                model: None,
                prompt: internal_baml_jinja::RenderedPrompt::Chat(prompt.clone()),
                start_time: system_now,
                invocation_params: self.properties.properties.clone(),
                latency: instant_now.elapsed(),
                message: format!(
                    "Expected exactly one content block, got {}",
                    response.candidates.len()
                ),
                code: ErrorCode::Other(200),
            });
        }

        LLMResponse::Success(LLMCompleteResponse {
            client: self.context.name.to_string(),
            prompt: internal_baml_jinja::RenderedPrompt::Chat(prompt.clone()),
            content: response.candidates[0].content.parts[0].text.clone(),
            start_time: system_now,
            latency: instant_now.elapsed(),
            invocation_params: self.properties.properties.clone(),
            model: self
                .properties
                .properties
                .get("model")
                .and_then(|v| v.as_str().map(|s| s.to_string()))
                .or_else(|| _ctx.env.get("default model").map(|s| s.to_string()))
                .unwrap_or_else(|| "".to_string()),
            metadata: LLMCompleteResponseMetadata {
                baml_is_complete: match response.candidates[0].finish_reason {
                    Some(FinishReason::Stop) => true,
                    _ => false,
                },
                finish_reason: response.candidates[0]
                    .finish_reason
                    .as_ref()
                    .map(|r| serde_json::to_string(r).unwrap_or("".into())),
                prompt_tokens: Some(response.usage_metadata.prompt_token_count),
                output_tokens: Some(response.usage_metadata.candidates_token_count),
                total_tokens: Some(response.usage_metadata.total_token_count),
            },
        })
    }
}

//simple, Map with key "prompt" and value of the prompt string
fn convert_completion_prompt_to_body(prompt: &String) -> HashMap<String, serde_json::Value> {
    let mut map = HashMap::new();
    let content = json!({
        "role": "user",
        "parts": [{
            "text": prompt
        }]
    });
    map.insert("contents".into(), json!([content]));
    map
}

//list of chat messages into JSON body
fn convert_chat_prompt_to_body(
    prompt: &Vec<RenderedChatMessage>,
) -> HashMap<String, serde_json::Value> {
    let mut map = HashMap::new();

    map.insert(
        "contents".into(),
        prompt
            .iter()
            .map(|m| {
                json!({
                    "role": m.role,
                    "parts": convert_message_parts_to_content(&m.parts)
                })
            })
            .collect::<serde_json::Value>(),
    );

    return map;
}

fn convert_message_parts_to_content(parts: &Vec<ChatMessagePart>) -> serde_json::Value {
    parts
        .iter()
        .map(|part| match part {
            ChatMessagePart::Text(text) => json!({
                "text": text
            }),
            ChatMessagePart::Image(media) | ChatMessagePart::Audio(media) => match media {
                BamlMedia::Base64(media_type, data) => json!({
                    "type": match media_type {
                        BamlMediaType::Image => "image",
                        BamlMediaType::Audio => "audio",
                    },
                    "source": {
                        "type": "base64",
                        "media_type": data.media_type,
                        "data": data.base64
                    }
                }),
                BamlMedia::Url(media_type, data) => json!({
                    "type": match media_type {
                        BamlMediaType::Image => "image",
                        BamlMediaType::Audio => "audio",
                    },
                    "source": {
                        "type": "url",
                        "url": data.url
                    }
                }),
            },
        })
        .collect()
}
