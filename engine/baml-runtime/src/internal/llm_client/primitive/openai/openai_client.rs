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
        primitive::request::{make_parsed_request, RequestBuilder, ResponseType},
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

/// Provider-specific strategies for handling different OpenAI-compatible APIs
enum ProviderStrategy {
    ResponsesApi,
    StandardOpenAI { provider: String },
}

impl ProviderStrategy {
    fn get_endpoint(&self, base_url: &str, is_completion: bool) -> String {
        match self {
            ProviderStrategy::ResponsesApi => format!("{base_url}/responses"),
            ProviderStrategy::StandardOpenAI { .. } => {
                if is_completion {
                    format!("{base_url}/completions")
                } else {
                    format!("{base_url}/chat/completions")
                }
            }
        }
    }

    fn build_body(
        &self,
        prompt: either::Either<&String, &[RenderedChatMessage]>,
        properties: &BamlMap<String, serde_json::Value>,
        chat_converter: &impl ToProviderMessageExt,
    ) -> Result<serde_json::Value> {
        match self {
            ProviderStrategy::ResponsesApi => {
                // Start with all properties passed through
                let mut body = properties.clone();

                let input = match prompt {
                    either::Either::Left(prompt) => {
                        // For simple string prompts, pass directly as string
                        json!(prompt)
                    }
                    either::Either::Right(messages) => {
                        let structured_messages: Result<Vec<_>> = messages
                                .iter()
                                .map(|msg| {
                                    // Convert message parts to Responses API format
                                    let content_parts: Result<Vec<_>> = msg
                                        .parts
                                        .iter()
                                        .map(|part| match part {
                                            ChatMessagePart::Text(text) => {
                                                let content_type = if msg.role == "assistant" {
                                                    "output_text"
                                                } else {
                                                    "input_text"
                                                };
                                                Ok(json!({
                                                    "type": content_type,
                                                    "text": text
                                                }))
                                            }
                                            ChatMessagePart::Media(media) => {
                                                // For assistant role, we only support text outputs in Responses API
                                                if msg.role == "assistant" {
                                                    anyhow::bail!(
                                                        "BAML internal error (openai-responses): assistant messages must be text; media not supported for assistant in Responses API"
                                                    );
                                                }
                                                match media.media_type {
                                                    baml_types::BamlMediaType::Image => {
                                                        let image_url = match &media.content {
                                                            baml_types::BamlMediaContent::Url(url_content) => url_content.url.clone(),
                                                            baml_types::BamlMediaContent::Base64(b64_media) => {
                                                                format!("data:{};base64,{}", media.mime_type_as_ok()?, b64_media.base64)
                                                            }
                                                            baml_types::BamlMediaContent::File(_) => {
                                                                anyhow::bail!("BAML internal error (openai-responses): image file should have been resolved, not processed directly.");
                                                            }
                                                        };
                                                        Ok(json!({
                                                            "type": "input_image",
                                                            "detail": "auto",
                                                            "image_url": image_url
                                                        }))
                                                    }
                                                    baml_types::BamlMediaType::Audio => {
                                                        match &media.content {
                                                            baml_types::BamlMediaContent::Base64(b64_media) => {
                                                                let mime_type = media.mime_type_as_ok()?;
                                                                let format = mime_type
                                                                    .strip_prefix("audio/")
                                                                    .unwrap_or(&mime_type);
                                                                Ok(json!({
                                                                    "type": "input_audio",
                                                                    "input_audio": {
                                                                        "data": b64_media.base64,
                                                                        "format": format
                                                                    }
                                                                }))
                                                            }
                                                            _ => {
                                                                anyhow::bail!("BAML internal error (openai-responses): audio must be base64 encoded for Responses API");
                                                            }
                                                        }
                                                    }
                                                    baml_types::BamlMediaType::Pdf => {
                                                        match &media.content {
                                                            baml_types::BamlMediaContent::Url(url_content) => {
                                                                Ok(json!({
                                                                    "type": "input_file",
                                                                    "file_url": url_content.url,
                                                                    "filename": "document.pdf"
                                                                }))
                                                            }
                                                            baml_types::BamlMediaContent::File(file_content) => {
                                                                anyhow::bail!("BAML internal error (openai-responses): Local PDF files are not supported by OpenAI Responses API - use file_url for remote files or upload file and use file_id. File path: {:?}", file_content.relpath);
                                                            }
                                                            baml_types::BamlMediaContent::Base64(b64_media) => {
                                                                Ok(json!({
                                                                    "type": "input_file",
                                                                    "file_data": format!("data:{};base64,{}", media.mime_type_as_ok()?, b64_media.base64),
                                                                    "filename": "document.pdf"
                                                                }))
                                                            }
                                                        }
                                                    }
                                                    baml_types::BamlMediaType::Video => {
                                                        anyhow::bail!("BAML internal error (openai-responses): video is not yet supported by OpenAI Responses API");
                                                    }
                                                }
                                            }
                                            ChatMessagePart::WithMeta(inner_part, _meta) => {
                                                // Recursively handle the inner part, ignoring metadata for now
                                                match inner_part.as_ref() {
                                                    ChatMessagePart::Text(text) => {
                                                        let content_type = if msg.role == "assistant" {
                                                            "output_text"
                                                        } else {
                                                            "input_text"
                                                        };
                                                        Ok(json!({
                                                            "type": content_type,
                                                            "text": text
                                                        }))
                                                    }
                                                    ChatMessagePart::Media(media) => {
                                                        // Handle media same as above - could refactor into helper function
                                                        if msg.role == "assistant" {
                                                            anyhow::bail!(
                                                                "BAML internal error (openai-responses): assistant messages must be text; media not supported for assistant in Responses API"
                                                            );
                                                        }
                                                        match media.media_type {
                                                            baml_types::BamlMediaType::Image => {
                                                                let image_url = match &media.content {
                                                                    baml_types::BamlMediaContent::Url(url_content) => url_content.url.clone(),
                                                                    baml_types::BamlMediaContent::Base64(b64_media) => {
                                                                        format!("data:{};base64,{}", media.mime_type_as_ok()?, b64_media.base64)
                                                                    }
                                                                    baml_types::BamlMediaContent::File(_) => {
                                                                        anyhow::bail!("BAML internal error (openai-responses): image file should have been resolved, not processed directly.");
                                                                    }
                                                                };
                                                                Ok(json!({
                                                                    "type": "input_image",
                                                                    "detail": "auto",
                                                                    "image_url": image_url
                                                                }))
                                                            }
                                                            _ => {
                                                                anyhow::bail!("BAML internal error (openai-responses): nested WithMeta media types other than images not yet supported");
                                                            }
                                                        }
                                                    }
                                                    _ => {
                                                        anyhow::bail!("BAML internal error (openai-responses): nested WithMeta parts not supported");
                                                    }
                                                }
                                            }
                                        })
                                        .collect();

                                    Ok(json!({
                                        "role": msg.role,
                                        "content": content_parts?
                                    }))
                                })
                                .collect();
                        json!(structured_messages?)
                    }
                };
                body.insert("input".into(), input);

                Ok(json!(body))
            }
            ProviderStrategy::StandardOpenAI { .. } => {
                let mut body = json!(properties);
                let body_obj = body.as_object_mut().unwrap();

                match prompt {
                    either::Either::Left(prompt) => {
                        body_obj.extend(convert_completion_prompt_to_body(prompt));
                    }
                    either::Either::Right(messages) => {
                        body_obj.extend(chat_converter.chat_to_message(messages)?);
                    }
                }

                Ok(body)
            }
        }
    }

    fn add_streaming_options(
        &self,
        body: &mut serde_json::Map<String, serde_json::Value>,
        stream: bool,
    ) {
        if stream {
            match self {
                ProviderStrategy::ResponsesApi => {
                    // Responses API supports streaming with the stream parameter
                    body.insert("stream".into(), json!(true));
                }
                ProviderStrategy::StandardOpenAI { provider } => {
                    body.insert("stream".into(), json!(true));
                    if provider == "openai" {
                        body.insert(
                            "stream_options".into(),
                            json!({
                                "include_usage": true,
                            }),
                        );
                    }
                }
            }
        }
    }

    fn format_message_content(
        &self,
        content: &RenderedChatMessage,
        parts_to_message: &dyn Fn(
            &[ChatMessagePart],
        )
            -> Result<Vec<serde_json::Map<String, serde_json::Value>>>,
    ) -> Result<serde_json::Value> {
        match self {
            ProviderStrategy::ResponsesApi => {
                // For responses API, use standard formatting
                Ok(json!(parts_to_message(&content.parts)?))
            }
            ProviderStrategy::StandardOpenAI { provider } => {
                if provider == "openai-generic" {
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

                        Ok(json!(combined_text))
                    } else {
                        // If there are media parts, use the existing structure
                        Ok(json!(parts_to_message(&content.parts)?))
                    }
                } else {
                    // For other providers, use the existing structure
                    Ok(json!(parts_to_message(&content.parts)?))
                }
            }
        }
    }
}

impl OpenAIClient {
    fn get_provider_strategy(&self) -> ProviderStrategy {
        if self.provider.as_str() == "openai-responses" {
            ProviderStrategy::ResponsesApi
        } else {
            ProviderStrategy::StandardOpenAI {
                provider: self.provider.clone(),
            }
        }
    }

    fn get_response_type(&self) -> ResponseType {
        match self.get_provider_strategy() {
            ProviderStrategy::ResponsesApi => ResponseType::OpenAIResponses,
            ProviderStrategy::StandardOpenAI { .. } => ResponseType::OpenAI,
        }
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

        let strategy = self.get_provider_strategy();
        let is_completion = prompt.is_left();
        let endpoint = strategy.get_endpoint(destination_url, is_completion);

        let mut req = self.client.post(endpoint);

        // Apply request timeout if configured
        // Defaults were already applied during client creation
        if let Some(ms) = self.properties.http_config.request_timeout_ms {
            if ms > 0 {
                req = req.timeout(std::time::Duration::from_millis(ms));
            }
            // If ms == 0, don't set timeout (infinite timeout)
        }

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

        let mut body = strategy.build_body(prompt, &self.properties.properties, self)?;
        let body_obj = body.as_object_mut().unwrap();

        strategy.add_streaming_options(body_obj, stream);

        Ok(req.json(&body))
    }

    fn request_options(&self) -> &BamlMap<String, serde_json::Value> {
        &self.properties.properties
    }

    fn http_config(&self) -> &internal_llm_client::HttpConfig {
        &self.properties.http_config
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
        let response_type = self.get_response_type();
        crate::internal::llm_client::primitive::stream_request::make_stream_request(
            self,
            either::Either::Right(prompt),
            model_name,
            response_type,
            ctx,
        )
        .await
    }
}

macro_rules! make_openai_client {
    ($client:ident, $properties:ident, $provider:expr, dynamic) => {{
        let http_client = create_http_client(&$properties.http_config)?;
        Ok(Self {
            name: $client.name.clone(),
            provider: $provider.into(),
            context: RenderContext_Client {
                name: $client.name.clone(),
                provider: $client.provider.to_string(),
                default_role: $properties.default_role(),
                allowed_roles: $properties.allowed_roles(),
                options: $properties.properties.clone(),
                remap_role: $properties.remap_role(),
            },
            features: ModelFeatures {
                chat: true,
                completion: false,
                max_one_system_prompt: false,
                resolve_audio_urls: $properties
                    .media_url_handler
                    .audio
                    .map(Into::into)
                    .unwrap_or(ResolveMediaUrls::SendBase64),
                resolve_image_urls: $properties
                    .media_url_handler
                    .images
                    .map(Into::into)
                    .unwrap_or(ResolveMediaUrls::SendUrl),
                resolve_pdf_urls: $properties
                    .media_url_handler
                    .pdf
                    .map(Into::into)
                    .unwrap_or(ResolveMediaUrls::SendUrl),
                resolve_video_urls: $properties
                    .media_url_handler
                    .video
                    .map(Into::into)
                    .unwrap_or(ResolveMediaUrls::SendUrl),
                allowed_metadata: $properties.allowed_metadata.clone(),
            },
            properties: $properties,
            retry_policy: $client.retry_policy.clone(),
            client: http_client,
        })
    }};
    ($client:ident, $properties:ident, $provider:expr) => {{
        let http_client = create_http_client(&$properties.http_config)?;
        Ok(Self {
            name: $client.name().into(),
            provider: $provider.into(),
            context: RenderContext_Client {
                name: $client.name().into(),
                provider: $client.elem().provider.to_string(),
                default_role: $properties.default_role(),
                allowed_roles: $properties.allowed_roles(),
                options: $properties.properties.clone(),
                remap_role: $properties.remap_role(),
            },
            features: ModelFeatures {
                chat: true,
                completion: false,
                max_one_system_prompt: false,
                resolve_audio_urls: $properties
                    .media_url_handler
                    .audio
                    .map(Into::into)
                    .unwrap_or(ResolveMediaUrls::SendBase64),
                resolve_image_urls: $properties
                    .media_url_handler
                    .images
                    .map(Into::into)
                    .unwrap_or(ResolveMediaUrls::SendUrl),
                resolve_pdf_urls: $properties
                    .media_url_handler
                    .pdf
                    .map(Into::into)
                    .unwrap_or(ResolveMediaUrls::SendUrl),
                resolve_video_urls: $properties
                    .media_url_handler
                    .video
                    .map(Into::into)
                    .unwrap_or(ResolveMediaUrls::SendUrl),
                allowed_metadata: $properties.allowed_metadata.clone(),
            },
            properties: $properties,
            retry_policy: $client
                .elem()
                .retry_policy_id
                .as_ref()
                .map(|s| s.to_string()),
            client: http_client,
        })
    }};
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

    pub fn new_responses(client: &ClientWalker, ctx: &RuntimeContext) -> Result<OpenAIClient> {
        let mut properties =
            properties::resolve_properties(&client.elem().provider, client.options(), ctx)?;
        // Override response type for responses API
        properties.client_response_type = internal_llm_client::ResponseType::OpenAIResponses;
        make_openai_client!(client, properties, "openai-responses")
    }

    pub fn new_openrouter(client: &ClientWalker, ctx: &RuntimeContext) -> Result<OpenAIClient> {
        let properties =
            properties::resolve_properties(&client.elem().provider, client.options(), ctx)?;
        make_openai_client!(client, properties, "openrouter")
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

    pub fn dynamic_new_responses(
        client: &ClientProperty,
        ctx: &RuntimeContext,
    ) -> Result<OpenAIClient> {
        let mut properties =
            properties::resolve_properties(&client.provider, &client.unresolved_options()?, ctx)?;
        // Override response type for responses API
        properties.client_response_type = internal_llm_client::ResponseType::OpenAIResponses;
        make_openai_client!(client, properties, "openai-responses", dynamic)
    }

    /// Creates an OpenRouter client from a dynamic client definition (e.g., from Python/TypeScript code).
    ///
    /// OpenRouter provides unified access to 300+ AI models through a single API.
    pub fn dynamic_new_openrouter(
        client: &ClientProperty,
        ctx: &RuntimeContext,
    ) -> Result<OpenAIClient> {
        let properties =
            properties::resolve_properties(&client.provider, &client.unresolved_options()?, ctx)?;
        make_openai_client!(client, properties, "openrouter", dynamic)
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
                        // remove "audio/" from the start of the mime type if it exists
                        let mime_type_str = mime_type_str
                            .strip_prefix("audio/")
                            .unwrap_or(mime_type_str.as_str());

                        // note: openai only supports mp3/wav for audio input
                        // but we can still send other formats and allow openai to handle
                        // the conversion
                        let format_str = match mime_type_str {
                            "mpeg" => "mp3",
                            other => other,
                        };
                        content.insert(
                            payload_key.into(),
                            json!({
                                "data": b64_media.base64,
                                "format": format_str
                            }),
                        );
                    }
                    BamlMediaContent::Url(url_content) => {
                        // note: openai only supports mp3/wav for audio input
                        // but we can still send other formats and allow openai to handle
                        // the conversion
                        let extension = url_content.url.split('.').next_back();

                        // use mime type if it exists otherwise use extension, otherwise error.
                        let extension = match media.mime_type.as_deref() {
                            Some(mime) => mime,
                            None => match extension {
                                Some(ext) => ext,
                                None => anyhow::bail!("BAML internal error (openai): audio url has no extension and no mime type"),
                            },
                        };

                        let format_str = match extension {
                            "mpeg" => "mp3",
                            other => other,
                        };
                        content.insert(
                            payload_key.into(),
                            json!({ "data": url_content.url, "format": format_str }),
                        );
                    }
                    BamlMediaContent::File(_) => {
                        anyhow::bail!(
                            "BAML internal error (openai): audio file should have been resolved to base64, not processed directly."
                        );
                    }
                }
            }
            BamlMediaType::Pdf => {
                let type_value = "file";
                let payload_key = "file";
                content.insert("type".into(), json!(type_value));

                match &media.content {
                    BamlMediaContent::Url(url_content) => {
                        // For URLs, we need to resolve them to base64 first
                        content.insert(
                            payload_key.into(),
                            json!({
                                "type": "input_file",
                                "file_url": url_content.url,
                                "filename": "document.pdf"
                            }),
                        );
                    }
                    BamlMediaContent::Base64(b64_media) => {
                        content.insert(
                            payload_key.into(),
                            json!({
                                "filename": "document.pdf",
                                "file_data": format!("data:{};base64,{}", media.mime_type_as_ok()?, b64_media.base64)
                            }),
                        );
                    }
                    BamlMediaContent::File(media_file) => {
                        // For files, we need to resolve them to base64 first
                        anyhow::bail!(
                            "BAML internal error (openai): Pdf file should have been resolved to base64 before this stage."
                        );
                    }
                }
            }
            BamlMediaType::Video => {
                // OpenAI video is only supported on the Realtime API (/v1/realtime), not on chat completions
                anyhow::bail!(
                    "Video input is only supported on OpenAI's Realtime API (/v1/realtime), not on chat completions. \
                    Consider extracting frames from the video as images instead. \
                    See: https://platform.openai.com/docs/guides/realtime"
                );
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

        let strategy = self.get_provider_strategy();
        let formatted_content =
            strategy.format_message_content(content, &|parts| self.parts_to_message(parts))?;
        message.insert("content".into(), formatted_content);

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

#[cfg(test)]
mod tests {
    use indexmap::IndexMap;
    use internal_baml_jinja::{ChatMessagePart, RenderedChatMessage};
    use internal_llm_client::{openai, RolesSelection, SupportedRequestModes};

    use super::*;

    #[test]
    fn test_provider_strategy_selection() {
        // Mock client with responses provider
        let responses_client = OpenAIClient {
            name: "test".to_string(),
            provider: "openai-responses".to_string(),
            retry_policy: None,
            context: RenderContext_Client {
                name: "test".to_string(),
                provider: "openai-responses".to_string(),
                default_role: "user".to_string(),
                allowed_roles: vec!["user".to_string(), "assistant".to_string()],
                remap_role: HashMap::new(),
                options: IndexMap::new(),
            },
            features: ModelFeatures {
                chat: true,
                completion: false,
                max_one_system_prompt: false,
                resolve_audio_urls: ResolveMediaUrls::SendBase64,
                resolve_image_urls: ResolveMediaUrls::SendUrl,
                resolve_pdf_urls: ResolveMediaUrls::SendUrl,
                resolve_video_urls: ResolveMediaUrls::SendUrl,
                allowed_metadata: AllowedRoleMetadata::All,
            },
            properties: ResolvedOpenAI {
                base_url: "https://api.openai.com/v1".to_string(),
                api_key: None,
                role_selection: RolesSelection::default(),
                allowed_metadata: AllowedRoleMetadata::All,
                supported_request_modes: SupportedRequestModes::default(),
                headers: IndexMap::new(),
                properties: BamlMap::new(),
                query_params: IndexMap::new(),
                proxy_url: None,
                finish_reason_filter: FinishReasonFilter::All,
                client_response_type: ResponseType::OpenAIResponses,
                media_url_handler: internal_llm_client::MediaUrlHandler::default(),
                http_config: Default::default(),
            },
            client: reqwest::Client::new(),
        };

        let strategy = responses_client.get_provider_strategy();

        // Should select ResponsesApi strategy
        match strategy {
            ProviderStrategy::ResponsesApi => {
                // Success!
            }
            _ => panic!("Expected ResponsesApi strategy for openai-responses provider"),
        }
    }

    #[test]
    fn test_standard_openai_strategy_selection() {
        // Mock client with standard openai provider
        let openai_client = OpenAIClient {
            name: "test".to_string(),
            provider: "openai".to_string(),
            retry_policy: None,
            context: RenderContext_Client {
                name: "test".to_string(),
                provider: "openai".to_string(),
                default_role: "user".to_string(),
                allowed_roles: vec!["user".to_string(), "assistant".to_string()],
                remap_role: HashMap::new(),
                options: IndexMap::new(),
            },
            features: ModelFeatures {
                chat: true,
                completion: false,
                max_one_system_prompt: false,
                resolve_audio_urls: ResolveMediaUrls::SendBase64,
                resolve_image_urls: ResolveMediaUrls::SendUrl,
                resolve_pdf_urls: ResolveMediaUrls::SendUrl,
                resolve_video_urls: ResolveMediaUrls::SendUrl,
                allowed_metadata: AllowedRoleMetadata::All,
            },
            properties: ResolvedOpenAI {
                base_url: "https://api.openai.com/v1".to_string(),
                api_key: None,
                role_selection: RolesSelection::default(),
                allowed_metadata: AllowedRoleMetadata::All,
                supported_request_modes: SupportedRequestModes::default(),
                headers: IndexMap::new(),
                properties: BamlMap::new(),
                query_params: IndexMap::new(),
                proxy_url: None,
                finish_reason_filter: FinishReasonFilter::All,
                client_response_type: ResponseType::OpenAI,
                media_url_handler: internal_llm_client::MediaUrlHandler::default(),
                http_config: Default::default(),
            },
            client: reqwest::Client::new(),
        };

        let strategy = openai_client.get_provider_strategy();

        // Should select StandardOpenAI strategy
        match strategy {
            ProviderStrategy::StandardOpenAI { provider } => {
                assert_eq!(provider, "openai");
            }
            _ => panic!("Expected StandardOpenAI strategy for openai provider"),
        }
    }

    #[test]
    fn test_responses_api_endpoint_generation() {
        let strategy = ProviderStrategy::ResponsesApi;
        let endpoint = strategy.get_endpoint("https://api.openai.com/v1", false);
        assert_eq!(endpoint, "https://api.openai.com/v1/responses");
    }

    #[test]
    fn test_standard_openai_endpoint_generation() {
        let strategy = ProviderStrategy::StandardOpenAI {
            provider: "openai".to_string(),
        };

        // Test chat completions endpoint
        let endpoint = strategy.get_endpoint("https://api.openai.com/v1", false);
        assert_eq!(endpoint, "https://api.openai.com/v1/chat/completions");

        // Test completions endpoint
        let endpoint = strategy.get_endpoint("https://api.openai.com/v1", true);
        assert_eq!(endpoint, "https://api.openai.com/v1/completions");
    }

    #[test]
    fn test_responses_api_builds_input_message_with_text_and_file() {
        let strategy = ProviderStrategy::ResponsesApi;

        // Properties include model
        let mut props = BamlMap::new();
        props.insert("model".into(), json!("gpt-5-mini"));

        // Build a user message with text and file (PDF url)
        let msg = RenderedChatMessage {
            role: "user".to_string(),
            allow_duplicate_role: false,
            parts: vec![
                ChatMessagePart::Text("what is in this file?".to_string()),
                ChatMessagePart::Media(baml_types::BamlMedia::url(
                    BamlMediaType::Pdf,
                    "https://www.berkshirehathaway.com/letters/2024ltr.pdf".to_string(),
                    Some("application/pdf".to_string()),
                )),
            ],
        };

        // chat_converter is not used in ResponsesApi branch; construct a minimal client
        let responses_client = OpenAIClient {
            name: "test".to_string(),
            provider: "openai-responses".to_string(),
            retry_policy: None,
            context: RenderContext_Client {
                name: "test".to_string(),
                provider: "openai-responses".to_string(),
                default_role: "user".to_string(),
                allowed_roles: vec!["user".to_string(), "assistant".to_string()],
                remap_role: HashMap::new(),
                options: IndexMap::new(),
            },
            features: ModelFeatures {
                chat: true,
                completion: false,
                max_one_system_prompt: false,
                resolve_audio_urls: ResolveMediaUrls::SendBase64,
                resolve_image_urls: ResolveMediaUrls::SendUrl,
                resolve_pdf_urls: ResolveMediaUrls::SendUrl,
                resolve_video_urls: ResolveMediaUrls::SendUrl,
                allowed_metadata: AllowedRoleMetadata::All,
            },
            properties: ResolvedOpenAI {
                base_url: "https://api.openai.com/v1".to_string(),
                api_key: None,
                role_selection: RolesSelection::default(),
                allowed_metadata: AllowedRoleMetadata::All,
                supported_request_modes: SupportedRequestModes::default(),
                headers: IndexMap::new(),
                properties: BamlMap::new(),
                query_params: IndexMap::new(),
                proxy_url: None,
                finish_reason_filter: FinishReasonFilter::All,
                client_response_type: ResponseType::OpenAIResponses,
                media_url_handler: internal_llm_client::MediaUrlHandler::default(),
                http_config: Default::default(),
            },
            client: reqwest::Client::new(),
        };

        let body_value = strategy
            .build_body(either::Either::Right(&[msg]), &props, &responses_client)
            .expect("should build body");

        let obj = body_value.as_object().expect("body should be an object");
        assert_eq!(obj.get("model"), Some(&json!("gpt-5-mini")));

        let input = obj
            .get("input")
            .and_then(|v| v.as_array())
            .expect("input should be array");
        assert_eq!(input.len(), 1);

        let first_msg = input[0].as_object().expect("message should be object");
        assert_eq!(first_msg.get("role"), Some(&json!("user")));
        let content = first_msg
            .get("content")
            .and_then(|v| v.as_array())
            .expect("content should be array");
        assert_eq!(content.len(), 2);

        // Validate text part
        let t = content[0].as_object().expect("text part object");
        assert_eq!(t.get("type"), Some(&json!("input_text")));
        assert_eq!(t.get("text"), Some(&json!("what is in this file?")));

        // Validate file part
        let f = content[1].as_object().expect("file part object");
        assert_eq!(f.get("type"), Some(&json!("input_file")));
        assert_eq!(
            f.get("file_url"),
            Some(&json!(
                "https://www.berkshirehathaway.com/letters/2024ltr.pdf"
            ))
        );
    }
}
