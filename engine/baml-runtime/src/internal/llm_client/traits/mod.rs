use std::{collections::HashMap, path::PathBuf, pin::Pin};

use anyhow::{Context, Result};
use aws_smithy_types::byte_stream::error::Error;
use internal_llm_client::{AllowedRoleMetadata, FinishReasonFilter};
use serde_json::{json, Map};

mod chat;
mod completion;
use std::borrow::Cow;

use baml_types::{BamlMedia, BamlMediaContent, BamlMediaType, BamlValue, MediaBase64, MediaUrl};
use base64::{prelude::BASE64_STANDARD, Engine};
use futures::stream::StreamExt;
use infer;
use internal_baml_core::ir::repr::IntermediateRepr;
use internal_baml_jinja::{
    ChatMessagePart, RenderContext_Client, RenderedChatMessage, RenderedPrompt,
};
use serde_json::Value as JsonValue;
use shell_escape::escape;

pub use self::{
    chat::{WithChat, WithStreamChat},
    completion::{WithCompletion, WithNoCompletion, WithStreamCompletion},
};
use super::{primitive::request::RequestBuilder, LLMResponse, ModelFeatures};
use crate::{
    internal::{llm_client::ResolveMediaUrls, prompt_renderer::PromptRenderer},
    RenderCurlSettings, RuntimeContext,
};

pub trait HttpContext {
    fn http_request_id(&self) -> &baml_ids::HttpRequestId;
    fn runtime_context(&self) -> &RuntimeContext;
}

// #[enum_dispatch]

// #[delegatable_trait]
// #[enum_dispatch]
pub trait WithRetryPolicy {
    fn retry_policy_name(&self) -> Option<&str>;
}

pub trait WithClientProperties {
    fn allowed_metadata(&self) -> &AllowedRoleMetadata;
    fn supports_streaming(&self) -> bool;
    fn finish_reason_filter(&self) -> &FinishReasonFilter;
    fn default_role(&self) -> String;
    fn allowed_roles(&self) -> Vec<String>;
}

pub trait WithSingleCallable {
    #[allow(async_fn_in_trait)]
    async fn single_call(&self, ctx: &impl HttpContext, prompt: &RenderedPrompt) -> LLMResponse;
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

pub trait CompletionToProviderBody {
    fn completion_to_provider_body(
        &self,
        prompt: &str,
    ) -> serde_json::Map<String, serde_json::Value>;
}

fn merge_messages(chat: &[RenderedChatMessage]) -> Vec<RenderedChatMessage> {
    let mut chat = chat.to_owned();
    let mut i: usize = 0;
    while i < chat.len().saturating_sub(1) {
        let (left, right) = chat.split_at_mut(i + 1);
        if left[i].role == right[0].role && !right[0].allow_duplicate_role {
            left[i].parts.append(&mut right[0].parts);
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
        chat: &[RenderedChatMessage],
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
                let mut content = self.part_to_message(content, p)?;
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
        parts: &[ChatMessagePart],
    ) -> Result<Vec<Map<String, serde_json::Value>>> {
        parts
            .iter()
            .map(|p| self.part_to_message(Map::new(), p))
            .collect::<Result<Vec<_>>>()
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
        prompt: &[RenderedChatMessage],
        render_settings: RenderCurlSettings,
    ) -> Result<String>;
}

impl<T> WithSingleCallable for T
where
    T: WithClient + WithChat + WithCompletion + WithClientProperties,
{
    #[allow(async_fn_in_trait)]
    async fn single_call(&self, ctx: &impl HttpContext, prompt: &RenderedPrompt) -> LLMResponse {
        match prompt {
            RenderedPrompt::Chat(chat) => match process_media_urls(
                self.model_features().resolve_audio_urls,
                self.model_features().resolve_image_urls,
                self.model_features().resolve_pdf_urls,
                self.model_features().resolve_video_urls,
                true,
                None,
                ctx.runtime_context(),
                chat,
            )
            .await
            {
                Ok(messages) => self.chat(ctx, &messages).await,
                Err(e) => LLMResponse::InternalFailure(format!("Error occurred:\n\n{e:?}")),
            },

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
    env_vars: &std::collections::HashMap<String, String>,
    expose_secrets: bool,
) -> String {
    let mut curl_command = format!("curl -X {method} '{url}'");

    // Prepare headers, scrubbing if secrets should not be exposed
    for (key, value) in headers.iter() {
        let key_str = key.as_str();
        let value_str = value.to_str().unwrap_or("");
        let value_str =
            crate::redaction::scrub_header_value(key_str, value_str, env_vars, expose_secrets);
        let header = format!(" -H \"{key_str}: {value_str}\"");
        curl_command.push_str(&header);
    }

    // Body: pretty print JSON if possible, then scrub if secrets shouldn't be exposed
    let body_json = String::from_utf8_lossy(&body).to_string();
    let mut pretty_body_json = match serde_json::from_str::<serde_json::Value>(&body_json) {
        Ok(json_value) => serde_json::to_string_pretty(&json_value).unwrap_or(body_json),
        Err(_) => body_json,
    };

    pretty_body_json =
        crate::redaction::scrub_body_string(&pretty_body_json, env_vars, expose_secrets);
    let fully_escaped_body_json = escape_single_quotes(&pretty_body_json);
    let body_part = format!(" -d {fully_escaped_body_json}");
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
                let chat = process_media_urls(
                    features.resolve_audio_urls,
                    features.resolve_image_urls,
                    features.resolve_pdf_urls,
                    features.resolve_video_urls,
                    true,
                    None,
                    ctx,
                    &chat,
                )
                .await?;
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

        if features.max_one_system_prompt {
            // Do some more fixes.
            if let RenderedPrompt::Chat(chat) = &mut prompt {
                if chat.len() == 1 && chat[0].role == "system" {
                    // If there is only one message and it is a system message, change it to a user message,
                    // because these models always requires a user message.
                    chat[0].role = "user".into();
                } else {
                    // Otherwise, proceed with the existing logic for other messages.
                    chat.iter_mut().skip(1).for_each(|c| {
                        if c.role == "system" {
                            c.role = "user".into();
                        }
                    });
                }
            }
        }

        Ok(prompt)
    }
}

impl<T> WithRenderRawCurl for T
where
    T: WithClient + WithChat + WithCompletion + RequestBuilder + WithClientProperties,
{
    async fn render_raw_curl(
        &self,
        ctx: &RuntimeContext,
        prompt: &[internal_baml_jinja::RenderedChatMessage],
        render_settings: RenderCurlSettings,
    ) -> Result<String> {
        let chat_messages: Vec<RenderedChatMessage> = process_media_urls(
            self.model_features().resolve_audio_urls,
            self.model_features().resolve_image_urls,
            self.model_features().resolve_pdf_urls,
            self.model_features().resolve_video_urls,
            true,
            Some(render_settings),
            ctx,
            prompt,
        )
        .await?;

        let request_builder = self
            .build_request(
                either::Right(&chat_messages),
                false,
                render_settings.stream && self.supports_streaming(),
                render_settings.expose_secrets,
            )
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
        let request_str = to_curl_command(
            &url_str,
            "POST",
            request.headers(),
            body,
            ctx.env_vars(),
            render_settings.expose_secrets,
        );

        Ok(request_str)
    }
}

// Stream related
pub trait SseResponseTrait {
    fn response_stream(
        &self,
        resp: reqwest::Response,
        prompt: &[internal_baml_jinja::RenderedChatMessage],
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
    async fn stream(&self, ctx: &impl HttpContext, prompt: &RenderedPrompt) -> StreamResponse;
}

impl<T> WithStreamable for T
where
    T: WithClient
        + WithStreamChat
        + WithStreamCompletion
        + WithClientProperties
        + WithChat
        + WithCompletion,
{
    #[allow(async_fn_in_trait)]
    async fn stream(&self, ctx: &impl HttpContext, prompt: &RenderedPrompt) -> StreamResponse {
        let prompt = {
            if let RenderedPrompt::Chat(ref chat) = prompt {
                match process_media_urls(
                    self.model_features().resolve_audio_urls,
                    self.model_features().resolve_image_urls,
                    self.model_features().resolve_pdf_urls,
                    self.model_features().resolve_video_urls,
                    true,
                    None,
                    ctx.runtime_context(),
                    chat,
                )
                .await
                {
                    Ok(messages) => &RenderedPrompt::Chat(messages),
                    Err(e) => {
                        return Err(LLMResponse::InternalFailure(format!(
                            "Error occurred:\n\n{e:?}"
                        )))
                    }
                }
            } else {
                prompt
            }
        };

        match prompt {
            RenderedPrompt::Chat(p) => {
                if self.supports_streaming() {
                    self.stream_chat(ctx, p).await
                } else {
                    let res = self.chat(ctx, p).await;
                    Ok(Box::pin(futures::stream::once(async move { res })))
                }
            }
            RenderedPrompt::Completion(p) => {
                if self.supports_streaming() {
                    self.stream_completion(ctx, p).await
                } else {
                    let res = self.completion(ctx, p).await;
                    Ok(Box::pin(futures::stream::once(async move { res })))
                }
            }
        }
    }
}

/// We assume b64 with mime-type is the universally accepted format in an API
/// request. Other formats will be converted into that, depending on what
/// formats are allowed according to supported_media_formats.
async fn process_media_urls(
    resolve_audio_urls: ResolveMediaUrls,
    resolve_image_urls: ResolveMediaUrls,
    resolve_pdf_urls: ResolveMediaUrls,
    resolve_video_urls: ResolveMediaUrls,
    resolve_files: bool,
    render_settings: Option<RenderCurlSettings>,
    ctx: &RuntimeContext,
    chat: &[RenderedChatMessage],
) -> Result<Vec<RenderedChatMessage>, anyhow::Error> {
    let render_settings = render_settings.unwrap_or(RenderCurlSettings {
        stream: false,
        as_shell_commands: false,
        expose_secrets: false,
    });

    futures::stream::iter(chat.iter().map(|p| {
        let new_parts = p
            .parts
            .iter()
            .map(|any_part| async move {
                let Some(part) = any_part.as_media() else {
                    return Ok::<ChatMessagePart, anyhow::Error>(any_part.clone());
                };
                let resolve_mode = match part.media_type {
                    BamlMediaType::Audio => resolve_audio_urls,
                    BamlMediaType::Image => resolve_image_urls,
                    BamlMediaType::Pdf => resolve_pdf_urls,
                    BamlMediaType::Video => resolve_video_urls,
                };
                let media = process_media(resolve_mode, resolve_files, render_settings, ctx, part)
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
    .collect::<Result<Vec<_>, _>>()
}

async fn process_media(
    resolve_mode: ResolveMediaUrls,
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
                .context(format!("Failed to read file {media_path:#}"))?;

            let mut mime_type = part.mime_type.clone();

            if mime_type.is_none() {
                if let Some(ext) = media_file.extension() {
                    mime_type = Some(format!("{}/{}", part.media_type, ext));
                }
            }

            if mime_type.is_none() {
                if let Some(t) = infer::get(&bytes) {
                    mime_type = Some(t.mime_type().to_string());
                }
            }

            // ENFORCEMENT: For PDF, the mime type must be application/pdf
            if part.media_type == BamlMediaType::Pdf {
                match &mime_type {
                    Some(mt) if mt != "application/pdf" => {
                        anyhow::bail!(
                            "File provided for PDF input is not a PDF. Detected mime type: '{}'. Only application/pdf is allowed.",
                            mt
                        );
                    }
                    None => {
                        anyhow::bail!(
                            "Could not determine mime type for PDF input. Only application/pdf is allowed."
                        );
                    }
                    _ => {}
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
            //  - Vertex is ResolveMediaUrls::SendUrlAddMimeType and is the only one that supports URLs w/ mime-type
            //  - OpenAI is ResolveMediaUrls::SendUrl and allows passing in URLs with optionally specified mime-type (but it has different behavior for audio inputs)

            // NOTE(sam): if a provider accepts URLs but requires mime-type
            // (i.e. Vertex), we currently send it to them as b64. This
            // is how it was implemented originally, and while that could be
            // problematic in theory, I'm not going to change it until a
            // customer complains.
            match (resolve_mode, part.mime_type.as_deref()) {
                (ResolveMediaUrls::SendBase64, _) => {}
                (ResolveMediaUrls::SendUrlAddMimeType, Some(""))
                | (ResolveMediaUrls::SendUrlAddMimeType, None) => {}
                (ResolveMediaUrls::SendBase64UnlessGoogleUrl, _) => {
                    if media_url.url.starts_with("gs://") {
                        return Ok(part.clone());
                    }
                }
                (ResolveMediaUrls::SendUrl, _) | (ResolveMediaUrls::SendUrlAddMimeType, _) => {
                    return Ok(part.clone());
                }
            }

            let (base64, inferred_mime_type) =
                to_base64_with_inferred_mime_type(ctx, media_url).await?;

            // Validate MIME type – if the user has explicitly set one, or if the
            // media type implies a canonical MIME (e.g. PDFs), ensure the fetched
            // content matches what was requested.

            let expected_mime_type: Option<String> = if let Some(mt) = &part.mime_type {
                if !mt.is_empty() {
                    Some(mt.clone())
                } else {
                    None
                }
            } else {
                match part.media_type {
                    BamlMediaType::Pdf => Some("application/pdf".to_string()),
                    _ => None,
                }
            };

            if let Some(expected) = &expected_mime_type {
                // we accept subtype matches (e.g. image/jpeg starts_with image/)
                let mismatch = if expected.contains('/') {
                    &inferred_mime_type != expected
                } else {
                    !inferred_mime_type.starts_with(expected)
                };

                if mismatch {
                    anyhow::bail!(
                        "Requested media of MIME type '{}' but fetched '{}' from URL {}. Please ensure the URL points to the correct file or update the mime_type in BAML.",
                        expected,
                        inferred_mime_type,
                        media_url.url
                    );
                }
            }

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

            if mime_type.is_none() {
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
    if let Some((mime_type, base64)) = as_base64(media_url.url.as_str()) {
        return Ok((base64.to_string(), mime_type.to_string()));
    }
    let response = match fetch_with_proxy(&media_url.url, ctx.proxy_url()).await {
        Ok(response) => response,
        Err(e) => return Err(anyhow::anyhow!("Failed to fetch media: {e:?}")),
    };
    if response.status().is_success() {
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
    } else {
        Err(anyhow::anyhow!(
            "Failed to fetch media: {} {}, {}",
            response.status(),
            media_url.url,
            response.text().await.unwrap_or_default(),
        ))
    }
}

/// A naive implementation of the data URL parser, returning the (mime_type, base64)
/// if parsing succeeds. Specifically, we only support specifying a single mime-type (so
/// fields like 'charset' will be ignored) and only base64 data URLs.
///
/// See: https://fetch.spec.whatwg.org/#data-urls
fn as_base64(maybe_base64_url: &str) -> Option<(&str, &str)> {
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

    let request = if let Some(proxy) = proxy_url {
        let new_proxy_url = format!(
            "{}{}",
            proxy,
            url.parse::<url::Url>()
                .map_err(|e| anyhow::anyhow!("Failed to parse URL: {}", e))?
                .path()
        );
        client.get(new_proxy_url).header("baml-original-url", url)
    } else {
        client.get(url)
    };

    let response = request.send().await?;
    Ok(response)
}

#[cfg(test)]
mod tests_scrub {
    use std::collections::HashMap;

    use baml_types::BamlMap;
    use serde_json::json;

    use crate::redaction::scrub_baml_options;

    #[test]
    fn test_scrub_exact_match_and_bearer() {
        let mut opts: BamlMap<String, serde_json::Value> = BamlMap::new();
        opts.insert("api_key".to_string(), json!("secret-xyz"));
        opts.insert(
            "headers".to_string(),
            json!({
                "x-api-key": "sek-parallel",
                "authorization": "Bearer sek-openai",
                "other": "ok"
            }),
        );
        opts.insert("model".to_string(), json!("gpt-5-2025-08-07"));

        let mut envs = HashMap::new();
        envs.insert("OPENAI_API_KEY".to_string(), "sek-openai".to_string());
        envs.insert("PARALLEL_API_KEY".to_string(), "sek-parallel".to_string());
        envs.insert("SOME_OTHER".to_string(), "secret-xyz".to_string());

        let scrubbed = scrub_baml_options(&opts, &envs, false);

        // api_key matches SOME_OTHER
        assert_eq!(
            scrubbed.get("api_key").and_then(|v| v.as_str()),
            Some("$SOME_OTHER")
        );

        // headers.x-api-key matches PARALLEL_API_KEY
        assert_eq!(
            scrubbed
                .get("headers")
                .and_then(|h| h.get("x-api-key"))
                .and_then(|v| v.as_str()),
            Some("$PARALLEL_API_KEY")
        );

        // headers.authorization matches OPENAI_API_KEY with Bearer prefix
        assert_eq!(
            scrubbed
                .get("headers")
                .and_then(|h| h.get("authorization"))
                .and_then(|v| v.as_str()),
            Some("Bearer $OPENAI_API_KEY")
        );

        // Non-sensitive key remains unchanged
        assert_eq!(
            scrubbed.get("model").and_then(|v| v.as_str()),
            Some("gpt-5-2025-08-07")
        );

        // headers.other remains unchanged
        assert_eq!(
            scrubbed
                .get("headers")
                .and_then(|h| h.get("other"))
                .and_then(|v| v.as_str()),
            Some("ok")
        );
    }

    #[test]
    fn test_scrub_sensitive_no_match() {
        let mut opts: BamlMap<String, serde_json::Value> = BamlMap::new();
        opts.insert("api_key".to_string(), json!("plain-secret"));
        let envs: HashMap<String, String> = HashMap::new();

        let scrubbed = scrub_baml_options(&opts, &envs, false);
        assert_eq!(
            scrubbed.get("api_key").and_then(|v| v.as_str()),
            Some("$REDACTED")
        );
    }
}

#[cfg(test)]
mod tests_merge_messages {
    use internal_baml_jinja::{ChatMessagePart, RenderedChatMessage};

    use super::merge_messages;

    #[test]
    fn test_merge_messages_empty() {
        // This would trigger the panic: empty vector causes underflow
        // chat.len() - 1 = usize::MAX, making the loop condition always true
        let chat: Vec<RenderedChatMessage> = vec![];
        let result = merge_messages(&chat);
        assert_eq!(result.len(), 0);
    }

    #[test]
    fn test_merge_messages_single() {
        let chat = vec![RenderedChatMessage {
            role: "user".to_string(),
            allow_duplicate_role: false,
            parts: vec![ChatMessagePart::Text("hello".to_string())],
        }];
        let result = merge_messages(&chat);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].parts.len(), 1);
    }

    #[test]
    fn test_merge_messages_merge_consecutive_same_role() {
        // This tests the merge logic with consecutive messages of the same role
        let chat = vec![
            RenderedChatMessage {
                role: "user".to_string(),
                allow_duplicate_role: false,
                parts: vec![ChatMessagePart::Text("hello".to_string())],
            },
            RenderedChatMessage {
                role: "user".to_string(),
                allow_duplicate_role: false,
                parts: vec![ChatMessagePart::Text("world".to_string())],
            },
        ];
        let result = merge_messages(&chat);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].parts.len(), 2);
    }
}
