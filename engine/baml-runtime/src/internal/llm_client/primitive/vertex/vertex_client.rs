use crate::client_registry::ClientProperty;

use crate::RuntimeContext;
use crate::{
    internal::llm_client::{
        primitive::{
            request::{make_parsed_request, make_request, RequestBuilder},
            vertex::types::{FinishReason, GoogleResponse},
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

use crate::PathBuf;
use baml_types::BamlMedia;
use eventsource_stream::Eventsource;
use futures::StreamExt;
use gcp_auth::{CustomServiceAccount, TokenProvider};
use internal_baml_core::ir::ClientWalker;
use internal_baml_jinja::{ChatMessagePart, RenderContext_Client, RenderedChatMessage};
use reqwest::Response;
use serde_json::json;
use std::collections::HashMap;
struct PostRequestProperities {
    default_role: String,
    base_url: String,
    api_key: Option<CustomServiceAccount>,
    headers: HashMap<String, String>,
    proxy_url: Option<String>,
    properties: HashMap<String, serde_json::Value>,
    project_id: Option<String>,
    model_id: Option<String>,
    location: Option<String>,
}

pub struct VertexClient {
    pub name: String,
    pub client: reqwest::Client,
    pub retry_policy: Option<String>,
    pub context: RenderContext_Client,
    pub features: ModelFeatures,
    pub properties: PostRequestProperities,
}

fn resolve_properties(
    mut properties: HashMap<String, serde_json::Value>,
    ctx: &RuntimeContext,
) -> Result<PostRequestProperities, anyhow::Error> {
    let default_role = properties
        .remove("default_role")
        .and_then(|v| v.as_str().map(|s| s.to_string()))
        .unwrap_or_else(|| "user".to_string());

    let base_url = "";

    let mut service_account = ctx.env.get("VERTEX_PLAYGROUND_CREDENTIALS").map_or_else(
        || None,
        |creds| CustomServiceAccount::from_json(creds.as_str()).ok(),
    );

    if service_account.is_none() {
        let credentials_path = PathBuf::from(ctx.env.get("VERTEX_TERMINAL_CREDENTIALS").unwrap());
        service_account = CustomServiceAccount::from_file(credentials_path).ok();
    }

    let project_id = properties
        .remove("project_id")
        .and_then(|v| v.as_str().map(|s| s.to_string()))
        .or_else(|| ctx.env.get("GOOGLE_PROJECT_ID").map(|s| s.to_string()));

    let model_id = properties
        .remove("model")
        .and_then(|v| v.as_str().map(|s| s.to_string()))
        .or_else(|| Some("gemini-1.5-pro-001".to_string()));

    let location: Option<String> = properties
        .remove("location")
        .and_then(|v| v.as_str().map(|s| s.to_string()))
        .or_else(|| Some("us-central1".to_string()));

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
        base_url: base_url.to_string(),
        api_key: service_account,
        headers,
        properties,
        project_id,
        model_id,
        location,
        proxy_url: ctx.env.get("BOUNDARY_PROXY_URL").map(|s| s.to_string()),
    })
}

impl WithRetryPolicy for VertexClient {
    fn retry_policy_name(&self) -> Option<&str> {
        self.retry_policy.as_deref()
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
        prompt: &Vec<RenderedChatMessage>,
        system_start: web_time::SystemTime,
        instant_start: web_time::Instant,
    ) -> StreamResponse {
        let prompt = prompt.clone();
        let client_name = self.context.name.clone();
        let params = self.properties.properties.clone();
        Ok(Box::pin(
            resp.bytes_stream()
                .eventsource()
                .take_while(|event| {
                    std::future::ready(event.as_ref().is_ok_and(|e| e.data != "[DONE]"))
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
                        model: "".to_string(),
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
                                        code: ErrorCode::Other(2),
                                    },
                                )));
                            }
                        };

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

impl VertexClient {
    pub fn new(client: &ClientWalker, ctx: &RuntimeContext) -> Result<Self> {
        let properties = super::super::resolve_properties_walker(client, ctx)?;
        let properties = resolve_properties(properties, ctx)?;
        let default_role = properties.default_role.clone();
        Ok(Self {
            name: client.name().into(),
            properties,
            context: RenderContext_Client {
                name: client.name().into(),
                provider: client.elem().provider.clone(),
                default_role,
            },
            features: ModelFeatures {
                chat: true,
                completion: false,
                anthropic_system_constraints: false,
                resolve_media_urls: true,
            },
            retry_policy: client
                .elem()
                .retry_policy_id
                .as_ref()
                .map(|s| s.to_string()),
            client: create_client()?,
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
            properties,
            context: RenderContext_Client {
                name: client.name.clone(),
                provider: client.provider.clone(),
                default_role,
            },
            features: ModelFeatures {
                chat: true,
                completion: false,
                anthropic_system_constraints: false,
                resolve_media_urls: false,
            },
            retry_policy: client.retry_policy.clone(),
            client: create_client()?,
        })
    }
}

impl RequestBuilder for VertexClient {
    fn http_client(&self) -> &reqwest::Client {
        &self.client
    }

    async fn build_request(
        &self,
        prompt: either::Either<&String, &Vec<RenderedChatMessage>>,
        allow_proxy: bool,
        stream: bool,
    ) -> Result<reqwest::RequestBuilder> {
        //disabled proxying for testing

        log::info!("Building request for Google client");

        let mut should_stream = "generateContent";
        if stream {
            should_stream = "streamGenerateContent";
        }

        let location = self
            .properties
            .location
            .clone()
            .unwrap_or_else(|| "us-central1".to_string());
        let project_id = self
            .properties
            .project_id
            .clone()
            .unwrap_or_else(|| "gloo-ai".to_string());
        let model_id = self
            .properties
            .model_id
            .clone()
            .unwrap_or_else(|| "gemini-1.5-pro-001".to_string());

        let baml_original_url = format!(
            "https://{}-aiplatform.googleapis.com/v1/projects/{}/locations/{}/publishers/google/models/{}:{}",
            location,
            project_id,
            location,
            model_id,
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

        let scopes = &["https://www.googleapis.com/auth/cloud-platform"];
        let token = match &self.properties.api_key {
            Some(api_key) => api_key.token(scopes).await?,
            None => return Err(anyhow::anyhow!("API key is missing")),
        };

        req = req.header("Authorization", format!("Bearer {}", token.as_str()));

        let mut body = json!(self.properties.properties);
        let body_obj = body.as_object_mut().unwrap();

        match prompt {
            either::Either::Left(prompt) => {
                body_obj.extend(convert_completion_prompt_to_body(prompt))
            }
            either::Either::Right(messages) => {
                body_obj.extend(convert_chat_prompt_to_body(messages))
            }
        }

        Ok(req.json(&body))
    }

    fn request_options(&self) -> &HashMap<String, serde_json::Value> {
        &self.properties.properties
    }
}

impl WithChat for VertexClient {
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

//list of chat messages into JSON body
fn convert_chat_prompt_to_body(
    prompt: &Vec<RenderedChatMessage>,
) -> HashMap<String, serde_json::Value> {
    let mut map = HashMap::new();

    map.insert(
        "contents".into(),
        prompt
            .iter()
            .map(|m| {
                json!({
                    "role": m.role,
                    "parts": convert_message_parts_to_content(&m.parts)
                })
            })
            .collect::<serde_json::Value>(),
    );

    log::debug!("converted chat prompt to body: {:#?}", map);

    return map;
}

fn convert_message_parts_to_content(parts: &Vec<ChatMessagePart>) -> serde_json::Value {
    parts
        .iter()
        .map(|part| match part {
            ChatMessagePart::Text(text) => json!({
                "text": text
            }),
            ChatMessagePart::Image(image) => convert_media_to_content(image),
            ChatMessagePart::Audio(audio) => convert_media_to_content(audio),
        })
        .collect()
}

fn convert_media_to_content(media: &BamlMedia) -> serde_json::Value {
    match media {
        BamlMedia::Base64(_, data) => json!({
            "blob": {
                "mime_type": format!("{}", data.media_type),
                "data": data.base64
            }
        }),
        BamlMedia::Url(_, data) => json!({
            "fileData": {
                "mime_type": format!("{:?}", data.media_type),
                "file_uri": data.url
            }
        }),
        _ => panic!("Unsupported media type"),
    }
}
