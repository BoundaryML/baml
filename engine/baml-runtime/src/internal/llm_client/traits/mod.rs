use std::{collections::HashMap, path::PathBuf, pin::Pin};

use anyhow::{Context, Result};
use aws_smithy_types::byte_stream::error::Error;
use serde_json::{json, Map};

mod chat;
mod completion;
pub use self::{
    chat::{WithChat, WithStreamChat},
    completion::{WithCompletion, WithNoCompletion, WithStreamCompletion},
};
use super::{primitive::request::RequestBuilder, LLMResponse, ModelFeatures};
use crate::{internal::llm_client::ResolveMediaUrls, RenderCurlSettings};
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

pub trait WithClientProperties {
    fn client_properties(&self) -> &HashMap<String, serde_json::Value>;
    fn allowed_metadata(&self) -> &super::AllowedMetadata;
}

pub trait WithSingleCallable {
    #[allow(async_fn_in_trait)]
    async fn single_call(&self, ctx: &RuntimeContext, prompt: &RenderedPrompt) -> LLMResponse;
}

pub trait WithClient {
    fn context(&self) -> &RenderContext_Client;

    fn model_features(&self) -> &ModelFeatures;
}

pub trait ToProviderMessage: WithClient {
    fn to_chat_message(
        &self,
        content: Map<String, serde_json::Value>,
        text: &str,
    ) -> Result<Map<String, serde_json::Value>>;
    fn to_media_message(
        &self,
        content: Map<String, serde_json::Value>,
        media: &baml_types::BamlMedia,
    ) -> Result<Map<String, serde_json::Value>>;
    fn role_to_message(
        &self,
        content: &RenderedChatMessage,
    ) -> Result<Map<String, serde_json::Value>>;
}

fn merge_messages(chat: &Vec<RenderedChatMessage>) -> Vec<RenderedChatMessage> {
    let mut chat = chat.clone();
    let mut i = 0;
    while i < chat.len() - 1 {
        let (left, right) = chat.split_at_mut(i + 1);
        if left[i].role == right[0].role && !right[0].allow_duplicate_role {
            left[i].parts.extend(right[0].parts.drain(..));
            chat.remove(i + 1);
        } else {
            i += 1;
        }
    }
    chat
}

pub trait ToProviderMessageExt: ToProviderMessage {
    fn chat_to_message(
        &self,
        chat: &Vec<RenderedChatMessage>,
    ) -> Result<Map<String, serde_json::Value>>;

    fn part_to_message(
        &self,
        content: Map<String, serde_json::Value>,
        part: &ChatMessagePart,
    ) -> Result<Map<String, serde_json::Value>> {
        match part {
            ChatMessagePart::Text(t) => self.to_chat_message(content, t),
            ChatMessagePart::Media(m) => self.to_media_message(content, m),
            ChatMessagePart::WithMeta(p, meta) => {
                let mut content = self.part_to_message(content, &p)?;
                for (k, v) in meta {
                    if self.model_features().allowed_metadata.is_allowed(k) {
                        content.insert(k.clone(), v.clone());
                    }
                }
                Ok(content)
            }
        }
    }

    fn parts_to_message(
        &self,
        parts: &Vec<ChatMessagePart>,
    ) -> Result<Vec<Map<String, serde_json::Value>>> {
        Ok(parts
            .iter()
            .map(|p| self.part_to_message(Map::new(), p))
            .collect::<Result<Vec<_>>>()?)
    }
}

pub trait WithPrompt<'ir> {
    #[allow(async_fn_in_trait)]
    async fn render_prompt(
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
                self.model_features().resolve_media_urls,
                true,
                None,
                ctx,
                chat,
            )
            .await
            {
                Ok(messages) => return self.chat(ctx, &messages).await,
                Err(e) => return LLMResponse::OtherFailure(format!("Error occurred:\n\n{:?}", e)),
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
    async fn render_prompt(
        &'ir self,
        ir: &'ir IntermediateRepr,
        renderer: &PromptRenderer,
        ctx: &RuntimeContext,
        params: &BamlValue,
    ) -> Result<RenderedPrompt> {
        let features = self.model_features();

        let prompt = renderer.render_prompt(ir, ctx, params, self.context())?;

        let prompt = match prompt {
            RenderedPrompt::Completion(_) => prompt,
            RenderedPrompt::Chat(chat) => {
                let chat = merge_messages(&chat);
                // We never need to resolve media URLs here: webview rendering understands how to handle URLs and file refs
                let chat =
                    process_media_urls(ResolveMediaUrls::Never, true, None, ctx, &chat).await?;
                RenderedPrompt::Chat(chat)
            }
        };

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
            self.model_features().resolve_media_urls,
            true,
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
                self.model_features().resolve_media_urls,
                true,
                None,
                ctx,
                chat,
            )
            .await
            {
                Ok(messages) => return self.stream_chat(ctx, &messages).await,
                Err(e) => {
                    return Err(LLMResponse::OtherFailure(format!(
                        "Error occurred:\n\n{:?}",
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
    resolve_media_urls: ResolveMediaUrls,
    resolve_files: bool,
    render_settings: Option<RenderCurlSettings>,
    ctx: &RuntimeContext,
    chat: &Vec<RenderedChatMessage>,
) -> Result<Vec<RenderedChatMessage>, anyhow::Error> {
    let render_settings = render_settings.unwrap_or(RenderCurlSettings {
        stream: false,
        as_shell_commands: false,
    });
    let messages_result = futures::stream::iter(chat.iter().map(|p| {
        let new_parts = p
            .parts
            .iter()
            .map(|any_part| async move {
                let Some(part) = any_part.as_media() else {
                    return Ok::<ChatMessagePart, anyhow::Error>(any_part.clone());
                };
                let media = process_media(
                    resolve_media_urls,
                    resolve_files,
                    render_settings,
                    ctx,
                    &part,
                )
                .await
                .map(ChatMessagePart::Media)?;

                if let Some(meta) = any_part.meta() {
                    Ok(media.with_meta(meta.clone()))
                } else {
                    Ok(media)
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
                allow_duplicate_role: p.allow_duplicate_role,
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

async fn process_media(
    resolve_media_urls: ResolveMediaUrls,
    resolve_files: bool,
    render_settings: RenderCurlSettings,
    ctx: &RuntimeContext,
    part: &BamlMedia,
) -> Result<BamlMedia> {
    match &part.content {
        BamlMediaContent::File(media_file) => {
            // Prompt rendering preserves files, because the vscode webview understands files.
            // In all other cases, we always convert files to base64.
            if !resolve_files {
                return Ok(part.clone());
            }

            let media_path = media_file.path()?.to_string_lossy().into_owned();

            if let Some(ext) = media_file.extension() {
                if render_settings.as_shell_commands {
                    return Ok(BamlMedia::base64(
                        part.media_type,
                        format!(
                            "$(base64 '{}')",
                            media_path
                                .strip_prefix("file://")
                                .unwrap_or(media_path.as_str())
                        ),
                        Some(format!("{}/{}", part.media_type, ext)),
                    ));
                }
            }

            let Some(ref baml_src_reader) = *ctx.baml_src else {
                anyhow::bail!("Internal error: no baml src reader provided");
            };

            let bytes = baml_src_reader(media_path.as_str())
                .await
                .context(format!("Failed to read file {:#}", media_path))?;

            let mut mime_type = part.mime_type.clone();

            if mime_type == None {
                if let Some(ext) = media_file.extension() {
                    mime_type = Some(format!("{}/{}", part.media_type, ext));
                }
            }

            if mime_type == None {
                if let Some(t) = infer::get(&bytes) {
                    mime_type = Some(t.mime_type().to_string());
                }
            }

            Ok(BamlMedia::base64(
                part.media_type,
                if render_settings.as_shell_commands {
                    format!(
                        "$(base64 '{}')",
                        media_path
                            .strip_prefix("file://")
                            .unwrap_or(media_path.as_str())
                    )
                } else {
                    BASE64_STANDARD.encode(&bytes)
                },
                mime_type,
            ))
        }
        BamlMediaContent::Url(media_url) => {
            // URLs may have an attached mime-type or not
            // URLs can be converted to either a url with mime-type or base64 with mime-type

            // Here is the table that defines the transformation logic:
            //
            //                           ResolveMediaUrls
            //              --------------------------------------------
            //              | Never      | EnsureMime   | Always       |
            //              |------------|--------------|--------------|
            // url w/o mime | unchanged  | url w/ mime  | b64 w/ mime  |
            // url w/ mime  | unchanged  | unchanged    | b64 w/ mime  |

            // Currently:
            //  - Vertex is ResolveMediaUrls::EnsureMime and is the only one that supports URLs w/ mime-type
            //  - OpenAI is ResolveMediaUrls::Never and allows passing in URLs with optionally specified mime-type

            // NOTE(sam): if a provider accepts URLs but requires mime-type
            // (i.e. Vertex), we currently send it to them as b64. This
            // is how it was implemented originally, and while that could be
            // problematic in theory, I'm not going to change it until a
            // customer complains.
            match (
                resolve_media_urls,
                part.mime_type.as_ref().map(|s| s.as_str()),
            ) {
                (ResolveMediaUrls::Always, _) => {}
                (ResolveMediaUrls::EnsureMime, Some("")) | (ResolveMediaUrls::EnsureMime, None) => {
                }
                (ResolveMediaUrls::Never, _) | (ResolveMediaUrls::EnsureMime, _) => {
                    return Ok(part.clone());
                }
            }

            let (base64, inferred_mime_type) =
                to_base64_with_inferred_mime_type(&ctx, media_url).await?;

            Ok(BamlMedia::base64(
                part.media_type,
                if render_settings.as_shell_commands {
                    format!("$(curl -L '{}' | base64)", &media_url.url)
                } else {
                    base64
                },
                Some(part.mime_type.clone().unwrap_or(inferred_mime_type)),
            ))
        }
        BamlMediaContent::Base64(media_b64) => {
            // Every provider requires mime-type to be attached when passing in b64 data
            // Our initial implementation does not enforce that mime_type is set, so an unset
            // mime_type in a BAML file is actually an empty string when it gets to this point.

            // Ignore 'media_type' even if it is set, if the base64 URL contains a mime-type
            if let Some((mime_type, base64)) = as_base64(media_b64.base64.as_str()) {
                return Ok(BamlMedia::base64(
                    part.media_type,
                    base64.to_string(),
                    Some(mime_type.to_string()),
                ));
            }

            let bytes = BASE64_STANDARD.decode(&media_b64.base64).context(
                format!(
                    "Failed to decode '{}...' as base64 ({}); see https://docs.boundaryml.com/docs/snippets/test-cases#images",
                    media_b64.base64.chars().take(10).collect::<String>(),
                    if media_b64.base64.starts_with("data:") {
                        "it looks like a data URL, not a base64 string"
                    } else {
                        "is it a valid base64 string?"
                    }
                )
            )?;

            let mut mime_type = part.mime_type.clone();

            if mime_type == None {
                if let Some(t) = infer::get(&bytes) {
                    mime_type = Some(t.mime_type().to_string());
                }
            }

            Ok(BamlMedia::base64(
                part.media_type,
                media_b64.base64.clone(),
                mime_type,
            ))
        }
    }
}

async fn to_base64_with_inferred_mime_type(
    ctx: &RuntimeContext,
    media_url: &MediaUrl,
) -> Result<(String, String)> {
    if let Some((mime_type, base64)) = as_base64(&media_url.url.as_str()) {
        return Ok((base64.to_string(), mime_type.to_string()));
    }
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
        Err(e) => return Err(anyhow::anyhow!("Failed to fetch media: {e:?}")),
    };
    let bytes = match response.bytes().await {
        Ok(bytes) => bytes,
        Err(e) => return Err(anyhow::anyhow!("Failed to fetch media bytes: {e:?}")),
    };
    let base64 = BASE64_STANDARD.encode(&bytes);
    // TODO: infer based on file extension?
    let mime_type = match infer::get(&bytes) {
        Some(t) => t.mime_type(),
        None => "application/octet-stream",
    }
    .to_string();
    Ok((base64, mime_type))
}

/// A naive implementation of the data URL parser, returning the (mime_type, base64)
/// if parsing succeeds. Specifically, we only support specifying a single mime-type (so
/// fields like 'charset' will be ignored) and only base64 data URLs.
///
/// See: https://fetch.spec.whatwg.org/#data-urls
fn as_base64<'s>(maybe_base64_url: &'s str) -> Option<(&'s str, &'s str)> {
    if let Some(data_url) = maybe_base64_url.strip_prefix("data:") {
        if let Some((mime_type, base64)) = data_url.split_once(";base64,") {
            return Some((mime_type, base64));
        }
    }

    None
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
