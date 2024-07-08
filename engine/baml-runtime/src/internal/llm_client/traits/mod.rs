use std::pin::Pin;

use anyhow::Result;

mod chat;
mod completion;
pub use self::{
    chat::{WithChat, WithStreamChat},
    completion::{WithCompletion, WithNoCompletion, WithStreamCompletion},
};
use super::{primitive::request::RequestBuilder, LLMResponse, ModelFeatures};

use crate::{internal::prompt_renderer::PromptRenderer, RuntimeContext};
use baml_types::{BamlMedia, BamlMediaType, BamlValue, MediaBase64};
use base64::{prelude::BASE64_STANDARD, Engine};
use futures::stream::{StreamExt, TryStreamExt};
use infer;
use internal_baml_core::ir::repr::IntermediateRepr;
use internal_baml_jinja::{ChatMessagePart, RenderedChatMessage};
use internal_baml_jinja::{RenderContext_Client, RenderedPrompt};

use reqwest::Url;

use shell_escape::escape;
use std::borrow::Cow;

use std::str::FromStr; // Add this line at the top of your file // Add this line at the top of your file

// #[enum_dispatch]

// #[delegatable_trait]
// #[enum_dispatch]
pub trait WithRetryPolicy {
    fn retry_policy_name(&self) -> Option<&str>;
}

pub trait WithSingleCallable {
    #[allow(async_fn_in_trait)]
    async fn single_call(&self, ctx: &RuntimeContext, prompt: &RenderedPrompt) -> LLMResponse;
}

pub trait WithCurl {
    #[allow(async_fn_in_trait)]
    async fn curl_call(
        &self,
        ctx: &RuntimeContext,
        prompt: &RenderedPrompt,
    ) -> Result<Vec<RenderedChatMessage>, LLMResponse>;
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

// #[delegatable_trait]
// #[enum_dispatch]
pub trait WithRenderRawCurl {
    #[allow(async_fn_in_trait)]
    async fn render_raw_curl(
        &self,
        ctx: &RuntimeContext,
        prompt: &Vec<RenderedChatMessage>,
        stream: bool,
    ) -> Result<String>;
}

impl<T> WithSingleCallable for T
where
    T: WithClient + WithChat + WithCompletion,
{
    #[allow(async_fn_in_trait)]
    async fn single_call(&self, ctx: &RuntimeContext, prompt: &RenderedPrompt) -> LLMResponse {
        if self.model_features().resolve_media_urls {
            if let RenderedPrompt::Chat(ref chat) = prompt {
                let messages_result = futures::stream::iter(chat.iter().map(|p| {
                    let new_parts = p
                        .parts
                        .iter()
                        .map(|part| async move {
                            match part {
                                ChatMessagePart::Image(BamlMedia::Url(_, media_url))
                                | ChatMessagePart::Audio(BamlMedia::Url(_, media_url)) => {


                                    
                                    let (base64, mut mime_type) = if media_url.url.starts_with("data:") {
                                        let parts: Vec<&str> =
                                            media_url.url.splitn(2, ',').collect();
                                        let base64 = parts.get(1).unwrap().to_string();
                                        let prefix = parts.get(0).unwrap();
                                        let mut mime_type =
                                            prefix.splitn(2, ':').next().unwrap().to_string()
                                            .split('/').last().unwrap().to_string();

                                        (base64, mime_type)
                                    } else {
                                        let client = reqwest::Client::new();
                                        let response = match client.get(&media_url.url)
                                            // NB(sam): this would workaround CORS issues, but https://github.com/seanmonstar/reqwest/issues/1401
                                            //.fetch_mode_no_cors()
                                            .send().await
                                        {
                                            Ok(response) => response,
                                            Err(e) => {
                                                return Err(LLMResponse::OtherFailure(
                                                    format!("Failed to fetch image due to CORS issue: {e:?}")
                                                ))
                                            } // replace with your error conversion logic
                                        };
                                        let bytes = match response.bytes().await {
                                            Ok(bytes) => bytes,
                                            Err(e) => {
                                                return Err(LLMResponse::OtherFailure(
                                                    e.to_string(),
                                                ));
                                            } // replace with your error conversion logic
                                        };
                                        let base64 = BASE64_STANDARD.encode(&bytes);
                                        let inferred_type = infer::get(&bytes);
                                        let mut mime_type = inferred_type.map_or_else(
                                            || "application/octet-stream".into(),
                                            |t| t.extension().into(),
                                        );
                                        (base64, mime_type)
                                    };

                                    if let Some(media_type) = &media_url.media_type {
                                        mime_type = media_type.clone().split('/').last().unwrap().to_string();
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
                                _ => Ok(part.clone()),
                            }
                        })
                        .collect::<Vec<_>>();
                    async move {
                        let new_parts = futures::stream::iter(new_parts)
                            .then(|f| f)
                            .collect::<Vec<_>>()
                            .await;

                        let new_parts = new_parts.into_iter().collect::<Result<Vec<_>, _>>()?;

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
                        return LLMResponse::OtherFailure(format!("Error occurred: {}", e));
                    }
                };
                return self.chat(ctx, &messages).await;
            }
        }

        match prompt {
            RenderedPrompt::Chat(p) => self.chat(ctx, p).await,
            RenderedPrompt::Completion(p) => self.completion(ctx, p).await,
        }
    }
}

impl<T> WithCurl for T
where
    T: WithClient + WithChat + WithCompletion,
{
    #[allow(async_fn_in_trait)]
    async fn curl_call(
        &self,
        ctx: &RuntimeContext,
        prompt: &RenderedPrompt,
    ) -> Result<Vec<RenderedChatMessage>, LLMResponse> {
        if self.model_features().resolve_media_urls {
            if let RenderedPrompt::Chat(ref chat) = prompt {
                let messages_result = futures::stream::iter(chat.iter().map(|p| {
                    let new_parts = p
                        .parts
                        .iter()
                        .map(|part| async move {
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
                                        let client = reqwest::Client::new();
                                        let response = match client.get(&media_url.url).send().await
                                        {
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
                                        base64 = BASE64_STANDARD.encode(&bytes);
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
                                _ => Ok(part.clone()),
                            }
                        })
                        .collect::<Vec<_>>();
                    async move {
                        let new_parts = futures::stream::iter(new_parts)
                            .then(|f| f)
                            .collect::<Vec<_>>()
                            .await;

                        let new_parts = new_parts.into_iter().collect::<Result<Vec<_>, _>>()?;

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
                        return Err(LLMResponse::OtherFailure(format!("Error occurred: {}", e)));
                    }
                };
                return Ok(messages);
            }
        }

        match prompt {
            RenderedPrompt::Chat(p) => Ok(p.clone()),
            RenderedPrompt::Completion(p) => Err(LLMResponse::OtherFailure(
                "Completion prompts are not supported by this provider".to_string(),
            )),
        }
    }
}

fn escape_single_quotes(s: &str) -> String {
    escape(Cow::Borrowed(s)).to_string()
}

fn to_curl_command(
    url: &str,
    method: &str,
    headers: &reqwest::header::HeaderMap,
    body: Vec<u8>,
) -> String {
    let mut curl_command = format!("curl -X {} '{}'", method, url);

    for (key, value) in headers.iter() {
        let header = format!(" -H \"{}: {}\"", key.as_str(), value.to_str().unwrap());
        curl_command.push_str(&header);
    }

    let body_json = String::from_utf8_lossy(&body).to_string(); // Convert body to string
    let pretty_body_json = match serde_json::from_str::<serde_json::Value>(&body_json) {
        Ok(json_value) => serde_json::to_string_pretty(&json_value).unwrap_or(body_json),
        Err(_) => body_json,
    };
    let fully_escaped_body_json = escape_single_quotes(&pretty_body_json);
    let body_part = format!(" -d {}", fully_escaped_body_json);
    curl_command.push_str(&body_part);

    curl_command
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

impl<T> WithRenderRawCurl for T
where
    T: WithClient + WithChat + WithCompletion + RequestBuilder,
{
    async fn render_raw_curl(
        &self,
        ctx: &RuntimeContext,
        prompt: &Vec<internal_baml_jinja::RenderedChatMessage>,
        stream: bool,
    ) -> Result<String> {
        let rendered_prompt = RenderedPrompt::Chat(prompt.clone());
        log::info!("Stream is {}", stream);
        let chat_messages = self.curl_call(ctx, &rendered_prompt).await?;
        let request_builder = self
            .build_request(either::Right(&chat_messages), stream)
            .await?;
        let mut request = request_builder.build()?;
        let url_header_value = {
            let headers = request.headers_mut();
            let url_header_value = headers
                .get("baml-render-url")
                .ok_or(anyhow::anyhow!("Missing header 'baml-render-url'"))?;
            url_header_value.to_owned()
        };

        let url_str = url_header_value
            .to_str()
            .map_err(|_| anyhow::anyhow!("Invalid header 'baml-render-url'"))?;
        let mut new_url = Url::from_str(url_str)?;
        new_url.set_query(request.url().query()); // Preserve query parameters

        *request.url_mut() = new_url;

        
        let headers = request.headers_mut();
        headers.remove("baml-original-url");
        headers.remove("baml-render-url");
        

        let body = request
            .body()
            .map(|b| b.as_bytes().unwrap_or_default().to_vec())
            .unwrap_or_default(); // Add this line to handle the Option
        let request_str = to_curl_command(url_str, "POST", request.headers(), body);

        Ok(request_str)
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
        if self.model_features().resolve_media_urls {
            if let RenderedPrompt::Chat(ref chat) = prompt {
                let messages = futures::stream::iter(chat.iter().map(|p| {
                    let new_parts = p
                        .parts
                        .iter()
                        .map(|part| async move {
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
                                        let client = reqwest::Client::new();
                                        let response = match client
                                            .get(&media_url.url)
                                            // NB(sam): this would workaround CORS issues, but https://github.com/seanmonstar/reqwest/issues/1401
                                            //.fetch_mode_no_cors()
                                            .send()
                                            .await
                                        {
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
                                        base64 = BASE64_STANDARD.encode(&bytes);
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
                                _ => Ok(part.clone()),
                            }
                        })
                        .collect::<Vec<_>>();
                    async move {
                        let new_parts = futures::stream::iter(new_parts)
                            .then(|f| f)
                            .collect::<Vec<_>>()
                            .await;

                        let new_parts = new_parts.into_iter().collect::<Result<Vec<_>, _>>()?;

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

                return self.stream_chat(ctx, &messages).await;
            }
        }

        match prompt {
            RenderedPrompt::Chat(p) => self.stream_chat(ctx, p).await,
            RenderedPrompt::Completion(p) => self.stream_completion(ctx, p).await,
        }
    }
}
