use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use anyhow::{anyhow, Context, Error, Result};
use internal_baml_core::ir::{repr::Expression, ClientWalker, RetryPolicyWalker};
use internal_baml_jinja::{
    ChatMessagePart, RenderContext_Client, RenderedChatMessage, RenderedPrompt,
};
use serde_json::{json, Value};

use crate::internal::llm_client::{
    anthropic::types::StopReason,
    common::images::{self, download_image_as_base64},
    state::LlmClientState,
    traits::{
        WithChat, WithClient, WithNoCompletion, WithRetryPolicy, WithStreamChat,
        WithStreamCompletion,
    },
    LLMResponse, LLMResponseStream, ModelFeatures,
};

use crate::request::call_request_with_json;
use crate::RuntimeContext;

struct PostRequestProperities {
    default_role: String,
    base_url: String,
    api_key: Option<String>,
    headers: HashMap<String, String>,

    // These are passed directly to the Anthropic API.
    properties: HashMap<String, serde_json::Value>,
}

pub struct AnthropicClient {
    retry_policy: Option<String>,
    context: RenderContext_Client,
    features: ModelFeatures,
    properties: PostRequestProperities,

    internal_state: Arc<Mutex<LlmClientState>>,
}

fn resolve_properties(
    client: &ClientWalker,
    ctx: &RuntimeContext,
) -> Result<PostRequestProperities> {
    let mut properties = (&client.item.elem.options)
        .iter()
        .map(|(k, v)| {
            Ok((
                k.into(),
                ctx.resolve_expression(v).context(format!(
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
        .or_else(|| {
            ctx.env
                .get("BOUNDARY_ANTHROPIC_PROXY_URL")
                .map(|s| s.to_string())
        })
        .unwrap_or_else(|| "https://api.anthropic.com".to_string());

    let api_key = properties
        .remove("api_key")
        .and_then(|v| v.as_str().map(|s| s.to_string()))
        .or_else(|| ctx.env.get("ANTHROPIC_API_KEY").map(|s| s.to_string()));

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

    headers
        .entry("anthropic-version".to_string())
        .or_insert("2023-06-01".to_string());

    Ok(PostRequestProperities {
        default_role,
        base_url,
        api_key,
        headers,
        properties,
    })
}

impl WithRetryPolicy for AnthropicClient {
    fn retry_policy_name(&self) -> Option<&str> {
        self.retry_policy.as_deref()
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

impl AnthropicClient {
    pub fn new(client: &ClientWalker, ctx: &RuntimeContext) -> Result<AnthropicClient> {
        Ok(Self {
            properties: resolve_properties(client, ctx)?,
            context: RenderContext_Client {
                name: client.name().into(),
                provider: client.elem().provider.clone(),
            },
            features: ModelFeatures {
                chat: true,
                completion: false,
            },
            retry_policy: client
                .elem()
                .retry_policy_id
                .as_ref()
                .map(|s| s.to_string()),
            internal_state: Arc::new(Mutex::new(LlmClientState::new())),
        })
    }

    #[cfg(not(feature = "no_wasm"))]
    fn build_http_request(
        &self,
        _ctx: &RuntimeContext,
        path: &str,
        prompt: &Vec<RenderedChatMessage>,
    ) -> Result<web_sys::Request> {
        use web_sys::RequestMode;

        let chat_request = build_anthropic_chat_request(prompt)?;

        let mut body = json!(self.properties.properties);
        let chat_request_map = chat_request.as_object().unwrap().clone();
        body.as_object_mut().unwrap().extend(chat_request_map);

        log::info!("Request body: {:#?}", body);

        let mut init = web_sys::RequestInit::new();
        init.method("POST");
        init.mode(RequestMode::Cors);
        init.body(Some(&wasm_bindgen::JsValue::from_str(
            &serde_json::to_string(&body).map_err(|e| anyhow::format_err!("{:#?}", e))?,
        )));

        let headers = web_sys::Headers::new().map_err(|e| anyhow::format_err!("{:#?}", e))?;
        headers
            .set("Content-Type", "application/json")
            .map_err(|e| anyhow::format_err!("{:#?}", e))?;
        match &self.properties.api_key {
            Some(key) => {
                headers
                    .set("x-api-key", key)
                    .map_err(|e| anyhow::format_err!("{:#?}", e))?;
            }
            None => {}
        }
        for (k, v) in &self.properties.headers {
            headers
                .set(k, v)
                .map_err(|e| anyhow::format_err!("{:#?}", e))?;
        }
        init.headers(&headers);

        match web_sys::Request::new_with_str_and_init(
            &format!("{}{}", self.properties.base_url, path),
            &init,
        ) {
            Ok(req) => Ok(req),
            Err(e) => Err(anyhow::anyhow!("Failed to create request: {:#?}", e)),
        }
    }

    #[cfg(feature = "no_wasm")]
    fn build_http_request(
        &self,
        ctx: &RuntimeContext,
        path: &str,
        prompt: &Vec<RenderedChatMessage>,
    ) -> Result<reqwest::RequestBuilder> {
        // TODO: ideally like to keep this alive longer.
        let client = reqwest::Client::new();

        let chat_request = build_anthropic_chat_request(prompt)?;

        let mut body = json!(self.properties.properties);
        let chat_request_map = chat_request.as_object().unwrap().clone();
        body.as_object_mut().unwrap().extend(chat_request_map);

        log::info!("Request body: {:#?}", body);

        let mut req = client
            .post(format!("{}{}", self.properties.base_url, path))
            .json(&body);
        match self.properties.api_key {
            Some(ref key) => {
                req = req.header("x-api-key", key);
            }
            None => {}
        }
        for (k, v) in &self.properties.headers {
            req = req.header(k, v);
        }

        // Add all the properties as data parameters.
        Ok(req)
    }
}

impl WithChat for AnthropicClient {
    fn chat_options(&self, _ctx: &RuntimeContext) -> Result<internal_baml_jinja::ChatOptions> {
        Ok(internal_baml_jinja::ChatOptions::new(
            self.properties.default_role.clone(),
            None,
        ))
    }

    #[cfg(not(feature = "no_wasm"))]
    async fn chat(
        &self,
        ctx: &RuntimeContext,
        prompt: &Vec<RenderedChatMessage>,
    ) -> Result<LLMResponse> {
        use wasm_bindgen::JsCast;
        use web_sys::Response;
        use web_time::SystemTime;

        use crate::internal::llm_client::{
            anthropic::types::AnthropicErrorResponse, ErrorCode, LLMCompleteResponse,
            LLMErrorResponse,
        };

        use super::types::AnthropicMessageResponse;
        let request = self.build_http_request(ctx, "/v1/messages", prompt)?;
        let window = match web_sys::window() {
            Some(w) => w,
            None => {
                return Ok(LLMResponse::OtherFailures(
                    "Failed to get window object".into(),
                ));
            }
        };

        let now = SystemTime::now();
        let resp_value = wasm_bindgen_futures::JsFuture::from(window.fetch_with_request(&request))
            .await
            .map_err(|e| anyhow::anyhow!("Failed to fetch request: {:#?}", e))?;

        // `resp_value` is a `Response` object.
        assert!(resp_value.is_instance_of::<Response>());
        let resp: Response = resp_value
            .dyn_into()
            .map_err(|e| anyhow::anyhow!("Failed to convert response to Response: {:#?}", e))?;

        let status = resp.status();
        if status != 200 {
            let json = resp.json().map_err(|e| anyhow::anyhow!("{:#?}", e))?;
            let err_message = match wasm_bindgen_futures::JsFuture::from(json).await {
                Ok(body) => {
                    let body =
                        serde_wasm_bindgen::from_value::<serde_json::Value>(body).map_err(|e| {
                            anyhow::anyhow!("Failed to convert response to JSON: {:#?}", e)
                        })?;
                    let err_message = match serde_json::from_value::<AnthropicErrorResponse>(body) {
                        Ok(err) => {
                            format!("API Error ({}): {}", err.error.r#type, err.error.message)
                        }
                        Err(e) => {
                            format!("Does this support the Anthropic Response type?\n{:#?}", e)
                        }
                    };
                    err_message
                }
                Err(e) => {
                    format!("Does this support the Anthropic Response type?\n{:#?}", e)
                }
            };
            return Ok(LLMResponse::LLMFailure(LLMErrorResponse {
                client: self.context.name.to_string(),
                model: None,
                prompt: internal_baml_jinja::RenderedPrompt::Chat(prompt.clone()),
                start_time_unix_ms: now
                    .duration_since(web_time::UNIX_EPOCH)
                    .unwrap()
                    .as_millis() as u64,
                latency_ms: now.elapsed().unwrap().as_millis() as u64,
                message: err_message,
                code: ErrorCode::Other(status),
            }));
        }

        let body = resp.json().map_err(|e| anyhow::anyhow!("{:#?}", e))?;
        let response = wasm_bindgen_futures::JsFuture::from(body)
            .await
            .map_err(|e| {
                anyhow::anyhow!("Does this support the Anthropic Response type?\n{:#?}", e)
            })?;

        let body =
            match serde_wasm_bindgen::from_value::<AnthropicMessageResponse>(response.clone()) {
                Ok(body) => body,
                Err(e) => {
                    return Ok(LLMResponse::LLMFailure(LLMErrorResponse {
                        client: self.context.name.to_string(),
                        model: None,
                        prompt: internal_baml_jinja::RenderedPrompt::Chat(prompt.clone()),
                        start_time_unix_ms: now
                            .duration_since(web_time::UNIX_EPOCH)
                            .unwrap()
                            .as_millis() as u64,
                        latency_ms: now.elapsed().unwrap().as_millis() as u64,
                        message: e.to_string(),
                        code: ErrorCode::UnsupportedResponse(status),
                    }));
                }
            };

        if body.content.len() < 1 {
            return Ok(LLMResponse::LLMFailure(LLMErrorResponse {
                client: self.context.name.to_string(),
                model: None,
                prompt: internal_baml_jinja::RenderedPrompt::Chat(prompt.clone()),
                start_time_unix_ms: now
                    .duration_since(web_time::UNIX_EPOCH)
                    .unwrap()
                    .as_millis() as u64,
                latency_ms: now.elapsed().unwrap().as_millis() as u64,
                message: format!("No content in response:\n{:#?}", body),
                code: ErrorCode::Other(status),
            }));
        }

        let usage = body.usage;

        Ok(LLMResponse::Success(LLMCompleteResponse {
            client: self.context.name.to_string(),
            prompt: internal_baml_jinja::RenderedPrompt::Chat(prompt.clone()),
            content: body.content[0].text.clone(),
            start_time_unix_ms: now
                .duration_since(web_time::UNIX_EPOCH)
                .unwrap()
                .as_millis() as u64,
            latency_ms: now.elapsed().unwrap().as_millis() as u64,
            model: body.model,
            metadata: json!({
                "baml_is_complete": match body.stop_reason {
                    None => true,
                    Some(StopReason::STOP_SEQUENCE) => true,
                    Some(StopReason::END_TURN)  => true,
                    _ => false,
                },
                "finish_reason": body.stop_reason,
                "prompt_tokens": usage.input_tokens,
                "output_tokens": usage.output_tokens,
                "total_tokens": usage.input_tokens + usage.output_tokens,
            }),
        }))
    }

    #[cfg(feature = "no_wasm")]
    async fn chat(
        &self,
        ctx: &RuntimeContext,
        prompt: &Vec<RenderedChatMessage>,
    ) -> Result<LLMResponse> {
        use crate::internal::llm_client::{
            anthropic::types::{
                AnthropicErrorResponse, AnthropicMessageContent, AnthropicMessageResponse,
                StopReason,
            },
            ErrorCode, LLMCompleteResponse, LLMErrorResponse,
        };

        let req = self.build_http_request(ctx, "/v1/messages", prompt)?;

        match self.internal_state.clone().lock() {
            Ok(mut state) => {
                state.call_count += 1;
            }
            Err(e) => {
                log::warn!(
                    "Failed to increment call count for AnthropicClient: {:#?}",
                    e
                );
            }
        }
        let now = std::time::SystemTime::now();
        let res = req.send().await?;

        let status = res.status();

        // Raise for status.
        if !status.is_success() {
            let err_code = ErrorCode::from_status(status);

            let err_message = match res.json::<serde_json::Value>().await {
                Ok(body) => match serde_json::from_value::<AnthropicErrorResponse>(body) {
                    Ok(err) => format!("API Error ({}): {}", err.error.r#type, err.error.message),
                    Err(e) => format!("Does this support the Anthropic Response type?\n{:#?}", e),
                },
                Err(e) => {
                    format!("Does this support the Anthropic Response type?\n{:#?}", e)
                }
            };

            return Ok(LLMResponse::LLMFailure(LLMErrorResponse {
                client: self.context.name.to_string(),
                model: None,
                prompt: internal_baml_jinja::RenderedPrompt::Chat(prompt.clone()),
                start_time_unix_ms: now
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_millis() as u64,
                latency_ms: now.elapsed().unwrap().as_millis() as u64,
                message: err_message,
                code: err_code,
            }));
        }

        let body = match res.json::<AnthropicMessageResponse>().await {
            Ok(body) => body,
            Err(e) => {
                return Ok(LLMResponse::LLMFailure(LLMErrorResponse {
                    client: self.context.name.to_string(),
                    model: None,
                    prompt: internal_baml_jinja::RenderedPrompt::Chat(prompt.clone()),
                    start_time_unix_ms: now
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap()
                        .as_millis() as u64,
                    latency_ms: now.elapsed().unwrap().as_millis() as u64,
                    message: format!("Does this support the Anthropic Response type?\n{:#?}", e),
                    code: ErrorCode::UnsupportedResponse(status.as_u16()),
                }));
            }
        };

        if body.content.len() < 1 {
            return Ok(LLMResponse::LLMFailure(LLMErrorResponse {
                client: self.context.name.to_string(),
                model: None,
                prompt: internal_baml_jinja::RenderedPrompt::Chat(prompt.clone()),
                start_time_unix_ms: now
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_millis() as u64,
                latency_ms: now.elapsed().unwrap().as_millis() as u64,
                message: format!("No content in response:\n{:#?}", body),
                code: ErrorCode::Other(status.as_u16()),
            }));
        }

        let usage = body.usage;

        Ok(LLMResponse::Success(LLMCompleteResponse {
            client: self.context.name.to_string(),
            prompt: internal_baml_jinja::RenderedPrompt::Chat(prompt.clone()),
            content: body.content[0].text.clone(),
            start_time_unix_ms: now
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_millis() as u64,
            latency_ms: now.elapsed().unwrap().as_millis() as u64,
            model: body.model,
            metadata: json!({
                "baml_is_complete": match body.stop_reason {
                    None => true,
                    Some(StopReason::STOP_SEQUENCE) => true,
                    Some(StopReason::END_TURN)  => true,
                    _ => false,
                },
                "finish_reason": body.stop_reason,
                "prompt_tokens": usage.input_tokens,
                "output_tokens": usage.output_tokens,
                "total_tokens": usage.input_tokens + usage.output_tokens,
            }),
        }))
    }
}

impl WithStreamCompletion for AnthropicClient {
    async fn stream_completion(&self, ctx: &RuntimeContext, prompt: &String) -> LLMResponseStream {
        todo!()
    }
}

impl WithStreamChat for AnthropicClient {
    async fn stream_chat(
        &self,
        ctx: &RuntimeContext,
        prompt: &Vec<RenderedChatMessage>,
    ) -> LLMResponseStream {
        todo!()
    }
}

fn build_anthropic_chat_request(prompt: &Vec<RenderedChatMessage>) -> Result<Value> {
    // ensure the number of system messages is <= 1.
    // if there is a system message in the prompt, extract it out of the list and put it in the top-level "system" property of the request.
    let (system, messages): (Vec<&RenderedChatMessage>, Vec<&RenderedChatMessage>) =
        prompt.iter().partition(|m| m.role == "system");

    if system.len() > 1 {
        return Err(anyhow!("Too many system messages in prompt"));
    }

    let mut request = json!({
        "messages": messages
            .iter()
            .map(|m| {
                // Use `convert_message_parts_to_content` with `?` to handle errors
                match convert_message_parts_to_content(&m.parts) {
                    Ok(content) => Ok(json!({
                        "role": m.role,
                        "content": content,
                    })),
                    Err(err) => return Err(err),
                }
            })
            .collect::<Result<Vec<Value>>>()?, // Collect results, propagate error if any
    });

    if let Some(system_message) = system.first() {
        let system_content = convert_system_message_parts_to_text(&system_message.parts)?;
        request["system"] = json!(system_content);
    }

    Ok(request)
}

fn convert_message_parts_to_content(parts: &Vec<ChatMessagePart>) -> Result<Value> {
    let content: Vec<Value> = parts
        .iter()
        .map(|part| match part {
            ChatMessagePart::Text(text) => Ok(json!({
                "type": "text",
                "text": text
            })),
            ChatMessagePart::Image(image) => match image {
                // internal_baml_jinja::BamlImage::Url(image_url) => {
                //     let rt = tokio::runtime::Builder::new_current_thread()
                //         .enable_all()
                //         .build()?;

                //     let (media_type, base64_data) = rt
                //         .block_on(download_image_as_base64(&image_url.url))
                //         .map_err(|e| anyhow!("Failed to fetch image: {}", e))?;
                //     Ok(json!({
                //         "type": "image",
                //         "source": {
                //             "type": "base64",
                //             "media_type": media_type,
                //             "data": base64_data
                //         }
                //     }))
                // }
                _ => Err(anyhow!("Unsupported image type")),
                internal_baml_jinja::BamlImage::Base64(image) => Ok(json!({
                    "type": "image",
                    "source": {
                        "type": "base64",
                        "media_type": image.media_type,
                        "data": image.base64
                    }
                })),
            },
        })
        .collect::<Result<Vec<Value>, Error>>()?;

    Ok(Value::Array(content))
}

fn convert_system_message_parts_to_text(parts: &Vec<ChatMessagePart>) -> Result<String> {
    if parts.len() != 1 {
        return Err(anyhow!("System message must contain exactly one text part, but we detected a file-type in the system message."));
    }

    match &parts[0] {
        ChatMessagePart::Text(text) => Ok(text.clone()),
        _ => Err(anyhow!("System message contains non-text parts")),
    }
}
