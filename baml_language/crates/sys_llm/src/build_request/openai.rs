//! OpenAI-format HTTP request builder.
//!
//! Supports: `OpenAi`, `OpenAiGeneric`, `AzureOpenAi`, Ollama, `OpenRouter`,
//! and `OpenAiResponses` (Responses API).

use std::fmt::Write;

use baml_builtins::{PromptAst, PromptAstSimple};
use indexmap::IndexMap;
use serde::Serialize;

use super::{BuildRequestError, LlmPrimitiveClient, LlmRequestBuilder, get_string_option};
use crate::LlmProvider;

/// A single chat message in the `OpenAI` Chat Completions format.
#[derive(Serialize)]
struct ChatMessage {
    role: String,
    content: Vec<ContentPart>,
}

/// A content part within a Chat Completions message.
///
/// Serializes with `{"type": "<variant>", ...}` via `#[serde(tag = "type")]`.
#[derive(Debug, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum ContentPart {
    Text { text: String },
    ImageUrl { image_url: ImageUrl },
    InputAudio { input_audio: InputAudio },
    File { file: FileRef },
}

/// URL wrapper for image content parts.
#[derive(Debug, Serialize)]
struct ImageUrl {
    url: String,
}

/// Base64-encoded audio data with its format (e.g. "mp3", "wav").
#[derive(Debug, Serialize)]
struct InputAudio {
    data: String,
    format: String,
}

/// A file reference that can be specified by URL, inline base64 data, or file ID.
#[derive(Debug, Serialize)]
struct FileRef {
    #[serde(skip_serializing_if = "Option::is_none")]
    file_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    file_data: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    file_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    filename: Option<String>,
}

/// Builder for OpenAI-compatible providers.
pub(crate) struct OpenAiBuilder<'a> {
    provider: &'a LlmProvider,
}

impl<'a> OpenAiBuilder<'a> {
    pub(crate) fn new(provider: &'a LlmProvider) -> Self {
        Self { provider }
    }
}

impl LlmRequestBuilder for OpenAiBuilder<'_> {
    fn provider_skip_keys(&self) -> &'static [&'static str] {
        &["resource_name", "api_version"]
    }

    fn build_url(&self, client: &LlmPrimitiveClient) -> Result<String, BuildRequestError> {
        let base_url = get_string_option(client, "base_url")
            .unwrap_or_else(|| "https://api.openai.com/v1".to_string());
        let base_url = base_url.trim_end_matches('/');

        // Azure uses a different URL pattern
        if *self.provider == LlmProvider::AzureOpenAi {
            let deployment = get_string_option(client, "resource_name")
                .ok_or_else(|| BuildRequestError::MissingOption("resource_name".into()))?;
            let model = get_string_option(client, "model")
                .ok_or_else(|| BuildRequestError::MissingOption("model".into()))?;
            let mut url = format!(
                "https://{deployment}.openai.azure.com/openai/deployments/{model}/chat/completions"
            );
            if let Some(api_version) = get_string_option(client, "api_version") {
                write!(url, "?api-version={api_version}").unwrap();
            }
            return Ok(url);
        }

        Ok(format!("{base_url}/chat/completions"))
    }

    fn build_auth_headers(&self, client: &LlmPrimitiveClient) -> IndexMap<String, String> {
        let mut headers = IndexMap::new();
        if let Some(api_key) = get_string_option(client, "api_key") {
            if *self.provider == LlmProvider::AzureOpenAi {
                headers.insert("api-key".to_string(), api_key);
            } else {
                headers.insert("authorization".to_string(), format!("Bearer {api_key}"));
            }
        }
        headers
    }

    fn build_body(
        &self,
        client: &LlmPrimitiveClient,
        prompt: bex_vm_types::PromptAst,
    ) -> Result<String, BuildRequestError> {
        let mut body = serde_json::Map::new();
        if let Some(model) = get_string_option(client, "model") {
            body.insert("model".to_string(), serde_json::Value::String(model));
        }
        body.extend(self.build_prompt_body(client, prompt)?);
        self.forward_options(client, &mut body);

        // Azure OpenAI: default max_tokens to 4096 if neither max_tokens nor
        // max_completion_tokens is set.
        if *self.provider == LlmProvider::AzureOpenAi {
            if !body.contains_key("max_completion_tokens") && !body.contains_key("max_tokens") {
                body.insert("max_tokens".to_string(), serde_json::json!(4096));
            } else if body
                .get("max_tokens")
                .is_some_and(serde_json::Value::is_null)
            {
                body.remove("max_tokens");
            }
        }

        serde_json::to_string(&body).map_err(|e| BuildRequestError::InvalidOption {
            key: "body".into(),
            reason: e.to_string(),
        })
    }

    fn build_prompt_body(
        &self,
        client: &LlmPrimitiveClient,
        prompt: bex_vm_types::PromptAst,
    ) -> Result<serde_json::Map<String, serde_json::Value>, BuildRequestError> {
        let mut map = serde_json::Map::new();
        let messages = prompt_to_openai_messages(&prompt, &client.default_role)?;
        map.insert(
            "messages".to_string(),
            serde_json::to_value(messages).expect("infallible"),
        );
        Ok(map)
    }
}

/// Converts a top-level [`PromptAst`] into a list of `OpenAI` Chat Completions messages.
fn prompt_to_openai_messages(
    prompt: &bex_vm_types::PromptAst,
    default_role: &str,
) -> Result<Vec<serde_json::Value>, BuildRequestError> {
    match prompt.as_ref() {
        PromptAst::Vec(items) => items
            .iter()
            .map(|node| prompt_node_to_message(node, default_role))
            .collect(),
        _ => Ok(vec![prompt_node_to_message(prompt, default_role)?]),
    }
}

/// Converts a single [`PromptAst`] node into an `OpenAI` Chat Completions message JSON value.
///
/// Metadata (e.g. `cache_control`) is merged into the last content part.
fn prompt_node_to_message(
    node: &bex_vm_types::PromptAst,
    default_role: &str,
) -> Result<serde_json::Value, BuildRequestError> {
    match node.as_ref() {
        PromptAst::Message {
            role,
            content,
            metadata,
        } => {
            let parts = openai_content_parts(content.as_ref())?;
            let mut value = serde_json::to_value(ChatMessage {
                role: role.clone(),
                content: parts,
            })
            .expect("infallible");

            // Apply metadata (e.g., cache_control) to the last content part,
            // matching the engine's WithMeta behavior.
            if let serde_json::Value::Object(meta_map) = metadata {
                if !meta_map.is_empty() {
                    if let Some(content_arr) =
                        value.get_mut("content").and_then(|c| c.as_array_mut())
                    {
                        if let Some(serde_json::Value::Object(last_part)) = content_arr.last_mut() {
                            for (k, v) in meta_map {
                                last_part.insert(k.clone(), v.clone());
                            }
                        }
                    }
                }
            }

            Ok(value)
        }
        PromptAst::Simple(content) => {
            let parts = openai_content_parts(content.as_ref())?;
            Ok(serde_json::to_value(ChatMessage {
                role: default_role.to_string(),
                content: parts,
            })
            .expect("infallible"))
        }
        PromptAst::Vec(_) => unreachable!("Nested vecs should not appear after specialization"),
    }
}

/// Converts a [`PromptAstSimple`] content node into Chat Completions content parts.
fn openai_content_parts(content: &PromptAstSimple) -> Result<Vec<ContentPart>, BuildRequestError> {
    match content {
        PromptAstSimple::String(s) => Ok(vec![ContentPart::Text { text: s.clone() }]),
        PromptAstSimple::Media(media) => media.read_content(|c| openai_media_part(media, c)),
        PromptAstSimple::Multiple(multiple) => {
            let mut parts = Vec::new();
            for item in multiple {
                parts.extend(openai_content_parts(item)?);
            }
            Ok(parts)
        }
    }
}

/// Converts a media value into Chat Completions content parts (image, audio, file, etc.).
fn openai_media_part(
    media: &baml_builtins::MediaValue,
    content: &baml_builtins::MediaContent,
) -> Result<Vec<ContentPart>, BuildRequestError> {
    use baml_base::MediaKind;
    use baml_builtins::MediaContent;

    match media.kind {
        MediaKind::Image | MediaKind::Generic => match content {
            MediaContent::Url { url, .. } => Ok(vec![ContentPart::ImageUrl {
                image_url: ImageUrl { url: url.clone() },
            }]),
            MediaContent::Base64 { base64_data, .. } => {
                let data_url = format!(
                    "data:{};base64,{}",
                    media.mime_type.as_deref().unwrap_or("image/png"),
                    base64_data
                );
                Ok(vec![ContentPart::ImageUrl {
                    image_url: ImageUrl { url: data_url },
                }])
            }
            MediaContent::File { .. } => {
                unreachable!("image file should have been resolved before request building")
            }
        },
        MediaKind::Audio => match content {
            MediaContent::Base64 { base64_data, .. } => {
                let format = media
                    .mime_type
                    .as_deref()
                    .and_then(|m| m.strip_prefix("audio/"))
                    .map(|ext| if ext == "mpeg" { "mp3" } else { ext })
                    .unwrap_or("mp3");
                Ok(vec![ContentPart::InputAudio {
                    input_audio: InputAudio {
                        data: base64_data.clone(),
                        format: format.to_string(),
                    },
                }])
            }
            MediaContent::Url { url, .. } => {
                // Prefer mime_type, fall back to URL extension
                let format = media
                    .mime_type
                    .as_deref()
                    .and_then(|m| m.strip_prefix("audio/"))
                    .or_else(|| url.rsplit('.').next())
                    .map(|ext| if ext == "mpeg" { "mp3" } else { ext })
                    .unwrap_or("mp3");
                Ok(vec![ContentPart::InputAudio {
                    input_audio: InputAudio {
                        data: url.clone(),
                        format: format.to_string(),
                    },
                }])
            }
            MediaContent::File { .. } => {
                unreachable!("audio file should have been resolved before request building")
            }
        },
        MediaKind::Pdf => match content {
            MediaContent::Url { url, .. } => Ok(vec![ContentPart::File {
                file: FileRef {
                    file_url: Some(url.clone()),
                    filename: Some("document.pdf".to_string()),
                    file_data: None,
                    file_id: None,
                },
            }]),
            MediaContent::Base64 { base64_data, .. } => {
                let data_url = format!(
                    "data:{};base64,{}",
                    media.mime_type.as_deref().unwrap_or("application/pdf"),
                    base64_data
                );
                Ok(vec![ContentPart::File {
                    file: FileRef {
                        file_data: Some(data_url),
                        filename: Some("document.pdf".to_string()),
                        file_url: None,
                        file_id: None,
                    },
                }])
            }
            MediaContent::File { .. } => {
                unreachable!("PDF file should have been resolved before request building")
            }
        },
        MediaKind::Video => Err(BuildRequestError::UnsupportedMedia(
            "video input is not supported on OpenAI chat completions".into(),
        )),
    }
}

/// A single message in the `OpenAI` Responses API format.
#[derive(Debug, Serialize)]
struct ResponsesMessage {
    role: String,
    content: Vec<ResponsesContentPart>,
}

/// A content part within a Responses API message.
///
/// Uses `input_text`/`output_text` instead of just `text`, and `input_image`/`input_audio`/`input_file`
/// instead of `image_url`/`input_audio`/`file`.
#[derive(Debug, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum ResponsesContentPart {
    InputText {
        text: String,
    },
    OutputText {
        text: String,
    },
    InputImage {
        #[serde(skip_serializing_if = "Option::is_none")]
        detail: Option<String>,
        image_url: String,
    },
    InputAudio {
        input_audio: InputAudio,
    },
    InputFile {
        #[serde(skip_serializing_if = "Option::is_none")]
        file_url: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        file_data: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        file_id: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        filename: Option<String>,
    },
}

/// Builder for the `OpenAI` Responses API (`/v1/responses`).
///
/// Uses `"input"` instead of `"messages"` and different content part types:
/// `input_text`, `output_text`, `input_image`, `input_audio`, `input_file`.
pub(crate) struct OpenAiResponsesBuilder;

impl LlmRequestBuilder for OpenAiResponsesBuilder {
    fn provider_skip_keys(&self) -> &'static [&'static str] {
        &[]
    }

    fn build_url(&self, client: &LlmPrimitiveClient) -> Result<String, BuildRequestError> {
        let base_url = get_string_option(client, "base_url")
            .unwrap_or_else(|| "https://api.openai.com/v1".to_string());
        let base_url = base_url.trim_end_matches('/');
        Ok(format!("{base_url}/responses"))
    }

    fn build_auth_headers(&self, client: &LlmPrimitiveClient) -> IndexMap<String, String> {
        let mut headers = IndexMap::new();
        if let Some(api_key) = get_string_option(client, "api_key") {
            headers.insert("authorization".to_string(), format!("Bearer {api_key}"));
        }
        headers
    }

    fn build_prompt_body(
        &self,
        client: &LlmPrimitiveClient,
        prompt: bex_vm_types::PromptAst,
    ) -> Result<serde_json::Map<String, serde_json::Value>, BuildRequestError> {
        let mut map = serde_json::Map::new();
        let input = prompt_to_responses_input(&prompt, &client.default_role)?;
        map.insert(
            "input".to_string(),
            serde_json::to_value(input).expect("infallible"),
        );
        Ok(map)
    }
}

/// Converts a top-level [`PromptAst`] into a list of Responses API input messages.
fn prompt_to_responses_input(
    prompt: &bex_vm_types::PromptAst,
    default_role: &str,
) -> Result<Vec<ResponsesMessage>, BuildRequestError> {
    match prompt.as_ref() {
        PromptAst::Vec(items) => items
            .iter()
            .map(|node| responses_node_to_message(node, default_role))
            .collect(),
        _ => Ok(vec![responses_node_to_message(prompt, default_role)?]),
    }
}

/// Converts a single [`PromptAst`] node into a Responses API input message.
fn responses_node_to_message(
    node: &bex_vm_types::PromptAst,
    default_role: &str,
) -> Result<ResponsesMessage, BuildRequestError> {
    match node.as_ref() {
        PromptAst::Message { role, content, .. } => {
            let parts = responses_content_parts(content.as_ref(), role)?;
            Ok(ResponsesMessage {
                role: role.clone(),
                content: parts,
            })
        }
        PromptAst::Simple(content) => {
            let parts = responses_content_parts(content.as_ref(), default_role)?;
            Ok(ResponsesMessage {
                role: default_role.to_string(),
                content: parts,
            })
        }
        PromptAst::Vec(_) => unreachable!("Nested vecs should not appear after specialization"),
    }
}

/// Converts a [`PromptAstSimple`] content node into Responses API content parts.
///
/// Assistant-role text uses `output_text`; all other roles use `input_text`.
fn responses_content_parts(
    content: &PromptAstSimple,
    role: &str,
) -> Result<Vec<ResponsesContentPart>, BuildRequestError> {
    match content {
        PromptAstSimple::String(s) => {
            if role == "assistant" {
                Ok(vec![ResponsesContentPart::OutputText { text: s.clone() }])
            } else {
                Ok(vec![ResponsesContentPart::InputText { text: s.clone() }])
            }
        }
        PromptAstSimple::Media(media) => media.read_content(|c| responses_media_part(media, c)),
        PromptAstSimple::Multiple(multiple) => {
            let mut parts = Vec::new();
            for item in multiple {
                parts.extend(responses_content_parts(item, role)?);
            }
            Ok(parts)
        }
    }
}

/// Converts a media value into Responses API content parts (`input_image`, `input_audio`, `input_file`).
fn responses_media_part(
    media: &baml_builtins::MediaValue,
    content: &baml_builtins::MediaContent,
) -> Result<Vec<ResponsesContentPart>, BuildRequestError> {
    use baml_base::MediaKind;
    use baml_builtins::MediaContent;

    match media.kind {
        MediaKind::Image | MediaKind::Generic => {
            let image_url = match content {
                MediaContent::Url { url, .. } => url.clone(),
                MediaContent::Base64 { base64_data, .. } => {
                    format!(
                        "data:{};base64,{}",
                        media.mime_type.as_deref().unwrap_or("image/png"),
                        base64_data
                    )
                }
                MediaContent::File { .. } => {
                    unreachable!("image file should have been resolved before request building")
                }
            };
            Ok(vec![ResponsesContentPart::InputImage {
                detail: Some("auto".to_string()),
                image_url,
            }])
        }
        MediaKind::Audio => match content {
            MediaContent::Base64 { base64_data, .. } => {
                let format = media
                    .mime_type
                    .as_deref()
                    .and_then(|m| m.strip_prefix("audio/"))
                    .map(|ext| if ext == "mpeg" { "mp3" } else { ext })
                    .unwrap_or("mp3");
                Ok(vec![ResponsesContentPart::InputAudio {
                    input_audio: InputAudio {
                        data: base64_data.clone(),
                        format: format.to_string(),
                    },
                }])
            }
            MediaContent::Url { .. } => Err(BuildRequestError::UnsupportedMedia(
                "audio URL is not supported on OpenAI Responses API; use base64-encoded audio instead".into(),
            )),
            MediaContent::File { .. } => {
                unreachable!("audio file should have been resolved before request building")
            }
        },
        MediaKind::Pdf => match content {
            MediaContent::Url { url, .. } => Ok(vec![ResponsesContentPart::InputFile {
                file_url: Some(url.clone()),
                filename: Some("document.pdf".to_string()),
                file_data: None,
                file_id: None,
            }]),
            MediaContent::Base64 { base64_data, .. } => {
                let data_url = format!(
                    "data:{};base64,{}",
                    media.mime_type.as_deref().unwrap_or("application/pdf"),
                    base64_data
                );
                Ok(vec![ResponsesContentPart::InputFile {
                    file_data: Some(data_url),
                    filename: Some("document.pdf".to_string()),
                    file_url: None,
                    file_id: None,
                }])
            }
            MediaContent::File { .. } => {
                unreachable!("PDF file should have been resolved before request building")
            }
        },
        MediaKind::Video => Err(BuildRequestError::UnsupportedMedia(
            "video input is not supported on OpenAI Responses API".into(),
        )),
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use baml_base::MediaKind;
    use baml_builtins::{MediaContent, MediaValue, PromptAst};
    use bex_external_types::BexExternalValue;
    use indexmap::IndexMap;

    use super::*;
    use crate::build_request::{LlmPrimitiveClient, LlmRequestBuilder, build_request};

    // -- helpers --

    fn make_media(kind: MediaKind, content: MediaContent, mime: Option<&str>) -> MediaValue {
        MediaValue::new(kind, content, mime.map(String::from))
    }

    fn make_client(provider: &str, options: Vec<(&str, BexExternalValue)>) -> LlmPrimitiveClient {
        let mut opts = IndexMap::new();
        for (k, v) in options {
            opts.insert(k.to_string(), v);
        }
        LlmPrimitiveClient {
            name: "test".to_string(),
            provider: provider.to_string(),
            default_role: "user".to_string(),
            allowed_roles: vec!["system".into(), "user".into(), "assistant".into()],
            options: opts,
        }
    }

    fn msg(role: &str, text: &str) -> Arc<PromptAst> {
        Arc::new(PromptAst::Message {
            role: role.to_string(),
            content: Arc::new(text.to_string().into()),
            metadata: serde_json::Value::Null,
        })
    }

    fn parse_body(body: &str) -> serde_json::Value {
        serde_json::from_str(body).unwrap()
    }

    // ========================================================================
    // Chat Completions: media content parts
    // ========================================================================

    #[test]
    fn chat_image_url() {
        let media = make_media(
            MediaKind::Image,
            MediaContent::Url {
                url: "https://example.com/cat.png".into(),
                base64_data: None,
            },
            Some("image/png"),
        );
        let parts = media
            .read_content(|c| openai_media_part(&media, c))
            .unwrap();
        let json = serde_json::to_value(&parts[0]).unwrap();
        assert_eq!(
            json,
            serde_json::json!({"type": "image_url", "image_url": {"url": "https://example.com/cat.png"}})
        );
    }

    #[test]
    fn chat_image_base64() {
        let media = make_media(
            MediaKind::Image,
            MediaContent::Base64 {
                base64_data: "iVBORw0KGgo=".into(),
            },
            Some("image/png"),
        );
        let parts = media
            .read_content(|c| openai_media_part(&media, c))
            .unwrap();
        let json = serde_json::to_value(&parts[0]).unwrap();
        assert_eq!(
            json,
            serde_json::json!({"type": "image_url", "image_url": {"url": "data:image/png;base64,iVBORw0KGgo="}})
        );
    }

    #[test]
    fn chat_audio_base64_wav() {
        let media = make_media(
            MediaKind::Audio,
            MediaContent::Base64 {
                base64_data: "AAAA".into(),
            },
            Some("audio/wav"),
        );
        let parts = media
            .read_content(|c| openai_media_part(&media, c))
            .unwrap();
        let json = serde_json::to_value(&parts[0]).unwrap();
        assert_eq!(
            json,
            serde_json::json!({"type": "input_audio", "input_audio": {"data": "AAAA", "format": "wav"}})
        );
    }

    #[test]
    fn chat_audio_mpeg_becomes_mp3() {
        let media = make_media(
            MediaKind::Audio,
            MediaContent::Base64 {
                base64_data: "AAAA".into(),
            },
            Some("audio/mpeg"),
        );
        let parts = media
            .read_content(|c| openai_media_part(&media, c))
            .unwrap();
        let json = serde_json::to_value(&parts[0]).unwrap();
        assert_eq!(json["input_audio"]["format"], "mp3");
    }

    #[test]
    fn chat_audio_url() {
        let media = make_media(
            MediaKind::Audio,
            MediaContent::Url {
                url: "https://example.com/speech.wav".into(),
                base64_data: None,
            },
            Some("audio/wav"),
        );
        let parts = media
            .read_content(|c| openai_media_part(&media, c))
            .unwrap();
        let json = serde_json::to_value(&parts[0]).unwrap();
        assert_eq!(
            json,
            serde_json::json!({"type": "input_audio", "input_audio": {"data": "https://example.com/speech.wav", "format": "wav"}})
        );
    }

    #[test]
    fn chat_pdf_url() {
        let media = make_media(
            MediaKind::Pdf,
            MediaContent::Url {
                url: "https://example.com/doc.pdf".into(),
                base64_data: None,
            },
            Some("application/pdf"),
        );
        let parts = media
            .read_content(|c| openai_media_part(&media, c))
            .unwrap();
        let json = serde_json::to_value(&parts[0]).unwrap();
        assert_eq!(
            json,
            serde_json::json!({"type": "file", "file": {"file_url": "https://example.com/doc.pdf", "filename": "document.pdf"}})
        );
    }

    #[test]
    fn chat_pdf_base64() {
        let media = make_media(
            MediaKind::Pdf,
            MediaContent::Base64 {
                base64_data: "JVBERi0=".into(),
            },
            Some("application/pdf"),
        );
        let parts = media
            .read_content(|c| openai_media_part(&media, c))
            .unwrap();
        let json = serde_json::to_value(&parts[0]).unwrap();
        assert_eq!(
            json,
            serde_json::json!({"type": "file", "file": {"file_data": "data:application/pdf;base64,JVBERi0=", "filename": "document.pdf"}})
        );
    }

    #[test]
    fn chat_video_unsupported() {
        let media = make_media(
            MediaKind::Video,
            MediaContent::Url {
                url: "https://example.com/clip.mp4".into(),
                base64_data: None,
            },
            Some("video/mp4"),
        );
        let err = media
            .read_content(|c| openai_media_part(&media, c))
            .unwrap_err();
        assert!(err.to_string().contains("video"));
    }

    // ========================================================================
    // Chat Completions: message building
    // ========================================================================

    #[test]
    fn chat_single_message() {
        let prompt = msg("user", "hello");
        let messages = prompt_to_openai_messages(&prompt, "user").unwrap();
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0]["role"], "user");
        assert_eq!(messages[0]["content"][0]["text"], "hello");
    }

    #[test]
    fn chat_multiple_messages() {
        let prompt = Arc::new(PromptAst::Vec(vec![
            msg("system", "Be helpful."),
            msg("user", "Hi"),
        ]));
        let messages = prompt_to_openai_messages(&prompt, "user").unwrap();
        assert_eq!(messages.len(), 2);
        assert_eq!(messages[0]["role"], "system");
        assert_eq!(messages[1]["role"], "user");
    }

    #[test]
    fn chat_three_role_conversation() {
        let prompt = Arc::new(PromptAst::Vec(vec![
            msg("system", "You are a helpful assistant."),
            msg("user", "What is 2+2?"),
            msg("assistant", "4"),
        ]));
        let messages = prompt_to_openai_messages(&prompt, "user").unwrap();
        assert_eq!(messages.len(), 3);
        assert_eq!(messages[0]["role"], "system");
        assert_eq!(
            messages[0]["content"][0]["text"],
            "You are a helpful assistant."
        );
        assert_eq!(messages[1]["role"], "user");
        assert_eq!(messages[1]["content"][0]["text"], "What is 2+2?");
        assert_eq!(messages[2]["role"], "assistant");
        assert_eq!(messages[2]["content"][0]["text"], "4");
    }

    #[test]
    fn chat_multi_turn_conversation() {
        let prompt = Arc::new(PromptAst::Vec(vec![
            msg("system", "Be concise."),
            msg("user", "Hello"),
            msg("assistant", "Hi!"),
            msg("user", "How are you?"),
            msg("assistant", "Good, thanks!"),
            msg("user", "Goodbye"),
        ]));
        let messages = prompt_to_openai_messages(&prompt, "user").unwrap();
        assert_eq!(messages.len(), 6);
        assert_eq!(messages[0]["role"], "system");
        assert_eq!(messages[1]["role"], "user");
        assert_eq!(messages[2]["role"], "assistant");
        assert_eq!(messages[3]["role"], "user");
        assert_eq!(messages[4]["role"], "assistant");
        assert_eq!(messages[5]["role"], "user");
        assert_eq!(messages[5]["content"][0]["text"], "Goodbye");
    }

    #[test]
    fn chat_simple_node_uses_default_role() {
        let prompt = Arc::new(PromptAst::Vec(vec![
            msg("system", "System prompt."),
            Arc::new(PromptAst::Simple(Arc::new("bare text".to_string().into()))),
            msg("user", "User msg."),
        ]));
        let messages = prompt_to_openai_messages(&prompt, "user").unwrap();
        assert_eq!(messages.len(), 3);
        assert_eq!(messages[0]["role"], "system");
        assert_eq!(messages[1]["role"], "user");
        assert_eq!(messages[1]["content"][0]["text"], "bare text");
        assert_eq!(messages[2]["role"], "user");
    }

    #[test]
    fn chat_metadata_merged_to_last_part() {
        let prompt = Arc::new(PromptAst::Message {
            role: "user".to_string(),
            content: Arc::new("hello".to_string().into()),
            metadata: serde_json::json!({"cache_control": {"type": "ephemeral"}}),
        });
        let messages = prompt_to_openai_messages(&prompt, "user").unwrap();
        assert_eq!(
            messages[0]["content"][0]["cache_control"],
            serde_json::json!({"type": "ephemeral"})
        );
    }

    // ========================================================================
    // Chat Completions: Azure URL + max_tokens defaults
    // ========================================================================

    #[test]
    fn azure_url_pattern() {
        let client = make_client(
            "azure-openai",
            vec![
                ("model", BexExternalValue::String("gpt-4o".into())),
                (
                    "resource_name",
                    BexExternalValue::String("my-resource".into()),
                ),
                ("api_key", BexExternalValue::String("sk-test".into())),
            ],
        );
        let builder = OpenAiBuilder::new(&LlmProvider::AzureOpenAi);
        let url = builder.build_url(&client).unwrap();
        assert_eq!(
            url,
            "https://my-resource.openai.azure.com/openai/deployments/gpt-4o/chat/completions"
        );
    }

    #[test]
    fn azure_url_with_api_version() {
        let client = make_client(
            "azure-openai",
            vec![
                ("model", BexExternalValue::String("gpt-4o".into())),
                (
                    "resource_name",
                    BexExternalValue::String("my-resource".into()),
                ),
                (
                    "api_version",
                    BexExternalValue::String("2024-02-15-preview".into()),
                ),
            ],
        );
        let builder = OpenAiBuilder::new(&LlmProvider::AzureOpenAi);
        let url = builder.build_url(&client).unwrap();
        assert_eq!(
            url,
            "https://my-resource.openai.azure.com/openai/deployments/gpt-4o/chat/completions?api-version=2024-02-15-preview"
        );
    }

    #[test]
    fn azure_auth_header_uses_api_key() {
        let client = make_client(
            "azure-openai",
            vec![("api_key", BexExternalValue::String("sk-azure".into()))],
        );
        let builder = OpenAiBuilder::new(&LlmProvider::AzureOpenAi);
        let headers = builder.build_auth_headers(&client);
        assert_eq!(headers.get("api-key").unwrap(), "sk-azure");
        assert!(headers.get("authorization").is_none());
    }

    #[test]
    fn azure_defaults_max_tokens_4096() {
        let client = make_client(
            "azure-openai",
            vec![
                ("model", BexExternalValue::String("gpt-4o".into())),
                ("resource_name", BexExternalValue::String("res".into())),
                ("api_key", BexExternalValue::String("sk".into())),
            ],
        );
        let result = build_request(&client, msg("user", "hi")).unwrap();
        let body = parse_body(&result.body);
        assert_eq!(body["max_tokens"], 4096);
    }

    #[test]
    fn azure_no_default_when_max_tokens_set() {
        let client = make_client(
            "azure-openai",
            vec![
                ("model", BexExternalValue::String("gpt-4o".into())),
                ("resource_name", BexExternalValue::String("res".into())),
                ("max_tokens", BexExternalValue::Int(1000)),
            ],
        );
        let result = build_request(&client, msg("user", "hi")).unwrap();
        let body = parse_body(&result.body);
        assert_eq!(body["max_tokens"], 1000);
    }

    #[test]
    fn azure_no_default_when_max_completion_tokens_set() {
        let client = make_client(
            "azure-openai",
            vec![
                ("model", BexExternalValue::String("gpt-4o".into())),
                ("resource_name", BexExternalValue::String("res".into())),
                ("max_completion_tokens", BexExternalValue::Int(2000)),
            ],
        );
        let result = build_request(&client, msg("user", "hi")).unwrap();
        let body = parse_body(&result.body);
        assert!(body.get("max_tokens").is_none());
        assert_eq!(body["max_completion_tokens"], 2000);
    }

    #[test]
    fn openai_no_default_max_tokens() {
        let client = make_client(
            "openai",
            vec![("model", BexExternalValue::String("gpt-4o".into()))],
        );
        let result = build_request(&client, msg("user", "hi")).unwrap();
        let body = parse_body(&result.body);
        assert!(body.get("max_tokens").is_none());
    }

    #[test]
    fn openai_no_model_when_absent() {
        let client = make_client("openai", vec![]);
        let result = build_request(&client, msg("user", "hi")).unwrap();
        let body = parse_body(&result.body);
        assert!(body.get("model").is_none());
    }

    // ========================================================================
    // Responses API: media content parts
    // ========================================================================

    #[test]
    fn responses_image_url() {
        let media = make_media(
            MediaKind::Image,
            MediaContent::Url {
                url: "https://example.com/cat.png".into(),
                base64_data: None,
            },
            Some("image/png"),
        );
        let parts = media
            .read_content(|c| responses_media_part(&media, c))
            .unwrap();
        let json = serde_json::to_value(&parts[0]).unwrap();
        assert_eq!(
            json,
            serde_json::json!({"type": "input_image", "detail": "auto", "image_url": "https://example.com/cat.png"})
        );
    }

    #[test]
    fn responses_image_base64() {
        let media = make_media(
            MediaKind::Image,
            MediaContent::Base64 {
                base64_data: "iVBORw0KGgo=".into(),
            },
            Some("image/png"),
        );
        let parts = media
            .read_content(|c| responses_media_part(&media, c))
            .unwrap();
        let json = serde_json::to_value(&parts[0]).unwrap();
        assert_eq!(json["type"], "input_image");
        assert_eq!(json["image_url"], "data:image/png;base64,iVBORw0KGgo=");
    }

    #[test]
    fn responses_audio_base64() {
        let media = make_media(
            MediaKind::Audio,
            MediaContent::Base64 {
                base64_data: "AAAA".into(),
            },
            Some("audio/wav"),
        );
        let parts = media
            .read_content(|c| responses_media_part(&media, c))
            .unwrap();
        let json = serde_json::to_value(&parts[0]).unwrap();
        assert_eq!(
            json,
            serde_json::json!({"type": "input_audio", "input_audio": {"data": "AAAA", "format": "wav"}})
        );
    }

    #[test]
    fn responses_pdf_url() {
        let media = make_media(
            MediaKind::Pdf,
            MediaContent::Url {
                url: "https://example.com/doc.pdf".into(),
                base64_data: None,
            },
            Some("application/pdf"),
        );
        let parts = media
            .read_content(|c| responses_media_part(&media, c))
            .unwrap();
        let json = serde_json::to_value(&parts[0]).unwrap();
        assert_eq!(
            json,
            serde_json::json!({"type": "input_file", "file_url": "https://example.com/doc.pdf", "filename": "document.pdf"})
        );
    }

    #[test]
    fn responses_pdf_base64() {
        let media = make_media(
            MediaKind::Pdf,
            MediaContent::Base64 {
                base64_data: "JVBERi0=".into(),
            },
            Some("application/pdf"),
        );
        let parts = media
            .read_content(|c| responses_media_part(&media, c))
            .unwrap();
        let json = serde_json::to_value(&parts[0]).unwrap();
        assert_eq!(json["type"], "input_file");
        assert_eq!(json["file_data"], "data:application/pdf;base64,JVBERi0=");
        assert_eq!(json["filename"], "document.pdf");
    }

    #[test]
    fn responses_video_unsupported() {
        let media = make_media(
            MediaKind::Video,
            MediaContent::Url {
                url: "https://example.com/clip.mp4".into(),
                base64_data: None,
            },
            Some("video/mp4"),
        );
        let err = media
            .read_content(|c| responses_media_part(&media, c))
            .unwrap_err();
        assert!(matches!(err, BuildRequestError::UnsupportedMedia(_)));
    }

    // ========================================================================
    // Responses API: message building
    // ========================================================================

    #[test]
    fn responses_assistant_uses_output_text() {
        let parts = responses_content_parts(
            &baml_builtins::PromptAstSimple::String("hi".into()),
            "assistant",
        )
        .unwrap();
        let json = serde_json::to_value(&parts[0]).unwrap();
        assert_eq!(json["type"], "output_text");
    }

    #[test]
    fn responses_user_uses_input_text() {
        let parts =
            responses_content_parts(&baml_builtins::PromptAstSimple::String("hi".into()), "user")
                .unwrap();
        let json = serde_json::to_value(&parts[0]).unwrap();
        assert_eq!(json["type"], "input_text");
    }

    #[test]
    fn responses_three_role_conversation() {
        let prompt = Arc::new(PromptAst::Vec(vec![
            msg("system", "You are a helpful assistant."),
            msg("user", "What is 2+2?"),
            msg("assistant", "4"),
        ]));
        let messages = prompt_to_responses_input(&prompt, "user").unwrap();
        assert_eq!(messages.len(), 3);
        assert_eq!(messages[0].role, "system");
        assert_eq!(messages[1].role, "user");
        assert_eq!(messages[2].role, "assistant");
        // assistant content should use output_text
        let json = serde_json::to_value(&messages[2].content[0]).unwrap();
        assert_eq!(json["type"], "output_text");
        assert_eq!(json["text"], "4");
        // system/user content should use input_text
        let json = serde_json::to_value(&messages[0].content[0]).unwrap();
        assert_eq!(json["type"], "input_text");
    }

    #[test]
    fn responses_multi_turn_conversation() {
        let prompt = Arc::new(PromptAst::Vec(vec![
            msg("system", "Be concise."),
            msg("user", "Hello"),
            msg("assistant", "Hi!"),
            msg("user", "How are you?"),
            msg("assistant", "Good, thanks!"),
            msg("user", "Goodbye"),
        ]));
        let messages = prompt_to_responses_input(&prompt, "user").unwrap();
        assert_eq!(messages.len(), 6);
        assert_eq!(messages[0].role, "system");
        assert_eq!(messages[1].role, "user");
        assert_eq!(messages[2].role, "assistant");
        assert_eq!(messages[3].role, "user");
        assert_eq!(messages[4].role, "assistant");
        assert_eq!(messages[5].role, "user");
        // Verify assistant messages use output_text, others use input_text
        let json2 = serde_json::to_value(&messages[2].content[0]).unwrap();
        assert_eq!(json2["type"], "output_text");
        let json4 = serde_json::to_value(&messages[4].content[0]).unwrap();
        assert_eq!(json4["type"], "output_text");
        let json5 = serde_json::to_value(&messages[5].content[0]).unwrap();
        assert_eq!(json5["type"], "input_text");
        assert_eq!(json5["text"], "Goodbye");
    }

    #[test]
    fn responses_url_default() {
        let client = make_client("openai-responses", vec![]);
        let url = OpenAiResponsesBuilder.build_url(&client).unwrap();
        assert_eq!(url, "https://api.openai.com/v1/responses");
    }

    #[test]
    fn responses_url_custom_base() {
        let client = make_client(
            "openai-responses",
            vec![(
                "base_url",
                BexExternalValue::String("https://custom.api.com/v1".into()),
            )],
        );
        let url = OpenAiResponsesBuilder.build_url(&client).unwrap();
        assert_eq!(url, "https://custom.api.com/v1/responses");
    }
}
