//! `OpenAI` Responses API body builder.
//!
//! Builds the JSON body for the `/v1/responses` endpoint.

use std::sync::Arc;

use baml_base::MediaKind;
use baml_builtins2::{MediaContent, MediaValue, PromptAst, PromptAstSimple};
use serde::Serialize;

// ============================================================================
// Serde types for the Responses API request body
// ============================================================================

#[derive(Debug, Serialize)]
struct ResponsesMessage {
    role: String,
    content: Vec<ResponsesContentPart>,
    #[serde(flatten)]
    extra: serde_json::Map<String, serde_json::Value>,
}

#[derive(Debug, Serialize)]
#[serde(tag = "type")]
#[serde(rename_all = "snake_case")]
enum ResponsesContentPart {
    /// Text from a user or system message.
    #[serde(rename = "input_text")]
    InputText { text: String },
    /// Text from an assistant message.
    #[serde(rename = "output_text")]
    OutputText { text: String },
    /// Image input.
    #[serde(rename = "input_image")]
    InputImage {
        #[serde(skip_serializing_if = "Option::is_none")]
        image_url: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        file_id: Option<String>,
    },
    /// Audio input.
    #[serde(rename = "input_audio")]
    InputAudio { data: String, format: String },
    /// File input (PDF, etc.).
    #[serde(rename = "input_file")]
    InputFile {
        #[serde(skip_serializing_if = "Option::is_none")]
        file_id: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        filename: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        file_data: Option<String>,
    },
}

// ============================================================================
// Body builder
// ============================================================================

/// Full request body for `/v1/responses`.
#[derive(Debug, Serialize)]
struct RequestBody {
    model: String,
    input: Vec<ResponsesMessage>,
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
    let body = RequestBody {
        model: client.model.clone(),
        input: prompt_to_responses_input(prompt)?,
        extra: client.extra_body.clone(),
    };
    let body_str = serde_json::to_string(&body)?;

    Ok(crate::baml_std::HttpRequest {
        method: "POST".to_string(),
        url: format!(
            "{}/responses",
            client.options.base_url.as_deref().unwrap_or_default()
        ),
        headers,
        body: body_str,
    })
}

// ============================================================================
// Prompt conversion
// ============================================================================

fn prompt_to_responses_input(
    prompt: &bex_vm_types::PromptAst,
) -> Result<Vec<ResponsesMessage>, crate::build_request::BuildRequestError> {
    let items = match prompt.as_ref() {
        PromptAst::Vec(items) => items.clone(),
        _ => vec![prompt.clone()],
    };

    let mut messages = Vec::new();
    for item in &items {
        if let Some(msg) = responses_node_to_message(item)? {
            messages.push(msg);
        }
    }
    Ok(messages)
}

fn responses_node_to_message(
    node: &bex_vm_types::PromptAst,
) -> Result<Option<ResponsesMessage>, crate::build_request::BuildRequestError> {
    match node.as_ref() {
        PromptAst::Message {
            role,
            content,
            metadata,
        } => {
            let parts = responses_content_parts(content.as_ref(), role)?;
            let extra = match metadata {
                serde_json::Value::Object(map) => map.clone(),
                _ => serde_json::Map::new(),
            };

            Ok(Some(ResponsesMessage {
                role: role.clone(),
                content: parts,
                extra,
            }))
        }
        _ => Ok(None),
    }
}

fn responses_content_parts(
    content: &PromptAstSimple,
    role: &str,
) -> Result<Vec<ResponsesContentPart>, crate::build_request::BuildRequestError> {
    match content {
        PromptAstSimple::String(s) => {
            if role == "assistant" {
                Ok(vec![ResponsesContentPart::OutputText { text: s.clone() }])
            } else {
                Ok(vec![ResponsesContentPart::InputText { text: s.clone() }])
            }
        }
        PromptAstSimple::Media(media) => {
            if role != "user" {
                return Err(unsupported_media_role(role, media.kind));
            }
            responses_media_part(media).map(|part| vec![part])
        }
        PromptAstSimple::Multiple(items) => {
            let mut parts = Vec::new();
            for item in items {
                parts.extend(responses_content_parts(item, role)?);
            }
            Ok(parts)
        }
    }
}

fn unsupported_media_role(role: &str, kind: MediaKind) -> crate::build_request::BuildRequestError {
    crate::build_request::BuildRequestError::UnsupportedMedia(format!(
        "OpenAI Responses API only supports {kind} input in user messages; found media in a {role} message"
    ))
}

fn responses_media_part(
    media: &Arc<MediaValue>,
) -> Result<ResponsesContentPart, crate::build_request::BuildRequestError> {
    let mime = crate::build_request::mime_type_as_ok(media)?;

    match media.kind {
        MediaKind::Image => media.read_content(|c| {
            let url = content_to_url_or_data_url(c, &mime)?;
            Ok(ResponsesContentPart::InputImage {
                image_url: Some(url),
                file_id: None,
            })
        }),
        MediaKind::Audio => media.read_content(|c| {
            let data = content_to_base64(c)?;
            let format = audio_format_from_mime(&mime);
            Ok(ResponsesContentPart::InputAudio { data, format })
        }),
        MediaKind::Pdf => media.read_content(|c| {
            let data_url = content_to_url_or_data_url(c, &mime)?;
            Ok(ResponsesContentPart::InputFile {
                file_id: None,
                filename: None,
                file_data: Some(data_url),
            })
        }),
        MediaKind::Video => Err(crate::build_request::BuildRequestError::UnsupportedMedia(
            "OpenAI Responses API does not support video content".to_string(),
        )),
        MediaKind::Generic => Err(crate::build_request::BuildRequestError::UnsupportedMedia(
            "generic media kind not supported by OpenAI Responses API".to_string(),
        )),
    }
}

// ============================================================================
// Media helpers
// ============================================================================

fn content_to_url_or_data_url(
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

fn audio_format_from_mime(mime: &str) -> String {
    match mime {
        "audio/wav" | "audio/x-wav" => "wav".to_string(),
        "audio/mp3" | "audio/mpeg" => "mp3".to_string(),
        "audio/flac" | "audio/x-flac" => "flac".to_string(),
        "audio/ogg" => "ogg".to_string(),
        "audio/webm" => "webm".to_string(),
        other => other.to_string(),
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use baml_builtins2::{MediaContent, MediaValue, PromptAst, PromptAstSimple};

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

    fn msg_with_content(role: &str, content: PromptAstSimple) -> Arc<PromptAst> {
        Arc::new(PromptAst::Message {
            role: role.to_string(),
            content: Arc::new(content),
            metadata: serde_json::Value::Null,
        })
    }

    // ========================================================================
    // Media tests
    // ========================================================================

    #[test]
    fn responses_image_url() {
        let media = make_media(
            MediaKind::Image,
            MediaContent::Url {
                url: "https://example.com/img.png".into(),
                base64_data: None,
            },
            Some("image/png"),
        );
        let part = responses_media_part(&media).unwrap();
        let json = serde_json::to_value(&part).unwrap();
        assert_eq!(
            json,
            serde_json::json!({
                "type": "input_image",
                "image_url": "https://example.com/img.png"
            })
        );
    }

    #[test]
    fn responses_image_base64() {
        let media = make_media(
            MediaKind::Image,
            MediaContent::Base64 {
                base64_data: "abc123".into(),
            },
            Some("image/jpeg"),
        );
        let part = responses_media_part(&media).unwrap();
        let json = serde_json::to_value(&part).unwrap();
        assert_eq!(
            json,
            serde_json::json!({
                "type": "input_image",
                "image_url": "data:image/jpeg;base64,abc123"
            })
        );
    }

    #[test]
    fn responses_audio_base64() {
        let media = make_media(
            MediaKind::Audio,
            MediaContent::Base64 {
                base64_data: "audiodata".into(),
            },
            Some("audio/mp3"),
        );
        let part = responses_media_part(&media).unwrap();
        let json = serde_json::to_value(&part).unwrap();
        assert_eq!(
            json,
            serde_json::json!({
                "type": "input_audio",
                "data": "audiodata",
                "format": "mp3"
            })
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
        let part = responses_media_part(&media).unwrap();
        let json = serde_json::to_value(&part).unwrap();
        assert_eq!(
            json,
            serde_json::json!({
                "type": "input_file",
                "file_data": "https://example.com/doc.pdf"
            })
        );
    }

    #[test]
    fn responses_pdf_base64() {
        let media = make_media(
            MediaKind::Pdf,
            MediaContent::Base64 {
                base64_data: "pdfdata".into(),
            },
            Some("application/pdf"),
        );
        let part = responses_media_part(&media).unwrap();
        let json = serde_json::to_value(&part).unwrap();
        assert_eq!(
            json,
            serde_json::json!({
                "type": "input_file",
                "file_data": "data:application/pdf;base64,pdfdata"
            })
        );
    }

    #[test]
    fn responses_image_file_with_base64_data() {
        let media = make_media(
            MediaKind::Image,
            MediaContent::File {
                file: "test.png".into(),
                base64_data: Some("resolved_img".into()),
            },
            Some("image/png"),
        );
        let part = responses_media_part(&media).unwrap();
        let json = serde_json::to_value(&part).unwrap();
        assert_eq!(
            json,
            serde_json::json!({
                "type": "input_image",
                "image_url": "data:image/png;base64,resolved_img"
            })
        );
    }

    #[test]
    fn responses_image_file_not_resolved_error() {
        let media = make_media(
            MediaKind::Image,
            MediaContent::File {
                file: "test.png".into(),
                base64_data: None,
            },
            Some("image/png"),
        );
        let result = responses_media_part(&media);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("test.png"));
    }

    #[test]
    fn responses_pdf_file_with_base64_data() {
        let media = make_media(
            MediaKind::Pdf,
            MediaContent::File {
                file: "doc.pdf".into(),
                base64_data: Some("resolved_pdf".into()),
            },
            Some("application/pdf"),
        );
        let part = responses_media_part(&media).unwrap();
        let json = serde_json::to_value(&part).unwrap();
        assert_eq!(
            json,
            serde_json::json!({
                "type": "input_file",
                "file_data": "data:application/pdf;base64,resolved_pdf"
            })
        );
    }

    #[test]
    fn responses_pdf_file_not_resolved_error() {
        let media = make_media(
            MediaKind::Pdf,
            MediaContent::File {
                file: "doc.pdf".into(),
                base64_data: None,
            },
            Some("application/pdf"),
        );
        let result = responses_media_part(&media);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("doc.pdf"));
    }

    #[test]
    fn responses_video_unsupported() {
        let media = make_media(
            MediaKind::Video,
            MediaContent::Base64 {
                base64_data: "videodata".into(),
            },
            Some("video/mp4"),
        );
        let result = responses_media_part(&media);
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
    fn responses_assistant_uses_output_text() {
        let content = PromptAstSimple::String("hello".into());
        let parts = responses_content_parts(&content, "assistant").unwrap();
        let json = serde_json::to_value(&parts[0]).unwrap();
        assert_eq!(
            json,
            serde_json::json!({"type": "output_text", "text": "hello"})
        );
    }

    #[test]
    fn responses_user_uses_input_text() {
        let content = PromptAstSimple::String("hello".into());
        let parts = responses_content_parts(&content, "user").unwrap();
        let json = serde_json::to_value(&parts[0]).unwrap();
        assert_eq!(
            json,
            serde_json::json!({"type": "input_text", "text": "hello"})
        );
    }

    #[test]
    fn responses_three_role_conversation() {
        let prompt = Arc::new(PromptAst::Vec(vec![
            msg("system", "You are helpful."),
            msg("user", "Hi"),
            msg("assistant", "Hello!"),
        ]));
        let messages = prompt_to_responses_input(&prompt).unwrap();
        assert_eq!(
            serde_json::to_value(&messages).unwrap(),
            serde_json::json!([
                {"role": "system", "content": [{"type": "input_text", "text": "You are helpful."}]},
                {"role": "user", "content": [{"type": "input_text", "text": "Hi"}]},
                {"role": "assistant", "content": [{"type": "output_text", "text": "Hello!"}]}
            ])
        );
    }

    #[test]
    fn responses_multi_turn_conversation() {
        let prompt = Arc::new(PromptAst::Vec(vec![
            msg("system", "You are helpful."),
            msg("user", "Hi"),
            msg("assistant", "Hello!"),
            msg("user", "How are you?"),
            msg("assistant", "I'm well."),
        ]));
        let messages = prompt_to_responses_input(&prompt).unwrap();
        assert_eq!(
            serde_json::to_value(&messages).unwrap(),
            serde_json::json!([
                {"role": "system", "content": [{"type": "input_text", "text": "You are helpful."}]},
                {"role": "user", "content": [{"type": "input_text", "text": "Hi"}]},
                {"role": "assistant", "content": [{"type": "output_text", "text": "Hello!"}]},
                {"role": "user", "content": [{"type": "input_text", "text": "How are you?"}]},
                {"role": "assistant", "content": [{"type": "output_text", "text": "I'm well."}]}
            ])
        );
    }

    #[test]
    fn responses_rejects_system_image_before_http() {
        let media = make_media(
            MediaKind::Image,
            MediaContent::Url {
                url: "https://example.com/img.png".into(),
                base64_data: None,
            },
            Some("image/png"),
        );
        let prompt = msg_with_content("system", PromptAstSimple::Media(media));

        let err = prompt_to_responses_input(&prompt).unwrap_err();
        assert!(
            err.to_string()
                .contains("only supports image input in user messages")
        );
    }
}
