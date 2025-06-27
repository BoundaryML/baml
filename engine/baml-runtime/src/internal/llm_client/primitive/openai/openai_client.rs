use std::collections::HashMap;

use anyhow::Result;
use baml_types::{BamlMap, BamlMedia, BamlMediaContent, BamlMediaType};
use eventsource_stream::Eventsource;
use futures::StreamExt;
use internal_baml_core::ir::ClientWalker;
use internal_baml_jinja::{ChatMessagePart, RenderContext_Client, RenderedChatMessage};
use internal_llm_client::{openai::ResolvedOpenAI, AllowedRoleMetadata, FinishReasonFilter};
use secrecy::ExposeSecret;
use serde_json::json;

use super::{
    properties,
    types::{ChatCompletionResponse, ChatCompletionResponseDelta},
};
use crate::{
    client_registry::ClientProperty,
    internal::llm_client::{
        primitive::request::{make_parsed_request, make_request, RequestBuilder, ResponseType},
        traits::{
            CompletionToProviderBody, HttpContext, SseResponseTrait, StreamResponse,
            ToProviderMessage, ToProviderMessageExt, WithChat, WithClient, WithClientProperties,
            WithNoCompletion, WithRetryPolicy, WithStreamChat,
        },
        ErrorCode, LLMCompleteResponse, LLMCompleteResponseMetadata, LLMErrorResponse, LLMResponse,
        ModelFeatures, ResolveMediaUrls,
    },
    request::create_client,
    RuntimeContext,
};

pub struct OpenAIClient {
    pub name: String,
    provider: String,
    // client: ClientWalker<'ir>,
    retry_policy: Option<String>,
    context: RenderContext_Client,
    features: ModelFeatures,
    properties: ResolvedOpenAI,
    // clients
    client: reqwest::Client,
}

impl WithRetryPolicy for OpenAIClient {
    fn retry_policy_name(&self) -> Option<&str> {
        self.retry_policy.as_deref()
    }
}

impl WithClientProperties for OpenAIClient {
    fn allowed_metadata(&self) -> &AllowedRoleMetadata {
        &self.properties.allowed_metadata
    }

    fn finish_reason_filter(&self) -> &FinishReasonFilter {
        &self.properties.finish_reason_filter
    }

    fn allowed_roles(&self) -> Vec<String> {
        self.properties.allowed_roles()
    }

    fn default_role(&self) -> String {
        self.properties.default_role()
    }

    fn supports_streaming(&self) -> bool {
        self.properties.supports_streaming()
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
    async fn chat(&self, ctx: &impl HttpContext, prompt: &[RenderedChatMessage]) -> LLMResponse {
        let model_name = self
            .request_options()
            .get("model")
            .and_then(serde_json::Value::as_str)
            .map(str::to_string);
        make_parsed_request(
            self,
            model_name,
            either::Either::Right(prompt),
            false,
            self.properties.client_response_type.clone(),
            ctx,
        )
        .await
    }
}

impl RequestBuilder for OpenAIClient {
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
        let destination_url = if allow_proxy {
            self.properties
                .proxy_url
                .as_ref()
                .unwrap_or(&self.properties.base_url)
        } else {
            &self.properties.base_url
        };

        let mut req = self.client.post(if prompt.is_left() {
            format!("{destination_url}/completions")
        } else {
            format!("{destination_url}/chat/completions")
        });

        if !self.properties.query_params.is_empty() {
            req = req.query(&self.properties.query_params);
        }

        for (key, value) in &self.properties.headers {
            req = req.header(key, value);
        }
        if let Some(key) = &self.properties.api_key {
            req = req.bearer_auth(key.render(expose_secrets));
        }

        // Don't attach BAML creds to localhost requests, i.e. ollama
        if allow_proxy {
            req = req.header("baml-original-url", self.properties.base_url.as_str());
        }

        let mut body = json!(self.properties.properties);

        let body_obj = body.as_object_mut().unwrap();
        match prompt {
            either::Either::Left(prompt) => {
                body_obj.extend(convert_completion_prompt_to_body(prompt));
            }
            either::Either::Right(messages) => {
                body_obj.extend(self.chat_to_message(messages)?);
            }
        }

        if stream {
            body_obj.insert("stream".into(), json!(true));
            if self.provider == "openai" {
                body_obj.insert(
                    "stream_options".into(),
                    json!({
                        "include_usage": true,
                    }),
                );
            }
        }

        Ok(req.json(&body))
    }

    fn request_options(&self) -> &BamlMap<String, serde_json::Value> {
        &self.properties.properties
    }
}

impl WithStreamChat for OpenAIClient {
    async fn stream_chat(
        &self,
        ctx: &impl HttpContext,
        prompt: &[RenderedChatMessage],
    ) -> StreamResponse {
        let model_name = self
            .request_options()
            .get("model")
            .and_then(serde_json::Value::as_str)
            .map(str::to_string);
        crate::internal::llm_client::primitive::stream_request::make_stream_request(
            self,
            either::Either::Right(prompt),
            model_name,
            ResponseType::OpenAI,
            ctx,
        )
        .await
    }
}

macro_rules! make_openai_client {
    ($client:ident, $properties:ident, $provider:expr, dynamic) => {
        Ok(Self {
            name: $client.name.clone(),
            provider: $provider.into(),
            context: RenderContext_Client {
                name: $client.name.clone(),
                provider: $client.provider.to_string(),
                default_role: $properties.default_role(),
                allowed_roles: $properties.allowed_roles(),
            },
            features: ModelFeatures {
                chat: true,
                completion: false,
                max_one_system_prompt: false,
                resolve_audio_urls: ResolveMediaUrls::Always,
                resolve_image_urls: ResolveMediaUrls::Never,
                allowed_metadata: $properties.allowed_metadata.clone(),
            },
            properties: $properties,
            retry_policy: $client.retry_policy.clone(),
            client: create_client()?,
        })
    };
    ($client:ident, $properties:ident, $provider:expr) => {
        Ok(Self {
            name: $client.name().into(),
            provider: $provider.into(),
            context: RenderContext_Client {
                name: $client.name().into(),
                provider: $client.elem().provider.to_string(),
                default_role: $properties.default_role(),
                allowed_roles: $properties.allowed_roles(),
            },
            features: ModelFeatures {
                chat: true,
                completion: false,
                max_one_system_prompt: false,
                resolve_audio_urls: ResolveMediaUrls::Always,
                resolve_image_urls: ResolveMediaUrls::Never,
                allowed_metadata: $properties.allowed_metadata.clone(),
            },
            properties: $properties,
            retry_policy: $client
                .elem()
                .retry_policy_id
                .as_ref()
                .map(|s| s.to_string()),
            client: create_client()?,
        })
    };
}

impl OpenAIClient {
    pub fn new(client: &ClientWalker, ctx: &RuntimeContext) -> Result<OpenAIClient> {
        let properties =
            properties::resolve_properties(&client.elem().provider, client.options(), ctx)?;
        make_openai_client!(client, properties, "openai")
    }

    pub fn new_generic(client: &ClientWalker, ctx: &RuntimeContext) -> Result<OpenAIClient> {
        let properties =
            properties::resolve_properties(&client.elem().provider, client.options(), ctx)?;
        make_openai_client!(client, properties, "openai-generic")
    }

    pub fn new_ollama(client: &ClientWalker, ctx: &RuntimeContext) -> Result<OpenAIClient> {
        let properties =
            properties::resolve_properties(&client.elem().provider, client.options(), ctx)?;
        make_openai_client!(client, properties, "ollama")
    }

    pub fn new_azure(client: &ClientWalker, ctx: &RuntimeContext) -> Result<OpenAIClient> {
        let properties =
            properties::resolve_properties(&client.elem().provider, client.options(), ctx)?;
        make_openai_client!(client, properties, "azure")
    }

    pub fn dynamic_new(client: &ClientProperty, ctx: &RuntimeContext) -> Result<OpenAIClient> {
        let properties =
            properties::resolve_properties(&client.provider, &client.unresolved_options()?, ctx)?;
        make_openai_client!(client, properties, "openai", dynamic)
    }

    pub fn dynamic_new_generic(
        client: &ClientProperty,
        ctx: &RuntimeContext,
    ) -> Result<OpenAIClient> {
        let properties =
            properties::resolve_properties(&client.provider, &client.unresolved_options()?, ctx)?;
        make_openai_client!(client, properties, "openai-generic", dynamic)
    }

    pub fn dynamic_new_ollama(
        client: &ClientProperty,
        ctx: &RuntimeContext,
    ) -> Result<OpenAIClient> {
        let properties =
            properties::resolve_properties(&client.provider, &client.unresolved_options()?, ctx)?;
        make_openai_client!(client, properties, "ollama", dynamic)
    }

    pub fn dynamic_new_azure(
        client: &ClientProperty,
        ctx: &RuntimeContext,
    ) -> Result<OpenAIClient> {
        let properties =
            properties::resolve_properties(&client.provider, &client.unresolved_options()?, ctx)?;
        make_openai_client!(client, properties, "azure", dynamic)
    }
}

impl ToProviderMessage for OpenAIClient {
    fn to_chat_message(
        &self,
        mut content: serde_json::Map<String, serde_json::Value>,
        text: &str,
    ) -> Result<serde_json::Map<String, serde_json::Value>> {
        content.insert("type".into(), json!("text"));
        content.insert("text".into(), json!(text));
        Ok(content)
    }

    fn to_media_message(
        &self,
        mut content: serde_json::Map<String, serde_json::Value>,
        media: &baml_types::BamlMedia,
    ) -> Result<serde_json::Map<String, serde_json::Value>> {
        match media.media_type {
            BamlMediaType::Image => {
                let type_value = "image_url";
                let payload_key = "image_url";
                content.insert("type".into(), json!(type_value));

                match &media.content {
                    BamlMediaContent::Url(url_content) => {
                        content.insert(payload_key.into(), json!({ "url": url_content.url }));
                    }
                    BamlMediaContent::Base64(b64_media) => {
                        content.insert(
                            payload_key.into(),
                            json!({
                                "url": format!("data:{};base64,{}", media.mime_type_as_ok()?, b64_media.base64)
                            }),
                        );
                    }
                    BamlMediaContent::File(_) => {
                        anyhow::bail!("BAML internal error (openai): image file should have been resolved, not processed directly.");
                    }
                }
            }
            BamlMediaType::Audio => {
                let type_value = "input_audio";
                let payload_key = "input_audio";
                content.insert("type".into(), json!(type_value));

                match &media.content {
                    BamlMediaContent::Base64(b64_media) => {
                        let mime_type_str = media.mime_type_as_ok()?;
                        let format_str = match mime_type_str.as_str() {
                            "audio/wav" | "wav" => "wav",
                            "audio/mp3" | "mp3" => "mp3",
                            "audio/mpeg" | "mpeg" => "mp3",
                            // "audio/ogg" | "ogg" => "ogg",
                            // "audio/webm" | "webm" => "webm",
                            // "audio/flac" | "flac" => "flac",
                            // "audio/m4a" | "m4a" => "m4a",
                            // "audio/x-m4a" | "x-m4a" => "x-m4a",
                            // Add other supported formats here if needed in the future
                            _ => anyhow::bail!(
                                "Unsupported audio format for input_audio: '{}'. OpenAI currently supports 'wav' and 'mp3' for this structure.",
                                mime_type_str
                            ),
                        };

                        content.insert(
                            payload_key.into(),
                            json!({
                                "data": b64_media.base64,
                                "format": format_str
                            }),
                        );
                    }
                    BamlMediaContent::Url(_) => {
                        anyhow::bail!(
                            "BAML internal error (openai): Audio content is a URL. Expected Base64 for '{}' type due to client's ResolveMediaUrls::Always setting. The URL should have been resolved to base64 before this stage.",
                            type_value
                        );
                    }
                    BamlMediaContent::File(_) => {
                        anyhow::bail!(
                            "BAML internal error (openai): audio file should have been resolved to base64, not processed directly."
                        );
                    }
                }
            }
        }
        Ok(content)
    }

    fn role_to_message(
        &self,
        content: &RenderedChatMessage,
    ) -> Result<serde_json::Map<String, serde_json::Value>> {
        let mut message = serde_json::Map::new();
        message.insert("role".into(), json!(content.role));
        if self.provider == "openai-generic" {
            // Check if all parts are text
            let all_text = content
                .parts
                .iter()
                .all(|part| matches!(part, ChatMessagePart::Text(_)));
            if all_text {
                // Concatenate all text parts into a single string
                let combined_text = content
                    .parts
                    .iter()
                    .map(|part| {
                        if let ChatMessagePart::Text(text) = part {
                            Ok(text.clone())
                        } else {
                            Err(anyhow::anyhow!("Non-text part encountered"))
                        }
                    })
                    .collect::<Result<Vec<String>>>()?
                    .join(" ");

                message.insert("content".into(), json!(combined_text));
            } else {
                // If there are media parts, use the existing structure
                message.insert(
                    "content".into(),
                    json!(self.parts_to_message(&content.parts)?),
                );
            }
        } else {
            // For other providers, use the existing structure
            message.insert(
                "content".into(),
                json!(self.parts_to_message(&content.parts)?),
            );
        }

        Ok(message)
    }
}

impl ToProviderMessageExt for OpenAIClient {
    fn chat_to_message(
        &self,
        chat: &[RenderedChatMessage],
    ) -> Result<serde_json::Map<String, serde_json::Value>> {
        // merge all adjacent roles of the same type
        let mut res = serde_json::Map::new();

        res.insert(
            "messages".into(),
            chat.iter()
                .map(|c| self.role_to_message(c))
                .collect::<Result<Vec<_>>>()?
                .into(),
        );

        Ok(res)
    }
}

impl CompletionToProviderBody for OpenAIClient {
    fn completion_to_provider_body(
        &self,
        prompt: &str,
    ) -> serde_json::Map<String, serde_json::Value> {
        convert_completion_prompt_to_body(prompt)
    }
}

// converts completion prompt into JSON body for request
fn convert_completion_prompt_to_body(prompt: &str) -> serde_json::Map<String, serde_json::Value> {
    let mut map = serde_json::Map::new();
    map.insert("prompt".into(), json!(prompt));
    map
}
