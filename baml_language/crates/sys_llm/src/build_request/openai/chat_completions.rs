//! `OpenAI` Chat Completions request builder.
//!
//! Supports: `OpenAi`, `OpenAiGeneric`, `AzureOpenAi`, Ollama, `OpenRouter`.

use baml_builtins::{PromptAst, PromptAstSimple};
use indexmap::IndexMap;
use serde::Serialize;

use crate::{
    LlmProvider,
    build_request::{
        BuildRequestError, LlmPrimitiveClient, LlmRequestBuilder, get_string_option,
        mime_type_as_ok, openai::build_openai_url,
    },
};

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
        build_openai_url(*self.provider, client, "/chat/completions")
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
        // max_completion_tokens is set. Holdover from engine, not sure why this was the case.
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

        // TODO: Handle default role in Ollama once compiler2 is merged.

        let messages = prompt_to_openai_messages(&prompt, client.default_role.as_str())?;
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
/// Should only be called by `prompt_to_openai_messages`, or something that otherwise ensures flattening of `PromptAst::Vec`.
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
        MediaKind::Image => match content {
            MediaContent::Url { url, .. } => Ok(vec![ContentPart::ImageUrl {
                image_url: ImageUrl { url: url.clone() },
            }]),
            MediaContent::Base64 { base64_data, .. }
            | MediaContent::File {
                base64_data: Some(base64_data),
                ..
            } => {
                let data_url = format!("data:{};base64,{}", mime_type_as_ok(media)?, base64_data);
                Ok(vec![ContentPart::ImageUrl {
                    image_url: ImageUrl { url: data_url },
                }])
            }
            MediaContent::File {
                base64_data: None, ..
            } => Err(BuildRequestError::FileNotResolved(
                "image file content was not resolved properly".into(),
            )),
        },
        MediaKind::Audio => match content {
            MediaContent::Base64 { base64_data, .. }
            | MediaContent::File {
                base64_data: Some(base64_data),
                ..
            }
            | MediaContent::Url {
                base64_data: Some(base64_data),
                ..
            } => {
                let mime = mime_type_as_ok(media)?;
                let format = mime.strip_prefix("audio/").unwrap_or(mime);
                let format = if format == "mpeg" { "mp3" } else { format };
                Ok(vec![ContentPart::InputAudio {
                    input_audio: InputAudio {
                        data: base64_data.clone(),
                        format: format.to_string(),
                    },
                }])
            }
            MediaContent::File {
                base64_data: None, ..
            } => Err(BuildRequestError::FileNotResolved(
                "audio file content was not resolved properly".into(),
            )),
            MediaContent::Url {
                base64_data: None, ..
            } => Err(BuildRequestError::UnsupportedMedia(
                "audio url content was not resolved properly".into(),
            )),
        },
        MediaKind::Pdf => match content {
            MediaContent::Base64 { base64_data, .. }
            | MediaContent::File {
                base64_data: Some(base64_data),
                ..
            }
            | MediaContent::Url {
                base64_data: Some(base64_data),
                ..
            } => {
                let data_url = format!("data:{};base64,{}", mime_type_as_ok(media)?, base64_data);
                Ok(vec![ContentPart::File {
                    file: FileRef {
                        file_data: Some(data_url),
                        filename: Some("document.pdf".to_string()),
                        file_id: None,
                    },
                }])
            }
            MediaContent::File {
                base64_data: None, ..
            } => Err(BuildRequestError::FileNotResolved(
                "pdf file content was not resolved properly".into(),
            )),
            MediaContent::Url {
                base64_data: None, ..
            } => Err(BuildRequestError::UnsupportedMedia(
                "pdf url content was not resolved properly".into(),
            )),
        },
        MediaKind::Video => Err(BuildRequestError::UnsupportedMedia(
            "video input is not supported on OpenAI chat completions".into(),
        )),
        MediaKind::Generic => Err(BuildRequestError::UnsupportedMedia(
            "generic media is currently unimplemented".into(), // TODO: Implement generic media support
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
        assert_eq!(
            json,
            serde_json::json!({"type": "input_audio", "input_audio": {"data": "AAAA", "format": "mp3"}})
        );
    }

    #[test]
    fn chat_audio_url_not_resolved_error() {
        let media = make_media(
            MediaKind::Audio,
            MediaContent::Url {
                url: "https://example.com/speech.wav".into(),
                base64_data: None,
            },
            Some("audio/wav"),
        );
        let err = media
            .read_content(|c| openai_media_part(&media, c))
            .unwrap_err();
        assert!(matches!(err, BuildRequestError::UnsupportedMedia(_)));
        assert!(
            err.to_string()
                .contains("audio url content was not resolved")
        );
    }

    #[test]
    fn chat_audio_url_with_base64_data() {
        let media = make_media(
            MediaKind::Audio,
            MediaContent::Url {
                url: "https://example.com/speech.wav".into(),
                base64_data: Some("AAAA".into()),
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
    fn chat_audio_file_with_base64_data() {
        let media = make_media(
            MediaKind::Audio,
            MediaContent::File {
                file: "speech.wav".into(),
                base64_data: Some("AAAA".into()),
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
    fn chat_audio_file_not_resolved_error() {
        let media = make_media(
            MediaKind::Audio,
            MediaContent::File {
                file: "speech.wav".into(),
                base64_data: None,
            },
            Some("audio/wav"),
        );
        let err = media
            .read_content(|c| openai_media_part(&media, c))
            .unwrap_err();
        assert!(matches!(err, BuildRequestError::FileNotResolved(_)));
    }

    #[test]
    fn chat_image_file_with_base64_data() {
        let media = make_media(
            MediaKind::Image,
            MediaContent::File {
                file: "cat.png".into(),
                base64_data: Some("iVBORw0KGgo=".into()),
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
    fn chat_image_file_not_resolved_error() {
        let media = make_media(
            MediaKind::Image,
            MediaContent::File {
                file: "cat.png".into(),
                base64_data: None,
            },
            Some("image/png"),
        );
        let err = media
            .read_content(|c| openai_media_part(&media, c))
            .unwrap_err();
        assert!(matches!(err, BuildRequestError::FileNotResolved(_)));
    }

    #[test]
    fn chat_pdf_url_not_resolved_error() {
        let media = make_media(
            MediaKind::Pdf,
            MediaContent::Url {
                url: "https://example.com/doc.pdf".into(),
                base64_data: None,
            },
            Some("application/pdf"),
        );
        let err = media
            .read_content(|c| openai_media_part(&media, c))
            .unwrap_err();
        assert!(matches!(err, BuildRequestError::UnsupportedMedia(_)));
        assert!(err.to_string().contains("pdf url content was not resolved"));
    }

    #[test]
    fn chat_pdf_url_with_base64_data() {
        let media = make_media(
            MediaKind::Pdf,
            MediaContent::Url {
                url: "https://example.com/doc.pdf".into(),
                base64_data: Some("JVBERi0=".into()),
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
    fn chat_pdf_file_with_base64_data() {
        let media = make_media(
            MediaKind::Pdf,
            MediaContent::File {
                file: "doc.pdf".into(),
                base64_data: Some("JVBERi0=".into()),
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
    fn chat_pdf_file_not_resolved_error() {
        let media = make_media(
            MediaKind::Pdf,
            MediaContent::File {
                file: "doc.pdf".into(),
                base64_data: None,
            },
            Some("application/pdf"),
        );
        let err = media
            .read_content(|c| openai_media_part(&media, c))
            .unwrap_err();
        assert!(matches!(err, BuildRequestError::FileNotResolved(_)));
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
                (
                    "api_version",
                    BexExternalValue::String("2024-02-15-preview".into()),
                ),
                ("api_key", BexExternalValue::String("sk-test".into())),
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
                (
                    "api_version",
                    BexExternalValue::String("2024-02-15-preview".into()),
                ),
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
                (
                    "api_version",
                    BexExternalValue::String("2024-02-15-preview".into()),
                ),
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
                (
                    "api_version",
                    BexExternalValue::String("2024-02-15-preview".into()),
                ),
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

    #[test]
    fn azure_missing_api_version() {
        let client = make_client(
            "azure-openai",
            vec![
                ("model", BexExternalValue::String("gpt-4o".into())),
                ("resource_name", BexExternalValue::String("res".into())),
            ],
        );
        let err = build_request(&client, msg("user", "hi")).unwrap_err();
        assert!(err.to_string().contains("api_version"));
    }

    // ========================================================================
    // Chat Completions: other OpenAI-like providers
    // ========================================================================

    #[test]
    fn openai_generic_requires_base_url() {
        let client = make_client("openai-generic", vec![]);
        let err = build_request(&client, msg("user", "hi")).unwrap_err();
        assert!(err.to_string().contains("base_url"));
    }

    #[test]
    fn openai_generic_with_base_url() {
        let client = make_client(
            "openai-generic",
            vec![
                (
                    "base_url",
                    BexExternalValue::String("https://my-llm.example.com/v1".into()),
                ),
                ("model", BexExternalValue::String("my-model".into())),
                ("api_key", BexExternalValue::String("sk-custom".into())),
            ],
        );
        let result = build_request(&client, msg("user", "hi")).unwrap();
        assert_eq!(result.url, "https://my-llm.example.com/v1/chat/completions");
        assert_eq!(
            result.headers.get("authorization").unwrap(),
            "Bearer sk-custom"
        );
        let body = parse_body(&result.body);
        assert_eq!(body["model"], "my-model");
        assert_eq!(body["messages"][0]["role"], "user");
    }

    #[test]
    fn ollama_default_url() {
        let client = make_client(
            "ollama",
            vec![("model", BexExternalValue::String("llama3".into()))],
        );
        let result = build_request(&client, msg("user", "hi")).unwrap();
        assert_eq!(result.url, "http://localhost:11434/v1/chat/completions");
        let body = parse_body(&result.body);
        assert_eq!(body["model"], "llama3");
    }

    #[test]
    fn ollama_custom_base_url() {
        let client = make_client(
            "ollama",
            vec![
                (
                    "base_url",
                    BexExternalValue::String("http://remote:11434/v1".into()),
                ),
                ("model", BexExternalValue::String("llama3".into())),
            ],
        );
        let result = build_request(&client, msg("user", "hi")).unwrap();
        assert_eq!(result.url, "http://remote:11434/v1/chat/completions");
    }

    #[test]
    fn ollama_no_auth_header() {
        let client = make_client(
            "ollama",
            vec![("model", BexExternalValue::String("llama3".into()))],
        );
        let result = build_request(&client, msg("user", "hi")).unwrap();
        assert!(result.headers.get("authorization").is_none());
        assert!(result.headers.get("api-key").is_none());
    }

    #[test]
    fn openrouter_default_url() {
        let client = make_client(
            "openrouter",
            vec![
                ("model", BexExternalValue::String("openai/gpt-4o".into())),
                ("api_key", BexExternalValue::String("sk-or-test".into())),
            ],
        );
        let result = build_request(&client, msg("user", "hi")).unwrap();
        assert_eq!(result.url, "https://openrouter.ai/api/v1/chat/completions");
        assert_eq!(
            result.headers.get("authorization").unwrap(),
            "Bearer sk-or-test"
        );
        let body = parse_body(&result.body);
        assert_eq!(body["model"], "openai/gpt-4o");
    }

    #[test]
    fn openrouter_forwards_options() {
        let client = make_client(
            "openrouter",
            vec![
                ("model", BexExternalValue::String("openai/gpt-4o".into())),
                ("temperature", BexExternalValue::Float(0.3)),
                ("max_tokens", BexExternalValue::Int(500)),
            ],
        );
        let result = build_request(&client, msg("user", "hi")).unwrap();
        let body = parse_body(&result.body);
        assert_eq!(body["temperature"], 0.3);
        assert_eq!(body["max_tokens"], 500);
    }
}
