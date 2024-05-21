use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use anyhow::{Context, Result};
use baml_types::BamlImage;
use internal_baml_core::ir::ClientWalker;
use internal_baml_jinja::{
    ChatMessagePart, RenderContext_Client, RenderedChatMessage, RenderedPrompt,
};

use serde_json::json;

use crate::internal::llm_client::{
    state::LlmClientState,
    traits::{WithChat, WithClient, WithNoCompletion, WithRetryPolicy},
    LLMResponse, ModelFeatures,
};

use crate::RuntimeContext;
use eventsource_stream::Eventsource;
use futures::{Stream, StreamExt};

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

    let default_role = properties
        .remove("default_role")
        .and_then(|v| v.as_str().map(|s| s.to_string()))
        .unwrap_or_else(|| "system".to_string());

    let base_url = properties
        .remove("base_url")
        .and_then(|v| v.as_str().map(|s| s.to_string()))
        .unwrap_or_else(|| "https://api.openai.com".to_string());

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

    Ok(PostRequestProperities {
        default_role,
        base_url,
        api_key,
        headers,
        properties,
    })
}
struct PostRequestProperities {
    default_role: String,
    base_url: String,
    api_key: Option<String>,
    headers: HashMap<String, String>,

    // These are passed directly to the OpenAI API.
    properties: HashMap<String, serde_json::Value>,
}

pub struct OpenAIClient {
    pub name: String,
    // client: ClientWalker<'ir>,
    retry_policy: Option<String>,
    context: RenderContext_Client,
    features: ModelFeatures,
    properties: PostRequestProperities,

    internal_state: Arc<Mutex<LlmClientState>>,
}

impl WithRetryPolicy for OpenAIClient {
    fn retry_policy_name(&self) -> Option<&str> {
        self.retry_policy.as_deref()
    }
}

impl WithClient for OpenAIClient {
    fn context(&self) -> &RenderContext_Client {
        &self.context
    }

    fn model_features(&self) -> &ModelFeatures {
        &self.features
    }
}

impl WithNoCompletion for OpenAIClient {}

impl WithChat for OpenAIClient {
    fn chat_options(&self, _ctx: &RuntimeContext) -> Result<internal_baml_jinja::ChatOptions> {
        Ok(internal_baml_jinja::ChatOptions::new(
            self.properties.default_role.clone(),
            None,
        ))
    }

    async fn chat(&self, _ctx: &RuntimeContext, prompt: &Vec<RenderedChatMessage>) -> LLMResponse {
        use crate::{
            internal::llm_client::{
                primitive::openai::types::{
                    ChatCompletionResponse, FinishReason, OpenAIErrorResponse,
                },
                ErrorCode, LLMCompleteResponse, LLMErrorResponse,
            },
            request::{self, RequestError},
        };

        let mut body = json!(self.properties.properties);
        body.as_object_mut().unwrap().insert(
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

        let mut headers = HashMap::default();
        match &self.properties.api_key {
            Some(key) => {
                headers.insert("Authorization".to_string(), format!("Bearer {}", key));
            }
            None => {}
        }
        for (k, v) in &self.properties.headers {
            headers.insert(k.to_string(), v.to_string());
        }

        let (system_now, instant_now) = (web_time::SystemTime::now(), web_time::Instant::now());
        match request::call_request_with_json::<ChatCompletionResponse, _>(
            &format!("{}{}", self.properties.base_url, "/v1/chat/completions"),
            &body,
            Some(headers),
        )
        .await
        {
            Ok(body) => {
                if body.choices.len() < 1 {
                    return LLMResponse::LLMFailure(LLMErrorResponse {
                        client: self.context.name.to_string(),
                        model: None,
                        prompt: internal_baml_jinja::RenderedPrompt::Chat(prompt.clone()),
                        start_time: system_now,
                        latency: instant_now.elapsed(),
                        message: format!("No content in response:\n{:#?}", body),
                        code: ErrorCode::Other(200),
                    });
                }

                let usage = body.usage.as_ref();

                LLMResponse::Success(LLMCompleteResponse {
                    client: self.context.name.to_string(),
                    prompt: internal_baml_jinja::RenderedPrompt::Chat(prompt.clone()),
                    content: body.choices[0]
                        .message
                        .content
                        .as_ref()
                        .map_or("", |s| s.as_str())
                        .to_string(),
                    start_time: system_now,
                    latency: instant_now.elapsed(),
                    model: body.model,
                    metadata: LLMCompleteResponseMetadata {
                        baml_is_complete: match body.choices[0].finish_reason {
                            None => true,
                            Some(FinishReason::Stop) => true,
                            _ => false,
                        },
                        finish_reason: match body.choices.get(0) {
                            Some(c) => match c.finish_reason {
                                Some(FinishReason::Stop) => Some("stop".to_string()),
                                _ => None,
                            },
                            None => None,
                        },
                        prompt_tokens: usage.map(|u| u.prompt_tokens),
                        output_tokens: usage.map(|u| u.completion_tokens),
                        total_tokens: usage.map(|u| u.total_tokens),
                    },
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
                    start_time: system_now,
                    latency: instant_now.elapsed(),
                    message: format!("Failed to make request: {:#?}", e),
                    code: ErrorCode::Other(2),
                }),
                RequestError::ResponseError(status, res) => {
                    match request::response_json::<OpenAIErrorResponse>(res).await {
                        Ok(err) => {
                            let err_message =
                                format!("API Error ({}): {}", err.error.r#type, err.error.message);
                            LLMResponse::LLMFailure(LLMErrorResponse {
                                client: self.context.name.to_string(),
                                model: None,
                                prompt: internal_baml_jinja::RenderedPrompt::Chat(prompt.clone()),
                                start_time: system_now,
                                latency: instant_now.elapsed(),
                                message: err_message,
                                code: ErrorCode::from_u16(status),
                            })
                        }
                        Err(e) => LLMResponse::LLMFailure(LLMErrorResponse {
                            client: self.context.name.to_string(),
                            model: None,
                            prompt: internal_baml_jinja::RenderedPrompt::Chat(prompt.clone()),
                            start_time: system_now,
                            latency: instant_now.elapsed(),
                            message: format!("Failed to parse error response: {:#?}", e),
                            code: ErrorCode::from_u16(status),
                        }),
                    }
                }
            },
        }
    }
}

use crate::internal::llm_client::{
    LLMCompleteResponse, LLMCompleteResponseMetadata, SseResponseTrait,
};

use super::types::ChatCompletionResponseDelta;

impl SseResponseTrait for OpenAIClient {
    fn build_request_for_stream(
        &self,
        prompt: &internal_baml_jinja::RenderedPrompt,
    ) -> Result<reqwest::RequestBuilder> {
        log::info!("stream chat starting");
        let RenderedPrompt::Chat(prompt) = prompt else {
            anyhow::bail!("Expected a chat prompt, got: {:#?}", prompt);
        };

        let mut body = json!(self.properties.properties);
        body.as_object_mut()
            .unwrap()
            .insert("stream".into(), json!(true));
        body.as_object_mut().unwrap().insert(
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

        let mut headers: HashMap<String, String> = HashMap::default();
        match &self.properties.api_key {
            Some(key) => {
                headers.insert("Authorization".to_string(), format!("Bearer {}", key));
            }
            None => {}
        }
        for (k, v) in &self.properties.headers {
            headers.insert(k.to_string(), v.to_string());
        }

        let mut request = reqwest::Client::new()
            .post(format!("{}/v1/chat/completions", self.properties.base_url))
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
                    "Failed to increment call count for OpejkjknAIClient: {:#?}",
                    e
                );
            }
        }
        log::info!("stream chat successfully built request");
        Ok(request)
    }

    fn response_stream(
        &self,
        resp: reqwest::Response,
        prompt: &internal_baml_jinja::RenderedPrompt,
        system_start: web_time::SystemTime,
        instant_start: web_time::Instant,
    ) -> impl Stream<Item = Result<LLMCompleteResponse>> {
        resp.bytes_stream()
            .eventsource()
            .take_while(|event| { std::future::ready(event.as_ref().is_ok_and(|e| e.data != "[DONE]"))})
            .map(|event| -> Result<ChatCompletionResponseDelta> {
                Ok(serde_json::from_str::<ChatCompletionResponseDelta>(
                    &event?.data,
                )?)
            })
            .inspect(|event| log::trace!("{:#?}", event))
            .scan(
                Ok(
LLMCompleteResponse {
                        client: self.context.name.to_string(),
                        prompt: prompt.clone(),
                        content: "".to_string(),
                        start_time: system_start,
                        latency: instant_start.elapsed(),
                        model: "".to_string(),
                        metadata: LLMCompleteResponseMetadata {
                            baml_is_complete: false,
                            finish_reason: None,
                            prompt_tokens: None,
                            output_tokens: None,
                            total_tokens: None,
                        },
                    }
                ),
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
                    if let Some(content) = event.choices[0].delta.content.as_ref() {
                        inner.content += content.as_str();
                        inner.model = event.model;
                        // TODO: set inner.metadata.finish_reason
                    }

                    std::future::ready(Some(Ok(inner.clone())))
                },
            )
    }
}

impl OpenAIClient {
    pub fn new(client: &ClientWalker, ctx: &RuntimeContext) -> Result<OpenAIClient> {
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
            internal_state: Arc::new(Mutex::new(LlmClientState::new())),
        })
    }
}

fn convert_message_parts_to_content(parts: &Vec<ChatMessagePart>) -> serde_json::Value {
    if parts.len() == 1 {
        match &parts[0] {
            ChatMessagePart::Text(text) => return json!(text),
            _ => {}
        }
    }

    let content: Vec<serde_json::Value> = parts
        .into_iter()
        .map(|part| match part {
            ChatMessagePart::Text(text) => json!({"type": "text", "text": text}),
            ChatMessagePart::Image(image) => match image {
                BamlImage::Url(image) => {
                    json!({"type": "image_url", "image_url": json!({
                        "url": image.url
                    })})
                }
                BamlImage::Base64(image) => {
                    json!({"type": "image_url", "image_url": json!({
                        "base64": image.base64
                    })})
                }
            },
        })
        .collect();

    json!(content)
}
