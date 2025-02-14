use crate::client_registry::ClientProperty;
use crate::internal::llm_client::traits::{
    ToProviderMessage, ToProviderMessageExt, WithClientProperties,
};
use crate::internal::llm_client::ResolveMediaUrls;
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
use baml_types::{BamlMap, BamlMedia, BamlMediaContent};
use eventsource_stream::Eventsource;
use futures::StreamExt;
use http::header;
use internal_baml_core::ir::ClientWalker;
use internal_baml_jinja::{ChatMessagePart, RenderContext_Client, RenderedChatMessage};
use internal_llm_client::google_ai::ResolvedGoogleAI;
use internal_llm_client::{
    AllowedRoleMetadata, ClientProvider, ResolvedClientProperty, UnresolvedClientProperty,
};
use secrecy::ExposeSecret;
use serde_json::json;
use std::collections::HashMap;

pub struct GoogleAIClient {
    pub name: String,
    pub client: reqwest::Client,
    pub retry_policy: Option<String>,
    pub context: RenderContext_Client,
    pub features: ModelFeatures,
    properties: ResolvedGoogleAI,
}

fn resolve_properties(
    provider: &ClientProvider,
    properties: &UnresolvedClientProperty<()>,
    ctx: &RuntimeContext,
) -> Result<ResolvedGoogleAI, anyhow::Error> {
    let properties = properties.resolve(provider, &ctx.eval_ctx(false))?;

    let ResolvedClientProperty::GoogleAI(props) = properties else {
        anyhow::bail!(
            "Invalid client property. Should have been a google-ai property but got: {}",
            properties.name()
        );
    };

    Ok(props)
}

impl WithRetryPolicy for GoogleAIClient {
    fn retry_policy_name(&self) -> Option<&str> {
        self.retry_policy.as_deref()
    }
}

impl WithClientProperties for GoogleAIClient {
    fn allowed_metadata(&self) -> &AllowedRoleMetadata {
        &self.properties.allowed_metadata
    }
    fn supports_streaming(&self) -> bool {
        self.properties
            .supported_request_modes
            .stream
            .unwrap_or(true)
    }
    fn finish_reason_filter(&self) -> &internal_llm_client::FinishReasonFilter {
        &self.properties.finish_reason_filter
    }
    fn default_role(&self) -> String {
        self.properties.default_role()
    }
    fn allowed_roles(&self) -> Vec<String> {
        self.properties.allowed_roles()
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
        prompt: &[RenderedChatMessage],
        system_start: web_time::SystemTime,
        instant_start: web_time::Instant,
    ) -> StreamResponse {
        let prompt = prompt.to_vec();
        let client_name = self.context.name.clone();
        let model_id = self.properties.model.clone();
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
                        model: model_id.clone(),
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
                                        model: if inner.model.is_empty() {
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
                            let part_index = content_part(&model_id);
                            if let Some(content) = choice
                                .content
                                .as_ref()
                                .and_then(|c| c.parts.get(part_index))
                            {
                                inner.content += &content.text;
                            }
                            if let Some(FinishReason::Stop) = choice.finish_reason.as_ref() {
                                inner.metadata.baml_is_complete = true;
                                inner.metadata.finish_reason = Some(FinishReason::Stop.to_string());
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
        prompt: &[RenderedChatMessage],
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
        let properties = resolve_properties(&client.elem().provider, &client.options(), ctx)?;
        Ok(Self {
            name: client.name().into(),
            context: RenderContext_Client {
                name: client.name().into(),
                provider: client.elem().provider.to_string(),
                default_role: properties.default_role(),
                allowed_roles: properties.allowed_roles(),
            },
            features: ModelFeatures {
                chat: true,
                completion: false,
                max_one_system_prompt: true,
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
        let properties = resolve_properties(&client.provider, &client.unresolved_options()?, ctx)?;

        Ok(Self {
            name: client.name.clone(),
            context: RenderContext_Client {
                name: client.name.clone(),
                provider: client.provider.to_string(),
                default_role: properties.default_role(),
                allowed_roles: properties.allowed_roles(),
            },
            features: ModelFeatures {
                chat: true,
                completion: false,
                max_one_system_prompt: true,
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
        prompt: either::Either<&String, &[RenderedChatMessage]>,
        allow_proxy: bool,
        stream: bool,
        expose_secrets: bool,
    ) -> Result<reqwest::RequestBuilder> {
        let mut should_stream = "generateContent";
        if stream {
            should_stream = "streamGenerateContent?alt=sse";
        }

        let baml_original_url = format!(
            "{}/models/{}:{}",
            self.properties.base_url,
            self.properties.model.clone(),
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
            self.properties.api_key.render(expose_secrets),
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

    fn request_options(&self) -> &BamlMap<String, serde_json::Value> {
        &self.properties.properties
    }
}

impl WithChat for GoogleAIClient {
    async fn chat(&self, _ctx: &RuntimeContext, prompt: &[RenderedChatMessage]) -> LLMResponse {
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
                prompt: internal_baml_jinja::RenderedPrompt::Chat(prompt.to_vec()),
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

        let Some(content) = response.candidates[0].content.as_ref() else {
            return LLMResponse::LLMFailure(LLMErrorResponse {
                client: self.context.name.to_string(),
                model: None,
                prompt: internal_baml_jinja::RenderedPrompt::Chat(prompt.to_vec()),
                start_time: system_now,
                request_options: self.properties.properties.clone(),
                latency: instant_now.elapsed(),
                message: "No content returned".to_string(),
                code: ErrorCode::Other(200),
            });
        };

        let part_index = content_part(&self.properties.model);
        LLMResponse::Success(LLMCompleteResponse {
            client: self.context.name.to_string(),
            prompt: internal_baml_jinja::RenderedPrompt::Chat(prompt.to_vec()),
            content: content.parts[part_index].text.clone(),
            start_time: system_now,
            latency: instant_now.elapsed(),
            request_options: self.properties.properties.clone(),
            model: self.properties.model.clone(),
            metadata: LLMCompleteResponseMetadata {
                baml_is_complete: matches!(
                    response.candidates[0].finish_reason,
                    Some(FinishReason::Stop)
                ),
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
        chat: &[RenderedChatMessage],
    ) -> Result<serde_json::Map<String, serde_json::Value>> {
        let mut res = serde_json::Map::new();
        let (first, others) = chat.split_at(1);
        if let Some(content) = first.first() {
            if content.role == "system" {
                res.insert(
                    "system_instruction".into(),
                    json!({
                        "parts": self.parts_to_message(&content.parts)?
                    }),
                );
                res.insert(
                    "contents".into(),
                    others
                        .iter()
                        .map(|c| self.role_to_message(c))
                        .collect::<Result<Vec<_>>>()?
                        .into(),
                );
                return Ok(res);
            }
        }
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

/// The Google Gemini 2 model has an experimental feature
/// called Flash Thinking Mode, which is turned on in a particular
/// named model: gemini-2.0-flash-thinking-exp-1219
///
/// When run in this mode, Gemini returns `candidates` with 2 parts each.
/// Part 0 is the chain of thought, part 1 is the actual output.
/// Other Gemini models put the output data in part 0.
///
/// TODO: Explicitly represent Flash Thinking Mode response and
/// do more thorough checking for the content part.
/// For examples of how to introspect the response more safely, see:
/// https://github.com/GoogleCloudPlatform/generative-ai/blob/main/gemini/getting-started/intro_gemini_2_0_flash_thinking_mode.ipynb
fn content_part(model_name: &str) -> usize {
    if model_name.contains("gemini-2.0-flash-thinking-exp-1219") {
        1
    } else {
        0
    }
}
