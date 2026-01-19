use std::{borrow::Cow, collections::HashMap, ops::Deref, sync::Arc};

use anyhow::{Context, Result};
use aws_config::{
    identity::IdentityCache, retry::RetryConfig, BehaviorVersion, ConfigLoader, Region,
};
use aws_credential_types::{
    provider::{
        error::{CredentialsError, CredentialsNotLoaded},
        future::ProvideCredentials as ProvideCredentialsFuture,
        ProvideCredentials,
    },
    Credentials,
};
use aws_sdk_bedrockruntime::{
    self as bedrock,
    config::{Intercept, StalledStreamProtectionConfig},
    operation::converse::ConverseOutput,
    types::CitationsConfig,
    Client as BedrockRuntimeClient,
};
use aws_smithy_json::serialize::JsonObjectWriter;
use aws_smithy_runtime_api::{client::result::SdkError, http::Headers};
use aws_smithy_types::{Blob, Document};
use baml_ids::{FunctionCallId, HttpRequestId};
use baml_types::{
    tracing::events::{
        ClientDetails, HTTPBody, HTTPRequest, HTTPResponse, HTTPResponseStream, SSEEvent,
        TraceData, TraceEvent,
    },
    ApiKeyWithProvenance, BamlMap, BamlMedia, BamlMediaContent, BamlMediaType,
};
use futures::stream;
use internal_baml_core::ir::ClientWalker;
use internal_baml_jinja::{ChatMessagePart, RenderContext_Client, RenderedChatMessage};
use internal_llm_client::{
    aws_bedrock::{self, ResolvedAwsBedrock},
    AllowedRoleMetadata, ClientProvider, ResolvedClientProperty, UnresolvedClientProperty,
};
use percent_encoding::{utf8_percent_encode, AsciiSet, NON_ALPHANUMERIC};
use secrecy::ExposeSecret;
use serde::Deserialize;
use serde_json::{json, Map};
use shell_escape::escape;
use url::Url;
use uuid::Uuid;
use web_time::{Instant, SystemTime};

// See https://github.com/awslabs/aws-sdk-rust/issues/169
use super::custom_http_client;
#[cfg(target_arch = "wasm32")]
use super::wasm::WasmAwsCreds;
use crate::{
    client_registry::ClientProperty,
    internal::llm_client::{
        primitive::request::RequestBuilder,
        traits::{
            HttpContext, StreamResponse, ToProviderMessageExt, WithChat, WithClient,
            WithClientProperties, WithNoCompletion, WithRenderRawCurl, WithRetryPolicy,
            WithStreamChat,
        },
        ErrorCode, LLMCompleteResponse, LLMCompleteResponseMetadata, LLMErrorResponse, LLMResponse,
        ModelFeatures, ResolveMediaUrls,
    },
    json_body,
    tracingv2::storage::storage::BAML_TRACER,
    JsonBodyInput, RenderCurlSettings, RuntimeContext,
};

// Strip the MIME type prefix ("type/subtype" -> "subtype").
fn strip_mime_prefix(mime: &str) -> &str {
    mime.split_once('/').map(|(_, s)| s).unwrap_or(mime)
}

fn media_to_content_block_json(media: &BamlMedia) -> Result<serde_json::Value> {
    let content_block = {
        let mut obj = Map::new();
        if let Some(mime) = media.mime_type.as_deref() {
            obj.insert("format".into(), json!(strip_mime_prefix(mime)));
        }
        let source = match &media.content {
            BamlMediaContent::File(media_file) => todo!(),
            BamlMediaContent::Url(url) => {
                let parsed = Url::parse(&url.url).with_context(|| {
                    format!("Invalid S3 URI for AWS Bedrock video source: {url}")
                })?;

                if parsed.scheme() != "s3" {
                    anyhow::bail!("AWS Bedrock requires s3:// URIs, but got: {}", url.url);
                }

                // unimplemented!("make sure the test works")
                json!({
                    "s3Location": {
                        "uri": url.url,
                    }
                })
            }
            BamlMediaContent::Base64(base64) => json!({
                "bytes": base64.base64,
            }),
        };
        obj.insert("source".into(), source);
        obj
    };
    match media.media_type {
        // _ => anyhow::bail!("AWS Bedrock only supports base64 image inputs in modular requests"),
        BamlMediaType::Image => Ok(json!({ "image": content_block })),
        BamlMediaType::Pdf => {
            let mut content_block = content_block;
            content_block.insert("name".into(), json!("document"));
            Ok(json!({ "document": content_block }))
        }
        BamlMediaType::Video => Ok(json!({ "video": content_block })),
        BamlMediaType::Audio => Ok(json!({ "audio": content_block })),
    }
}

fn system_part_to_json(part: &ChatMessagePart) -> Result<serde_json::Value> {
    match part {
        ChatMessagePart::Text(t) => Ok(json!({ "text": t })),
        ChatMessagePart::WithMeta(p, _) => system_part_to_json(p),
        other => anyhow::bail!("AWS Bedrock only supports text system blocks, but got {other:?}"),
    }
}

fn chat_part_to_json(part: &ChatMessagePart) -> Result<serde_json::Value> {
    match part {
        ChatMessagePart::Text(t) => Ok(json!({ "text": t })),
        ChatMessagePart::Media(media) => media_to_content_block_json(media),
        ChatMessagePart::WithMeta(inner, _) => chat_part_to_json(inner),
    }
}

// represents client that interacts with the Bedrock API
pub struct AwsClient {
    pub name: String,
    retry_policy: Option<String>,
    context: RenderContext_Client,
    features: ModelFeatures,
    properties: ResolvedAwsBedrock,
}

const PATH_SEGMENT_ENCODE_SET: &AsciiSet = &NON_ALPHANUMERIC
    .remove(b'-')
    .remove(b'_')
    .remove(b'.')
    .remove(b'~');

fn resolve_properties(
    provider: &ClientProvider,
    properties: &UnresolvedClientProperty<()>,
    ctx: &RuntimeContext,
) -> Result<ResolvedAwsBedrock> {
    let strict = {
        #[cfg(target_arch = "wasm32")]
        {
            false
        }

        #[cfg(not(target_arch = "wasm32"))]
        true
    };
    let properties = properties.resolve(provider, &ctx.eval_ctx(strict))?;

    let ResolvedClientProperty::AWSBedrock(props) = properties else {
        anyhow::bail!(
            "Invalid client property. Should have been a aws-bedrock property but got: {}",
            properties.name()
        );
    };

    Ok(props)
}

// Helper function to convert serde_json::Value to aws_smithy_types::Document
fn serde_json_to_aws_document(value: serde_json::Value) -> Document {
    match value {
        serde_json::Value::Null => Document::Null,
        serde_json::Value::Bool(b) => Document::Bool(b),
        serde_json::Value::Number(n) => {
            if n.is_i64() {
                Document::Number(aws_smithy_types::Number::NegInt(n.as_i64().unwrap()))
            } else if n.is_u64() {
                Document::Number(aws_smithy_types::Number::PosInt(n.as_u64().unwrap()))
            } else {
                // Fallback to f64
                Document::Number(aws_smithy_types::Number::Float(
                    n.as_f64().unwrap_or(f64::NAN),
                ))
            }
        }
        serde_json::Value::String(s) => Document::String(s),
        serde_json::Value::Array(arr) => {
            Document::Array(arr.into_iter().map(serde_json_to_aws_document).collect())
        }
        serde_json::Value::Object(map) => {
            let converted_map: HashMap<String, Document> = map
                .into_iter()
                .map(|(k, v)| (k, serde_json_to_aws_document(v)))
                .collect();
            Document::Object(converted_map)
        }
    }
}

#[derive(Debug)]
struct CollectorInterceptor {
    call_stack: Vec<baml_ids::FunctionCallId>,
    http_request_id: baml_ids::HttpRequestId,
    client_details: ClientDetails,
}

impl CollectorInterceptor {
    pub fn new(
        call_stack: Vec<baml_ids::FunctionCallId>,
        http_request_id: baml_ids::HttpRequestId,
        resolved_properties: &ResolvedAwsBedrock,
    ) -> Self {
        Self {
            call_stack,
            http_request_id,
            client_details: ClientDetails {
                name: resolved_properties.model.clone(),
                provider: "aws".to_string(),
                options: resolved_properties.client_options(),
            },
        }
    }
}

pub fn smithy_json_headers(headers: &Headers) -> HashMap<String, String> {
    let mut json_headers = HashMap::new();
    for (key, value) in headers.iter() {
        json_headers.insert(key.to_string(), value.to_string());
    }
    json_headers
}

impl aws_smithy_runtime_api::client::interceptors::Intercept for CollectorInterceptor {
    fn name(&self) -> &'static str {
        "CollectorInterceptor"
    }

    fn read_before_attempt(
        &self,
        context: &aws_sdk_bedrockruntime::config::interceptors::BeforeTransmitInterceptorContextRef<
            '_,
        >,
        _runtime_components: &aws_sdk_bedrockruntime::config::RuntimeComponents,
        _cfg: &mut aws_smithy_types::config_bag::ConfigBag,
    ) -> std::result::Result<(), aws_sdk_bedrockruntime::error::BoxError> {
        let request = context.request();
        let headers = smithy_json_headers(request.headers());
        let body = if let Some(bytes) = request.body().bytes() {
            json_body(JsonBodyInput::Bytes(bytes)).unwrap_or_default()
        } else {
            serde_json::Value::Null
        };
        let request = HTTPRequest::new(
            self.http_request_id.clone(),
            request.uri().to_string(),
            request.method().to_string(),
            headers,
            HTTPBody::new(request.body().bytes().unwrap_or_default().to_vec()),
            self.client_details.clone(),
        );
        let call_stack = self.call_stack.clone();
        let request = Arc::new(request);
        let event = TraceEvent::new_raw_llm_request(call_stack, request);
        BAML_TRACER.lock().unwrap().put(Arc::new(event));

        Ok(())
    }

    fn read_after_attempt(
        &self,
        context: &aws_sdk_bedrockruntime::config::interceptors::FinalizerInterceptorContextRef<'_>,
        _runtime_components: &aws_sdk_bedrockruntime::config::RuntimeComponents,
        _cfg: &mut aws_smithy_types::config_bag::ConfigBag,
    ) -> std::result::Result<(), aws_sdk_bedrockruntime::error::BoxError> {
        if let Some(response) = context.response() {
            let response = HTTPResponse::new(
                self.http_request_id.clone(),
                response.status().as_u16(),
                Some(smithy_json_headers(response.headers())),
                HTTPBody::new(response.body().bytes().unwrap_or_default().to_vec()),
                self.client_details.clone(),
            );

            let event =
                TraceEvent::new_raw_llm_response(self.call_stack.clone(), Arc::new(response));
            BAML_TRACER.lock().unwrap().put(Arc::new(event));
        }

        Ok(())
    }
}

/// If the user has explicitly provided credentials via options on a client,
/// we use this provider
#[derive(Debug)]
struct ExplicitCredentialsProvider {
    access_key_id: Option<String>,
    secret_access_key: Option<ApiKeyWithProvenance>,
    session_token: Option<String>,
}

impl aws_credential_types::provider::ProvideCredentials for ExplicitCredentialsProvider {
    fn provide_credentials<'a>(&'a self) -> ProvideCredentialsFuture<'a>
    where
        Self: 'a,
    {
        ProvideCredentialsFuture::ready(match (&self.access_key_id, &self.secret_access_key, &self.session_token) {
            (None, None, None) => {
                Err(CredentialsError::unhandled("BAML internal error: ExplicitCredentialsProvider should only be constructed if either access_key_id or secret_access_key are provided"))
            }
            (Some(access_key_id), Some(secret_access_key), session_token) => {
                Ok(Credentials::new(access_key_id, secret_access_key.api_key.expose_secret(), session_token.clone(), None, "baml-explicit-credentials"))
            }
            (_, _, None) => {
                Err(CredentialsError::invalid_configuration("If either access_key_id or secret_access_key are provided, both must be provided."))
            }
            (_, _, Some(_)) => {
                Err(CredentialsError::invalid_configuration("If either access_key_id or secret_access_key are provided, both must be provided. If session_token is provided, all three must be provided."))
            }
        })
    }
}

impl AwsClient {
    fn build_converse_body_json(
        &self,
        prompt: &[RenderedChatMessage],
    ) -> Result<serde_json::Map<String, serde_json::Value>> {
        let mut system_blocks: Option<Vec<serde_json::Value>> = None;
        let mut chat_slice = prompt;

        if let Some((first, remainder)) = chat_slice.split_first() {
            if first.role == "system" {
                let mut blocks = Vec::new();
                for part in &first.parts {
                    blocks.push(system_part_to_json(part)?);
                }
                system_blocks = Some(blocks);
                chat_slice = remainder;
            }
        }

        let mut messages_json: Vec<serde_json::Value> = Vec::new();
        for message in chat_slice {
            let mut content_blocks = Vec::new();
            for part in &message.parts {
                content_blocks.push(chat_part_to_json(part)?);
            }
            messages_json.push(json!({
                "role": message.role,
                "content": content_blocks,
            }));
        }

        let mut root = serde_json::Map::new();
        root.insert("messages".into(), serde_json::Value::Array(messages_json));

        if let Some(system) = system_blocks {
            root.insert("system".into(), serde_json::Value::Array(system));
        }

        if let Some(cfg) = &self.properties.inference_config {
            let mut map = serde_json::Map::new();
            if let Some(v) = cfg.max_tokens {
                map.insert("maxTokens".into(), json!(v));
            }
            if let Some(v) = cfg.temperature {
                map.insert("temperature".into(), json!(v));
            }
            if let Some(v) = cfg.top_p {
                map.insert("topP".into(), json!(v));
            }
            if let Some(v) = cfg.stop_sequences.as_ref() {
                map.insert("stopSequences".into(), json!(v));
            }
            if !map.is_empty() {
                root.insert("inferenceConfig".into(), serde_json::Value::Object(map));
            }
        }

        if !self.properties.additional_model_request_fields.is_empty() {
            let addl = serde_json::to_value(&self.properties.additional_model_request_fields)?;
            root.insert("additionalModelRequestFields".into(), addl);
        }

        Ok(root)
    }

    pub async fn build_modular_http_request(
        &self,
        ctx: &RuntimeContext,
        chat_messages: &[RenderedChatMessage],
        stream: bool,
        request_id: HttpRequestId,
    ) -> Result<HTTPRequest> {
        if stream {
            anyhow::bail!(
                "AWS Bedrock modular streaming is not supported. Use non-streaming modular requests."
            );
        }

        let region = self.properties.region.clone().unwrap_or_else(|| {
            ctx.env_vars()
                .get("AWS_REGION")
                .cloned()
                .unwrap_or_default()
        });

        if region.is_empty() {
            anyhow::bail!(
                "AWS region is required to build modular request. Set it in the client options or via AWS_REGION."
            );
        }

        let body_string = serde_json::to_string(&serde_json::Value::Object(
            self.build_converse_body_json(chat_messages)?,
        ))?;
        let body_bytes = body_string.as_bytes().to_vec();

        let host = format!("bedrock-runtime.{region}.amazonaws.com");
        let encoded_model =
            utf8_percent_encode(&self.properties.model, PATH_SEGMENT_ENCODE_SET).to_string();
        let url = format!("https://{host}/model/{encoded_model}/converse");

        let mut header_map = HashMap::new();
        header_map.insert("content-type".to_string(), "application/json".to_string());
        header_map.insert("accept".to_string(), "application/json".to_string());

        Ok(HTTPRequest::new(
            request_id,
            url,
            "POST".to_string(),
            header_map,
            HTTPBody::new(body_bytes),
            ClientDetails {
                name: self.context.name.clone(),
                provider: "aws-bedrock".to_string(),
                options: self.properties.client_options(),
            },
        ))
    }
    pub fn dynamic_new(client: &ClientProperty, ctx: &RuntimeContext) -> Result<AwsClient> {
        let properties = resolve_properties(&client.provider, &client.unresolved_options()?, ctx)?;

        Ok(Self {
            name: client.name.clone(),
            context: RenderContext_Client {
                name: client.name.clone(),
                provider: client.provider.to_string(),
                default_role: properties.default_role(),
                allowed_roles: properties.allowed_roles(),
                options: properties.client_options(),
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
                    .unwrap_or(ResolveMediaUrls::SendBase64),
                resolve_image_urls: properties
                    .media_url_handler
                    .images
                    .map(Into::into)
                    .unwrap_or(ResolveMediaUrls::SendBase64),
                resolve_pdf_urls: properties
                    .media_url_handler
                    .pdf
                    .map(Into::into)
                    .unwrap_or(ResolveMediaUrls::SendBase64),
                resolve_video_urls: properties
                    .media_url_handler
                    .video
                    .map(Into::into)
                    .unwrap_or(ResolveMediaUrls::SendUrl),
                allowed_metadata: properties.allowed_role_metadata.clone(),
            },
            retry_policy: client.retry_policy.as_ref().map(String::to_owned),
            properties,
        })
    }

    pub fn new(client: &ClientWalker, ctx: &RuntimeContext) -> Result<AwsClient> {
        let properties = resolve_properties(&client.elem().provider, client.options(), ctx)?;

        Ok(Self {
            name: client.name().into(),
            context: RenderContext_Client {
                name: client.name().into(),
                provider: client.elem().provider.to_string(),
                default_role: properties.default_role(),
                allowed_roles: properties.allowed_roles(),
                options: properties.client_options(),
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
                    .unwrap_or(ResolveMediaUrls::SendBase64),
                resolve_image_urls: properties
                    .media_url_handler
                    .images
                    .map(Into::into)
                    .unwrap_or(ResolveMediaUrls::SendBase64),
                resolve_pdf_urls: properties
                    .media_url_handler
                    .pdf
                    .map(Into::into)
                    .unwrap_or(ResolveMediaUrls::SendBase64),
                resolve_video_urls: properties
                    .media_url_handler
                    .video
                    .map(Into::into)
                    .unwrap_or(ResolveMediaUrls::SendUrl),
                allowed_metadata: properties.allowed_role_metadata.clone(),
            },
            retry_policy: client.elem().retry_policy_id.as_ref().map(String::to_owned),
            properties,
        })
    }

    pub fn request_options(&self) -> &BamlMap<String, serde_json::Value> {
        // TODO:(vbv) - use inference config for this.
        static DEFAULT_REQUEST_OPTIONS: std::sync::OnceLock<BamlMap<String, serde_json::Value>> =
            std::sync::OnceLock::new();
        DEFAULT_REQUEST_OPTIONS.get_or_init(Default::default)
    }

    pub fn http_config(&self) -> &internal_llm_client::HttpConfig {
        &self.properties.http_config
    }

    // TODO: this should be memoized on client construction, but because config loading is async,
    // we can't do this in AwsClient::new (which is called from LLMPRimitiveProvider::try_from)
    // Note: This function necessarily exposes secret keys when they are provided, so it should
    // only be called while generating real requests to the provider, not when rendering raw
    // cURL previews.
    async fn client_anyhow(
        &self,
        call_stack: Vec<baml_ids::FunctionCallId>,
        http_request_id: baml_ids::HttpRequestId,
    ) -> Result<bedrock::Client> {
        #[cfg(target_arch = "wasm32")]
        let loader = super::wasm::load_aws_config();

        #[cfg(not(target_arch = "wasm32"))]
        let loader = aws_config::defaults(BehaviorVersion::latest());

        let mut loader = match (
            self.properties.access_key_id.as_ref(),
            self.properties.secret_access_key.as_ref(),
            self.properties.session_token.as_ref(),
        ) {
            (None, None, None) => {
                #[cfg(target_arch = "wasm32")]
                {
                    loader.credentials_provider(WasmAwsCreds {
                        profile: self.properties.profile.clone(),
                    })
                }

                #[cfg(not(target_arch = "wasm32"))]
                {
                    let mut builder =
                        aws_config::default_provider::credentials::DefaultCredentialsChain::builder(
                        );
                    if let Some(profile) = self.properties.profile.as_ref() {
                        builder = builder.profile_name(profile);
                    }
                    // Add region to DefaultCredentialsChain for IRSA support.
                    // IRSA (IAM Roles for Service Accounts) uses STS AssumeRoleWithWebIdentity
                    // which requires a region to make the STS API call.
                    if let Some(region) = self.properties.region.as_ref() {
                        builder = builder.region(Region::new(region.clone()));
                    }
                    loader.credentials_provider(builder.build().await)
                }
            }
            // Env var resolution is pretty nasty, see
            // https://gloo-global.slack.com/archives/C03KV1PJ6EM/p1743832043661209
            _ => loader.credentials_provider(ExplicitCredentialsProvider {
                access_key_id: match &self.properties.access_key_id {
                    Some(access_key_id) => {
                        if access_key_id.starts_with("$") {
                            None
                        } else {
                            Some(access_key_id.clone())
                        }
                    }
                    None => None,
                },
                secret_access_key: match &self.properties.secret_access_key {
                    Some(secret_access_key) => {
                        if secret_access_key.api_key.expose_secret().starts_with("$") {
                            None
                        } else {
                            Some(secret_access_key.clone())
                        }
                    }
                    None => None,
                },
                session_token: match &self.properties.session_token {
                    Some(session_token) => {
                        if session_token.starts_with("$") {
                            None
                        } else {
                            Some(session_token.clone())
                        }
                    }
                    None => None,
                },
            }),
        };

        // Set region if specified
        if let Some(aws_region) = self.properties.region.as_ref() {
            if let Some(v) = aws_region.strip_prefix("$") {
                return Err(anyhow::anyhow!("AWS region expected, please set: env.{v}",));
            }

            loader = loader.region(Region::new(aws_region.clone()));
        }

        let config = loader.load().await;
        let http_client = custom_http_client::client()?;

        let mut bedrock_config = aws_sdk_bedrockruntime::config::Builder::from(&config)
            // To support HTTPS_PROXY https://github.com/awslabs/aws-sdk-rust/issues/169
            .http_client(http_client)
            // Adding a custom http client (above) breaks the stalled stream protection for some reason. If a bedrock request takes longer than 5s (the default grace period, it makes it error out), so we disable it.
            .stalled_stream_protection(StalledStreamProtectionConfig::disabled())
            .interceptor(CollectorInterceptor::new(
                call_stack,
                http_request_id.clone(),
                &self.properties,
            ));

        // Set endpoint_url if specified
        if let Some(endpoint_url) = self.properties.endpoint_url.as_ref() {
            bedrock_config = bedrock_config.endpoint_url(endpoint_url);
        }

        Ok(BedrockRuntimeClient::from_conf(bedrock_config.build()))
    }

    async fn chat_anyhow(&self, response: &ConverseOutput) -> Result<String> {
        let Some(bedrock::types::ConverseOutput::Message(ref message)) = response.output else {
            anyhow::bail!(
                "Expected message output in response, but is type {}",
                "unknown"
            );
        };
        // Try to extract text from all content blocks
        let mut extracted_text = String::new();
        let mut has_text = false;

        if message.content.is_empty() {
            anyhow::bail!("Expected message output to have content, but content is empty");
        }

        for content_block in &message.content {
            if let bedrock::types::ContentBlock::Text(text) = content_block {
                has_text = true;
                extracted_text.push_str(text);
            }
        }

        // If we found at least one text block, return the concatenated text
        if has_text {
            let content = extracted_text;
            return Ok(content);
        }

        // If we didn't find any text blocks, return an error with details about the content
        anyhow::bail!(
            "Expected message output to contain at least one text block, but found none. Content: {:?}",
            message.content.iter().map(|block| match block {
                bedrock::types::ContentBlock::Image(_) => "image",
                bedrock::types::ContentBlock::GuardContent(_) => "guardContent",
                bedrock::types::ContentBlock::ToolResult(_) => "toolResult",
                bedrock::types::ContentBlock::ToolUse(_) => "toolUse",
                bedrock::types::ContentBlock::Text(_) => "text",
                bedrock::types::ContentBlock::ReasoningContent(_) => "reasoningContent",
                // bedrock::types::ContentBlock::CachePoint(_) => "cachePoint",
                bedrock::types::ContentBlock::Document(_) => "document",
                bedrock::types::ContentBlock::Video(_) => "video",
                _ => "unknown",
            }).collect::<Vec<_>>()
        );
    }

    fn build_request(
        &self,
        ctx: &RuntimeContext,
        chat_messages: &[RenderedChatMessage],
    ) -> Result<bedrock::operation::converse::ConverseInput> {
        let mut system_message = None;
        let mut chat_slice = chat_messages;

        if let Some((first, remainder_slice)) = chat_slice.split_first() {
            if first.role == "system" {
                system_message = Some(
                    first
                        .parts
                        .iter()
                        .map(Self::part_to_system_message)
                        .collect::<Result<_>>()?,
                );
                chat_slice = remainder_slice;
            }
        }

        let converse_messages = chat_slice
            .iter()
            .map(|m| self.role_to_message(m))
            .collect::<Result<Vec<_>>>()?;

        let inference_config = self.properties.inference_config.as_ref().map(|curr| {
            aws_sdk_bedrockruntime::types::InferenceConfiguration::builder()
                .set_max_tokens(curr.max_tokens)
                .set_temperature(curr.temperature)
                .set_top_p(curr.top_p)
                .set_stop_sequences(curr.stop_sequences.clone())
                .build()
        });

        let additional_fields_doc = {
            let json_map: serde_json::Map<String, serde_json::Value> = self
                .properties
                .additional_model_request_fields
                .iter()
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect();
            let json_value = serde_json::Value::Object(json_map);
            serde_json_to_aws_document(json_value)
        };

        bedrock::operation::converse::ConverseInput::builder()
            .set_inference_config(inference_config)
            .set_additional_model_request_fields(Some(additional_fields_doc))
            .set_model_id(Some(self.properties.model.clone()))
            .set_system(system_message)
            .set_messages(Some(converse_messages))
            .build()
            .context("Failed to convert BAML prompt to AWS Bedrock request")
    }
}

fn try_to_json<
    Ser: Fn(
        &mut JsonObjectWriter,
        &T,
    ) -> Result<(), ::aws_smithy_types::error::operation::SerializationError>,
    T,
>(
    shape: Ser,
    input: &T,
) -> Result<String> {
    let mut out = String::new();
    let mut object = JsonObjectWriter::new(&mut out);
    shape(&mut object, input)?;
    object.finish();

    Ok(out)
}

/// Get the event type name for a Bedrock ConverseStreamOutput message.
fn bedrock_stream_event_type(message: &bedrock::types::ConverseStreamOutput) -> &'static str {
    match message {
        bedrock::types::ConverseStreamOutput::ContentBlockDelta(_) => "content_block_delta",
        bedrock::types::ConverseStreamOutput::ContentBlockStart(_) => "content_block_start",
        bedrock::types::ConverseStreamOutput::ContentBlockStop(_) => "content_block_stop",
        bedrock::types::ConverseStreamOutput::MessageStart(_) => "message_start",
        bedrock::types::ConverseStreamOutput::MessageStop(_) => "message_stop",
        bedrock::types::ConverseStreamOutput::Metadata(_) => "metadata",
        _ => "unknown",
    }
}

/// Serialize an AWS SDK struct to JSON using Debug format.
/// AWS SDK types don't implement Serialize, so we wrap Debug output in valid JSON.
/// See: https://github.com/awslabs/aws-sdk-rust/issues/645
fn serialize_aws_struct<T: std::fmt::Debug>(value: &T) -> String {
    json!({"debug": format!("{:?}", value)}).to_string()
}

impl WithRenderRawCurl for AwsClient {
    async fn render_raw_curl(
        &self,
        ctx: &RuntimeContext,
        prompt: &[internal_baml_jinja::RenderedChatMessage],
        render_settings: RenderCurlSettings,
    ) -> Result<String> {
        // Build CLI command
        let mut cmd = vec![];
        if let Some(region) = &self.properties.region {
            cmd.push(format!("AWS_REGION={region}"));
        }
        if let Some(profile) = &self.properties.profile {
            cmd.push(format!(" AWS_PROFILE={profile}"));
        }
        let base_cmd = if render_settings.stream && self.supports_streaming() {
            "aws bedrock-runtime converse-stream"
        } else {
            "aws bedrock-runtime converse"
        };
        cmd.push(base_cmd.to_string());

        cmd.push(format!("--model-id '{}'", self.properties.model));
        cmd.push("--output json".to_string());

        // Build --cli-input-json payload
        let root = self.build_converse_body_json(prompt)?;

        // pretty, multi-line JSON
        let input_json_str = serde_json::to_string_pretty(&serde_json::Value::Object(root))?;
        let input_json_escaped = escape(Cow::Borrowed(&input_json_str));
        cmd.push(format!("--cli-input-json {input_json_escaped}"));

        Ok(cmd.join(" "))
    }
}

// getters for client info
impl WithRetryPolicy for AwsClient {
    fn retry_policy_name(&self) -> Option<&str> {
        self.retry_policy.as_deref()
    }
}

impl WithClientProperties for AwsClient {
    fn allowed_metadata(&self) -> &AllowedRoleMetadata {
        &self.properties.allowed_role_metadata
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

impl WithClient for AwsClient {
    fn context(&self) -> &RenderContext_Client {
        &self.context
    }

    fn model_features(&self) -> &ModelFeatures {
        &self.features
    }
}

impl WithNoCompletion for AwsClient {}

impl WithStreamChat for AwsClient {
    async fn stream_chat(
        &self,
        ctx: &impl HttpContext,
        chat_messages: &[RenderedChatMessage],
    ) -> StreamResponse {
        let client = self.context.name.to_string();
        let model = Some(self.properties.model.clone());
        // TODO:(vbv) - use inference config for this.
        let request_options = Default::default();
        let prompt = internal_baml_jinja::RenderedPrompt::Chat(chat_messages.to_vec());

        let aws_client = match self
            .client_anyhow(
                ctx.runtime_context().call_id_stack.clone(),
                ctx.http_request_id().clone(),
            )
            .await
        {
            Ok(c) => c,
            Err(e) => {
                return Err(LLMResponse::LLMFailure(LLMErrorResponse {
                    client,
                    model,
                    prompt,
                    start_time: SystemTime::now(),
                    request_options,
                    latency: web_time::Duration::ZERO,
                    message: format!("{e:#?}"),
                    code: ErrorCode::Other(2),
                    raw_response: None,
                }));
            }
        };

        let request = match self.build_request(ctx.runtime_context(), chat_messages) {
            Ok(r) => r,
            Err(e) => {
                return Err(LLMResponse::LLMFailure(LLMErrorResponse {
                    client,
                    model,
                    prompt,
                    start_time: SystemTime::now(),
                    request_options,
                    latency: web_time::Duration::ZERO,
                    message: format!("{e:#?}"),
                    code: ErrorCode::Other(2),
                    raw_response: None,
                }))
            }
        };

        let additional_model_request_fields = request.additional_model_request_fields;

        let request = aws_client
            .converse_stream()
            .set_model_id(request.model_id)
            .set_inference_config(request.inference_config)
            .set_system(request.system)
            .set_messages(request.messages)
            .set_additional_model_request_fields(additional_model_request_fields);

        let system_start = SystemTime::now();
        let instant_start = Instant::now();

        let response = match request.send().await {
            Ok(resp) => resp,
            Err(e) => {
                return Err(LLMResponse::LLMFailure(LLMErrorResponse {
                    client,
                    model,
                    prompt,
                    start_time: system_start,
                    request_options,
                    latency: instant_start.elapsed(),
                    message: format!("{e:#?}"),
                    code: match e {
                        SdkError::ConstructionFailure(_) => ErrorCode::Other(2),
                        SdkError::TimeoutError(_) => ErrorCode::ServerError,
                        SdkError::DispatchFailure(_) => ErrorCode::ServerError,
                        SdkError::ResponseError(e) => {
                            ErrorCode::UnsupportedResponse(e.raw().status().as_u16())
                        }
                        SdkError::ServiceError(e) => {
                            let status = e.raw().status();
                            match status.as_u16() {
                                400 => ErrorCode::InvalidAuthentication,
                                403 => ErrorCode::NotSupported,
                                429 => ErrorCode::RateLimited,
                                500 => ErrorCode::ServerError,
                                503 => ErrorCode::ServiceUnavailable,
                                _ => {
                                    if status.is_server_error() {
                                        ErrorCode::ServerError
                                    } else {
                                        ErrorCode::Other(status.as_u16())
                                    }
                                }
                            }
                        }
                        _ => ErrorCode::Other(2),
                    },
                    raw_response: None,
                }));
            }
        };

        let call_id_stack = Arc::new(ctx.runtime_context().call_id_stack.clone());
        let http_request_id = Arc::new(ctx.http_request_id().clone());

        let stream = stream::unfold(
            (
                Some(LLMCompleteResponse {
                    client,
                    prompt,
                    content: "".to_string(),
                    start_time: system_start,
                    latency: instant_start.elapsed(),
                    model: self.properties.model.clone(),
                    request_options,
                    metadata: LLMCompleteResponseMetadata {
                        baml_is_complete: false,
                        finish_reason: None,
                        prompt_tokens: None,
                        output_tokens: None,
                        total_tokens: None,
                        cached_input_tokens: None,
                    },
                }),
                response,
            ),
            move |(initial_state, mut response)| {
                let call_id_stack = call_id_stack.clone();
                let http_request_id = http_request_id.clone();
                async move {
                    let mut new_state = initial_state?;
                    match response.stream.recv().await {
                        Ok(Some(message)) => {
                            log::trace!("Received message: {message:#?}");
                            {
                                let event_type = bedrock_stream_event_type(&message);
                                let event_data = serialize_aws_struct(&message);
                                let trace_event = TraceEvent::new_raw_llm_response_stream(
                                    call_id_stack.deref().clone(),
                                    std::sync::Arc::new(HTTPResponseStream::new(
                                        http_request_id.deref().clone(),
                                        SSEEvent::new(event_type.into(), event_data, "".into()),
                                    )),
                                );
                                BAML_TRACER
                                    .lock()
                                    .unwrap()
                                    .put(std::sync::Arc::new(trace_event));
                            }
                            match message {
                                bedrock::types::ConverseStreamOutput::ContentBlockDelta(
                                    content_block_delta,
                                ) => {
                                    if let Some(bedrock::types::ContentBlockDelta::Text(
                                        ref delta,
                                    )) = content_block_delta.delta
                                    {
                                        new_state.content += delta;
                                        // TODO- handle
                                    }
                                    // TODO- handle
                                }
                                bedrock::types::ConverseStreamOutput::ContentBlockStart(_) => {
                                    // TODO- handle
                                }
                                bedrock::types::ConverseStreamOutput::ContentBlockStop(_) => {
                                    // TODO- handle
                                }
                                bedrock::types::ConverseStreamOutput::MessageStart(_) => {
                                    // TODO- handle
                                }
                                bedrock::types::ConverseStreamOutput::MessageStop(stop) => {
                                    new_state.metadata.baml_is_complete = matches!(
                                        stop.stop_reason,
                                        bedrock::types::StopReason::StopSequence
                                            | bedrock::types::StopReason::EndTurn
                                    );
                                    // TODO- handle
                                }
                                bedrock::types::ConverseStreamOutput::Metadata(metadata) => {
                                    if let Some(usage) = metadata.usage() {
                                        new_state.metadata.prompt_tokens =
                                            Some(usage.input_tokens() as u64);
                                        new_state.metadata.output_tokens =
                                            Some(usage.output_tokens() as u64);
                                        new_state.metadata.total_tokens =
                                            Some((usage.total_tokens()) as u64);
                                        // AWS Bedrock does not currently support cached tokens
                                        new_state.metadata.cached_input_tokens = None;
                                    }
                                }
                                _ => {
                                    // TODO- handle
                                }
                            }
                            new_state.latency = instant_start.elapsed();
                            Some((
                                LLMResponse::Success(new_state.clone()),
                                (Some(new_state), response),
                            ))
                        }
                        Ok(None) => None,
                        Err(e) => Some((
                            LLMResponse::LLMFailure(LLMErrorResponse {
                                client: new_state.client,
                                model: Some(new_state.model),
                                prompt: new_state.prompt,
                                start_time: new_state.start_time,
                                request_options: new_state.request_options,
                                latency: instant_start.elapsed(),
                                message: format!("Failed to parse event: {e:#?}"),
                                code: ErrorCode::Other(2),
                                raw_response: None,
                            }),
                            (None, response),
                        )),
                    }
                }
            },
        );

        Ok(Box::pin(stream))
    }
}

impl AwsClient {
    fn to_chat_message(&self, text: &str) -> Result<bedrock::types::ContentBlock> {
        Ok(bedrock::types::ContentBlock::Text(text.to_string()))
    }

    fn to_media_message(
        &self,
        media: &baml_types::BamlMedia,
    ) -> Result<bedrock::types::ContentBlock> {
        match media.media_type {
            BamlMediaType::Image => {
                let format = bedrock::types::ImageFormat::from(
                    {
                        let mime_type = media.mime_type_as_ok()?;
                        match mime_type.strip_prefix("image/") {
                            Some(s) => s.to_string(),
                            None => mime_type,
                        }
                    }
                    .as_str(),
                );
                match &media.content {
                    BamlMediaContent::File(_) => {
                        anyhow::bail!(
                            "BAML internal error (AWSBedrock): file should have been resolved to base64"
                        )
                    }
                    BamlMediaContent::Url(url) => Ok(bedrock::types::ContentBlock::Image(
                        bedrock::types::ImageBlock::builder()
                            .set_format(Some(format))
                            .set_source(Some(bedrock::types::ImageSource::S3Location(
                                bedrock::types::S3Location::builder()
                                    .set_uri(Some(url.url.clone()))
                                    .build()
                                    .context("Failed to build S3Location block")?,
                            )))
                            .build()
                            .context("Failed to build Image block")?,
                    )),
                    BamlMediaContent::Base64(b64_media) => Ok(bedrock::types::ContentBlock::Image(
                        bedrock::types::ImageBlock::builder()
                            .set_format(Some(format))
                            .set_source(Some(bedrock::types::ImageSource::Bytes(Blob::new(
                                aws_smithy_types::base64::decode(b64_media.base64.clone())?,
                            ))))
                            .build()
                            .context("Failed to build image block")?,
                    )),
                }
            }
            BamlMediaType::Pdf => {
                match &media.content {
                    BamlMediaContent::File(_) => {
                        anyhow::bail!(
                            "BAML internal error (AWSBedrock): Pdf file should have been resolved to base64"
                        )
                    }
                    BamlMediaContent::Url(url_media) => {
                        // AWS Bedrock supports Pdf as document type via URL
                        Ok(bedrock::types::ContentBlock::Document(
                            bedrock::types::DocumentBlock::builder()
                                .set_format(Some(bedrock::types::DocumentFormat::Pdf))
                                .set_name(Some("document".to_string())) // Default name for URL-based Pdfs
                                .set_source(Some(bedrock::types::DocumentSource::Bytes(Blob::new(
                                    url_media.url.as_bytes().to_vec(),
                                ))))
                                .set_citations(Some(
                                    CitationsConfig::builder().set_enabled(Some(true)).build()?,
                                ))
                                .build()
                                .context("Failed to build Pdf document block")?,
                        ))
                    }
                    BamlMediaContent::Base64(b64_media) => {
                        // AWS Bedrock supports Pdf as document type via Base64
                        Ok(bedrock::types::ContentBlock::Document(
                            bedrock::types::DocumentBlock::builder()
                                .set_format(Some(bedrock::types::DocumentFormat::Pdf))
                                .set_name(Some("document".to_string())) // Default name for Base64 Pdfs
                                .set_source(Some(bedrock::types::DocumentSource::Bytes(Blob::new(
                                    aws_smithy_types::base64::decode(b64_media.base64.clone())?,
                                ))))
                                .build()
                                .context("Failed to build Pdf document block")?,
                        ))
                    }
                }
            }
            BamlMediaType::Video => {
                let format = bedrock::types::VideoFormat::from(
                    {
                        let mime_type = media.mime_type_as_ok()?;
                        match mime_type.strip_prefix("video/") {
                            Some(s) => s.to_string(),
                            None => mime_type,
                        }
                    }
                    .as_str(),
                );
                // AWS Bedrock supports video for Nova models with specific format
                match &media.content {
                    BamlMediaContent::File(_) => {
                        anyhow::bail!(
                            "BAML internal error (AWSBedrock): video file should have been resolved to base64"
                        )
                    }
                    BamlMediaContent::Url(url) => Ok(bedrock::types::ContentBlock::Video(
                        bedrock::types::VideoBlock::builder()
                            .set_format(Some(format))
                            .set_source(Some(bedrock::types::VideoSource::S3Location(
                                bedrock::types::S3Location::builder()
                                    .set_uri(Some(url.url.clone()))
                                    .build()
                                    .context("Failed to build S3Location block")?,
                            )))
                            .build()
                            .context("Failed to build Video document block")?,
                    )),
                    BamlMediaContent::Base64(b64_media) => Ok(bedrock::types::ContentBlock::Video(
                        bedrock::types::VideoBlock::builder()
                            .set_format(Some(format))
                            .set_source(Some(bedrock::types::VideoSource::Bytes(Blob::new(
                                aws_smithy_types::base64::decode(b64_media.base64.clone())?,
                            ))))
                            .build()
                            .context("AWS Bedrock error: mime_type must be explicitly set on base64 videos")?,
                    )),
                }
            }
            BamlMediaType::Audio => {
                anyhow::bail!(
                    "AWS Bedrock does not support audio media type: {:#?}",
                    media
                )
            }
        }
    }

    fn role_to_message(&self, msg: &RenderedChatMessage) -> Result<bedrock::types::Message> {
        let content = msg
            .parts
            .iter()
            .map(|part| self.part_to_message(part))
            .collect::<Result<Vec<_>>>()?;

        bedrock::types::Message::builder()
            .set_role(Some(msg.role.as_str().into()))
            .set_content(Some(content))
            .build()
            .map_err(|e: bedrock::error::BuildError| e.into())
    }

    fn part_to_system_message(
        part: &ChatMessagePart,
    ) -> Result<bedrock::types::SystemContentBlock> {
        match part {
            ChatMessagePart::Text(t) => Ok(bedrock::types::SystemContentBlock::Text(t.clone())),
            ChatMessagePart::Media(_) => anyhow::bail!(
                "AWS Bedrock only supports text blocks for system messages, but got {:#?}",
                part
            ),
            ChatMessagePart::WithMeta(p, _) => Self::part_to_system_message(p),
        }
    }

    fn part_to_message(&self, part: &ChatMessagePart) -> Result<bedrock::types::ContentBlock> {
        match part {
            ChatMessagePart::Text(t) => self.to_chat_message(t),
            ChatMessagePart::Media(m) => self.to_media_message(m),
            ChatMessagePart::WithMeta(p, _) => {
                // All metadata is dropped as AWS does not support it
                // this means caching, etc.
                self.part_to_message(p)
            }
        }
    }

    fn parts_to_message(
        &self,
        parts: &[ChatMessagePart],
    ) -> Result<Vec<bedrock::types::ContentBlock>> {
        parts
            .iter()
            .map(|p| self.part_to_message(p))
            .collect::<Result<Vec<_>>>()
    }
}

impl WithChat for AwsClient {
    async fn chat(
        &self,
        ctx: &impl HttpContext,
        chat_messages: &[RenderedChatMessage],
    ) -> LLMResponse {
        let client = self.context.name.to_string();
        let model = Some(self.properties.model.clone());
        // TODO:(vbv) - use inference config for this.
        let request_options = Default::default();
        let prompt = internal_baml_jinja::RenderedPrompt::Chat(chat_messages.to_vec());

        let aws_client = match self
            .client_anyhow(
                ctx.runtime_context().call_id_stack.clone(),
                ctx.http_request_id().clone(),
            )
            .await
        {
            Ok(c) => c,
            Err(e) => {
                return LLMResponse::LLMFailure(LLMErrorResponse {
                    client,
                    model,
                    prompt,
                    start_time: SystemTime::now(),
                    request_options,
                    latency: web_time::Duration::ZERO,
                    message: format!("{e:#?}"),
                    code: ErrorCode::Other(2),
                    raw_response: None,
                })
            }
        };

        let request = match self.build_request(ctx.runtime_context(), chat_messages) {
            Ok(r) => r,
            Err(e) => {
                return LLMResponse::LLMFailure(LLMErrorResponse {
                    client,
                    model,
                    prompt,
                    start_time: SystemTime::now(),
                    request_options,
                    latency: web_time::Duration::ZERO,
                    message: format!("{e:#?}"),
                    code: ErrorCode::Other(2),
                    raw_response: None,
                })
            }
        };
        let request = aws_client
            .converse()
            .set_model_id(request.model_id)
            .set_additional_model_request_fields(request.additional_model_request_fields)
            .set_inference_config(request.inference_config)
            .set_system(request.system)
            .set_messages(request.messages);

        let system_start = SystemTime::now();
        let instant_start = Instant::now();

        let response = match request.send().await {
            Ok(resp) => resp,
            Err(e) => {
                return LLMResponse::LLMFailure(LLMErrorResponse {
                    client,
                    model,
                    prompt,
                    start_time: system_start,
                    request_options,
                    latency: instant_start.elapsed(),
                    message: format!("{e:#?}"),
                    code: match e {
                        SdkError::ConstructionFailure(_) => ErrorCode::Other(2),
                        SdkError::TimeoutError(_) => ErrorCode::Other(2),
                        SdkError::DispatchFailure(_) => ErrorCode::Other(2),
                        SdkError::ResponseError(e) => {
                            ErrorCode::UnsupportedResponse(e.raw().status().as_u16())
                        }
                        SdkError::ServiceError(e) => {
                            let status = e.raw().status();
                            match status.as_u16() {
                                400 => ErrorCode::InvalidAuthentication,
                                403 => ErrorCode::NotSupported,
                                429 => ErrorCode::RateLimited,
                                500 => ErrorCode::ServerError,
                                503 => ErrorCode::ServiceUnavailable,
                                _ => {
                                    if status.is_server_error() {
                                        ErrorCode::ServerError
                                    } else {
                                        ErrorCode::Other(status.as_u16())
                                    }
                                }
                            }
                        }
                        _ => ErrorCode::Other(2),
                    },
                    raw_response: None,
                });
            }
        };

        match self.chat_anyhow(&response).await {
            Ok(content) => LLMResponse::Success(LLMCompleteResponse {
                client,
                prompt,
                content: content.clone(),
                start_time: system_start,
                latency: instant_start.elapsed(),
                request_options,
                model: self.properties.model.clone(),
                metadata: LLMCompleteResponseMetadata {
                    baml_is_complete: matches!(
                        response.stop_reason,
                        bedrock::types::StopReason::StopSequence
                            | bedrock::types::StopReason::EndTurn
                    ),
                    finish_reason: Some(response.stop_reason().as_str().into()),
                    prompt_tokens: response
                        .usage
                        .as_ref()
                        .and_then(|i| i.input_tokens.try_into().ok()),
                    output_tokens: response
                        .usage
                        .as_ref()
                        .and_then(|i| i.output_tokens.try_into().ok()),
                    total_tokens: response
                        .usage
                        .as_ref()
                        .and_then(|i| i.total_tokens.try_into().ok()),
                    cached_input_tokens: None, // AWS Bedrock does not currently support cached tokens
                },
            }),
            Err(e) => LLMResponse::LLMFailure(LLMErrorResponse {
                client,
                model,
                prompt,
                start_time: system_start,
                request_options,
                latency: instant_start.elapsed(),
                message: format!("{e:#?}"),
                code: ErrorCode::Other(200),
                raw_response: None,
            }),
        }
    }
}
