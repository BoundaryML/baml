//! Anthropic-format HTTP request builder.

use baml_base::MediaKind;
use baml_builtins::{MediaContent, PromptAst, PromptAstSimple};
use indexmap::IndexMap;
use serde::{Deserialize, Serialize};

use super::{
    BuildRequestError, LlmPrimitiveClient, LlmRequestBuilder, get_string_option, mime_type_as_ok,
};

/// Builder for the Anthropic provider.
pub(crate) struct AnthropicBuilder;

/// Default `max_tokens` and version for Anthropic requests (Anthropic requires this field).
/// Matches the default in `engine/baml-lib/llm-client/src/clients/anthropic.rs`.
const DEFAULT_MAX_TOKENS: u32 = 4096;
const DEFAULT_ANTHROPIC_VERSION: &str = "2023-06-01";

/// A content part within an Anthropic message.
///
/// Serializes with `{"type": "<variant>", ...}` via `#[serde(tag = "type")]`.
#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum ContentPart {
    Text { text: String },
    Image { source: MediaSource },
    Audio { source: MediaSource },
    Document { source: MediaSource },
}

/// Media source for Anthropic content parts.
///
/// Serializes with `{"type": "<variant>", ...}` via `#[serde(tag = "type")]`.
#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum MediaSource {
    Url { url: String },
    Base64 { media_type: String, data: String },
}

impl LlmRequestBuilder for AnthropicBuilder {
    fn provider_skip_keys(&self) -> &'static [&'static str] {
        &["anthropic_version"]
    }

    fn build_url(&self, client: &LlmPrimitiveClient) -> Result<String, BuildRequestError> {
        let base_url = get_string_option(client, "base_url")
            .unwrap_or_else(|| "https://api.anthropic.com".to_string());
        Ok(format!("{base_url}/v1/messages"))
    }

    fn build_auth_headers(&self, client: &LlmPrimitiveClient) -> IndexMap<String, String> {
        let mut headers = IndexMap::new();
        // Anthropic uses x-api-key header
        if let Some(api_key) = get_string_option(client, "api_key") {
            headers.insert("x-api-key".to_string(), api_key);
        }
        // Anthropic version header
        let version = get_string_option(client, "anthropic_version")
            .unwrap_or_else(|| DEFAULT_ANTHROPIC_VERSION.to_string());
        headers.insert("anthropic-version".to_string(), version);
        headers
    }

    fn build_prompt_body(
        &self,
        client: &LlmPrimitiveClient,
        prompt: bex_vm_types::PromptAst,
    ) -> Result<serde_json::Map<String, serde_json::Value>, super::BuildRequestError> {
        let mut map = serde_json::Map::new();
        // Anthropic requires max_tokens — inject default if not set by user.
        if !client.options.contains_key("max_tokens") {
            map.insert(
                "max_tokens".to_string(),
                serde_json::Value::Number(DEFAULT_MAX_TOKENS.into()),
            );
        }
        let (system_parts, messages) = extract_system_and_messages(prompt, &client.default_role)?;
        if !system_parts.is_empty() {
            map.insert("system".to_string(), serde_json::Value::Array(system_parts));
        }
        map.insert("messages".to_string(), serde_json::Value::Array(messages));
        Ok(map)
    }
}

/// Converts `MediaContent` into an Anthropic `MediaSource`, returning a
/// `FileNotResolved` error when a file reference has no resolved base64 data.
fn content_to_media_source(
    media: &baml_builtins::MediaValue,
    content: &baml_builtins::MediaContent,
    kind_label: &str,
) -> Result<MediaSource, BuildRequestError> {
    match content {
        MediaContent::Url { url, .. } => Ok(MediaSource::Url { url: url.clone() }),
        MediaContent::Base64 { base64_data, .. }
        | MediaContent::File {
            base64_data: Some(base64_data),
            ..
        } => Ok(MediaSource::Base64 {
            media_type: mime_type_as_ok(media)?.to_string(),
            data: base64_data.clone(),
        }),
        MediaContent::File {
            base64_data: None, ..
        } => Err(BuildRequestError::FileNotResolved(format!(
            "{kind_label} file content was not resolved properly"
        ))),
    }
}

/// Converts a media value into Anthropic content parts (image, audio, document).
fn anthropic_media_part(
    media: &baml_builtins::MediaValue,
    content: &baml_builtins::MediaContent,
) -> Result<Vec<ContentPart>, BuildRequestError> {
    match media.kind {
        MediaKind::Image => {
            let source = content_to_media_source(media, content, "image")?;
            Ok(vec![ContentPart::Image { source }])
        }
        MediaKind::Audio => {
            let source = content_to_media_source(media, content, "audio")?;
            Ok(vec![ContentPart::Audio { source }])
        }
        MediaKind::Pdf => {
            let source = content_to_media_source(media, content, "pdf")?;
            Ok(vec![ContentPart::Document { source }])
        }
        MediaKind::Video => Err(BuildRequestError::UnsupportedMedia(
            "video input is not supported on Anthropic".into(),
        )),
        MediaKind::Generic => Err(BuildRequestError::UnsupportedMedia(
            "generic media is currently unimplemented".into(),
        )),
    }
}

/// Converts a [`PromptAstSimple`] content node into Anthropic content parts.
fn anthropic_content_parts(
    content: &PromptAstSimple,
) -> Result<Vec<ContentPart>, BuildRequestError> {
    match content {
        PromptAstSimple::String(s) => Ok(vec![ContentPart::Text { text: s.clone() }]),
        PromptAstSimple::Media(media) => media.read_content(|c| anthropic_media_part(media, c)),
        PromptAstSimple::Multiple(multiple) => {
            let mut parts = Vec::new();
            for item in multiple {
                parts.extend(anthropic_content_parts(item)?);
            }
            Ok(parts)
        }
    }
}

/// Convert content to JSON values and merge metadata into the last part.
fn content_to_json(
    content: &PromptAstSimple,
    metadata: &serde_json::Value,
) -> Result<Vec<serde_json::Value>, BuildRequestError> {
    let parts = anthropic_content_parts(content)?;
    let mut values: Vec<serde_json::Value> = parts
        .into_iter()
        .map(|p| serde_json::to_value(p).expect("infallible"))
        .collect();

    if let serde_json::Value::Object(meta_map) = metadata {
        if !meta_map.is_empty() {
            if let Some(serde_json::Value::Object(part_map)) = values.last_mut() {
                for (k, v) in meta_map {
                    part_map.insert(k.clone(), v.clone());
                }
            }
        }
    }

    Ok(values)
}

/// Extract system messages to a separate array and return non-system messages.
///
/// Anthropic format:
/// - System: top-level `"system": [{"type": "text", "text": "..."}]`
/// - Messages: `[{"role": "user", "content": [{"type": "text", "text": "..."}]}]`
fn extract_system_and_messages(
    prompt: bex_vm_types::PromptAst,
    default_role: &str,
) -> Result<(Vec<serde_json::Value>, Vec<serde_json::Value>), BuildRequestError> {
    let mut system_parts = Vec::new();
    let mut messages = Vec::new();

    let items = match prompt.as_ref() {
        PromptAst::Vec(v) => v.clone(),
        _ => vec![prompt],
    };

    for item in &items {
        match item.as_ref() {
            PromptAst::Message {
                role,
                content,
                metadata,
            } if role == "system" => {
                system_parts.extend(content_to_json(content.as_ref(), metadata)?);
            }
            PromptAst::Message {
                role,
                content,
                metadata,
            } => {
                let content_values = content_to_json(content.as_ref(), metadata)?;
                let mut msg = serde_json::Map::new();
                msg.insert("role".to_string(), serde_json::Value::String(role.clone()));
                msg.insert(
                    "content".to_string(),
                    serde_json::Value::Array(content_values),
                );
                messages.push(serde_json::Value::Object(msg));
            }
            PromptAst::Simple(content) => {
                let content_values = content_to_json(content.as_ref(), &serde_json::Value::Null)?;
                let mut msg = serde_json::Map::new();
                msg.insert(
                    "role".to_string(),
                    serde_json::Value::String(default_role.to_string()),
                );
                msg.insert(
                    "content".to_string(),
                    serde_json::Value::Array(content_values),
                );
                messages.push(serde_json::Value::Object(msg));
            }
            PromptAst::Vec(_) => unreachable!(), // PromptAst::Vec should have been flattened.
        }
    }

    Ok((system_parts, messages))
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use baml_base::MediaKind;
    use baml_builtins::{MediaContent, MediaValue, PromptAst};
    use bex_external_types::BexExternalValue;
    use indexmap::IndexMap;

    use super::*;
    use crate::build_request::{LlmPrimitiveClient, build_request};

    // -- helpers --

    fn make_media(kind: MediaKind, content: MediaContent, mime: Option<&str>) -> MediaValue {
        MediaValue::new(kind, content, mime.map(String::from))
    }

    fn make_client(options: Vec<(&str, BexExternalValue)>) -> LlmPrimitiveClient {
        let mut opts = IndexMap::new();
        for (k, v) in options {
            opts.insert(k.to_string(), v);
        }
        LlmPrimitiveClient {
            name: "test".to_string(),
            provider: "anthropic".to_string(),
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
    // Anthropic: media content parts
    // ========================================================================

    #[test]
    fn anthropic_image_url() {
        let media = make_media(
            MediaKind::Image,
            MediaContent::Url {
                url: "https://example.com/cat.png".into(),
                base64_data: None,
            },
            Some("image/png"),
        );
        let parts = media
            .read_content(|c| anthropic_media_part(&media, c))
            .unwrap();
        let json = serde_json::to_value(&parts[0]).unwrap();
        assert_eq!(
            json,
            serde_json::json!({"type": "image", "source": {"type": "url", "url": "https://example.com/cat.png"}})
        );
    }

    #[test]
    fn anthropic_image_base64() {
        let media = make_media(
            MediaKind::Image,
            MediaContent::Base64 {
                base64_data: "iVBORw0KGgo=".into(),
            },
            Some("image/png"),
        );
        let parts = media
            .read_content(|c| anthropic_media_part(&media, c))
            .unwrap();
        let json = serde_json::to_value(&parts[0]).unwrap();
        assert_eq!(
            json,
            serde_json::json!({"type": "image", "source": {"type": "base64", "media_type": "image/png", "data": "iVBORw0KGgo="}})
        );
    }

    #[test]
    fn anthropic_image_file_with_base64_data() {
        let media = make_media(
            MediaKind::Image,
            MediaContent::File {
                file: "cat.png".into(),
                base64_data: Some("iVBORw0KGgo=".into()),
            },
            Some("image/png"),
        );
        let parts = media
            .read_content(|c| anthropic_media_part(&media, c))
            .unwrap();
        let json = serde_json::to_value(&parts[0]).unwrap();
        assert_eq!(
            json,
            serde_json::json!({"type": "image", "source": {"type": "base64", "media_type": "image/png", "data": "iVBORw0KGgo="}})
        );
    }

    #[test]
    fn anthropic_image_file_not_resolved_error() {
        let media = make_media(
            MediaKind::Image,
            MediaContent::File {
                file: "cat.png".into(),
                base64_data: None,
            },
            Some("image/png"),
        );
        let err = media
            .read_content(|c| anthropic_media_part(&media, c))
            .unwrap_err();
        assert!(matches!(err, BuildRequestError::FileNotResolved(_)));
        assert!(
            err.to_string()
                .contains("image file content was not resolved")
        );
    }

    #[test]
    fn anthropic_audio_url() {
        let media = make_media(
            MediaKind::Audio,
            MediaContent::Url {
                url: "https://example.com/speech.wav".into(),
                base64_data: None,
            },
            Some("audio/wav"),
        );
        let parts = media
            .read_content(|c| anthropic_media_part(&media, c))
            .unwrap();
        let json = serde_json::to_value(&parts[0]).unwrap();
        assert_eq!(
            json,
            serde_json::json!({"type": "audio", "source": {"type": "url", "url": "https://example.com/speech.wav"}})
        );
    }

    #[test]
    fn anthropic_audio_base64() {
        let media = make_media(
            MediaKind::Audio,
            MediaContent::Base64 {
                base64_data: "AAAA".into(),
            },
            Some("audio/wav"),
        );
        let parts = media
            .read_content(|c| anthropic_media_part(&media, c))
            .unwrap();
        let json = serde_json::to_value(&parts[0]).unwrap();
        assert_eq!(
            json,
            serde_json::json!({"type": "audio", "source": {"type": "base64", "media_type": "audio/wav", "data": "AAAA"}})
        );
    }

    #[test]
    fn anthropic_audio_url_with_base64_data() {
        let media = make_media(
            MediaKind::Audio,
            MediaContent::Url {
                url: "https://example.com/speech.wav".into(),
                base64_data: Some("AAAA".into()),
            },
            Some("audio/wav"),
        );
        let parts = media
            .read_content(|c| anthropic_media_part(&media, c))
            .unwrap();
        let json = serde_json::to_value(&parts[0]).unwrap();
        assert_eq!(
            json,
            serde_json::json!({"type": "audio", "source": {"type": "url", "url": "https://example.com/speech.wav"}})
        );
    }

    #[test]
    fn anthropic_pdf_url() {
        let media = make_media(
            MediaKind::Pdf,
            MediaContent::Url {
                url: "https://example.com/doc.pdf".into(),
                base64_data: None,
            },
            Some("application/pdf"),
        );
        let parts = media
            .read_content(|c| anthropic_media_part(&media, c))
            .unwrap();
        let json = serde_json::to_value(&parts[0]).unwrap();
        assert_eq!(
            json,
            serde_json::json!({"type": "document", "source": {"type": "url", "url": "https://example.com/doc.pdf"}})
        );
    }

    #[test]
    fn anthropic_pdf_base64() {
        let media = make_media(
            MediaKind::Pdf,
            MediaContent::Base64 {
                base64_data: "JVBERi0=".into(),
            },
            Some("application/pdf"),
        );
        let parts = media
            .read_content(|c| anthropic_media_part(&media, c))
            .unwrap();
        let json = serde_json::to_value(&parts[0]).unwrap();
        assert_eq!(
            json,
            serde_json::json!({"type": "document", "source": {"type": "base64", "media_type": "application/pdf", "data": "JVBERi0="}})
        );
    }

    #[test]
    fn anthropic_video_unsupported() {
        let media = make_media(
            MediaKind::Video,
            MediaContent::Url {
                url: "https://example.com/clip.mp4".into(),
                base64_data: None,
            },
            Some("video/mp4"),
        );
        let err = media
            .read_content(|c| anthropic_media_part(&media, c))
            .unwrap_err();
        assert!(matches!(err, BuildRequestError::UnsupportedMedia(_)));
        assert!(err.to_string().contains("video"));
        assert!(err.to_string().contains("not supported"));
    }

    // ========================================================================
    // Anthropic: message building and metadata
    // ========================================================================

    #[test]
    fn anthropic_single_user_message() {
        let client = make_client(vec![
            (
                "model",
                BexExternalValue::String("claude-3-haiku-20240307".into()),
            ),
            ("max_tokens", BexExternalValue::Int(1000)),
        ]);
        let prompt = msg("user", "Hello");
        let result = build_request(&client, prompt, false).unwrap();
        let body = parse_body(&result.body);
        assert_eq!(
            body,
            serde_json::json!({
                "model": "claude-3-haiku-20240307",
                "max_tokens": 1000,
                "messages": [
                    {
                        "role": "user",
                        "content": [{"type": "text", "text": "Hello"}]
                    }
                ]
            })
        );
    }

    #[test]
    fn anthropic_three_role_conversation() {
        let client = make_client(vec![
            (
                "model",
                BexExternalValue::String("claude-3-haiku-20240307".into()),
            ),
            ("max_tokens", BexExternalValue::Int(1000)),
        ]);
        let prompt = Arc::new(PromptAst::Vec(vec![
            msg("system", "You are a helpful assistant."),
            msg("user", "What is 2+2?"),
            msg("assistant", "4"),
        ]));
        let result = build_request(&client, prompt, false).unwrap();
        let body = parse_body(&result.body);
        assert_eq!(
            body,
            serde_json::json!({
                "model": "claude-3-haiku-20240307",
                "max_tokens": 1000,
                "system": [
                    {"type": "text", "text": "You are a helpful assistant."}
                ],
                "messages": [
                    {
                        "role": "user",
                        "content": [{"type": "text", "text": "What is 2+2?"}]
                    },
                    {
                        "role": "assistant",
                        "content": [{"type": "text", "text": "4"}]
                    }
                ]
            })
        );
    }

    #[test]
    fn anthropic_multi_turn_conversation() {
        let client = make_client(vec![
            (
                "model",
                BexExternalValue::String("claude-3-haiku-20240307".into()),
            ),
            ("max_tokens", BexExternalValue::Int(1000)),
        ]);
        let prompt = Arc::new(PromptAst::Vec(vec![
            msg("system", "Be concise."),
            msg("user", "Hello"),
            msg("assistant", "Hi!"),
            msg("user", "How are you?"),
            msg("assistant", "Good, thanks!"),
            msg("user", "Goodbye"),
        ]));
        let result = build_request(&client, prompt, false).unwrap();
        let body = parse_body(&result.body);
        assert_eq!(
            body,
            serde_json::json!({
                "model": "claude-3-haiku-20240307",
                "max_tokens": 1000,
                "system": [
                    {"type": "text", "text": "Be concise."}
                ],
                "messages": [
                    {
                        "role": "user",
                        "content": [{"type": "text", "text": "Hello"}]
                    },
                    {
                        "role": "assistant",
                        "content": [{"type": "text", "text": "Hi!"}]
                    },
                    {
                        "role": "user",
                        "content": [{"type": "text", "text": "How are you?"}]
                    },
                    {
                        "role": "assistant",
                        "content": [{"type": "text", "text": "Good, thanks!"}]
                    },
                    {
                        "role": "user",
                        "content": [{"type": "text", "text": "Goodbye"}]
                    }
                ]
            })
        );
    }

    #[test]
    fn anthropic_metadata_merged_to_last_part() {
        let client = make_client(vec![
            (
                "model",
                BexExternalValue::String("claude-3-haiku-20240307".into()),
            ),
            ("max_tokens", BexExternalValue::Int(1000)),
        ]);
        let prompt = Arc::new(PromptAst::Message {
            role: "user".to_string(),
            content: Arc::new("hello".to_string().into()),
            metadata: serde_json::json!({"cache_control": {"type": "ephemeral"}}),
        });
        let result = build_request(&client, prompt, false).unwrap();
        let body = parse_body(&result.body);
        assert_eq!(
            body,
            serde_json::json!({
                "model": "claude-3-haiku-20240307",
                "max_tokens": 1000,
                "messages": [
                    {
                        "role": "user",
                        "content": [
                            {
                                "type": "text",
                                "text": "hello",
                                "cache_control": {"type": "ephemeral"}
                            }
                        ]
                    }
                ]
            })
        );
    }

    #[test]
    fn anthropic_multiple_system_messages_combined() {
        let client = make_client(vec![
            (
                "model",
                BexExternalValue::String("claude-3-haiku-20240307".into()),
            ),
            ("max_tokens", BexExternalValue::Int(1000)),
        ]);
        let prompt = Arc::new(PromptAst::Vec(vec![
            msg("system", "First instruction."),
            msg("system", "Second instruction."),
            msg("user", "Hello"),
        ]));
        let result = build_request(&client, prompt, false).unwrap();
        let body = parse_body(&result.body);
        assert_eq!(
            body,
            serde_json::json!({
                "model": "claude-3-haiku-20240307",
                "max_tokens": 1000,
                "system": [
                    {"type": "text", "text": "First instruction."},
                    {"type": "text", "text": "Second instruction."}
                ],
                "messages": [
                    {
                        "role": "user",
                        "content": [{"type": "text", "text": "Hello"}]
                    }
                ]
            })
        );
    }

    #[test]
    fn anthropic_system_metadata_merged_to_last_part() {
        let client = make_client(vec![
            (
                "model",
                BexExternalValue::String("claude-3-haiku-20240307".into()),
            ),
            ("max_tokens", BexExternalValue::Int(1000)),
        ]);
        let prompt = Arc::new(PromptAst::Vec(vec![
            Arc::new(PromptAst::Message {
                role: "system".to_string(),
                content: Arc::new("You are a helpful assistant.".to_string().into()),
                metadata: serde_json::json!({"cache_control": {"type": "ephemeral"}}),
            }),
            msg("user", "Hello"),
        ]));
        let result = build_request(&client, prompt, false).unwrap();
        let body = parse_body(&result.body);
        assert_eq!(
            body,
            serde_json::json!({
                "model": "claude-3-haiku-20240307",
                "max_tokens": 1000,
                "system": [
                    {
                        "type": "text",
                        "text": "You are a helpful assistant.",
                        "cache_control": {"type": "ephemeral"}
                    }
                ],
                "messages": [
                    {
                        "role": "user",
                        "content": [{"type": "text", "text": "Hello"}]
                    }
                ]
            })
        );
    }

    #[test]
    fn anthropic_mixed_text_and_image() {
        let client = make_client(vec![
            (
                "model",
                BexExternalValue::String("claude-3-haiku-20240307".into()),
            ),
            ("max_tokens", BexExternalValue::Int(1000)),
        ]);

        let media = make_media(
            MediaKind::Image,
            MediaContent::Url {
                url: "https://example.com/photo.jpg".into(),
                base64_data: None,
            },
            Some("image/jpeg"),
        );

        let prompt = Arc::new(PromptAst::Message {
            role: "user".to_string(),
            content: Arc::new(baml_builtins::PromptAstSimple::Multiple(vec![
                Arc::new(baml_builtins::PromptAstSimple::String(
                    "What is in this image?".into(),
                )),
                Arc::new(baml_builtins::PromptAstSimple::Media(Arc::new(media))),
            ])),
            metadata: serde_json::Value::Null,
        });

        let result = build_request(&client, prompt, false).unwrap();
        let body = parse_body(&result.body);
        assert_eq!(
            body,
            serde_json::json!({
                "model": "claude-3-haiku-20240307",
                "max_tokens": 1000,
                "messages": [
                    {
                        "role": "user",
                        "content": [
                            {"type": "text", "text": "What is in this image?"},
                            {"type": "image", "source": {"type": "url", "url": "https://example.com/photo.jpg"}}
                        ]
                    }
                ]
            })
        );
    }
}
