use std::{path::PathBuf, pin::Pin};

use anyhow::{Context, Result};

mod chat;
mod completion;
pub use self::{
    chat::{WithChat, WithStreamChat},
    completion::{WithCompletion, WithNoCompletion, WithStreamCompletion},
};
use super::{primitive::request::RequestBuilder, LLMResponse, ModelFeatures};
use crate::{internal::llm_client::SupportedMediaFormats, RenderCurlSettings};
use crate::{internal::prompt_renderer::PromptRenderer, RuntimeContext};
use baml_types::{BamlMedia, BamlMediaContent, BamlMediaType, BamlValue, MediaBase64, MediaUrl};
use base64::{prelude::BASE64_STANDARD, Engine};
use futures::stream::StreamExt;
use infer;
use internal_baml_core::ir::repr::IntermediateRepr;
use internal_baml_jinja::{ChatMessagePart, RenderedChatMessage};
use internal_baml_jinja::{RenderContext_Client, RenderedPrompt};

use shell_escape::escape;
use std::borrow::Cow;

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
        render_settings: RenderCurlSettings,
    ) -> Result<String>;
}

impl<T> WithSingleCallable for T
where
    T: WithClient + WithChat + WithCompletion,
{
    #[allow(async_fn_in_trait)]
    async fn single_call(&self, ctx: &RuntimeContext, prompt: &RenderedPrompt) -> LLMResponse {
        if let RenderedPrompt::Chat(chat) = &prompt {
            match process_media_urls(
                self.model_features().supported_media_formats,
                None,
                ctx,
                chat,
            )
            .await
            {
                Ok(messages) => return self.chat(ctx, &messages).await,
                Err(e) => return LLMResponse::OtherFailure(format!("Error occurred: {:#}", e)),
            }
        }

        match prompt {
            RenderedPrompt::Chat(p) => self.chat(ctx, p).await,
            RenderedPrompt::Completion(p) => self.completion(ctx, p).await,
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
        render_settings: RenderCurlSettings,
    ) -> Result<String> {
        let chat_messages = process_media_urls(
            self.model_features().supported_media_formats,
            Some(render_settings),
            ctx,
            prompt,
        )
        .await?;

        let request_builder = self
            .build_request(either::Right(&chat_messages), false, render_settings.stream)
            .await?;
        let mut request = request_builder.build()?;
        let url_header_value = {
            let url_header_value = request.url();
            url_header_value.to_owned()
        };

        let url_str = url_header_value.to_string();

        {
            let headers = request.headers_mut();
            headers.remove("baml-original-url");
        }

        let body = request
            .body()
            .map(|b| b.as_bytes().unwrap_or_default().to_vec())
            .unwrap_or_default(); // Add this line to handle the Option
        let request_str = to_curl_command(&url_str, "POST", request.headers(), body);

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
        if let RenderedPrompt::Chat(ref chat) = prompt {
            match process_media_urls(
                self.model_features().supported_media_formats,
                None,
                ctx,
                chat,
            )
            .await
            {
                Ok(messages) => return self.stream_chat(ctx, &messages).await,
                Err(e) => {
                    return Err(LLMResponse::OtherFailure(format!(
                        "Error occurred: {:#}",
                        e
                    )))
                }
            }
        }

        match prompt {
            RenderedPrompt::Chat(p) => self.stream_chat(ctx, p).await,
            RenderedPrompt::Completion(p) => self.stream_completion(ctx, p).await,
        }
    }
}

/// We assume b64 with mime-type is the universally accepted format in an API request.
/// Other formats will be converted into that, depending on what formats are allowed according to supported_media_formats.
async fn process_media_urls(
    supported_media_formats: SupportedMediaFormats,
    render_settings: Option<RenderCurlSettings>,
    ctx: &RuntimeContext,
    chat: &Vec<RenderedChatMessage>,
) -> Result<Vec<RenderedChatMessage>, anyhow::Error> {
    let render_settings = render_settings.unwrap_or(RenderCurlSettings {
        stream: false,
        as_shell_commands: false,
    });
    if (render_settings.as_shell_commands) {
        return Ok(chat
            .iter()
            .map(|p| RenderedChatMessage {
                role: p.role.clone(),
                parts: p
                    .parts
                    .iter()
                    .map(|p| {
                        let ChatMessagePart::Media(m) = p else {
                            return p.clone();
                        };
                        let processed = match &m.content {
                            BamlMediaContent::File(media_file) => {
                                let ext =
                                    match std::path::Path::new(&media_file.relpath).extension() {
                                        Some(ext) => ext.to_string_lossy(),
                                        None => "".into(),
                                    };
                                let media_path = match media_file.baml_path.parent() {
                                    Some(parent) => parent
                                        .join(&media_file.relpath)
                                        .to_string_lossy()
                                        .into_owned(),
                                    None => media_file.relpath.clone(),
                                };
                                BamlMediaContent::Base64(MediaBase64 {
                                    base64: format!(
                                        "$(base64 '{}')",
                                        media_path
                                            .strip_prefix("file://")
                                            .unwrap_or(media_path.as_str())
                                    ),
                                    mime_type: format!("image/{ext}"),
                                })
                            }
                            BamlMediaContent::Base64(b64) => BamlMediaContent::Base64(b64.clone()),
                            BamlMediaContent::Url(url) => BamlMediaContent::Url(url.clone()),
                        };

                        ChatMessagePart::Media(BamlMedia {
                            media_type: m.media_type.clone(),
                            content: processed,
                        })
                    })
                    .collect(),
            })
            .collect());
    }
    let messages_result = futures::stream::iter(chat.iter().map(|p| {
        let new_parts = p
            .parts
            .iter()
            .map(|any_part| async move {
                let ChatMessagePart::Media(part) = any_part else {
                    return Ok(any_part.clone());
                };
                match &part.content {
                    BamlMediaContent::File(media_file) => {
                        let Some(ref baml_src_reader) = *ctx.baml_src else {
                            anyhow::bail!("Internal error: no baml src reader provided");
                        };

                        // TODO: ask the proxy in wasm
                        let media_path = media_file
                            .baml_path
                            .parent()
                            .context("Internal error: no path to resolve against")?
                            .join(&media_file.relpath);
                        let bytes = baml_src_reader(media_path.as_os_str())
                            .await
                            .context(format!("Failed to read file {:#}", media_path.display()))?;

                        // let inferred_type = infer::get(&bytes);
                        // if media_file.media_type.is_empty() {
                        //     let mime_type = inferred_type
                        //         .map(|t| t.extension().to_string())
                        //         .unwrap_or_else(|| "application/octet-stream".into());
                        // } else {
                        //     Ok(part.clone())
                        // }
                        Ok(ChatMessagePart::Media(BamlMedia::base64(
                            part.media_type,
                            BASE64_STANDARD.encode(&bytes),
                            media_file
                                .mime_type
                                .clone()
                                // TODO: actually infer the media type
                                .unwrap_or(format!("{}/???", part.media_type)),
                        )))
                    }
                    BamlMediaContent::Url(media_url) => {
                        // TODO: openai does not need mime type resolution
                        if media_url.mime_type.as_deref().unwrap_or("").is_empty() {
                            let (base64, mime_type) = if media_url.url.starts_with("data:") {
                                let parts: Vec<&str> = media_url.url.splitn(2, ',').collect();

                                let base64 = parts.get(1).unwrap().to_string();
                                let prefix = parts.get(0).unwrap();
                                let mime_type = prefix
                                    .splitn(2, ':')
                                    .nth(1)
                                    .unwrap()
                                    .split('/')
                                    .nth(1)
                                    .unwrap()
                                    .split(';')
                                    .next()
                                    .unwrap()
                                    .to_string();

                                (base64, mime_type)
                            } else {
                                let response = match fetch_with_proxy(
                                    &media_url.url,
                                    ctx.env
                                        .get("BOUNDARY_PROXY_URL")
                                        .as_deref()
                                        .map(|s| s.as_str()),
                                )
                                .await
                                {
                                    Ok(response) => response,
                                    Err(e) => {
                                        return Err(anyhow::anyhow!("Failed to fetch media: {e:?}"))
                                    }
                                };
                                let bytes = match response.bytes().await {
                                    Ok(bytes) => bytes,
                                    Err(e) => {
                                        return Err(anyhow::anyhow!(
                                            "Failed to fetch media bytes: {e:?}"
                                        ))
                                    }
                                };
                                let base64 = BASE64_STANDARD.encode(&bytes);
                                // TODO: infer based on file extension?
                                let inferred_type = infer::get(&bytes);
                                let mime_type = inferred_type.map_or_else(
                                    || "application/octet-stream".into(),
                                    |t| t.extension().into(),
                                );
                                (base64, mime_type)
                            };

                            Ok(ChatMessagePart::Media(BamlMedia::base64(
                                part.media_type,
                                base64,
                                format!("{}/{}", part.media_type, mime_type),
                            )))
                        } else {
                            Ok(any_part.clone())
                        }
                    }
                    BamlMediaContent::Base64(media_b64) => {
                        if media_b64.mime_type.is_empty() {
                            let bytes = match BASE64_STANDARD.decode(&media_b64.base64) {
                                Ok(bytes) => bytes,
                                Err(e) => {
                                    return Err(anyhow::anyhow!(
                                        "Failed to decode base64 media: {e:?}"
                                    ))
                                }
                            };
                            let inferred_type = infer::get(&bytes);
                            let mime_type = inferred_type
                                .map(|t| t.extension().to_string())
                                .unwrap_or_else(|| "application/octet-stream".into());

                            Ok(ChatMessagePart::Media(BamlMedia::base64(
                                part.media_type,
                                media_b64.base64.clone(),
                                format!("{}/{}", part.media_type, mime_type),
                            )))
                        } else {
                            Ok(any_part.clone())
                        }
                    }
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

    messages_result
}

async fn fetch_with_proxy(
    url: &str,
    proxy_url: Option<&str>,
) -> Result<reqwest::Response, anyhow::Error> {
    let client = reqwest::Client::new();
    let mut request = if let Some(proxy) = proxy_url {
        client.get(proxy).header("baml-original-url", url)
    } else {
        client.get(url)
    };

    let response = request.send().await?;
    Ok(response)
}
