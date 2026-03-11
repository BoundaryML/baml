//! `OpenAI` Responses API request builder (`/v1/responses`).
//!
//! Uses `"input"` instead of `"messages"` and different content part types:
//! `input_text`, `output_text`, `input_image`, `input_audio`, `input_file`.

use baml_builtins::{PromptAst, PromptAstSimple};
use indexmap::IndexMap;
use serde::Serialize;

use crate::build_request::{
    BuildRequestError, LlmPrimitiveClient, LlmProvider, LlmRequestBuilder, get_string_option,
    mime_type_as_ok, openai::build_openai_url,
};

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

/// Base64-encoded audio data with its format (e.g. "mp3", "wav").
#[derive(Debug, Serialize)]
struct InputAudio {
    data: String,
    format: String,
}

impl<'a> OpenAiResponsesBuilder<'a> {
    pub(crate) fn new(provider: &'a LlmProvider) -> Self {
        Self { provider }
    }
}

/// Builder for the `OpenAI` Responses API (`/v1/responses`).
pub(crate) struct OpenAiResponsesBuilder<'a> {
    provider: &'a LlmProvider,
}

impl LlmRequestBuilder for OpenAiResponsesBuilder<'_> {
    fn provider_skip_keys(&self) -> &'static [&'static str] {
        &[]
    }

    fn build_url(&self, client: &LlmPrimitiveClient) -> Result<String, BuildRequestError> {
        build_openai_url(*self.provider, client, "/responses")
    }

    fn build_auth_headers(&self, client: &LlmPrimitiveClient) -> IndexMap<String, String> {
        let mut headers = IndexMap::new();
        if let Some(api_key) = get_string_option(client, "api_key") {
            headers.insert("authorization".to_string(), format!("Bearer {api_key}"));
        }
        headers
    }

    fn build_body(
        &self,
        client: &LlmPrimitiveClient,
        prompt: bex_vm_types::PromptAst,
        stream: bool,
    ) -> Result<String, BuildRequestError> {
        let mut body = serde_json::Map::new();
        if let Some(model) = get_string_option(client, "model") {
            body.insert("model".to_string(), serde_json::Value::String(model));
        }
        if stream {
            body.insert("stream".to_string(), serde_json::Value::Bool(true));
        }
        body.extend(self.build_prompt_body(client, prompt)?);
        self.forward_options(client, &mut body);
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
    let raw = match prompt.as_ref() {
        PromptAst::Vec(items) => items
            .iter()
            .map(|node| responses_node_to_message(node, default_role))
            .collect::<Result<Vec<_>, _>>()?,
        _ => vec![responses_node_to_message(prompt, default_role)?],
    };

    // Merge adjacent messages with the same role by combining their content arrays.
    let mut result: Vec<ResponsesMessage> = Vec::with_capacity(raw.len());
    for msg in raw {
        if let Some(last) = result.last_mut() {
            if last.role == msg.role {
                last.content.extend(msg.content);
                continue;
            }
        }
        result.push(msg);
    }
    Ok(result)
}

/// Converts a single [`PromptAst`] node into a Responses API input message.
/// Should only be called by `prompt_to_responses_input`, or any other function that ensures that `PromptAst::Vec` has been flattened.
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
        PromptAstSimple::Media(media) => {
            if role == "assistant" {
                return Err(BuildRequestError::UnsupportedMedia(
                    "assistant messages must be text; media not supported for assistant in Responses API".into(),
                ));
            }
            media.read_content(|c| responses_media_part(media, c))
        }
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
        MediaKind::Image => {
            let image_url = match content {
                MediaContent::Url { url, .. } => url.clone(),
                MediaContent::Base64 { base64_data, .. }
                | MediaContent::File {
                    base64_data: Some(base64_data),
                    ..
                } => {
                    format!("data:{};base64,{}", mime_type_as_ok(media)?, base64_data)
                }
                MediaContent::File {
                    base64_data: None, ..
                } => {
                    return Err(BuildRequestError::FileNotResolved(
                        "image file content was not resolved properly".into(),
                    ));
                }
            };
            Ok(vec![ResponsesContentPart::InputImage {
                detail: Some("auto".to_string()),
                image_url,
            }])
        }
        MediaKind::Audio => match content {
            MediaContent::Base64 { base64_data, .. } => {
                let mime = mime_type_as_ok(media)?;
                let format = mime.strip_prefix("audio/").unwrap_or(mime);
                Ok(vec![ResponsesContentPart::InputAudio {
                    input_audio: InputAudio {
                        data: base64_data.clone(),
                        format: format.to_string(),
                    },
                }])
            }
            _ => Err(BuildRequestError::UnsupportedMedia(
                "audio must be base64 encoded for Responses API".into(),
            )),
        },
        MediaKind::Pdf => match content {
            MediaContent::Url { url, .. } => Ok(vec![ResponsesContentPart::InputFile {
                file_url: Some(url.clone()),
                filename: Some("document.pdf".to_string()),
                file_data: None,
                file_id: None,
            }]),
            MediaContent::Base64 { base64_data, .. }
            | MediaContent::File {
                base64_data: Some(base64_data),
                ..
            } => {
                let data_url = format!("data:{};base64,{}", mime_type_as_ok(media)?, base64_data);
                Ok(vec![ResponsesContentPart::InputFile {
                    file_data: Some(data_url),
                    filename: Some("document.pdf".to_string()),
                    file_url: None,
                    file_id: None,
                }])
            }
            MediaContent::File {
                base64_data: None, ..
            } => Err(BuildRequestError::FileNotResolved(
                "pdf file content was not resolved properly".into(),
            )),
        },
        MediaKind::Video => Err(BuildRequestError::UnsupportedMedia(
            "video input is not supported on OpenAI Responses API".into(),
        )),
        MediaKind::Generic => Err(BuildRequestError::UnsupportedMedia(
            "generic media is currently unimplemented".into(),
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
    use crate::build_request::{LlmPrimitiveClient, LlmRequestBuilder};

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
        assert_eq!(
            json,
            serde_json::json!({"type": "input_image", "detail": "auto", "image_url": "data:image/png;base64,iVBORw0KGgo="})
        );
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
        assert_eq!(
            json,
            serde_json::json!({"type": "input_file", "file_data": "data:application/pdf;base64,JVBERi0=", "filename": "document.pdf"})
        );
    }

    #[test]
    fn responses_image_file_with_base64_data() {
        let media = make_media(
            MediaKind::Image,
            MediaContent::File {
                file: "cat.png".into(),
                base64_data: Some("iVBORw0KGgo=".into()),
            },
            Some("image/png"),
        );
        let parts = media
            .read_content(|c| responses_media_part(&media, c))
            .unwrap();
        let json = serde_json::to_value(&parts[0]).unwrap();
        assert_eq!(
            json,
            serde_json::json!({"type": "input_image", "detail": "auto", "image_url": "data:image/png;base64,iVBORw0KGgo="})
        );
    }

    #[test]
    fn responses_image_file_not_resolved_error() {
        let media = make_media(
            MediaKind::Image,
            MediaContent::File {
                file: "cat.png".into(),
                base64_data: None,
            },
            Some("image/png"),
        );
        let err = media
            .read_content(|c| responses_media_part(&media, c))
            .unwrap_err();
        assert!(matches!(err, BuildRequestError::FileNotResolved(_)));
    }

    #[test]
    fn responses_pdf_file_with_base64_data() {
        let media = make_media(
            MediaKind::Pdf,
            MediaContent::File {
                file: "doc.pdf".into(),
                base64_data: Some("JVBERi0=".into()),
            },
            Some("application/pdf"),
        );
        let parts = media
            .read_content(|c| responses_media_part(&media, c))
            .unwrap();
        let json = serde_json::to_value(&parts[0]).unwrap();
        assert_eq!(
            json,
            serde_json::json!({"type": "input_file", "file_data": "data:application/pdf;base64,JVBERi0=", "filename": "document.pdf"})
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
        let err = media
            .read_content(|c| responses_media_part(&media, c))
            .unwrap_err();
        assert!(matches!(err, BuildRequestError::FileNotResolved(_)));
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
        assert_eq!(
            json,
            serde_json::json!({"type": "output_text", "text": "hi"})
        );
    }

    #[test]
    fn responses_user_uses_input_text() {
        let parts =
            responses_content_parts(&baml_builtins::PromptAstSimple::String("hi".into()), "user")
                .unwrap();
        let json = serde_json::to_value(&parts[0]).unwrap();
        assert_eq!(
            json,
            serde_json::json!({"type": "input_text", "text": "hi"})
        );
    }

    #[test]
    fn responses_three_role_conversation() {
        let prompt = Arc::new(PromptAst::Vec(vec![
            msg("system", "You are a helpful assistant."),
            msg("user", "What is 2+2?"),
            msg("assistant", "4"),
        ]));
        let messages = prompt_to_responses_input(&prompt, "user").unwrap();
        assert_eq!(
            serde_json::to_value(&messages).unwrap(),
            serde_json::json!([
                {
                    "role": "system",
                    "content": [{"type": "input_text", "text": "You are a helpful assistant."}]
                },
                {
                    "role": "user",
                    "content": [{"type": "input_text", "text": "What is 2+2?"}]
                },
                {
                    "role": "assistant",
                    "content": [{"type": "output_text", "text": "4"}]
                }
            ])
        );
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
        assert_eq!(
            serde_json::to_value(&messages).unwrap(),
            serde_json::json!([
                {
                    "role": "system",
                    "content": [{"type": "input_text", "text": "Be concise."}]
                },
                {
                    "role": "user",
                    "content": [{"type": "input_text", "text": "Hello"}]
                },
                {
                    "role": "assistant",
                    "content": [{"type": "output_text", "text": "Hi!"}]
                },
                {
                    "role": "user",
                    "content": [{"type": "input_text", "text": "How are you?"}]
                },
                {
                    "role": "assistant",
                    "content": [{"type": "output_text", "text": "Good, thanks!"}]
                },
                {
                    "role": "user",
                    "content": [{"type": "input_text", "text": "Goodbye"}]
                }
            ])
        );
    }

    #[test]
    fn responses_url_default() {
        let client = make_client("openai-responses", vec![]);
        let url = OpenAiResponsesBuilder::new(&LlmProvider::OpenAiResponses)
            .build_url(&client)
            .unwrap();
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
        let url = OpenAiResponsesBuilder::new(&LlmProvider::OpenAiResponses)
            .build_url(&client)
            .unwrap();
        assert_eq!(url, "https://custom.api.com/v1/responses");
    }

    #[test]
    fn stream_true_sets_stream() {
        let client = make_client(
            "openai-responses",
            vec![("model", BexExternalValue::String("gpt-4o".into()))],
        );
        let builder = OpenAiResponsesBuilder::new(&LlmProvider::OpenAiResponses);
        let body_str = builder
            .build_body(&client, msg("user", "hi"), true)
            .unwrap();
        let body: serde_json::Value = serde_json::from_str(&body_str).unwrap();
        assert_eq!(
            body,
            serde_json::json!({
                "model": "gpt-4o",
                "stream": true,
                "input": [
                    {"role": "user", "content": [{"type": "input_text", "text": "hi"}]}
                ]
            })
        );
    }

    #[test]
    fn stream_false_omits_stream() {
        let client = make_client(
            "openai-responses",
            vec![("model", BexExternalValue::String("gpt-4o".into()))],
        );
        let builder = OpenAiResponsesBuilder::new(&LlmProvider::OpenAiResponses);
        let body_str = builder
            .build_body(&client, msg("user", "hi"), false)
            .unwrap();
        let body: serde_json::Value = serde_json::from_str(&body_str).unwrap();
        assert_eq!(
            body,
            serde_json::json!({
                "model": "gpt-4o",
                "input": [
                    {"role": "user", "content": [{"type": "input_text", "text": "hi"}]}
                ]
            })
        );
    }
}
