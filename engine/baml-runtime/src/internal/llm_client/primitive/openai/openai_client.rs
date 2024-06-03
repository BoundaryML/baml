use std::collections::HashMap;

use anyhow::{Context, Result};
use baml_types::BamlImage;
use internal_baml_core::ir::ClientWalker;
use internal_baml_jinja::{ChatMessagePart, RenderContext_Client, RenderedChatMessage};

use serde_json::json;

use crate::internal::llm_client::primitive::request::{
    make_parsed_request, make_request, RequestBuilder,
};
use crate::internal::llm_client::traits::{
    SseResponseTrait, StreamResponse, WithCompletion, WithStreamChat,
};
use crate::internal::llm_client::{
    traits::{WithChat, WithClient, WithNoCompletion, WithRetryPolicy},
    LLMResponse, ModelFeatures,
};

use crate::request::create_client;
use crate::RuntimeContext;
use eventsource_stream::Eventsource;
use futures::StreamExt;

pub struct OpenAIClient {
    pub name: String,
    // client: ClientWalker<'ir>,
    retry_policy: Option<String>,
    context: RenderContext_Client,
    features: ModelFeatures,
    properties: PostRequestProperities,
    // clients
    client: reqwest::Client,
}

impl WithRetryPolicy for OpenAIClient {
    fn retry_policy_name(&self) -> Option<&str> {
        self.retry_policy.as_deref()
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
// TODO: Enable completion with support for completion streams
// impl WithCompletion for OpenAIClient {
//     fn completion_options(
//         &self,
//         ctx: &RuntimeContext,
//     ) -> Result<internal_baml_jinja::CompletionOptions> {
//         return Ok(internal_baml_jinja::CompletionOptions::new("\n".into()));
//     }

//     async fn completion(&self, ctx: &RuntimeContext, prompt: &String) -> LLMResponse {
//         let (response, system_start, instant_start) =
//             match make_parsed_request::<CompletionResponse>(
//                 self,
//                 either::Either::Left(prompt),
//                 false,
//             )
//             .await
//             {
//                 Ok(v) => v,
//                 Err(e) => return e,
//             };

//         if response.choices.len() != 1 {
//             return LLMResponse::LLMFailure(LLMErrorResponse {
//                 client: self.context.name.to_string(),
//                 model: None,
//                 prompt: internal_baml_jinja::RenderedPrompt::Completion(prompt.clone()),
//                 start_time: system_start,
//                 latency: instant_start.elapsed(),
//                 invocation_params: self.properties.properties.clone(),
//                 message: format!(
//                     "Expected exactly one choices block, got {}",
//                     response.choices.len()
//                 ),
//                 code: ErrorCode::Other(200),
//             });
//         }

//         let usage = response.usage.as_ref();

//         LLMResponse::Success(LLMCompleteResponse {
//             client: self.context.name.to_string(),
//             prompt: internal_baml_jinja::RenderedPrompt::Completion(prompt.clone()),
//             content: response.choices[0].text.clone(),
//             start_time: system_start,
//             latency: instant_start.elapsed(),
//             model: response.model,
//             invocation_params: self.properties.properties.clone(),
//             metadata: LLMCompleteResponseMetadata {
//                 baml_is_complete: match response.choices.get(0) {
//                     Some(c) => match c.finish_reason {
//                         Some(FinishReason::Stop) => true,
//                         _ => false,
//                     },
//                     None => false,
//                 },
//                 finish_reason: match response.choices.get(0) {
//                     Some(c) => match c.finish_reason {
//                         Some(FinishReason::Stop) => Some(FinishReason::Stop.to_string()),
//                         _ => None,
//                     },
//                     None => None,
//                 },
//                 prompt_tokens: usage.map(|u| u.prompt_tokens),
//                 output_tokens: usage.map(|u| u.completion_tokens),
//                 total_tokens: usage.map(|u| u.total_tokens),
//             },
//         })
//     }
// }

impl WithChat for OpenAIClient {
    fn chat_options(&self, _ctx: &RuntimeContext) -> Result<internal_baml_jinja::ChatOptions> {
        Ok(internal_baml_jinja::ChatOptions::new(
            self.properties.default_role.clone(),
            None,
        ))
    }

    async fn chat(&self, _ctx: &RuntimeContext, prompt: &Vec<RenderedChatMessage>) -> LLMResponse {
        let (response, system_start, instant_start) =
            match make_parsed_request::<ChatCompletionResponse>(
                self,
                either::Either::Right(prompt),
                false,
            )
            .await
            {
                Ok(v) => v,
                Err(e) => return e,
            };

        if response.choices.len() != 1 {
            return LLMResponse::LLMFailure(LLMErrorResponse {
                client: self.context.name.to_string(),
                model: None,
                prompt: internal_baml_jinja::RenderedPrompt::Chat(prompt.clone()),
                start_time: system_start,
                latency: instant_start.elapsed(),
                invocation_params: self.properties.properties.clone(),
                message: format!(
                    "Expected exactly one choices block, got {}",
                    response.choices.len()
                ),
                code: ErrorCode::Other(200),
            });
        }

        let usage = response.usage.as_ref();

        LLMResponse::Success(LLMCompleteResponse {
            client: self.context.name.to_string(),
            prompt: internal_baml_jinja::RenderedPrompt::Chat(prompt.clone()),
            content: response.choices[0]
                .message
                .content
                .as_ref()
                .map_or("", |s| s.as_str())
                .to_string(),
            start_time: system_start,
            latency: instant_start.elapsed(),
            model: response.model,
            invocation_params: self.properties.properties.clone(),
            metadata: LLMCompleteResponseMetadata {
                baml_is_complete: match response.choices.get(0) {
                    Some(c) => match c.finish_reason {
                        Some(FinishReason::Stop) => true,
                        _ => false,
                    },
                    None => false,
                },
                finish_reason: match response.choices.get(0) {
                    Some(c) => match c.finish_reason {
                        Some(FinishReason::Stop) => Some(FinishReason::Stop.to_string()),
                        _ => None,
                    },
                    None => None,
                },
                prompt_tokens: usage.map(|u| u.prompt_tokens),
                output_tokens: usage.map(|u| u.completion_tokens),
                total_tokens: usage.map(|u| u.total_tokens),
            },
        })
    }
}

use crate::internal::llm_client::{
    ErrorCode, LLMCompleteResponse, LLMCompleteResponseMetadata, LLMErrorResponse,
};

use super::properties::{
    resolve_azure_properties, resolve_ollama_properties, resolve_openai_properties,
    PostRequestProperities,
};
use super::types::{ChatCompletionResponse, ChatCompletionResponseDelta, FinishReason};

impl RequestBuilder for OpenAIClient {
    fn http_client(&self) -> &reqwest::Client {
        &self.client
    }

    fn build_request(
        &self,
        prompt: either::Either<&String, &Vec<RenderedChatMessage>>,
        stream: bool,
    ) -> reqwest::RequestBuilder {
        let mut req = self.client.post(if prompt.is_left() {
            format!("{}/completions", self.properties.base_url)
        } else {
            format!("{}/chat/completions", self.properties.base_url)
        });

        if !self.properties.query_params.is_empty() {
            req = req.query(&self.properties.query_params);
        }

        for (key, value) in &self.properties.headers {
            req = req.header(key, value);
        }
        if let Some(key) = &self.properties.api_key {
            req = req.bearer_auth(key)
        }

        req = req.header("target-provider", "openai");

        let mut body = json!(self.properties.properties);
        let body_obj = body.as_object_mut().unwrap();
        match prompt {
            either::Either::Left(prompt) => {
                body_obj.insert("prompt".into(), json!(prompt));
            }
            either::Either::Right(messages) => {
                body_obj.insert(
                    "messages".into(),
                    messages
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

impl SseResponseTrait for OpenAIClient {
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
                .map(|event| -> Result<ChatCompletionResponseDelta> {
                    Ok(serde_json::from_str::<ChatCompletionResponseDelta>(
                        &event?.data,
                    )?)
                })
                .inspect(|event| log::trace!("{:#?}", event))
                .scan(
                    Ok(LLMCompleteResponse {
                        client: client_name.clone(),
                        prompt: internal_baml_jinja::RenderedPrompt::Chat(prompt.clone()),
                        content: "".to_string(),
                        start_time: system_start,
                        latency: instant_start.elapsed(),
                        model: "".to_string(),
                        invocation_params: params.clone(),
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
                                        invocation_params: params.clone(),
                                        latency: instant_start.elapsed(),
                                        message: format!("Failed to parse event: {:#?}", e),
                                        code: ErrorCode::Other(2),
                                    },
                                )));
                            }
                        };
                        if let Some(choice) = event.choices.get(0) {
                            if let Some(content) = choice.delta.content.as_ref() {
                                inner.content += content.as_str();
                            }
                            inner.model = event.model;
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

impl WithStreamChat for OpenAIClient {
    async fn stream_chat(
        &self,
        ctx: &RuntimeContext,
        prompt: &Vec<RenderedChatMessage>,
    ) -> StreamResponse {
        let (resp, system_start, instant_start) =
            match make_request(self, either::Either::Right(prompt), true).await {
                Ok(v) => v,
                Err(e) => return Err(e),
            };
        self.response_stream(resp, prompt, system_start, instant_start)
    }
}

macro_rules! make_openai_client {
    ($client:ident, $properties:ident) => {
        Ok(Self {
            name: $client.name().into(),
            properties: $properties,
            context: RenderContext_Client {
                name: $client.name().into(),
                provider: $client.elem().provider.clone(),
            },
            features: ModelFeatures {
                chat: true,
                completion: false,
                anthropic_system_constraints: false,
            },
            retry_policy: $client
                .elem()
                .retry_policy_id
                .as_ref()
                .map(|s| s.to_string()),
            client: create_client()?,
        })
    };
}

impl OpenAIClient {
    pub fn new(client: &ClientWalker, ctx: &RuntimeContext) -> Result<OpenAIClient> {
        let properties = resolve_openai_properties(client, ctx)?;
        make_openai_client!(client, properties)
    }

    pub fn new_ollama(client: &ClientWalker, ctx: &RuntimeContext) -> Result<OpenAIClient> {
        let properties = resolve_ollama_properties(client, ctx)?;
        make_openai_client!(client, properties)
    }

    pub fn new_azure(client: &ClientWalker, ctx: &RuntimeContext) -> Result<OpenAIClient> {
        let properties = resolve_azure_properties(client, ctx)?;
        make_openai_client!(client, properties)
    }
}

fn convert_message_parts_to_content(parts: &Vec<ChatMessagePart>) -> serde_json::Value {
    if parts.len() == 1 {
        match &parts[0] {
            ChatMessagePart::Text(text) => return json!(text),
            _ => {}
        }
    }

    let content: Vec<serde_json::Value> = parts
        .into_iter()
        .map(|part| match part {
            ChatMessagePart::Text(text) => json!({"type": "text", "text": text}),
            ChatMessagePart::Image(image) => match image {
                BamlImage::Url(image) => {
                    json!({"type": "image_url", "image_url": json!({
                        "url": image.url
                    })})
                }
                BamlImage::Base64(image) => {
                    json!({"type": "image_url", "image_url": json!({
                        "base64": image.base64
                    })})
                }
            },
        })
        .collect();

    json!(content)
}
