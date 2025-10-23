use std::{collections::HashMap, time::Duration};

use anyhow::{Context, Result};
use baml_types::{BamlMap, BamlMedia, BamlMediaContent};
use eventsource_stream::Eventsource;
use futures::StreamExt;
use http::header;
use internal_baml_core::ir::ClientWalker;
use internal_baml_jinja::{ChatMessagePart, RenderContext_Client, RenderedChatMessage};
use internal_llm_client::{
    google_ai::ResolvedGoogleAI, AllowedRoleMetadata, ClientProvider, ResolvedClientProperty,
    UnresolvedClientProperty,
};
use secrecy::ExposeSecret;
use serde_json::json;

use crate::{
    client_registry::ClientProperty,
    internal::llm_client::{
        primitive::{
            google::types::GoogleResponse,
            request::{make_parsed_request, RequestBuilder, ResponseType},
        },
        traits::{
            CompletionToProviderBody, HttpContext, SseResponseTrait, StreamResponse,
            ToProviderMessage, ToProviderMessageExt, WithChat, WithClient, WithClientProperties,
            WithNoCompletion, WithRetryPolicy, WithStreamChat,
        },
        ErrorCode, LLMCompleteResponse, LLMCompleteResponseMetadata, LLMErrorResponse, LLMResponse,
        ModelFeatures, ResolveMediaUrls,
    },
    request::{create_client, create_http_client},
    RuntimeContext,
};

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

// makes the request to the google client, on success it triggers the response_stream function to handle continuous rendering with the response object
impl WithStreamChat for GoogleAIClient {
    async fn stream_chat(
        &self,
        ctx: &impl HttpContext,
        prompt: &[RenderedChatMessage],
    ) -> StreamResponse {
        let model_name = self.properties.model.clone();
        //incomplete, streaming response object is returned
        crate::internal::llm_client::primitive::stream_request::make_stream_request(
            self,
            either::Either::Right(prompt),
            Some(model_name),
            ResponseType::Google,
            ctx,
        )
        .await
    }
}

impl GoogleAIClient {
    pub fn new(client: &ClientWalker, ctx: &RuntimeContext) -> Result<Self> {
        let properties = resolve_properties(&client.elem().provider, client.options(), ctx)?;
        Ok(Self {
            name: client.name().into(),
            context: RenderContext_Client {
                name: client.name().into(),
                provider: client.elem().provider.to_string(),
                default_role: properties.default_role(),
                allowed_roles: properties.allowed_roles(),
                options: properties.properties.clone(),
                remap_role: properties.remap_role(),
            },
            features: ModelFeatures {
                chat: true,
                completion: false,
                max_one_system_prompt: true,
                resolve_audio_urls: properties
                    .media_url_handler
                    .audio
                    .map(Into::into)
                    .unwrap_or(ResolveMediaUrls::SendUrl),
                resolve_image_urls: properties
                    .media_url_handler
                    .images
                    .map(Into::into)
                    .unwrap_or(ResolveMediaUrls::SendBase64UnlessGoogleUrl),
                resolve_pdf_urls: properties
                    .media_url_handler
                    .pdf
                    .map(Into::into)
                    .unwrap_or(ResolveMediaUrls::SendUrl),
                resolve_video_urls: properties
                    .media_url_handler
                    .video
                    .map(Into::into)
                    .unwrap_or(ResolveMediaUrls::SendUrl),
                allowed_metadata: properties.allowed_metadata.clone(),
            },
            retry_policy: client.elem().retry_policy_id.as_ref().map(String::to_owned),
            client: create_http_client(&properties.http_config)?,
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
                options: properties.properties.clone(),
                remap_role: properties.remap_role(),
            },
            features: ModelFeatures {
                chat: true,
                completion: false,
                max_one_system_prompt: true,
                resolve_audio_urls: properties
                    .media_url_handler
                    .audio
                    .map(Into::into)
                    .unwrap_or(ResolveMediaUrls::SendUrl),
                resolve_image_urls: properties
                    .media_url_handler
                    .images
                    .map(Into::into)
                    .unwrap_or(ResolveMediaUrls::SendBase64UnlessGoogleUrl),
                resolve_pdf_urls: properties
                    .media_url_handler
                    .pdf
                    .map(Into::into)
                    .unwrap_or(ResolveMediaUrls::SendUrl),
                resolve_video_urls: properties
                    .media_url_handler
                    .video
                    .map(Into::into)
                    .unwrap_or(ResolveMediaUrls::SendUrl),
                allowed_metadata: properties.allowed_metadata.clone(),
            },
            retry_policy: client.retry_policy.clone(),
            client: create_http_client(&properties.http_config)?,
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

        // Apply request timeout if configured
        // Defaults were already applied during client creation
        if let Some(ms) = self.properties.http_config.request_timeout_ms {
            if ms > 0 {
                req = req.timeout(Duration::from_millis(ms));
            }
            // If ms == 0, don't set timeout (infinite timeout)
        }

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

    fn http_config(&self) -> &internal_llm_client::HttpConfig {
        &self.properties.http_config
    }
}

impl WithChat for GoogleAIClient {
    async fn chat(&self, ctx: &impl HttpContext, prompt: &[RenderedChatMessage]) -> LLMResponse {
        let model_name = self.properties.model.clone();
        //non-streaming, complete response is returned
        make_parsed_request(
            self,
            Some(model_name),
            either::Either::Right(prompt),
            false,
            ResponseType::Google,
            ctx,
        )
        .await
    }
}

//simple, Map with key "prompt" and value of the prompt string
fn convert_completion_prompt_to_body(prompt: &str) -> serde_json::Map<String, serde_json::Value> {
    let mut map = serde_json::Map::new();
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
                    "inline_data".into(),
                    json!({
                        "mime_type": media.mime_type_as_ok()?,
                        "data": data.base64
                    }),
                );
                Ok(content)
            }
            BamlMediaContent::Url(data) => {
                // Pass through external media via `file_data` as required by Gemini API.
                let mut file_data = json!({ "file_uri": data.url });
                if let Some(mime) = &media.mime_type {
                    file_data["mime_type"] = json!(mime);
                }
                content.insert("file_data".into(), file_data);
                Ok(content)
            }
            BamlMediaContent::File(_) => anyhow::bail!(
                "BAML internal error (google-ai): file should have been resolved to base64"
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

impl CompletionToProviderBody for GoogleAIClient {
    fn completion_to_provider_body(
        &self,
        prompt: &str,
    ) -> serde_json::Map<String, serde_json::Value> {
        convert_completion_prompt_to_body(prompt)
    }
}
