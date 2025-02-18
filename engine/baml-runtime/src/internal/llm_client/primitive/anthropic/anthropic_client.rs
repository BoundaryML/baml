use crate::internal::llm_client::{
    primitive::request::ResponseType,
    traits::{ToProviderMessage, ToProviderMessageExt, WithClientProperties},
    ResolveMediaUrls,
};
use secrecy::ExposeSecret;
use std::collections::HashMap;

use anyhow::{Context, Result};
use baml_types::{BamlMap, BamlMedia, BamlMediaContent};
use eventsource_stream::Eventsource;
use futures::StreamExt;
use internal_baml_core::ir::ClientWalker;
use internal_baml_jinja::{
    ChatMessagePart, RenderContext_Client, RenderedChatMessage, RenderedPrompt,
};
use internal_llm_client::{
    anthropic::ResolvedAnthropic, AllowedRoleMetadata, ClientProvider, ResolvedClientProperty,
    UnresolvedClientProperty,
};

use crate::{
    client_registry::ClientProperty,
    internal::llm_client::{
        primitive::{
            anthropic::types::{AnthropicMessageResponse, StopReason},
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
use serde_json::json;

use crate::RuntimeContext;

use super::types::MessageChunk;

// represents client that interacts with the Anthropic API
pub struct AnthropicClient {
    pub name: String,
    retry_policy: Option<String>,
    context: RenderContext_Client,
    features: ModelFeatures,
    properties: ResolvedAnthropic,

    // clients
    client: reqwest::Client,
}

// resolves/constructs PostRequestProperties from the client's options and runtime context, fleshing out the needed headers and parameters
// basically just reads the client's options and matches them to needed properties or defaults them
fn resolve_properties(
    provider: &ClientProvider,
    properties: &UnresolvedClientProperty<()>,
    ctx: &RuntimeContext,
) -> Result<ResolvedAnthropic, anyhow::Error> {
    let properties = properties.resolve(provider, &ctx.eval_ctx(false))?;

    let ResolvedClientProperty::Anthropic(props) = properties else {
        anyhow::bail!(
            "Invalid client property. Should have been a anthropic property but got: {}",
            properties.name()
        );
    };

    Ok(props)
}
// getters for client info
impl WithRetryPolicy for AnthropicClient {
    fn retry_policy_name(&self) -> Option<&str> {
        self.retry_policy.as_deref()
    }
}

impl WithClientProperties for AnthropicClient {
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

impl WithClient for AnthropicClient {
    fn context(&self) -> &RenderContext_Client {
        &self.context
    }

    fn model_features(&self) -> &ModelFeatures {
        &self.features
    }
}

impl WithNoCompletion for AnthropicClient {}

// handles streamign chat interactions, when sending prompt to API and processing response stream
impl WithStreamChat for AnthropicClient {
    async fn stream_chat(
        &self,
        _ctx: &RuntimeContext,
        prompt: &[RenderedChatMessage],
    ) -> StreamResponse {
        let model_name = self
            .request_options()
            .get("model")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        crate::internal::llm_client::primitive::stream_request::make_stream_request(
            self,
            either::Either::Right(prompt),
            model_name,
            ResponseType::Anthropic,
        )
        .await
    }
}

// constructs base client and resolves properties based on context
impl AnthropicClient {
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

    pub fn new(client: &ClientWalker, ctx: &RuntimeContext) -> Result<AnthropicClient> {
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
}

// how to build the HTTP request for requests
impl RequestBuilder for AnthropicClient {
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
            format!("{}/v1/complete", destination_url)
        } else {
            format!("{}/v1/messages", destination_url)
        });

        for (key, value) in &self.properties.headers {
            req = req.header(key, value);
        }
        let api_key = self.properties.api_key.render(expose_secrets);
        req = req.header("x-api-key", api_key);

        if allow_proxy {
            req = req.header("baml-original-url", self.properties.base_url.as_str());
        }
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

        if stream {
            body_obj.insert("stream".into(), true.into());
        }

        Ok(req.json(&body))
    }

    fn request_options(&self) -> &BamlMap<String, serde_json::Value> {
        &self.properties.properties
    }
}

impl WithChat for AnthropicClient {
    async fn chat(&self, _ctx: &RuntimeContext, prompt: &[RenderedChatMessage]) -> LLMResponse {
        let model_name = self
            .request_options()
            .get("model")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        make_parsed_request(
            self,
            model_name,
            either::Either::Right(prompt),
            false,
            ResponseType::Anthropic,
        )
        .await
    }
}

impl ToProviderMessage for AnthropicClient {
    fn to_chat_message(
        &self,
        mut content: serde_json::Map<String, serde_json::Value>,
        text: &str,
    ) -> Result<serde_json::Map<String, serde_json::Value>> {
        content.insert("type".into(), "text".into());
        content.insert("text".into(), text.into());
        Ok(content)
    }

    fn to_media_message(
        &self,
        mut content: serde_json::Map<String, serde_json::Value>,
        media: &baml_types::BamlMedia,
    ) -> Result<serde_json::Map<String, serde_json::Value>> {
        match &media.content {
            BamlMediaContent::Base64(data) => {
                content.insert("type".into(), media.media_type.to_string().into());
                let mut source = serde_json::Map::new();
                source.insert("type".into(), "base64".into());
                source.insert("media_type".into(), media.mime_type_as_ok()?.into());
                source.insert("data".into(), data.base64.clone().into());
                content.insert("source".into(), source.into());
            }
            BamlMediaContent::File(_) => {
                anyhow::bail!(
                    "BAML internal error (Anthropic): file should have been resolved to base64"
                )
            }
            BamlMediaContent::Url(_) => {
                anyhow::bail!(
                    "BAML internal error (Anthropic): media URL should have been resolved to base64"
                )
            }
        }
        Ok(content)
    }

    fn role_to_message(
        &self,
        content: &RenderedChatMessage,
    ) -> Result<serde_json::Map<String, serde_json::Value>> {
        let mut map = serde_json::Map::new();
        map.insert("role".into(), content.role.clone().into());
        map.insert(
            "content".into(),
            json!(self.parts_to_message(&content.parts)?),
        );
        Ok(map)
    }
}

impl ToProviderMessageExt for AnthropicClient {
    fn chat_to_message(
        &self,
        chat: &[RenderedChatMessage],
    ) -> Result<serde_json::Map<String, serde_json::Value>> {
        // merge all adjacent roles of the same type
        let mut res = serde_json::Map::new();
        let (first, others) = chat.split_at(1);
        if let Some(content) = first.first() {
            if content.role == "system" {
                res.insert(
                    "system".into(),
                    json!(self.parts_to_message(&content.parts)?),
                );
                res.insert(
                    "messages".into(),
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
            "messages".into(),
            chat.iter()
                .map(|c| self.role_to_message(c))
                .collect::<Result<Vec<_>>>()?
                .into(),
        );

        Ok(res)
    }
}

// converts completion prompt into JSON body for request
fn convert_completion_prompt_to_body(prompt: &String) -> HashMap<String, serde_json::Value> {
    let mut map = HashMap::new();
    map.insert("prompt".into(), json!(prompt));
    map
}
