use crate::client_registry::ClientProperty;
use crate::internal::llm_client::ResolveMedia;
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
use anyhow::Result;
use chrono::{Duration, Utc};
use futures::stream::TryStreamExt;
use futures::StreamExt;
use jsonwebtoken::{encode, Algorithm, EncodingKey, Header};
use serde::{Deserialize, Serialize};
use serde_json::Value;
#[cfg(not(target_arch = "wasm32"))]
use std::fs::File;
#[cfg(not(target_arch = "wasm32"))]
use std::io::BufReader;

use baml_types::BamlMedia;
use eventsource_stream::Eventsource;
use internal_baml_core::ir::ClientWalker;
use internal_baml_jinja::{ChatMessagePart, RenderContext_Client, RenderedChatMessage};

use serde_json::json;
use std::collections::HashMap;
struct PostRequestProperities {
    default_role: String,
    base_url: Option<String>,
    service_account_details: Option<(String, String)>,
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
    properties: PostRequestProperities,
}

#[derive(Debug, Serialize, Deserialize)]
struct Claims {
    iss: String,
    scope: String,
    aud: String,
    exp: i64,
    iat: i64,
}

#[derive(Debug, Deserialize)]
struct ServiceAccount {
    client_email: String,
    private_key: String,
}

fn resolve_properties(
    mut properties: HashMap<String, serde_json::Value>,
    ctx: &RuntimeContext,
) -> Result<PostRequestProperities, anyhow::Error> {
    let default_role = properties
        .remove("default_role")
        .and_then(|v| v.as_str().map(|s| s.to_string()))
        .unwrap_or_else(|| "user".to_string());

    let base_url = properties
        .remove("base_url")
        .and_then(|v| v.as_str().map(|s| s.to_string()))
        .unwrap_or("".to_string());

    let mut service_key: Option<(String, String)> = None;

    service_key = properties
        .remove("authorization")
        .map(|v| ("GOOGLE_TOKEN".to_string(), v.as_str().unwrap().to_string()))
        .or_else(|| {
            #[cfg(target_arch = "wasm32")]
            {
                properties
                    .remove("credentials_content")
                    .and_then(|v| {
                        v.as_str().map(|s| {
                            (
                                "GOOGLE_APPLICATION_CREDENTIALS_CONTENT".to_string(),
                                s.to_string(),
                            )
                        })
                    })
                    .or_else(|| {
                        ctx.env
                            .get("GOOGLE_APPLICATION_CREDENTIALS_CONTENT")
                            .map(|s| {
                                (
                                    "GOOGLE_APPLICATION_CREDENTIALS_CONTENT".to_string(),
                                    s.to_string(),
                                )
                            })
                    })
            }
            #[cfg(not(target_arch = "wasm32"))]
            {
                properties
                    .remove("credentials")
                    .and_then(|v| {
                        v.as_str()
                            .map(|s| ("GOOGLE_APPLICATION_CREDENTIALS".to_string(), s.to_string()))
                    })
                    .or_else(|| {
                        ctx.env
                            .get("GOOGLE_APPLICATION_CREDENTIALS")
                            .map(|s| ("GOOGLE_APPLICATION_CREDENTIALS".to_string(), s.to_string()))
                    })
            }
        });
    properties.remove("credentials");
    properties.remove("credentials_content");

    let project_id = properties
        .remove("project_id")
        .and_then(|v| v.as_str().map(|s| s.to_string()))
        .unwrap_or("".to_string());

    let model_id = properties
        .remove("model")
        .and_then(|v| v.as_str().map(|s| s.to_string()))
        .unwrap_or("".to_string());

    let location = properties
        .remove("location")
        .and_then(|v| v.as_str().map(|s| s.to_string()))
        .unwrap_or("".to_string());

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
        base_url: Some(base_url),
        service_account_details: service_key,
        headers,
        properties,
        project_id: Some(project_id),
        model_id: Some(model_id),
        location: Some(location),
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
        let model_id = self.properties.model_id.clone().unwrap_or_default();
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
                resolve_media_urls: ResolveMedia::MissingMime,
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
                resolve_media_urls: ResolveMedia::MissingMime,
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

        let mut should_stream = "generateContent";
        if stream {
            should_stream = "streamGenerateContent?alt=sse";
        }

        let location = self
            .properties
            .location
            .clone()
            .unwrap_or_else(|| "".to_string());
        let project_id = self
            .properties
            .project_id
            .clone()
            .unwrap_or_else(|| "".to_string());

        let model_id = self
            .properties
            .model_id
            .clone()
            .unwrap_or_else(|| "".to_string());

        let base_url = self
            .properties
            .base_url
            .clone()
            .unwrap_or_else(|| "".to_string());

        let baml_original_url = if base_url != "" {
            format!("{}{}:{}", base_url, model_id, should_stream)
        } else {
            format!(
                "https://{}-aiplatform.googleapis.com/v1/projects/{}/locations/{}/publishers/google/models/{}:{}",
                location,
                project_id,
                location,
                model_id,
                should_stream
            )
        };
        let mut req = match (&self.properties.proxy_url, allow_proxy) {
            (Some(proxy_url), true) => {
                let req = self.client.post(proxy_url.clone());
                req.header("baml-original-url", baml_original_url)
            }
            _ => self.client.post(baml_original_url),
        };

        let credentials = self.properties.service_account_details.clone();

        let access_token = if let Some((key, value)) = &credentials {
            if key == "GOOGLE_TOKEN" {
                value.clone()
            } else if key == "GOOGLE_APPLICATION_CREDENTIALS_CONTENT" {
                let service_account: ServiceAccount = serde_json::from_str(value).unwrap();
                let now = Utc::now();
                let claims = Claims {
                    iss: service_account.client_email,
                    scope: "https://www.googleapis.com/auth/cloud-platform".to_string(),
                    aud: "https://oauth2.googleapis.com/token".to_string(),
                    exp: (now + Duration::hours(1)).timestamp(),
                    iat: now.timestamp(),
                };

                // Create the JWT
                let header = Header::new(Algorithm::RS256);
                let key = EncodingKey::from_rsa_pem(service_account.private_key.as_bytes())?;
                let jwt = encode(&header, &claims, &key)?;

                // Make the token request
                let client = reqwest::Client::new();
                let params = [
                    ("grant_type", "urn:ietf:params:oauth:grant-type:jwt-bearer"),
                    ("assertion", &jwt),
                ];
                let res: Value = client
                    .post("https://oauth2.googleapis.com/token")
                    .form(&params)
                    .send()
                    .await?
                    .json()
                    .await?;

                // Extract and print the access token
                if let Some(access_token) = res["access_token"].as_str() {
                    println!("Access Token: {}", access_token);
                    access_token.to_string()
                } else {
                    println!("Failed to get access token. Response: {:?}", res);
                    return Err(anyhow::anyhow!("Failed to get access token"));
                }
            } else {
                #[cfg(not(target_arch = "wasm32"))]
                {
                    let file = File::open(value)?;
                    let reader = BufReader::new(file);
                    let service_account: ServiceAccount = serde_json::from_reader(reader)?;
                    let now = Utc::now();
                    let claims = Claims {
                        iss: service_account.client_email,
                        scope: "https://www.googleapis.com/auth/cloud-platform".to_string(),
                        aud: "https://oauth2.googleapis.com/token".to_string(),
                        exp: (now + Duration::hours(1)).timestamp(),
                        iat: now.timestamp(),
                    };

                    // Create the JWT
                    let header = Header::new(Algorithm::RS256);
                    let key = EncodingKey::from_rsa_pem(service_account.private_key.as_bytes())?;
                    let jwt = encode(&header, &claims, &key)?;

                    // Make the token request
                    let client = reqwest::Client::new();
                    let params = [
                        ("grant_type", "urn:ietf:params:oauth:grant-type:jwt-bearer"),
                        ("assertion", &jwt),
                    ];
                    let res: Value = client
                        .post("https://oauth2.googleapis.com/token")
                        .form(&params)
                        .send()
                        .await?
                        .json()
                        .await?;

                    // Extract and print the access token
                    if let Some(access_token) = res["access_token"].as_str() {
                        println!("Access Token: {}", access_token);
                        access_token.to_string()
                    } else {
                        println!("Failed to get access token. Response: {:?}", res);
                        return Err(anyhow::anyhow!("Failed to get access token"));
                    }
                }
                #[cfg(target_arch = "wasm32")]
                {
                    return Err(anyhow::anyhow!(
                        "Reading from files not supported in BAML playground. Pass in your credentials file as a string to the 'GOOGLE_APPLICATION_CREDENTIALS_CONTENT' environment variable."
                    ));
                }
            }
        } else {
            return Err(anyhow::anyhow!("Service account not found"));
        };

        req = req.header("Authorization", format!("Bearer {}", access_token));

        for (key, value) in &self.properties.headers {
            req = req.header(key, value);
        }

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
        let usage_metadata = response.usage_metadata.clone().unwrap();

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
            "inlineData": {
                "mime_type": format!("{}", data.media_type),
                "data": data.base64
            }
        }),
        BamlMedia::Url(_, data) => json!({
            "fileData": {
                "mime_type": data.media_type,
                "file_uri": data.url
            }
        }),
        _ => panic!("Unsupported media type"),
    }
}
