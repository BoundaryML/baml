use crate::client::ClientWalker;
use crate::internal::llm_client::primitive::google::types::ChatMessagePart;
use crate::internal::llm_client::primitive::types::BamlImage;
use crate::internal::llm_client::RenderedChatMessage;
use crate::internal::llm_client::RuntimeContext;
use crate::{
    internal::llm_client::{
        primitive::{
            google::types::{AnthropicErrorResponse, GoogleResponse, StopReason},
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
use internal_baml_jinja::RenderContext_Client;
use reqwest::Response;
use serde_json::json;
use std::collections::HashMap;
pub struct GoogleClient {
    pub name: String,
    pub client: reqwest::Client,
    pub retry_policy: Option<String>,
    pub context: RenderContext_Client,
    pub features: ModelFeatures,
    pub properties: PostRequestProperities,
}

struct PostRequestProperities {
    default_role: String,
    base_url: String,
    api_key: Option<String>,
    headers: HashMap<String, String>,
    proxy_url: Option<String>,
    // These are passed directly to the Anthropic API.
    properties: HashMap<String, serde_json::Value>,
}

impl GoogleClient {
    pub fn new(client: &ClientWalker, ctx: &RuntimeContext) -> Result<GoogleClient, anyhow::Error> {
        Ok(Self {
            name: client.name().into(),
            properties: resolve_properties(client, ctx)?,
            context: RenderContext_Client {
                name: client.name().into(),
                provider: client.elem().provider.clone(),
            },
            features: ModelFeatures {
                chat: true,
                completion: false,
                anthropic_system_constraints: true,
            },
            retry_policy: client
                .elem()
                .retry_policy_id
                .as_ref()
                .map(|s| s.to_string()),
            client: create_client()?,
        })
    }
}

fn resolve_properties(
    client: &ClientWalker,
    ctx: &RuntimeContext,
) -> Result<PostRequestProperities, anyhow::Error> {
    let mut properties = (&client.item.elem.options)
        .iter()
        .map(|(k, v)| {
            Ok((
                k.into(),
                ctx.resolve_expression::<serde_json::Value>(v)
                    .context(format!(
                        "client {} could not resolve options.{}",
                        client.name(),
                        k
                    ))?,
            ))
        })
        .collect::<Result<HashMap<_, _>>>()?;
    // this is a required field
    properties
        .entry("max_tokens".into())
        .or_insert_with(|| 4096.into());

    let default_role = properties
        .remove("default_role")
        .and_then(|v| v.as_str().map(|s| s.to_string()))
        .unwrap_or_else(|| "system".to_string());

    let base_url = properties
        .remove("base_url")
        .and_then(|v| v.as_str().map(|s| s.to_string()))
        .unwrap_or_else(|| "".to_string());

    let api_key = properties
        .remove("api_key")
        .and_then(|v| v.as_str().map(|s| s.to_string()))
        .or_else(|| ctx.env.get("GOOGLE_API_KEY").map(|s| s.to_string()));

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

    let mut headers = match headers {
        Some(h) => h?,
        None => Default::default(),
    };

    Ok(PostRequestProperities {
        default_role,
        base_url,
        api_key,
        headers,
        properties,
        proxy_url: ctx.env.get("BOUNDARY_PROXY_URL").map(|s| s.to_string()),
    })
}

impl RequestBuilder for GoogleClient {
    fn http_client(&self) -> &reqwest::Client {
        &self.client
    }

    fn build_request(
        &self,
        prompt: either::Either<&String, &Vec<RenderedChatMessage>>,
        stream: bool,
    ) -> reqwest::RequestBuilder {
        let mut req = self.client.post(if prompt.is_left() {
            format!(
                "{}/v1/complete",
                self.properties
                    .proxy_url
                    .as_ref()
                    .unwrap_or(&self.properties.base_url)
                    .clone()
            )
        } else {
            format!(
                "{}/v1/messages",
                self.properties
                    .proxy_url
                    .as_ref()
                    .unwrap_or(&self.properties.base_url)
                    .clone()
            )
        });

        for (key, value) in &self.properties.headers {
            req = req.header(key, value);
        }
        if let Some(key) = &self.properties.api_key {
            req = req.header("x-api-key", key);
        }

        req = req.header("baml-original-url", self.properties.base_url.as_str());

        let mut body = json!(self.properties.properties);
        let body_obj = body.as_object_mut().unwrap();
        match prompt {
            either::Either::Left(prompt) => {
                body_obj.extend(convert_completion_prompt_to_body(prompt))
            }
            either::Either::Right(messages) => {
                body_obj.extend(convert_chat_prompt_to_body(messages));
            }
        }

        if stream {
            body_obj.insert("stream".into(), true.into());
        }

        req.json(&body)
    }

    fn invocation_params(&self) -> &HashMap<String, serde_json::Value> {
        &self.properties.properties
    }
}

impl WithClient for GoogleClient {
    fn name(&self) -> &str {
        &self.name
    }

    fn context(&self) -> &RenderContext_Client {
        &self.context
    }

    fn model_features(&self) -> &ModelFeatures {
        &self.features
    }
}

impl WithChat for GoogleClient {
    fn chat_options(&self, _ctx: &RuntimeContext) -> Result<internal_baml_jinja::ChatOptions> {
        Ok(internal_baml_jinja::ChatOptions::new(
            self.properties.default_role.clone(),
            None,
        ))
    }

    async fn chat(&self, _ctx: &RuntimeContext, prompt: &Vec<RenderedChatMessage>) -> LLMResponse {
        let (response, system_now, instant_now) =
            match make_parsed_request::<GoogleResponse>(self, either::Either::Right(prompt), false)
                .await
            {
                Ok(v) => v,
                Err(e) => return e,
            };

        if response.content.len() != 1 {
            return LLMResponse::LLMFailure(LLMErrorResponse {
                client: self.context.name.to_string(),
                model: None,
                prompt: internal_baml_jinja::RenderedPrompt::Chat(prompt.clone()),
                start_time: system_now,
                invocation_params: self.properties.properties.clone(),
                latency: instant_now.elapsed(),
                message: format!(
                    "Expected exactly one content block, got {}",
                    response.content.len()
                ),
                code: ErrorCode::Other(200),
            });
        }

        LLMResponse::Success(LLMCompleteResponse {
            client: self.context.name.to_string(),
            prompt: internal_baml_jinja::RenderedPrompt::Chat(prompt.clone()),
            content: response.content[0].text.clone(),
            start_time: system_now,
            latency: instant_now.elapsed(),
            invocation_params: self.properties.properties.clone(),
            model: response.model,
            metadata: LLMCompleteResponseMetadata {
                baml_is_complete: match response.stop_reason {
                    Some(StopReason::StopSequence) | Some(StopReason::EndTurn) => true,
                    _ => false,
                },
                finish_reason: response
                    .stop_reason
                    .as_ref()
                    .map(|r| serde_json::to_string(r).unwrap_or("".into())),
                prompt_tokens: Some(response.usage.input_tokens),
                output_tokens: Some(response.usage.output_tokens),
                total_tokens: Some(response.usage.input_tokens + response.usage.output_tokens),
            },
        })
    }
}

fn convert_completion_prompt_to_body(prompt: &String) -> HashMap<String, serde_json::Value> {
    let mut map = HashMap::new();
    map.insert("prompt".into(), json!(prompt));
    map
}

fn convert_chat_prompt_to_body(
    prompt: &Vec<RenderedChatMessage>,
) -> HashMap<String, serde_json::Value> {
    let mut map = HashMap::new();
    log::debug!("converting chat prompt to body: {:#?}", prompt);

    if let Some(first) = prompt.get(0) {
        if first.role == "system" {
            map.insert(
                "system".into(),
                convert_message_parts_to_content(&first.parts),
            );
            map.insert(
                "messages".into(),
                prompt
                    .iter()
                    .skip(1)
                    .map(|m| {
                        json!({
                            "role": m.role,
                            "content": convert_message_parts_to_content(&m.parts)
                        })
                    })
                    .collect::<serde_json::Value>(),
            );
        } else {
            map.insert(
                "messages".into(),
                prompt
                    .iter()
                    .map(|m| {
                        json!({
                            "role": m.role,
                            "content": convert_message_parts_to_content(&m.parts)
                        })
                    })
                    .collect::<serde_json::Value>(),
            );
        }
    } else {
        map.insert(
            "messages".into(),
            prompt
                .iter()
                .map(|m| {
                    json!({
                        "role": m.role,
                        "content": convert_message_parts_to_content(&m.parts)
                    })
                })
                .collect::<serde_json::Value>(),
        );
    }
    log::debug!("converted chat prompt to body: {:#?}", map);

    return map;
}

fn convert_message_parts_to_content(parts: &Vec<ChatMessagePart>) -> serde_json::Value {
    if parts.len() == 1 {
        if let ChatMessagePart::Text(text) = &parts[0] {
            return json!(text);
        }
    }

    parts
        .iter()
        .map(|part| match part {
            ChatMessagePart::Text(text) => json!({
                "type": "text",
                "text": text
            }),
            ChatMessagePart::Image(image) => match image {
                BamlImage::Base64(image) => json!({
                    "type": "image",
                    "source": {
                        "type": "base64",
                        "media_type": image.media_type,
                        "data": image.base64
                    }
                }),
                BamlImage::Url(image) => json!({
                    "type": "image",
                    "source": {
                        "type": "url",
                        "url": image.url
                    }
                }),
            },
        })
        .collect()
}
