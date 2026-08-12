//! `OpenAI` Chat Completions API body builder.
//!
//! Builds the JSON body for `/v1/chat/completions` endpoints used by `OpenAI`,
//! Azure `OpenAI`, Ollama, `OpenRouter`, and other OpenAI-compatible providers.

use std::sync::Arc;

use baml_base::MediaKind;
use baml_builtins2::{MediaContent, MediaValue, PromptAst, PromptAstSimple};
use serde::Serialize;

// ============================================================================
// Serde types for the Chat Completions request body
// ============================================================================

#[derive(Debug, Serialize)]
struct ChatMessage {
    role: String,
    content: Vec<ContentPart>,
    #[serde(flatten)]
    extra: serde_json::Map<String, serde_json::Value>,
}

#[derive(Debug, Serialize)]
#[serde(tag = "type")]
#[serde(rename_all = "snake_case")]
enum ContentPart {
    Text {
        text: String,
    },
    #[serde(rename = "image_url")]
    ImageUrl {
        image_url: ImageUrl,
    },
    #[serde(rename = "input_audio")]
    InputAudio {
        input_audio: InputAudio,
    },
    File {
        file: FileRef,
    },
}

#[derive(Debug, Serialize)]
struct ImageUrl {
    url: String,
}

#[derive(Debug, Serialize)]
struct InputAudio {
    data: String,
    format: String,
}

#[derive(Debug, Serialize)]
struct FileRef {
    #[serde(skip_serializing_if = "Option::is_none")]
    file_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    filename: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    file_data: Option<String>,
}

// ============================================================================
// Body builder
// ============================================================================

/// Full request body for `/v1/chat/completions`.
#[derive(Debug, Serialize)]
struct RequestBody {
    model: String,
    messages: Vec<ChatMessage>,
    #[serde(flatten)]
    extra: serde_json::Map<String, serde_json::Value>,
}

pub(crate) fn build_request(
    client: &crate::baml_std::PrimitiveClient,
    prompt: &bex_vm_types::PromptAst,
) -> Result<crate::baml_std::HttpRequest, crate::build_request::BuildRequestError> {
    let mut headers = indexmap::IndexMap::new();
    headers.insert("content-type".to_string(), "application/json".to_string());

    // Body
    let mut extra = client.extra_body.clone();

    // Azure: inject max_tokens default if not already set via request_body.
    if let Some(crate::baml_std::ProviderOptions::AzureOpenAi(azure)) = &client.provider_options {
        if let Some(mt) = azure.max_tokens {
            extra
                .entry("max_tokens")
                .or_insert(serde_json::Value::Number(mt.into()));
        }
    }

    let body = RequestBody {
        model: client.model.clone(),
        messages: prompt_to_openai_messages(prompt)?,
        extra,
    };
    let body_str = serde_json::to_string(&body)?;

    Ok(crate::baml_std::HttpRequest {
        method: "POST".to_string(),
        url: resolve_url(client),
        headers,
        body: body_str,
    })
}

fn resolve_url(client: &crate::baml_std::PrimitiveClient) -> String {
    if let Some(crate::baml_std::ProviderOptions::AzureOpenAi(azure)) = &client.provider_options {
        let base = match (
            &client.options.base_url,
            &azure.resource_name,
            &azure.deployment_id,
        ) {
            (Some(url), _, _) => url.clone(),
            (None, Some(rn), Some(did)) => {
                format!("https://{rn}.openai.azure.com/openai/deployments/{did}")
            }
            // Validated whenever PrimitiveClient is constructed.
            _ => unreachable!("azure-openai requires base_url or resource_name + deployment_id"),
        };
        return format!("{base}/chat/completions?api-version={}", azure.api_version);
    }

    format!(
        "{}/chat/completions",
        client.options.base_url.as_deref().unwrap_or_default()
    )
}

// ============================================================================
// Prompt conversion
// ============================================================================

fn prompt_to_openai_messages(
    prompt: &bex_vm_types::PromptAst,
) -> Result<Vec<ChatMessage>, crate::build_request::BuildRequestError> {
    let items = match prompt.as_ref() {
        PromptAst::Vec(items) => items.clone(),
        _ => vec![prompt.clone()],
    };

    let mut messages = Vec::new();
    for item in &items {
        if let Some(msg) = prompt_node_to_message(item)? {
            messages.push(msg);
        }
    }
    Ok(merge_adjacent_openai_messages(messages))
}

fn prompt_node_to_message(
    node: &bex_vm_types::PromptAst,
) -> Result<Option<ChatMessage>, crate::build_request::BuildRequestError> {
    match node.as_ref() {
        PromptAst::Message {
            role,
            content,
            metadata,
        } => {
            let parts = openai_content_parts(content.as_ref(), role)?;

            // Merge metadata into the last content part (e.g. cache_control).
            let extra = metadata_to_map(metadata);

            Ok(Some(ChatMessage {
                role: role.clone(),
                content: parts,
                extra,
            }))
        }
        _ => Ok(None),
    }
}

/// Merge metadata JSON object into a flat map for serde flattening.
fn metadata_to_map(metadata: &serde_json::Value) -> serde_json::Map<String, serde_json::Value> {
    match metadata {
        serde_json::Value::Object(map) => map.clone(),
        _ => serde_json::Map::new(),
    }
}

/// Merge consecutive messages with the same role into a single message.
fn merge_adjacent_openai_messages(messages: Vec<ChatMessage>) -> Vec<ChatMessage> {
    let mut merged: Vec<ChatMessage> = Vec::new();
    for msg in messages {
        if let Some(last) = merged.last_mut() {
            if last.role == msg.role && last.extra == msg.extra {
                last.content.extend(msg.content);
                continue;
            }
        }
        merged.push(msg);
    }
    merged
}

fn openai_content_parts(
    content: &PromptAstSimple,
    role: &str,
) -> Result<Vec<ContentPart>, crate::build_request::BuildRequestError> {
    match content {
        PromptAstSimple::String(s) => Ok(vec![ContentPart::Text { text: s.clone() }]),
        PromptAstSimple::Media(media) => {
            if role != "user" {
                return Err(unsupported_media_role(role, media.kind));
            }
            openai_media_part(media).map(|part| vec![part])
        }
        PromptAstSimple::Multiple(items) => {
            let mut parts = Vec::new();
            for item in items {
                parts.extend(openai_content_parts(item, role)?);
            }
            Ok(parts)
        }
    }
}

fn unsupported_media_role(role: &str, kind: MediaKind) -> crate::build_request::BuildRequestError {
    crate::build_request::BuildRequestError::UnsupportedMedia(format!(
        "OpenAI Chat Completions only supports {kind} input in user messages; found media in a {role} message"
    ))
}

fn openai_media_part(
    media: &Arc<MediaValue>,
) -> Result<ContentPart, crate::build_request::BuildRequestError> {
    let mime = crate::build_request::mime_type_as_ok(media)?;

    match media.kind {
        MediaKind::Image => media.read_content(|c| {
            let url = content_to_data_url(c, &mime)?;
            Ok(ContentPart::ImageUrl {
                image_url: ImageUrl { url },
            })
        }),
        MediaKind::Audio => media.read_content(|c| {
            let data = content_to_base64(c)?;
            let format = audio_format_from_mime(&mime)?;
            Ok(ContentPart::InputAudio {
                input_audio: InputAudio { data, format },
            })
        }),
        MediaKind::Pdf => media.read_content(|c| {
            let data_url = content_to_data_url(c, &mime)?;
            Ok(ContentPart::File {
                file: FileRef {
                    file_id: None,
                    filename: None,
                    file_data: Some(data_url),
                },
            })
        }),
        MediaKind::Video => Err(crate::build_request::BuildRequestError::UnsupportedMedia(
            "OpenAI Chat Completions does not support video content".to_string(),
        )),
        MediaKind::Generic => Err(crate::build_request::BuildRequestError::UnsupportedMedia(
            "generic media kind not supported by OpenAI Chat Completions".to_string(),
        )),
    }
}

// ============================================================================
// Media helpers
// ============================================================================

/// Convert media content to a URL string (data URL for base64, original URL otherwise).
fn content_to_data_url(
    content: &MediaContent,
    mime: &str,
) -> Result<String, crate::build_request::BuildRequestError> {
    if let Some(url) = content.url() {
        return Ok(url.to_string());
    }
    if let Some(b64) = content.base64_data() {
        return Ok(format!("data:{mime};base64,{b64}"));
    }
    Err(crate::build_request::BuildRequestError::FileNotResolved(
        content.file_path().unwrap_or("<unknown>").to_string(),
    ))
}

/// Extract base64 data from media content (for audio, which needs raw data).
fn content_to_base64(
    content: &MediaContent,
) -> Result<String, crate::build_request::BuildRequestError> {
    if let Some(b64) = content.base64_data() {
        return Ok(b64.to_string());
    }
    if let Some(url) = content.url() {
        return Err(crate::build_request::BuildRequestError::UnsupportedMedia(
            format!("audio URL not pre-fetched: {url}"),
        ));
    }
    Err(crate::build_request::BuildRequestError::FileNotResolved(
        content.file_path().unwrap_or("<unknown>").to_string(),
    ))
}

/// Derive the `OpenAI` audio format string from a MIME type.
///
/// Chat Completions only accepts `wav` and `mp3` for `input_audio.format`.
fn audio_format_from_mime(mime: &str) -> Result<String, crate::build_request::BuildRequestError> {
    match mime {
        "audio/wav" | "audio/x-wav" => Ok("wav".to_string()),
        "audio/mp3" | "audio/mpeg" => Ok("mp3".to_string()),
        other => Err(crate::build_request::BuildRequestError::UnsupportedMedia(
            format!("OpenAI Chat Completions input_audio only supports wav and mp3, got: {other}"),
        )),
    }
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

    fn msg_with_content(role: &str, content: PromptAstSimple) -> Arc<PromptAst> {
        Arc::new(PromptAst::Message {
            role: role.to_string(),
            content: Arc::new(content),
            metadata: serde_json::Value::Null,
        })
    }

    fn make_client(
        provider: &str,
        options: crate::baml_std::PrimitiveClientOptions,
    ) -> crate::baml_std::PrimitiveClient {
        crate::baml_std::PrimitiveClient::new("test".to_string(), provider.to_string(), options)
            .unwrap()
    }

    // ========================================================================
    // Media tests
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
        let part = openai_media_part(&media).unwrap();
        let json = serde_json::to_value(&part).unwrap();
        assert_eq!(
            json,
            serde_json::json!({
                "type": "image_url",
                "image_url": {"url": "https://example.com/cat.png"}
            })
        );
    }

    #[test]
    fn chat_image_base64() {
        let media = make_media(
            MediaKind::Image,
            MediaContent::Base64 {
                base64_data: "abc123".into(),
            },
            Some("image/jpeg"),
        );
        let part = openai_media_part(&media).unwrap();
        let json = serde_json::to_value(&part).unwrap();
        assert_eq!(
            json,
            serde_json::json!({
                "type": "image_url",
                "image_url": {"url": "data:image/jpeg;base64,abc123"}
            })
        );
    }

    #[test]
    fn chat_audio_base64_wav() {
        let media = make_media(
            MediaKind::Audio,
            MediaContent::Base64 {
                base64_data: "audiodata".into(),
            },
            Some("audio/wav"),
        );
        let part = openai_media_part(&media).unwrap();
        let json = serde_json::to_value(&part).unwrap();
        assert_eq!(
            json,
            serde_json::json!({
                "type": "input_audio",
                "input_audio": {"data": "audiodata", "format": "wav"}
            })
        );
    }

    #[test]
    fn chat_audio_mpeg_becomes_mp3() {
        let media = make_media(
            MediaKind::Audio,
            MediaContent::Base64 {
                base64_data: "audiodata".into(),
            },
            Some("audio/mpeg"),
        );
        let part = openai_media_part(&media).unwrap();
        let json = serde_json::to_value(&part).unwrap();
        assert_eq!(
            json,
            serde_json::json!({
                "type": "input_audio",
                "input_audio": {"data": "audiodata", "format": "mp3"}
            })
        );
    }

    #[test]
    fn chat_audio_url_not_resolved_error() {
        let media = make_media(
            MediaKind::Audio,
            MediaContent::Url {
                url: "https://example.com/audio.wav".into(),
                base64_data: None,
            },
            Some("audio/wav"),
        );
        let result = openai_media_part(&media);
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("audio URL not pre-fetched")
        );
    }

    #[test]
    fn chat_audio_url_with_base64_data() {
        let media = make_media(
            MediaKind::Audio,
            MediaContent::Url {
                url: "https://example.com/audio.wav".into(),
                base64_data: Some("prefetched_audio".into()),
            },
            Some("audio/wav"),
        );
        let part = openai_media_part(&media).unwrap();
        let json = serde_json::to_value(&part).unwrap();
        assert_eq!(
            json,
            serde_json::json!({
                "type": "input_audio",
                "input_audio": {"data": "prefetched_audio", "format": "wav"}
            })
        );
    }

    #[test]
    fn chat_audio_file_with_base64_data() {
        let media = make_media(
            MediaKind::Audio,
            MediaContent::File {
                file: "test.wav".into(),
                base64_data: Some("resolved_audio".into()),
            },
            Some("audio/wav"),
        );
        let part = openai_media_part(&media).unwrap();
        let json = serde_json::to_value(&part).unwrap();
        assert_eq!(
            json,
            serde_json::json!({
                "type": "input_audio",
                "input_audio": {"data": "resolved_audio", "format": "wav"}
            })
        );
    }

    #[test]
    fn chat_audio_file_not_resolved_error() {
        let media = make_media(
            MediaKind::Audio,
            MediaContent::File {
                file: "test.wav".into(),
                base64_data: None,
            },
            Some("audio/wav"),
        );
        let result = openai_media_part(&media);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("test.wav"));
    }

    #[test]
    fn chat_image_file_with_base64_data() {
        let media = make_media(
            MediaKind::Image,
            MediaContent::File {
                file: "test.png".into(),
                base64_data: Some("resolved_data".into()),
            },
            Some("image/png"),
        );
        let part = openai_media_part(&media).unwrap();
        let json = serde_json::to_value(&part).unwrap();
        assert_eq!(
            json,
            serde_json::json!({
                "type": "image_url",
                "image_url": {"url": "data:image/png;base64,resolved_data"}
            })
        );
    }

    #[test]
    fn chat_image_file_not_resolved_error() {
        let media = make_media(
            MediaKind::Image,
            MediaContent::File {
                file: "test.png".into(),
                base64_data: None,
            },
            Some("image/png"),
        );
        let result = openai_media_part(&media);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("test.png"));
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
        // PDF URLs are supported (content_to_data_url returns the URL directly)
        let part = openai_media_part(&media).unwrap();
        let json = serde_json::to_value(&part).unwrap();
        assert_eq!(
            json,
            serde_json::json!({
                "type": "file",
                "file": {"file_data": "https://example.com/doc.pdf"}
            })
        );
    }

    #[test]
    fn chat_pdf_url_with_base64_data() {
        let media = make_media(
            MediaKind::Pdf,
            MediaContent::Url {
                url: "https://example.com/doc.pdf".into(),
                base64_data: Some("prefetched_pdf".into()),
            },
            Some("application/pdf"),
        );
        // When URL has base64, the URL itself is still used (content_to_data_url prefers URL)
        let part = openai_media_part(&media).unwrap();
        let json = serde_json::to_value(&part).unwrap();
        assert_eq!(
            json,
            serde_json::json!({
                "type": "file",
                "file": {"file_data": "https://example.com/doc.pdf"}
            })
        );
    }

    #[test]
    fn chat_pdf_file_with_base64_data() {
        let media = make_media(
            MediaKind::Pdf,
            MediaContent::File {
                file: "doc.pdf".into(),
                base64_data: Some("resolved_pdf".into()),
            },
            Some("application/pdf"),
        );
        let part = openai_media_part(&media).unwrap();
        let json = serde_json::to_value(&part).unwrap();
        assert_eq!(
            json,
            serde_json::json!({
                "type": "file",
                "file": {"file_data": "data:application/pdf;base64,resolved_pdf"}
            })
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
        let result = openai_media_part(&media);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("doc.pdf"));
    }

    #[test]
    fn chat_pdf_base64() {
        let media = make_media(
            MediaKind::Pdf,
            MediaContent::Base64 {
                base64_data: "pdfdata".into(),
            },
            Some("application/pdf"),
        );
        let part = openai_media_part(&media).unwrap();
        let json = serde_json::to_value(&part).unwrap();
        assert_eq!(
            json,
            serde_json::json!({
                "type": "file",
                "file": {"file_data": "data:application/pdf;base64,pdfdata"}
            })
        );
    }

    #[test]
    fn chat_video_unsupported() {
        let media = make_media(
            MediaKind::Video,
            MediaContent::Base64 {
                base64_data: "videodata".into(),
            },
            Some("video/mp4"),
        );
        let result = openai_media_part(&media);
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("does not support video")
        );
    }

    // ========================================================================
    // Message tests
    // ========================================================================

    #[test]
    fn chat_single_message() {
        let messages = prompt_to_openai_messages(&msg("user", "hello")).unwrap();
        assert_eq!(
            serde_json::to_value(&messages).unwrap(),
            serde_json::json!([
                {"role": "user", "content": [{"type": "text", "text": "hello"}]}
            ])
        );
    }

    #[test]
    fn chat_multiple_messages() {
        let prompt = Arc::new(PromptAst::Vec(vec![
            msg("system", "You are helpful."),
            msg("user", "Hello"),
        ]));
        let messages = prompt_to_openai_messages(&prompt).unwrap();
        assert_eq!(
            serde_json::to_value(&messages).unwrap(),
            serde_json::json!([
                {"role": "system", "content": [{"type": "text", "text": "You are helpful."}]},
                {"role": "user", "content": [{"type": "text", "text": "Hello"}]}
            ])
        );
    }

    #[test]
    fn chat_three_role_conversation() {
        let prompt = Arc::new(PromptAst::Vec(vec![
            msg("system", "You are helpful."),
            msg("user", "Hi"),
            msg("assistant", "Hello!"),
        ]));
        let messages = prompt_to_openai_messages(&prompt).unwrap();
        assert_eq!(
            serde_json::to_value(&messages).unwrap(),
            serde_json::json!([
                {"role": "system", "content": [{"type": "text", "text": "You are helpful."}]},
                {"role": "user", "content": [{"type": "text", "text": "Hi"}]},
                {"role": "assistant", "content": [{"type": "text", "text": "Hello!"}]}
            ])
        );
    }

    #[test]
    fn chat_multi_turn_conversation() {
        let prompt = Arc::new(PromptAst::Vec(vec![
            msg("system", "You are helpful."),
            msg("user", "Hi"),
            msg("assistant", "Hello!"),
            msg("user", "How are you?"),
            msg("assistant", "I'm well."),
        ]));
        let messages = prompt_to_openai_messages(&prompt).unwrap();
        assert_eq!(
            serde_json::to_value(&messages).unwrap(),
            serde_json::json!([
                {"role": "system", "content": [{"type": "text", "text": "You are helpful."}]},
                {"role": "user", "content": [{"type": "text", "text": "Hi"}]},
                {"role": "assistant", "content": [{"type": "text", "text": "Hello!"}]},
                {"role": "user", "content": [{"type": "text", "text": "How are you?"}]},
                {"role": "assistant", "content": [{"type": "text", "text": "I'm well."}]}
            ])
        );
    }

    #[test]
    fn chat_simple_node_uses_default_role() {
        // Two adjacent messages with the same role get merged
        let prompt = Arc::new(PromptAst::Vec(vec![
            msg("user", "part 1"),
            msg("user", "part 2"),
        ]));
        let messages = prompt_to_openai_messages(&prompt).unwrap();
        assert_eq!(
            serde_json::to_value(&messages).unwrap(),
            serde_json::json!([{
                "role": "user",
                "content": [
                    {"type": "text", "text": "part 1"},
                    {"type": "text", "text": "part 2"}
                ]
            }])
        );
    }

    #[test]
    fn chat_metadata_merged_to_last_part() {
        let prompt = msg_with_metadata(
            "user",
            "hello",
            serde_json::json!({"cache_control": {"type": "ephemeral"}}),
        );
        let messages = prompt_to_openai_messages(&prompt).unwrap();
        let json = serde_json::to_value(&messages[0]).unwrap();
        assert_eq!(
            json,
            serde_json::json!({
                "role": "user",
                "content": [{"type": "text", "text": "hello"}],
                "cache_control": {"type": "ephemeral"}
            })
        );
    }

    #[test]
    fn chat_rejects_system_image_before_http() {
        let media = make_media(
            MediaKind::Image,
            MediaContent::Url {
                url: "https://example.com/cat.png".into(),
                base64_data: None,
            },
            Some("image/png"),
        );
        let prompt = msg_with_content("system", PromptAstSimple::Media(media));

        let err = prompt_to_openai_messages(&prompt).unwrap_err();
        assert!(
            err.to_string()
                .contains("only supports image input in user messages")
        );
    }

    // ========================================================================
    // Azure tests
    // ========================================================================

    #[test]
    fn azure_defaults_max_tokens_4096() {
        let client = make_client(
            "azure-openai",
            crate::baml_std::PrimitiveClientOptions {
                model: Some("gpt-4o".to_string()),
                provider_options: crate::baml_std::AzureOpenAiOptions {
                    resource_name: Some("my-resource".to_string()),
                    deployment_id: Some("gpt-4o".to_string()),
                    api_version: "2024-02-15-preview".to_string(),
                    max_tokens: Some(4096),
                }
                .into_bex_external_value(),
                ..Default::default()
            },
        );
        let prompt = msg("user", "hello");
        let result = build_request(&client, &prompt).unwrap();
        let body: serde_json::Value = serde_json::from_str(&result.body).unwrap();
        assert_eq!(
            body,
            serde_json::json!({
                "model": "gpt-4o",
                "max_tokens": 4096,
                "messages": [
                    {"role": "user", "content": [{"type": "text", "text": "hello"}]}
                ]
            })
        );
    }

    #[test]
    fn azure_explicit_max_tokens_overrides_default() {
        let client = make_client(
            "azure-openai",
            crate::baml_std::PrimitiveClientOptions {
                model: Some("gpt-4o".to_string()),
                request_body: indexmap::IndexMap::from([(
                    "max_tokens".to_string(),
                    bex_external_types::BexExternalValue::Int(2048),
                )]),
                provider_options: crate::baml_std::AzureOpenAiOptions {
                    resource_name: Some("my-resource".to_string()),
                    deployment_id: Some("gpt-4o".to_string()),
                    api_version: "2024-02-15-preview".to_string(),
                    max_tokens: Some(4096),
                }
                .into_bex_external_value(),
                ..Default::default()
            },
        );
        let prompt = msg("user", "hello");
        let result = build_request(&client, &prompt).unwrap();
        let body: serde_json::Value = serde_json::from_str(&result.body).unwrap();
        assert_eq!(
            body,
            serde_json::json!({
                "model": "gpt-4o",
                "max_tokens": 2048,
                "messages": [
                    {"role": "user", "content": [{"type": "text", "text": "hello"}]}
                ]
            })
        );
    }

    #[test]
    fn azure_max_completion_tokens_forwarded() {
        let client = make_client(
            "azure-openai",
            crate::baml_std::PrimitiveClientOptions {
                model: Some("gpt-4o".to_string()),
                provider_options: crate::baml_std::AzureOpenAiOptions {
                    resource_name: Some("my-resource".to_string()),
                    deployment_id: Some("gpt-4o".to_string()),
                    api_version: "2024-02-15-preview".to_string(),
                    max_tokens: Some(4096),
                }
                .into_bex_external_value(),
                request_body: IndexMap::from([(
                    "max_completion_tokens".to_string(),
                    BexExternalValue::Int(2048),
                )]),
                ..Default::default()
            },
        );
        let prompt = msg("user", "hello");
        let result = build_request(&client, &prompt).unwrap();
        let body: serde_json::Value = serde_json::from_str(&result.body).unwrap();
        assert_eq!(
            body,
            serde_json::json!({
                "model": "gpt-4o",
                "max_tokens": 4096,
                "max_completion_tokens": 2048,
                "messages": [
                    {"role": "user", "content": [{"type": "text", "text": "hello"}]}
                ]
            })
        );
    }

    #[test]
    fn openai_no_default_max_tokens() {
        let client = make_client(
            "openai",
            crate::baml_std::PrimitiveClientOptions {
                model: Some("gpt-4o".to_string()),
                ..Default::default()
            },
        );
        let prompt = msg("user", "hello");
        let result = build_request(&client, &prompt).unwrap();
        let body: serde_json::Value = serde_json::from_str(&result.body).unwrap();
        assert_eq!(
            body,
            serde_json::json!({
                "model": "gpt-4o",
                "messages": [
                    {"role": "user", "content": [{"type": "text", "text": "hello"}]}
                ]
            })
        );
    }
}
