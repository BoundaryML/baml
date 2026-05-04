//! Anthropic Messages API body builder.
//!
//! Builds the JSON body for `/v1/messages`. System messages are extracted to a
//! top-level `"system"` field as required by the Anthropic API.

use std::sync::Arc;

use baml_base::MediaKind;
use baml_builtins2::{MediaContent, MediaValue, PromptAst, PromptAstSimple};
use serde::Serialize;

// ============================================================================
// Serde types for the Anthropic Messages API
// ============================================================================

#[derive(Debug, Serialize)]
#[serde(tag = "type")]
#[serde(rename_all = "snake_case")]
enum ContentPart {
    Text {
        text: String,
        #[serde(flatten)]
        extra: serde_json::Map<String, serde_json::Value>,
    },
    Image {
        source: MediaSource,
        #[serde(flatten)]
        extra: serde_json::Map<String, serde_json::Value>,
    },
    #[serde(rename = "input_audio")]
    Audio {
        source: MediaSource,
        #[serde(flatten)]
        extra: serde_json::Map<String, serde_json::Value>,
    },
    Document {
        source: MediaSource,
        #[serde(flatten)]
        extra: serde_json::Map<String, serde_json::Value>,
    },
}

#[derive(Debug, Serialize)]
#[serde(tag = "type")]
#[serde(rename_all = "snake_case")]
enum MediaSource {
    Url { url: String },
    Base64 { media_type: String, data: String },
}

// ============================================================================
// Request body
// ============================================================================

/// Full request body for `/v1/messages`.
#[derive(Debug, Serialize)]
struct RequestBody {
    model: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    system: Vec<ContentPart>,
    messages: Vec<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_tokens: Option<i64>,
    #[serde(flatten)]
    extra: serde_json::Map<String, serde_json::Value>,
}

/// Default `max_tokens` when the user does not specify one.
///
/// The Anthropic API requires `max_tokens` and has no server-side default.
/// 8 192 is a safe middle ground: large enough for most structured outputs,
/// small enough to avoid non-streaming timeout errors from the Anthropic SDK
/// (which rejects requests estimated to take >10 minutes).
pub(super) const DEFAULT_MAX_TOKENS: i64 = 8_192;

// ============================================================================
// Request builder
// ============================================================================

pub(crate) fn build_request(
    client: &crate::baml_std::PrimitiveClient,
    prompt: &bex_vm_types::PromptAst,
) -> Result<crate::baml_std::HttpRequest, super::BuildRequestError> {
    // Headers
    let mut headers = indexmap::IndexMap::new();
    headers.insert("content-type".to_string(), "application/json".to_string());
    headers.insert("anthropic-version".to_string(), "2023-06-01".to_string());

    // Body — use the user-provided max_tokens, or fall back to a safe default.
    let max_tokens = match &client.provider_options {
        Some(crate::baml_std::ProviderOptions::Anthropic(opts)) => opts.max_tokens,
        _ => None,
    };
    let max_tokens = Some(max_tokens.unwrap_or(DEFAULT_MAX_TOKENS));
    let body_str = build_anthropic_body_str(&client.model, prompt, max_tokens, &client.extra_body)?;

    Ok(crate::baml_std::HttpRequest {
        method: "POST".to_string(),
        url: format!(
            "{}/v1/messages",
            client.options.base_url.as_deref().unwrap_or_default()
        ),
        headers,
        body: body_str,
    })
}

/// Build the Anthropic Messages API body as a JSON string.
///
/// Reused by Vertex AI's Anthropic-on-Vertex path (`rawPredict`).
pub(super) fn build_anthropic_body_str(
    model: &str,
    prompt: &bex_vm_types::PromptAst,
    max_tokens: Option<i64>,
    extra: &serde_json::Map<String, serde_json::Value>,
) -> Result<String, super::BuildRequestError> {
    let (system, messages) = extract_system_and_messages(prompt)?;
    let body = RequestBody {
        model: model.to_string(),
        system,
        messages,
        max_tokens,
        extra: extra.clone(),
    };
    serde_json::to_string(&body).map_err(Into::into)
}

// ============================================================================
// Prompt conversion
// ============================================================================

/// Split the prompt into system content parts and non-system messages.
///
/// Anthropic requires system content in a top-level `"system"` field, not
/// inside the `"messages"` array.
fn extract_system_and_messages(
    prompt: &bex_vm_types::PromptAst,
) -> Result<(Vec<ContentPart>, Vec<serde_json::Value>), super::BuildRequestError> {
    let items = match prompt.as_ref() {
        PromptAst::Vec(v) => v.clone(),
        _ => vec![prompt.clone()],
    };

    let mut system_parts = Vec::new();
    let mut messages = Vec::new();

    for item in &items {
        match item.as_ref() {
            PromptAst::Message {
                role,
                content,
                metadata,
            } if role == "system" => {
                let mut parts = anthropic_content_parts(content.as_ref(), role)?;
                // Merge metadata into the last content part (e.g. cache_control).
                merge_metadata_into_last(&mut parts, metadata);
                system_parts.extend(parts);
            }
            PromptAst::Message {
                role,
                content,
                metadata,
            } => {
                let mut parts = anthropic_content_parts(content.as_ref(), role)?;
                merge_metadata_into_last(&mut parts, metadata);

                let parts_json = serde_json::to_value(&parts)?;

                let mut msg = serde_json::Map::new();
                msg.insert("role".to_string(), serde_json::Value::String(role.clone()));
                msg.insert("content".to_string(), parts_json);
                messages.push(serde_json::Value::Object(msg));
            }
            _ => {} // Skip non-message nodes
        }
    }

    Ok((system_parts, messages))
}

/// Merge metadata key-value pairs into the `extra` map of the last content part.
///
/// Anthropic uses this for features like `cache_control`, which is attached to
/// the last content block of a message.
fn merge_metadata_into_last(parts: &mut [ContentPart], metadata: &serde_json::Value) {
    let serde_json::Value::Object(map) = metadata else {
        return;
    };
    if map.is_empty() {
        return;
    }
    let Some(last) = parts.last_mut() else {
        return;
    };
    let extra = match last {
        ContentPart::Text { extra, .. }
        | ContentPart::Image { extra, .. }
        | ContentPart::Audio { extra, .. }
        | ContentPart::Document { extra, .. } => extra,
    };
    extra.extend(map.iter().map(|(k, v)| (k.clone(), v.clone())));
}

fn anthropic_content_parts(
    content: &PromptAstSimple,
    role: &str,
) -> Result<Vec<ContentPart>, super::BuildRequestError> {
    match content {
        PromptAstSimple::String(s) => Ok(vec![ContentPart::Text {
            text: s.clone(),
            extra: serde_json::Map::new(),
        }]),
        PromptAstSimple::Media(media) => {
            if role != "user" {
                return Err(unsupported_media_role(role, media.kind));
            }
            anthropic_media_part(media).map(|part| vec![part])
        }
        PromptAstSimple::Multiple(items) => {
            let mut parts = Vec::new();
            for item in items {
                parts.extend(anthropic_content_parts(item, role)?);
            }
            Ok(parts)
        }
    }
}

fn unsupported_media_role(role: &str, kind: MediaKind) -> super::BuildRequestError {
    super::BuildRequestError::UnsupportedMedia(format!(
        "Anthropic only supports {kind} input in user messages; found media in a {role} message"
    ))
}

fn anthropic_media_part(media: &Arc<MediaValue>) -> Result<ContentPart, super::BuildRequestError> {
    let mime = super::mime_type_as_ok(media)?;
    let source = media.read_content(|c| content_to_media_source(c, &mime))?;

    match media.kind {
        MediaKind::Image => Ok(ContentPart::Image {
            source,
            extra: serde_json::Map::new(),
        }),
        MediaKind::Audio => Ok(ContentPart::Audio {
            source,
            extra: serde_json::Map::new(),
        }),
        MediaKind::Pdf => Ok(ContentPart::Document {
            source,
            extra: serde_json::Map::new(),
        }),
        MediaKind::Video => Err(super::BuildRequestError::UnsupportedMedia(
            "Anthropic does not support video content".into(),
        )),
        MediaKind::Generic => Err(super::BuildRequestError::UnsupportedMedia(
            "generic media kind not supported by Anthropic".into(),
        )),
    }
}

fn content_to_media_source(
    content: &MediaContent,
    mime: &str,
) -> Result<MediaSource, super::BuildRequestError> {
    if let Some(url) = content.url() {
        return Ok(MediaSource::Url {
            url: url.to_string(),
        });
    }
    if let Some(b64) = content.base64_data() {
        return Ok(MediaSource::Base64 {
            media_type: mime.to_string(),
            data: b64.to_string(),
        });
    }
    Err(super::BuildRequestError::FileNotResolved(
        content.file_path().unwrap_or("<unknown>").to_string(),
    ))
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use baml_builtins2::{MediaContent, MediaValue, PromptAst, PromptAstSimple};
    use bex_external_types::{AsBexExternalValue, BexExternalValue};
    use indexmap::IndexMap;

    use super::*;

    fn make_media(kind: MediaKind, content: MediaContent, mime: Option<&str>) -> Arc<MediaValue> {
        Arc::new(MediaValue::new(kind, content, mime.map(String::from)))
    }

    fn msg(role: &str, text: &str) -> Arc<PromptAst> {
        Arc::new(PromptAst::Message {
            role: role.to_string(),
            content: Arc::new(PromptAstSimple::String(text.to_string())),
            metadata: serde_json::Value::Null,
        })
    }

    fn msg_with_metadata(role: &str, text: &str, metadata: serde_json::Value) -> Arc<PromptAst> {
        Arc::new(PromptAst::Message {
            role: role.to_string(),
            content: Arc::new(PromptAstSimple::String(text.to_string())),
            metadata,
        })
    }

    fn msg_with_content(
        role: &str,
        content: PromptAstSimple,
        metadata: serde_json::Value,
    ) -> Arc<PromptAst> {
        Arc::new(PromptAst::Message {
            role: role.to_string(),
            content: Arc::new(content),
            metadata,
        })
    }

    fn make_client(options: Vec<(&str, BexExternalValue)>) -> crate::baml_std::PrimitiveClient {
        let mut request_body = IndexMap::new();
        let mut model = None;
        for (k, v) in options {
            if k == "model" {
                if let BexExternalValue::String(s) = &v {
                    model = Some(s.clone());
                }
            } else {
                request_body.insert(k.to_string(), v);
            }
        }
        crate::baml_std::PrimitiveClient::new(
            "test".to_string(),
            "anthropic".to_string(),
            crate::baml_std::PrimitiveClientOptions {
                model,
                request_body,
                base_url: Some("https://api.anthropic.com".to_string()),
                provider_options: crate::baml_std::AnthropicOptions {
                    max_tokens: Some(4096),
                }
                .into_bex_external_value(),
                ..Default::default()
            },
        )
        .unwrap()
    }

    // ========================================================================
    // Media tests
    // ========================================================================

    #[test]
    fn anthropic_image_url() {
        let media = make_media(
            MediaKind::Image,
            MediaContent::Url {
                url: "https://example.com/img.png".into(),
                base64_data: None,
            },
            Some("image/png"),
        );
        let part = anthropic_media_part(&media).unwrap();
        assert_eq!(
            serde_json::to_value(&part).unwrap(),
            serde_json::json!({
                "type": "image",
                "source": {"type": "url", "url": "https://example.com/img.png"}
            })
        );
    }

    #[test]
    fn anthropic_image_base64() {
        let media = make_media(
            MediaKind::Image,
            MediaContent::Base64 {
                base64_data: "abc123".into(),
            },
            Some("image/jpeg"),
        );
        let part = anthropic_media_part(&media).unwrap();
        assert_eq!(
            serde_json::to_value(&part).unwrap(),
            serde_json::json!({
                "type": "image",
                "source": {"type": "base64", "media_type": "image/jpeg", "data": "abc123"}
            })
        );
    }

    #[test]
    fn anthropic_image_file_with_base64_data() {
        let media = make_media(
            MediaKind::Image,
            MediaContent::File {
                file: "test.png".into(),
                base64_data: Some("resolved_data".into()),
            },
            Some("image/png"),
        );
        let part = anthropic_media_part(&media).unwrap();
        assert_eq!(
            serde_json::to_value(&part).unwrap(),
            serde_json::json!({
                "type": "image",
                "source": {"type": "base64", "media_type": "image/png", "data": "resolved_data"}
            })
        );
    }

    #[test]
    fn anthropic_image_file_not_resolved_error() {
        let media = make_media(
            MediaKind::Image,
            MediaContent::File {
                file: "test.png".into(),
                base64_data: None,
            },
            Some("image/png"),
        );
        let result = anthropic_media_part(&media);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("test.png"));
    }

    #[test]
    fn anthropic_audio_url() {
        let media = make_media(
            MediaKind::Audio,
            MediaContent::Url {
                url: "https://example.com/audio.wav".into(),
                base64_data: None,
            },
            Some("audio/wav"),
        );
        let part = anthropic_media_part(&media).unwrap();
        assert_eq!(
            serde_json::to_value(&part).unwrap(),
            serde_json::json!({
                "type": "input_audio",
                "source": {"type": "url", "url": "https://example.com/audio.wav"}
            })
        );
    }

    #[test]
    fn anthropic_audio_base64() {
        let media = make_media(
            MediaKind::Audio,
            MediaContent::Base64 {
                base64_data: "audiodata".into(),
            },
            Some("audio/wav"),
        );
        let part = anthropic_media_part(&media).unwrap();
        assert_eq!(
            serde_json::to_value(&part).unwrap(),
            serde_json::json!({
                "type": "input_audio",
                "source": {"type": "base64", "media_type": "audio/wav", "data": "audiodata"}
            })
        );
    }

    #[test]
    fn anthropic_audio_url_with_base64_data() {
        let media = make_media(
            MediaKind::Audio,
            MediaContent::Url {
                url: "https://example.com/audio.wav".into(),
                base64_data: Some("prefetched_audio".into()),
            },
            Some("audio/wav"),
        );
        // Anthropic's content_to_media_source prefers URL when present
        let part = anthropic_media_part(&media).unwrap();
        assert_eq!(
            serde_json::to_value(&part).unwrap(),
            serde_json::json!({
                "type": "input_audio",
                "source": {"type": "url", "url": "https://example.com/audio.wav"}
            })
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
        let part = anthropic_media_part(&media).unwrap();
        assert_eq!(
            serde_json::to_value(&part).unwrap(),
            serde_json::json!({
                "type": "document",
                "source": {"type": "url", "url": "https://example.com/doc.pdf"}
            })
        );
    }

    #[test]
    fn anthropic_pdf_base64() {
        let media = make_media(
            MediaKind::Pdf,
            MediaContent::Base64 {
                base64_data: "pdfdata".into(),
            },
            Some("application/pdf"),
        );
        let part = anthropic_media_part(&media).unwrap();
        assert_eq!(
            serde_json::to_value(&part).unwrap(),
            serde_json::json!({
                "type": "document",
                "source": {"type": "base64", "media_type": "application/pdf", "data": "pdfdata"}
            })
        );
    }

    #[test]
    fn anthropic_video_unsupported() {
        let media = make_media(
            MediaKind::Video,
            MediaContent::Base64 {
                base64_data: "videodata".into(),
            },
            Some("video/mp4"),
        );
        let result = anthropic_media_part(&media);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("video"));
    }

    // ========================================================================
    // Message / body tests
    // ========================================================================

    #[test]
    fn anthropic_single_user_message() {
        let client = make_client(vec![(
            "model",
            BexExternalValue::String("claude-3-haiku-20240307".into()),
        )]);
        let prompt = msg("user", "Hello");
        let result = build_request(&client, &prompt).unwrap();
        let body: serde_json::Value = serde_json::from_str(&result.body).unwrap();
        assert_eq!(
            body,
            serde_json::json!({
                "model": "claude-3-haiku-20240307",
                "max_tokens": 4096,
                "messages": [
                    {"role": "user", "content": [{"type": "text", "text": "Hello"}]}
                ]
            })
        );
    }

    #[test]
    fn anthropic_three_role_conversation() {
        let client = make_client(vec![(
            "model",
            BexExternalValue::String("claude-3-haiku-20240307".into()),
        )]);
        let prompt = Arc::new(PromptAst::Vec(vec![
            msg("system", "You are helpful."),
            msg("user", "Hi"),
            msg("assistant", "Hello!"),
        ]));
        let result = build_request(&client, &prompt).unwrap();
        let body: serde_json::Value = serde_json::from_str(&result.body).unwrap();
        assert_eq!(
            body,
            serde_json::json!({
                "model": "claude-3-haiku-20240307",
                "max_tokens": 4096,
                "system": [{"type": "text", "text": "You are helpful."}],
                "messages": [
                    {"role": "user", "content": [{"type": "text", "text": "Hi"}]},
                    {"role": "assistant", "content": [{"type": "text", "text": "Hello!"}]}
                ]
            })
        );
    }

    #[test]
    fn anthropic_multi_turn_conversation() {
        let client = make_client(vec![(
            "model",
            BexExternalValue::String("claude-3-haiku-20240307".into()),
        )]);
        let prompt = Arc::new(PromptAst::Vec(vec![
            msg("system", "You are helpful."),
            msg("user", "Hi"),
            msg("assistant", "Hello!"),
            msg("user", "How are you?"),
            msg("assistant", "I'm well."),
        ]));
        let result = build_request(&client, &prompt).unwrap();
        let body: serde_json::Value = serde_json::from_str(&result.body).unwrap();
        assert_eq!(
            body,
            serde_json::json!({
                "model": "claude-3-haiku-20240307",
                "max_tokens": 4096,
                "system": [{"type": "text", "text": "You are helpful."}],
                "messages": [
                    {"role": "user", "content": [{"type": "text", "text": "Hi"}]},
                    {"role": "assistant", "content": [{"type": "text", "text": "Hello!"}]},
                    {"role": "user", "content": [{"type": "text", "text": "How are you?"}]},
                    {"role": "assistant", "content": [{"type": "text", "text": "I'm well."}]}
                ]
            })
        );
    }

    #[test]
    fn anthropic_metadata_merged_to_last_part() {
        let client = make_client(vec![(
            "model",
            BexExternalValue::String("claude-3-haiku-20240307".into()),
        )]);
        let prompt = msg_with_metadata(
            "user",
            "hello",
            serde_json::json!({"cache_control": {"type": "ephemeral"}}),
        );
        let result = build_request(&client, &prompt).unwrap();
        let body: serde_json::Value = serde_json::from_str(&result.body).unwrap();
        assert_eq!(
            body,
            serde_json::json!({
                "model": "claude-3-haiku-20240307",
                "max_tokens": 4096,
                "messages": [
                    {
                        "role": "user",
                        "content": [{"type": "text", "text": "hello", "cache_control": {"type": "ephemeral"}}]
                    }
                ]
            })
        );
    }

    #[test]
    fn anthropic_multiple_system_messages_combined() {
        let client = make_client(vec![(
            "model",
            BexExternalValue::String("claude-3-haiku-20240307".into()),
        )]);
        let prompt = Arc::new(PromptAst::Vec(vec![
            msg("system", "You are helpful."),
            msg("system", "Be concise."),
            msg("user", "Hi"),
        ]));
        let result = build_request(&client, &prompt).unwrap();
        let body: serde_json::Value = serde_json::from_str(&result.body).unwrap();
        assert_eq!(
            body,
            serde_json::json!({
                "model": "claude-3-haiku-20240307",
                "max_tokens": 4096,
                "system": [
                    {"type": "text", "text": "You are helpful."},
                    {"type": "text", "text": "Be concise."}
                ],
                "messages": [
                    {"role": "user", "content": [{"type": "text", "text": "Hi"}]}
                ]
            })
        );
    }

    #[test]
    fn anthropic_system_metadata_merged_to_last_part() {
        let client = make_client(vec![(
            "model",
            BexExternalValue::String("claude-3-haiku-20240307".into()),
        )]);
        let prompt = msg_with_metadata(
            "system",
            "cached prompt",
            serde_json::json!({"cache_control": {"type": "ephemeral"}}),
        );
        let result = build_request(&client, &prompt).unwrap();
        let body: serde_json::Value = serde_json::from_str(&result.body).unwrap();
        assert_eq!(
            body,
            serde_json::json!({
                "model": "claude-3-haiku-20240307",
                "max_tokens": 4096,
                "system": [
                    {"type": "text", "text": "cached prompt", "cache_control": {"type": "ephemeral"}}
                ],
                "messages": []
            })
        );
    }

    #[test]
    fn anthropic_mixed_text_and_image() {
        let client = make_client(vec![(
            "model",
            BexExternalValue::String("claude-3-haiku-20240307".into()),
        )]);
        let media = make_media(
            MediaKind::Image,
            MediaContent::Url {
                url: "https://example.com/img.png".into(),
                base64_data: None,
            },
            Some("image/png"),
        );
        let prompt = msg_with_content(
            "user",
            PromptAstSimple::Multiple(vec![
                Arc::new(PromptAstSimple::String("Look at this:".into())),
                Arc::new(PromptAstSimple::Media(media)),
            ]),
            serde_json::Value::Null,
        );
        let result = build_request(&client, &prompt).unwrap();
        let body: serde_json::Value = serde_json::from_str(&result.body).unwrap();
        assert_eq!(
            body,
            serde_json::json!({
                "model": "claude-3-haiku-20240307",
                "max_tokens": 4096,
                "messages": [
                    {
                        "role": "user",
                        "content": [
                            {"type": "text", "text": "Look at this:"},
                            {"type": "image", "source": {"type": "url", "url": "https://example.com/img.png"}}
                        ]
                    }
                ]
            })
        );
    }

    #[test]
    fn anthropic_rejects_system_image_before_http() {
        let client = make_client(vec![(
            "model",
            BexExternalValue::String("claude-3-haiku-20240307".into()),
        )]);
        let media = make_media(
            MediaKind::Image,
            MediaContent::Url {
                url: "https://example.com/img.png".into(),
                base64_data: None,
            },
            Some("image/png"),
        );
        let prompt = msg_with_content(
            "system",
            PromptAstSimple::Media(media),
            serde_json::Value::Null,
        );

        let err = build_request(&client, &prompt).unwrap_err();
        assert!(
            err.to_string()
                .contains("only supports image input in user messages")
        );
    }

    // ========================================================================
    // default max_tokens tests
    // ========================================================================

    fn make_client_without_max_tokens(model: &str) -> crate::baml_std::PrimitiveClient {
        crate::baml_std::PrimitiveClient::new(
            "test".to_string(),
            "anthropic".to_string(),
            crate::baml_std::PrimitiveClientOptions {
                model: Some(model.to_string()),
                request_body: IndexMap::new(),
                base_url: Some("https://api.anthropic.com".to_string()),
                provider_options: crate::baml_std::AnthropicOptions { max_tokens: None }
                    .into_bex_external_value(),
                ..Default::default()
            },
        )
        .unwrap()
    }

    #[test]
    fn anthropic_default_max_tokens_when_not_specified() {
        let client = make_client_without_max_tokens("claude-sonnet-4-6-20260101");
        let prompt = msg("user", "Hello");
        let result = build_request(&client, &prompt).unwrap();
        let body: serde_json::Value = serde_json::from_str(&result.body).unwrap();
        assert_eq!(body["max_tokens"], serde_json::json!(8192));
    }

    #[test]
    fn anthropic_explicit_max_tokens_not_overridden() {
        let client = make_client(vec![(
            "model",
            BexExternalValue::String("claude-sonnet-4-6-20260101".into()),
        )]);
        let prompt = msg("user", "Hello");
        let result = build_request(&client, &prompt).unwrap();
        let body: serde_json::Value = serde_json::from_str(&result.body).unwrap();
        // make_client sets max_tokens: Some(4096), so it should stay 4096
        assert_eq!(body["max_tokens"], serde_json::json!(4096));
    }

    #[test]
    fn anthropic_version_header_from_provider_defaults() {
        let client = make_client(vec![(
            "model",
            BexExternalValue::String("claude-3-haiku-20240307".into()),
        )]);
        let prompt = msg("user", "hello");
        let result = build_request(&client, &prompt).unwrap();
        assert_eq!(
            result.headers.get("anthropic-version").unwrap(),
            "2023-06-01"
        );
    }
}
