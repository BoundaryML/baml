use std::collections::HashMap;

use anyhow::{Context, Result};
use baml_types::BamlMediaContent;
use chrono::{Duration, Utc};
use eventsource_stream::Eventsource;
use futures::StreamExt;
#[cfg(not(target_arch = "wasm32"))]
use gcp_auth::TokenProvider;
use internal_baml_core::ir::ClientWalker;
use internal_baml_jinja::{RenderContext_Client, RenderedChatMessage};
use internal_llm_client::{
    vertex::{BaseUrlOrLocation, ResolvedGcpAuthStrategy, ResolvedVertex},
    AllowedRoleMetadata, ClientProvider, ResolvedClientProperty, UnresolvedClientProperty,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

#[cfg(target_arch = "wasm32")]
use crate::internal::wasm_jwt::{encode_jwt, JwtError};
use crate::{
    client_registry::ClientProperty,
    internal::llm_client::{
        primitive::{
            anthropic::{self, AnthropicClient},
            request::{make_parsed_request, RequestBuilder, ResponseType},
            stream_request::make_stream_request,
            vertex::types::VertexResponse,
        },
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

pub struct VertexClient {
    pub name: String,
    pub client: reqwest::Client,
    pub retry_policy: Option<String>,
    pub context: RenderContext_Client,
    pub features: ModelFeatures,
    properties: ResolvedVertex,
}

fn resolve_properties(
    provider: &ClientProvider,
    properties: &UnresolvedClientProperty<()>,
    ctx: &RuntimeContext,
) -> Result<ResolvedVertex, anyhow::Error> {
    let properties = properties.resolve(provider, &ctx.eval_ctx(false))?;

    let ResolvedClientProperty::Vertex(mut props) = properties else {
        anyhow::bail!(
            "Invalid client property. Should have been a vertex property but got: {}",
            properties.name()
        );
    };

    if props.anthropic_version.is_none() && props.model.starts_with("claude") {
        props.anthropic_version =
            Some(internal_llm_client::anthropic::DEFAULT_ANTHROPIC_VERSION.to_string());
    }

    if let Some(anthropic_version) = &props.anthropic_version {
        props
            .properties
            .entry("anthropic_version".into())
            .or_insert_with(|| json!(anthropic_version));
        props
            .properties
            .entry("max_tokens".into())
            .or_insert_with(|| json!(internal_llm_client::anthropic::DEFAULT_MAX_TOKENS));
    }

    Ok(props)
}

impl WithRetryPolicy for VertexClient {
    fn retry_policy_name(&self) -> Option<&str> {
        self.retry_policy.as_deref()
    }
}

impl WithClientProperties for VertexClient {
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

impl WithClient for VertexClient {
    fn context(&self) -> &RenderContext_Client {
        &self.context
    }

    fn model_features(&self) -> &ModelFeatures {
        &self.features
    }
}

impl WithNoCompletion for VertexClient {}

// makes the request to the google client, on success it triggers the response_stream function to handle continuous rendering with the response object
impl WithStreamChat for VertexClient {
    async fn stream_chat(
        &self,
        ctx: &impl HttpContext,
        prompt: &[RenderedChatMessage],
    ) -> StreamResponse {
        //incomplete, streaming response object is returned
        make_stream_request(
            self,
            either::Either::Right(prompt),
            Some(self.properties.model.clone()),
            ResponseType::Vertex,
            ctx,
        )
        .await
    }
}

impl VertexClient {
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
                resolve_audio_urls: ResolveMediaUrls::EnsureMime,
                resolve_image_urls: ResolveMediaUrls::EnsureMime,
                resolve_pdf_urls: ResolveMediaUrls::Never,
                resolve_video_urls: ResolveMediaUrls::Never,
                allowed_metadata: properties.allowed_metadata.clone(),
            },
            retry_policy: client.elem().retry_policy_id.as_ref().map(String::to_owned),
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
                options: properties.properties.clone(),
                remap_role: properties.remap_role(),
            },
            features: ModelFeatures {
                chat: true,
                completion: false,
                max_one_system_prompt: true,
                resolve_audio_urls: ResolveMediaUrls::EnsureMime,
                resolve_image_urls: ResolveMediaUrls::EnsureMime,
                resolve_pdf_urls: ResolveMediaUrls::Never,
                resolve_video_urls: ResolveMediaUrls::Never,
                allowed_metadata: properties.allowed_metadata.clone(),
            },
            retry_policy: client.retry_policy.clone(),
            client: create_client()?,
            properties,
        })
    }
}

impl RequestBuilder for VertexClient {
    fn http_client(&self) -> &reqwest::Client {
        &self.client
    }

    async fn build_request(
        &self,
        prompt: either::Either<&String, &[RenderedChatMessage]>,
        allow_proxy: bool,
        stream: bool,
        // There are no leakable secrets in the Vertex request because
        // VertexAuth can not be built in the WASM environment.
        _expose_secrets: bool,
    ) -> Result<reqwest::RequestBuilder> {
        // Determine if API key auth is being used (query param 'key')
        let has_api_key_query = self.properties.query_params.contains_key("key");
        let mut vertex_auth: Option<std::sync::Arc<super::auth::VertexAuth>> = None;

        let base_url = match &self.properties.base_url_or_location {
            BaseUrlOrLocation::BaseUrl(base_url) => base_url.to_string(),
            BaseUrlOrLocation::Location(location) => {
                let domain = if location == "global" {
                    "aiplatform.googleapis.com".to_string()
                } else {
                    format!("{location}-aiplatform.googleapis.com")
                };
                let project_id = match self.properties.project_id.as_ref() {
                    Some(project_id) => project_id.to_string(),
                    None => {
                        if has_api_key_query {
                            anyhow::bail!(
                                "options.project_id is required when using API key auth with Vertex 'location' URLs;"
                            );
                        }
                        // Fallback to GCP Application Default Credentials only when not using API key
                        let va = match &vertex_auth {
                            Some(va) => va,
                            None => {
                                vertex_auth = Some(
                                    super::auth::VertexAuth::get_or_create(
                                        &self.properties.auth_strategy,
                                    )
                                    .await?,
                                );
                                vertex_auth.as_ref().unwrap()
                            }
                        };
                        va.project_id().await?.to_string()
                    }
                };
                format!(
                    "https://{domain}/v1/projects/{project_id}/locations/{location}/publishers/google/models",
                    domain = domain,
                    location = location,
                    project_id = project_id,
                )
            }
        };

        let baml_original_url = format!(
            "{base_url}/{model}:{rpc_and_protocol}",
            model = self.properties.model,
            rpc_and_protocol = match (&self.properties.anthropic_version, stream) {
                (Some(ref anthropic_version), true) => {
                    "streamRawPredict"
                }
                (Some(ref anthropic_version), false) => {
                    "rawPredict"
                }
                (None, true) => "streamGenerateContent?alt=sse",
                (None, false) => "generateContent",
            }
        );

        // Build original URL with any configured query params appended (preserving alt=sse)
        let original_with_query = if self.properties.query_params.is_empty() {
            baml_original_url.clone()
        } else {
            let mut url = baml_original_url.clone();
            let mut first = !url.contains('?');
            for (k, v) in &self.properties.query_params {
                if first {
                    url.push('?');
                    first = false;
                } else {
                    url.push('&');
                }
                // Note: values are appended as-is; expect callers to supply safe values
                url.push_str(k);
                url.push('=');
                url.push_str(v);
            }
            url
        };

        let mut req = match (&self.properties.proxy_url, allow_proxy) {
            (Some(proxy_url), true) => {
                let req = self.client.post(proxy_url.clone());
                req.header("baml-original-url", original_with_query)
            }
            _ => {
                let mut rb = self.client.post(baml_original_url);
                if !self.properties.query_params.is_empty() {
                    rb = rb.query(&self.properties.query_params);
                }
                rb
            }
        };

        // Use OAuth2 bearer auth unless an API key is provided via query params (query_params.key)
        // https://developers.google.com/identity/protocols/oauth2/scopes
        const DEFAULT_SCOPE: &str = "https://www.googleapis.com/auth/cloud-platform";
        if !has_api_key_query {
            let va = match &vertex_auth {
                Some(va) => va,
                None => {
                    vertex_auth = Some(
                        super::auth::VertexAuth::get_or_create(&self.properties.auth_strategy)
                            .await?,
                    );
                    vertex_auth.as_ref().unwrap()
                }
            };
            req = req.bearer_auth(va.token(&[DEFAULT_SCOPE]).await?.as_str());
        }

        for (key, value) in &self.properties.headers {
            req = req.header(key, value);
        }

        let mut json_body = self.properties.properties.clone();

        match (&self.properties.anthropic_version, prompt) {
            (Some(ref anthropic_version), either::Either::Left(prompt)) => {
                json_body.extend(anthropic::convert_completion_prompt_to_body(prompt));
            }
            (Some(ref anthropic_version), either::Either::Right(messages)) => {
                let anthropic_client = AnthropicClient::synthetic_for_vertex_anthropic(
                    self.name.clone(),
                    self.context.clone(),
                    self.properties.role_selection.clone(),
                )?;
                json_body.extend(anthropic_client.chat_to_message(messages)?);
            }
            (None, either::Either::Left(prompt)) => {
                json_body.extend(convert_completion_prompt_to_body(prompt));
            }
            (None, either::Either::Right(messages)) => {
                json_body.extend(self.chat_to_message(messages)?);
            }
        }

        let req = req.json(&json_body);

        Ok(req)
    }

    fn request_options(&self) -> &indexmap::IndexMap<String, serde_json::Value> {
        &self.properties.properties
    }
}

impl WithChat for VertexClient {
    async fn chat(&self, ctx: &impl HttpContext, prompt: &[RenderedChatMessage]) -> LLMResponse {
        let model_name = self.properties.model.clone();
        //non-streaming, complete response is returned
        make_parsed_request(
            self,
            Some(model_name),
            either::Either::Right(prompt),
            false,
            match self.properties.anthropic_version {
                Some(ref anthropic_version) => ResponseType::Anthropic,
                None => ResponseType::Vertex,
            },
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

impl ToProviderMessage for VertexClient {
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
            BamlMediaContent::File(_) => anyhow::bail!(
                "BAML internal error (Vertex): file should have been resolved to base64"
            ),
            BamlMediaContent::Url(data) => {
                let mime_type = match &media.mime_type {
                    Some(mime) if !mime.is_empty() => mime.clone(),
                    _ => {
                        // Provide default mime types when none specified
                        match media.media_type {
                            baml_types::BamlMediaType::Video => "video/mp4".to_string(),
                            _ => media.mime_type_as_ok()?,
                        }
                    }
                };
                content.insert(
                    "fileData".into(),
                    json!({
                        "fileUri": data.url,
                        "mimeType": mime_type
                    }),
                );
                Ok(content)
            }
            BamlMediaContent::Base64(data) => {
                content.insert(
                    "inlineData".into(),
                    json!({
                        "data": data.base64,
                        "mimeType": media.mime_type_as_ok()?
                    }),
                );
                Ok(content)
            }
        }
    }

    fn role_to_message(
        &self,
        content: &RenderedChatMessage,
    ) -> Result<serde_json::Map<String, serde_json::Value>> {
        let mut map = serde_json::Map::new();
        map.insert("role".into(), json!(content.role));
        map.insert(
            "parts".into(),
            json!(self.parts_to_message(&content.parts)?),
        );
        Ok(map)
    }
}

impl ToProviderMessageExt for VertexClient {
    fn chat_to_message(
        &self,
        chat: &[RenderedChatMessage],
    ) -> Result<serde_json::Map<String, serde_json::Value>> {
        // merge all adjacent roles of the same type
        let mut res = serde_json::Map::new();

        // https://ai.google.dev/gemini-api/docs/text-generation?lang=rest#system-instructions
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

impl CompletionToProviderBody for VertexClient {
    fn completion_to_provider_body(
        &self,
        prompt: &str,
    ) -> serde_json::Map<String, serde_json::Value> {
        convert_completion_prompt_to_body(prompt)
    }
}
