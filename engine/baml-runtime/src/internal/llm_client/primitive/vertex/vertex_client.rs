use crate::client_registry::ClientProperty;
use crate::internal::llm_client::traits::{
    ToProviderMessage, ToProviderMessageExt, WithClientProperties,
};
use crate::internal::llm_client::ResolveMediaUrls;
#[cfg(target_arch = "wasm32")]
use crate::internal::wasm_jwt::{encode_jwt, JwtError};
use crate::RuntimeContext;
use crate::{
    internal::llm_client::{
        primitive::{
            request::{make_parsed_request, make_request, RequestBuilder},
            vertex::types::{FinishReason, VertexResponse},
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
use chrono::{Duration, Utc};
use futures::StreamExt;
#[cfg(not(target_arch = "wasm32"))]
use gcp_auth::TokenProvider;
use internal_llm_client::vertex::{
    BaseUrlOrLocation, ResolvedGcpAuthStrategy, ResolvedVertex, ServiceAccount,
};
use internal_llm_client::{
    AllowedRoleMetadata, ClientProvider, ResolvedClientProperty, UnresolvedClientProperty,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use baml_types::BamlMediaContent;
use eventsource_stream::Eventsource;
use internal_baml_core::ir::ClientWalker;
use internal_baml_jinja::{RenderContext_Client, RenderedChatMessage};

use serde_json::json;
use std::collections::HashMap;

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

    let ResolvedClientProperty::Vertex(props) = properties else {
        anyhow::bail!(
            "Invalid client property. Should have been a vertex property but got: {}",
            properties.name()
        );
    };

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

impl SseResponseTrait for VertexClient {
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
                .map(|event| -> Result<VertexResponse> {
                    Ok(serde_json::from_str::<VertexResponse>(&event?.data)?)
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
                                        model: if inner.model.is_empty() {
                                            None
                                        } else {
                                            Some(inner.model.clone())
                                        },
                                        prompt: internal_baml_jinja::RenderedPrompt::Chat(
                                            prompt.to_vec(),
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
                        if let Some(choice) = event.candidates.first() {
                            if let Some(content) = choice
                                .content
                                .as_ref()
                                .and_then(|c| c.parts.first().map(|p| p.text.as_ref()))
                            {
                                inner.content += content;
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
impl WithStreamChat for VertexClient {
    async fn stream_chat(
        &self,
        ctx: &RuntimeContext,
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
            },
            features: ModelFeatures {
                chat: true,
                completion: false,
                max_one_system_prompt: true,
                resolve_media_urls: ResolveMediaUrls::EnsureMime,
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
                resolve_media_urls: ResolveMediaUrls::EnsureMime,
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
        let vertex_auth = super::auth::VertexAuth::new(&self.properties.auth_strategy).await?;

        let base_url = match &self.properties.base_url_or_location {
            BaseUrlOrLocation::BaseUrl(base_url) => base_url.to_string(),
            BaseUrlOrLocation::Location(location) => format!(
                "https://{location}-aiplatform.googleapis.com/v1/projects/{project_id}/locations/{location}/publishers/google/models",
                location = location,
                project_id = match self.properties.project_id.as_ref() {
                    Some(project_id) => project_id.to_string(),
                    None => vertex_auth.project_id().await?.to_string(),
                }
            ),
        };

        let baml_original_url = format!(
            "{base_url}/{model}:{rpc_and_protocol}",
            model = self.properties.model,
            rpc_and_protocol = if stream {
                "streamGenerateContent?alt=sse"
            } else {
                "generateContent"
            }
        );

        let mut req = match (&self.properties.proxy_url, allow_proxy) {
            (Some(proxy_url), true) => {
                let req = self.client.post(proxy_url.clone());
                req.header("baml-original-url", baml_original_url)
            }
            _ => self.client.post(baml_original_url),
        };

        // This is currently hardcoded, but we could make it a property if we wanted
        // https://developers.google.com/identity/protocols/oauth2/scopes
        const DEFAULT_SCOPE: &str = "https://www.googleapis.com/auth/cloud-platform";
        req = req.bearer_auth(vertex_auth.token(&[DEFAULT_SCOPE]).await?.as_str());

        for (key, value) in &self.properties.headers {
            req = req.header(key, value);
        }

        let mut json_body = self.properties.properties.clone();

        match prompt {
            either::Either::Left(prompt) => {
                json_body.extend(convert_completion_prompt_to_body(prompt))
            }
            either::Either::Right(messages) => json_body.extend(self.chat_to_message(messages)?),
        }

        let req = req.json(&json_body);

        Ok(req)
    }

    fn request_options(&self) -> &indexmap::IndexMap<String, serde_json::Value> {
        &self.properties.properties
    }
}

impl WithChat for VertexClient {
    async fn chat(&self, _ctx: &RuntimeContext, prompt: &[RenderedChatMessage]) -> LLMResponse {
        //non-streaming, complete response is returned
        let (response, system_now, instant_now) =
            match make_parsed_request::<VertexResponse>(self, either::Either::Right(prompt), false)
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

        let content = if let Some(content) = response.candidates.first().and_then(|c| {
            c.content
                .as_ref()
                .and_then(|c| c.parts.first().map(|p| p.text.clone()))
        }) {
            content
        } else {
            return LLMResponse::LLMFailure(LLMErrorResponse {
                client: self.context.name.to_string(),
                model: None,
                prompt: internal_baml_jinja::RenderedPrompt::Chat(prompt.to_vec()),
                start_time: system_now,
                request_options: self.properties.properties.clone(),
                latency: instant_now.elapsed(),
                message: "No content".to_string(),
                code: ErrorCode::Other(200),
            });
        };

        let usage_metadata = response.usage_metadata.clone().unwrap();

        LLMResponse::Success(LLMCompleteResponse {
            client: self.context.name.to_string(),
            prompt: internal_baml_jinja::RenderedPrompt::Chat(prompt.to_vec()),
            content,
            start_time: system_now,
            latency: instant_now.elapsed(),
            request_options: self.properties.properties.clone(),
            model: self
                .properties
                .properties
                .get("model")
                .and_then(|v| v.as_str().map(|s| s.to_string()))
                .unwrap_or_default(),
            metadata: LLMCompleteResponseMetadata {
                baml_is_complete: matches!(
                    response.candidates[0].finish_reason,
                    Some(FinishReason::Stop)
                ),
                finish_reason: response.candidates[0]
                    .finish_reason
                    .as_ref()
                    .map(|r| serde_json::to_string(r).unwrap_or("".into())),
                prompt_tokens: usage_metadata.prompt_token_count,
                output_tokens: usage_metadata.candidates_token_count,
                total_tokens: usage_metadata.total_token_count,
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
                content.insert(
                    "fileData".into(),
                    json!({"file_uri": data.url, "mime_type": media.mime_type}),
                );
                Ok(content)
            }
            BamlMediaContent::Base64(data) => {
                content.insert(
                    "inlineData".into(),
                    json!({
                        "data": data.base64,
                        "mime_type": media.mime_type_as_ok()?
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
