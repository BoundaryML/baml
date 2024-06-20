use std::{fmt::format, pin::Pin};

use anyhow::Result;
mod chat;
mod completion;
pub use self::{
    chat::{WithChat, WithStreamChat},
    completion::{WithCompletion, WithNoCompletion, WithStreamCompletion},
};
use super::{retry_policy::CallablePolicy, LLMResponse, ModelFeatures};
use crate::{internal::prompt_renderer::PromptRenderer, RuntimeContext};
use baml_types::{BamlMedia, BamlMediaType, BamlValue, MediaBase64, MediaUrl};
use base64::encode;
use futures::stream::{StreamExt, TryStreamExt};
use infer;
use internal_baml_core::ir::repr::IntermediateRepr;
use internal_baml_jinja::{ChatMessagePart, RenderedChatMessage};
use internal_baml_jinja::{RenderContext_Client, RenderedPrompt};
use reqwest::get;

pub trait WithRetryPolicy {
    fn retry_policy_name(&self) -> Option<&str>;
}

pub trait WithSingleCallable {
    #[allow(async_fn_in_trait)]
    async fn single_call(&self, ctx: &RuntimeContext, prompt: &RenderedPrompt) -> LLMResponse;
}

pub trait WithClient {
    fn context(&self) -> &RenderContext_Client;

    fn model_features(&self) -> &ModelFeatures;
}

pub trait WithPrompt<'ir> {
    fn render_prompt(
        &'ir self,
        ir: &'ir IntermediateRepr,
        renderer: &PromptRenderer,
        ctx: &RuntimeContext,
        params: &BamlValue,
    ) -> Result<RenderedPrompt>;
}

impl<T> WithSingleCallable for T
where
    T: WithClient + WithChat + WithCompletion,
{
    #[allow(async_fn_in_trait)]
    async fn single_call(&self, ctx: &RuntimeContext, prompt: &RenderedPrompt) -> LLMResponse {
        log::info!("Starting stream");
        if self.model_features().resolve_media_urls {
            log::info!("Resolving media URLs");

            if let RenderedPrompt::Chat(ref chat) = prompt {
                log::info!("Processing chat prompt");
                let messages_result = futures::stream::iter(chat.iter().map(|p| {
                    log::info!("Processing chat message");
                    let new_parts = p
                        .parts
                        .iter()
                        .map(|part| async move {
                            log::info!("Processing chat message part");
                            match part {
                                ChatMessagePart::Image(BamlMedia::Url(_, media_url))
                                | ChatMessagePart::Audio(BamlMedia::Url(_, media_url)) => {
                                    let mut base64 = "".to_string();
                                    let mut mime_type = "".to_string();
                                    if media_url.url.starts_with("data:") {
                                        let parts: Vec<&str> =
                                            media_url.url.splitn(2, ',').collect();
                                        base64 = parts.get(1).unwrap().to_string();
                                        let prefix = parts.get(0).unwrap();
                                        mime_type =
                                            prefix.splitn(2, ':').next().unwrap().to_string();
                                        mime_type =
                                            mime_type.split('/').last().unwrap().to_string();
                                    } else {
                                        let response = match get(&media_url.url).await {
                                            Ok(response) => response,
                                            Err(e) => {
                                                return Err(LLMResponse::OtherFailure(
                                                    "Failed to fetch image due to CORS issue"
                                                        .to_string(),
                                                ))
                                            } // replace with your error conversion logic
                                        };
                                        let bytes = match response.bytes().await {
                                            Ok(bytes) => bytes,
                                            Err(e) => {
                                                return Err(LLMResponse::OtherFailure(
                                                    e.to_string(),
                                                ))
                                            } // replace with your error conversion logic
                                        };
                                        base64 = encode(&bytes);
                                        let inferred_type = infer::get(&bytes);
                                        mime_type = inferred_type.map_or_else(
                                            || "application/octet-stream".into(),
                                            |t| t.extension().into(),
                                        );
                                    }

                                    Ok(if matches!(part, ChatMessagePart::Image(_)) {
                                        ChatMessagePart::Image(BamlMedia::Base64(
                                            BamlMediaType::Image,
                                            MediaBase64 {
                                                base64: base64,
                                                media_type: format!("image/{}", mime_type),
                                            },
                                        ))
                                    } else {
                                        ChatMessagePart::Audio(BamlMedia::Base64(
                                            BamlMediaType::Audio,
                                            MediaBase64 {
                                                base64: base64,
                                                media_type: format!("audio/{}", mime_type),
                                            },
                                        ))
                                    })
                                }
                                _ => {
                                    log::info!("Processing text part");
                                    Ok(part.clone())
                                }
                            }
                        })
                        .collect::<Vec<_>>();
                    async move {
                        log::info!("Collecting new parts");
                        let new_parts = futures::stream::iter(new_parts)
                            .then(|f| f)
                            .collect::<Vec<_>>()
                            .await;

                        let new_parts = new_parts.into_iter().collect::<Result<Vec<_>, _>>()?;

                        log::info!("Creating rendered chat message");
                        Ok::<_, anyhow::Error>(RenderedChatMessage {
                            role: p.role.clone(),
                            parts: new_parts,
                        })
                    }
                }))
                .then(|f| f)
                .collect::<Vec<_>>()
                .await
                .into_iter()
                .collect::<Result<Vec<_>, _>>();

                let messages = match messages_result {
                    Ok(messages) => messages,
                    Err(e) => {
                        log::error!("Error occurred: {}", e);
                        return LLMResponse::OtherFailure(format!("Error occurred: {}", e));
                    }
                };
                return self.chat(ctx, &messages).await;
            }
        }
        log::info!("Processing prompt");
        match prompt {
            RenderedPrompt::Chat(p) => {
                log::info!("Streaming chat");
                self.chat(ctx, p).await
            }
            RenderedPrompt::Completion(p) => {
                log::info!("Streaming completion");
                self.completion(ctx, p).await
            }
        }
    }
}

impl<'ir, T> WithPrompt<'ir> for T
where
    T: WithClient + WithChat + WithCompletion,
{
    fn render_prompt(
        &'ir self,
        ir: &'ir IntermediateRepr,
        renderer: &PromptRenderer,
        ctx: &RuntimeContext,
        params: &BamlValue,
    ) -> Result<RenderedPrompt> {
        let features = self.model_features();

        let prompt = renderer.render_prompt(ir, ctx, params, self.context())?;
        log::debug!("WithPrompt.render_prompt => {:#?}", prompt);

        let mut prompt = match (features.completion, features.chat) {
            (true, false) => {
                let options = self.completion_options(ctx)?;
                prompt.as_completion(&options)
            }
            (false, true) => {
                let options = self.chat_options(ctx)?;
                prompt.as_chat(&options)
            }
            (true, true) => prompt,
            (false, false) => anyhow::bail!("No model type supported"),
        };

        if features.anthropic_system_constraints {
            // Do some more fixes.
            match &mut prompt {
                RenderedPrompt::Chat(chat) => {
                    if chat.len() == 1 && chat[0].role == "system" {
                        // If there is only one message and it is a system message, change it to a user message, because anthropic always requires a user message.
                        chat[0].role = "user".into();
                    } else {
                        // Otherwise, proceed with the existing logic for other messages.
                        chat.iter_mut().skip(1).for_each(|c| {
                            if c.role == "system" {
                                c.role = "assistant".into();
                            }
                        });
                    }
                }
                _ => {}
            }
        }

        Ok(prompt)
    }
}

// Stream related
pub trait SseResponseTrait {
    fn response_stream(
        &self,
        resp: reqwest::Response,
        prompt: &Vec<internal_baml_jinja::RenderedChatMessage>,
        system_start: web_time::SystemTime,
        instant_start: web_time::Instant,
    ) -> StreamResponse;
}

#[cfg(target_arch = "wasm32")]
pub type StreamResponse = Result<Pin<Box<dyn futures::Stream<Item = LLMResponse>>>, LLMResponse>;

#[cfg(not(target_arch = "wasm32"))]
pub type StreamResponse =
    Result<Pin<Box<dyn futures::Stream<Item = LLMResponse> + Send + Sync>>, LLMResponse>;

pub trait WithStreamable {
    /// Retries are not supported for streaming calls.
    #[allow(async_fn_in_trait)]
    async fn stream(&self, ctx: &RuntimeContext, prompt: &RenderedPrompt) -> StreamResponse;
}

impl<T> WithStreamable for T
where
    T: WithClient + WithStreamChat + WithStreamCompletion,
{
    #[allow(async_fn_in_trait)]
    async fn stream(&self, ctx: &RuntimeContext, prompt: &RenderedPrompt) -> StreamResponse {
        log::info!("Starting stream");
        if self.model_features().resolve_media_urls {
            log::info!("Resolving media URLs");

            if let RenderedPrompt::Chat(ref chat) = prompt {
                log::info!("Processing chat prompt");
                let messages = futures::stream::iter(chat.iter().map(|p| {
                    log::info!("Processing chat message");
                    let new_parts = p
                        .parts
                        .iter()
                        .map(|part| async move {
                            log::info!("Processing chat message part");
                            match part {
                                ChatMessagePart::Image(BamlMedia::Url(media_type, media_url))
                                | ChatMessagePart::Audio(BamlMedia::Url(media_type, media_url)) => {
                                    let mut base64 = "".to_string();
                                    let mut mime_type = "".to_string();
                                    if media_url.url.starts_with("data:") {
                                        let parts: Vec<&str> =
                                            media_url.url.splitn(2, ',').collect();
                                        base64 = parts.get(1).unwrap().to_string();
                                        let prefix = parts.get(0).unwrap();
                                        mime_type = prefix
                                            .splitn(2, ':')
                                            .last()
                                            .unwrap() // Get the part after "data:"
                                            .split('/')
                                            .last()
                                            .unwrap() // Get the part after "image/"
                                            .split(';')
                                            .next()
                                            .unwrap() // Get the part before ";base64"
                                            .to_string();
                                    } else {
                                        let response = match get(&media_url.url).await {
                                            Ok(response) => response,
                                            Err(e) => {
                                                return Err(LLMResponse::OtherFailure(
                                                    "Failed to fetch image due to CORS issue"
                                                        .to_string(),
                                                ))
                                            } // replace with your error conversion logic
                                        };
                                        let bytes = match response.bytes().await {
                                            Ok(bytes) => bytes,
                                            Err(e) => {
                                                return Err(LLMResponse::OtherFailure(
                                                    e.to_string(),
                                                ))
                                            } // replace with your error conversion logic
                                        };
                                        base64 = encode(&bytes);
                                        let inferred_type = infer::get(&bytes);
                                        mime_type = inferred_type.map_or_else(
                                            || "application/octet-stream".into(),
                                            |t| t.extension().into(),
                                        );
                                    }
                                    Ok(if matches!(part, ChatMessagePart::Image(_)) {
                                        ChatMessagePart::Image(BamlMedia::Base64(
                                            BamlMediaType::Image,
                                            MediaBase64 {
                                                base64: base64,
                                                media_type: format!("image/{}", mime_type),
                                            },
                                        ))
                                    } else {
                                        ChatMessagePart::Audio(BamlMedia::Base64(
                                            BamlMediaType::Audio,
                                            MediaBase64 {
                                                base64: base64,
                                                media_type: format!("audio/{}", mime_type),
                                            },
                                        ))
                                    })
                                }
                                _ => {
                                    log::info!("Processing text part");
                                    Ok(part.clone())
                                }
                            }
                        })
                        .collect::<Vec<_>>();
                    async move {
                        log::info!("Collecting new parts");
                        let new_parts = futures::stream::iter(new_parts)
                            .then(|f| f)
                            .collect::<Vec<_>>()
                            .await;

                        let new_parts = new_parts.into_iter().collect::<Result<Vec<_>, _>>()?;

                        log::info!("Creating rendered chat message");
                        Ok(RenderedChatMessage {
                            role: p.role.clone(),
                            parts: new_parts,
                        })
                    }
                }))
                .then(|f| f)
                .collect::<Vec<_>>()
                .await
                .into_iter()
                .collect::<Result<Vec<_>, _>>()?;
                log::info!("Streaming chat");
                return self.stream_chat(ctx, &messages).await;
            }
        }
        log::info!("Processing prompt");
        match prompt {
            RenderedPrompt::Chat(p) => {
                log::info!("Streaming chat");
                self.stream_chat(ctx, p).await
            }
            RenderedPrompt::Completion(p) => {
                log::info!("Streaming completion");
                self.stream_completion(ctx, p).await
            }
        }
    }
}
