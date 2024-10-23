use crate::client_registry::ClientProperty;
use crate::internal::llm_client::traits::{
    ToProviderMessage, ToProviderMessageExt, WithClientProperties,
};
use crate::internal::llm_client::{AllowedMetadata, ResolveMediaUrls};
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
use baml_types::{BamlMedia, BamlMediaContent};
use eventsource_stream::Eventsource;
use futures::StreamExt;
use internal_baml_core::ir::ClientWalker;
use internal_baml_jinja::{ChatMessagePart, RenderContext_Client, RenderedChatMessage};
use serde_json::json;
use std::collections::HashMap;
struct PostRequestProperties {
    default_role: String,
    api_key: Option<String>,
    headers: HashMap<String, String>,
    base_url: String,
    proxy_url: Option<String>,
    model_id: Option<String>,
    properties: HashMap<String, serde_json::Value>,
    allowed_metadata: AllowedMetadata,
}

pub struct GoogleAIClient {
    pub name: String,
    pub client: reqwest::Client,
    pub retry_policy: Option<String>,
    pub context: RenderContext_Client,
    pub features: ModelFeatures,
    properties: PostRequestProperties,
}

fn resolve_properties(
    mut properties: HashMap<String, serde_json::Value>,
    ctx: &RuntimeContext,
) -> Result<PostRequestProperties, anyhow::Error> {
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

    let base_url = properties
        .remove("base_url")
        .and_then(|v| v.as_str().map(|s| s.to_string()))
        .unwrap_or_else(|| "https://generativelanguage.googleapis.com/v1beta".to_string());
    let allowed_metadata = match properties.remove("allowed_role_metadata") {
        Some(allowed_metadata) => serde_json::from_value(allowed_metadata).context(
            "allowed_role_metadata must be an array of keys. For example: ['key1', 'key2']",
        )?,
        None => AllowedMetadata::None,
    };

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

    Ok(PostRequestProperties {
        default_role,
        api_key,
        headers,
        properties,
        base_url,
        model_id,
        proxy_url: ctx.env.get("BOUNDARY_PROXY_URL").map(|s| s.to_string()),
        allowed_metadata,
    })
}

impl WithRetryPolicy for GoogleAIClient {
    fn retry_policy_name(&self) -> Option<&str> {
        self.retry_policy.as_deref()
    }
}

impl WithClientProperties for GoogleAIClient {
    fn client_properties(&self) -> &HashMap<String, serde_json::Value> {
        &self.properties.properties
    }
    fn allowed_metadata(&self) -> &crate::internal::llm_client::AllowedMetadata {
        &self.properties.allowed_metadata
    }
}

impl WithClient for GoogleAIClient {
    fn context(&self) -> &RenderContext_Client {
        &self.context
    }

    fn model_features(&self) -> &ModelFeatures {
        &self.features
    }
}

impl WithNoCompletion for GoogleAIClient {}

impl SseResponseTrait for GoogleAIClient {
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
                .inspect(|event| log::trace!("Received event: {:?}", event))
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
                                        request_options: params.clone(),
                                        latency: instant_start.elapsed(),
                                        message: format!("Failed to parse event: {:#?}", e),
                                        code: ErrorCode::UnsupportedResponse(2),
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
impl WithStreamChat for GoogleAIClient {
    async fn stream_chat(
        &self,
        _ctx: &RuntimeContext,
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

impl GoogleAIClient {
    pub fn new(client: &ClientWalker, ctx: &RuntimeContext) -> Result<Self> {
        let properties = super::super::resolve_properties_walker(client, ctx)?;
        let properties = resolve_properties(properties, ctx)?;
        let default_role = properties.default_role.clone();
        Ok(Self {
            name: client.name().into(),
            context: RenderContext_Client {
                name: client.name().into(),
                provider: client.elem().provider.clone(),
                default_role,
            },
            features: ModelFeatures {
                chat: true,
                completion: false,
                anthropic_system_constraints: false,
                resolve_media_urls: ResolveMediaUrls::Always,
                allowed_metadata: properties.allowed_metadata.clone(),
            },
            retry_policy: client
                .elem()
                .retry_policy_id
                .as_ref()
                .map(|s| s.to_string()),
            client: create_client()?,
            properties,
        })
    }

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
            context: RenderContext_Client {
                name: client.name.clone(),
                provider: client.provider.clone(),
                default_role,
            },
            features: ModelFeatures {
                chat: true,
                completion: false,
                anthropic_system_constraints: false,
                resolve_media_urls: ResolveMediaUrls::Always,
                allowed_metadata: properties.allowed_metadata.clone(),
            },
            retry_policy: client.retry_policy.clone(),
            client: create_client()?,
            properties,
        })
    }
}

impl RequestBuilder for GoogleAIClient {
    fn http_client(&self) -> &reqwest::Client {
        &self.client
    }

    async fn build_request(
        &self,
        prompt: either::Either<&String, &Vec<RenderedChatMessage>>,
        allow_proxy: bool,
        stream: bool,
    ) -> Result<reqwest::RequestBuilder> {
        let mut should_stream = "generateContent";
        if stream {
            should_stream = "streamGenerateContent?alt=sse";
        }

        let baml_original_url = format!(
            "{}/models/{}:{}",
            self.properties.base_url,
            self.properties.model_id.as_ref().unwrap_or(&"".to_string()),
            should_stream
        );

        let mut req = match (&self.properties.proxy_url, allow_proxy) {
            (Some(proxy_url), true) => {
                let req = self.client.post(proxy_url.clone());
                req.header("baml-original-url", baml_original_url)
            }
            _ => self.client.post(baml_original_url),
        };

        for (key, value) in &self.properties.headers {
            req = req.header(key, value);
        }

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
                body_obj.extend(self.chat_to_message(messages)?);
            }
        }

        Ok(req.json(&body))
    }

    fn request_options(&self) -> &HashMap<String, serde_json::Value> {
        &self.properties.properties
    }
}

impl WithChat for GoogleAIClient {
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
                request_options: self.properties.properties.clone(),
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
            request_options: self.properties.properties.clone(),
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
                prompt_tokens: response.usage_metadata.prompt_token_count,
                output_tokens: response.usage_metadata.candidates_token_count,
                total_tokens: response.usage_metadata.total_token_count,
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

impl ToProviderMessageExt for GoogleAIClient {
    fn chat_to_message(
        &self,
        chat: &Vec<RenderedChatMessage>,
    ) -> Result<serde_json::Map<String, serde_json::Value>> {
        let mut res = serde_json::Map::new();
        res.insert(
            "contents".into(),
            chat.iter()
                .map(|c| self.role_to_message(c))
                .collect::<Result<Vec<_>>>()?
                .into(),
        );
        Ok(res)
    }
}

impl ToProviderMessage for GoogleAIClient {
    fn to_chat_message(
        &self,
        mut content: serde_json::Map<String, serde_json::Value>,
        text: &str,
    ) -> Result<serde_json::Map<String, serde_json::Value>> {
        content.insert("text".into(), json!(text));
        Ok(content)
    }

    fn to_media_message(
        &self,
        mut content: serde_json::Map<String, serde_json::Value>,
        media: &baml_types::BamlMedia,
    ) -> Result<serde_json::Map<String, serde_json::Value>> {
        match &media.content {
            BamlMediaContent::Base64(data) => {
                content.insert(
                    "inlineData".into(),
                    json!({
                        "mimeType": media.mime_type_as_ok()?,
                        "data": data.base64
                    }),
                );
                Ok(content)
            }
            BamlMediaContent::File(_) => anyhow::bail!(
                "BAML internal error (google-ai): file should have been resolved to base64"
            ),
            BamlMediaContent::Url(_) => anyhow::bail!(
                "BAML internal error (google-ai): media URL should have been resolved to base64"
            ),
        }
    }

    fn role_to_message(
        &self,
        content: &RenderedChatMessage,
    ) -> Result<serde_json::Map<String, serde_json::Value>> {
        let mut message = serde_json::Map::new();
        message.insert("role".into(), json!(content.role));
        message.insert(
            "parts".into(),
            json!(self.parts_to_message(&content.parts)?),
        );
        Ok(message)
    }
}
