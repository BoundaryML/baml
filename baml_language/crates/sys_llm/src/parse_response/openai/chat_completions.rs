use std::sync::Arc;

use baml_builtins2::MediaValue;
use serde::{Deserialize, Deserializer};

use super::CompletionUsage;
use crate::parse_response::{FinishReason, LlmProviderResponse, ParseResponseError, TokenUsage};

// == Serde types ===================================================

#[derive(Debug, Deserialize, Clone, PartialEq)]
struct ChatCompletionResponse {
    pub id: Option<String>,
    pub choices: Vec<ChatCompletionChoice>,
    #[serde(default, deserialize_with = "deserialize_float_to_u32")]
    pub created: Option<u32>,
    pub model: String,
    pub system_fingerprint: Option<String>,
    pub object: Option<String>,
    pub usage: Option<CompletionUsage>,
}

fn deserialize_float_to_u32<'de, D>(deserializer: D) -> Result<Option<u32>, D::Error>
where
    D: Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum FloatOrInt {
        Int(u64),
        Float(f64),
    }

    match Option::<FloatOrInt>::deserialize(deserializer)? {
        #[allow(clippy::cast_possible_truncation)]
        Some(FloatOrInt::Int(i)) => Ok(Some(i.min(u64::from(u32::MAX)) as u32)),
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        Some(FloatOrInt::Float(f)) => Ok(Some(f.clamp(0.0, f64::from(u32::MAX)).floor() as u32)),
        None => Ok(None),
    }
}

#[derive(Debug, Deserialize, Clone, PartialEq)]
struct ChatCompletionChoice {
    pub index: u32,
    pub message: ChatCompletionResponseMessage,
    pub finish_reason: Option<String>,
    pub logprobs: Option<ChatChoiceLogprobs>,
}

#[derive(Debug, Deserialize, Clone, PartialEq)]
struct ChatCompletionResponseMessage {
    pub content: Option<ChatMessageContent>,
    #[serde(default)]
    pub images: Vec<ChatMessageImage>,
    pub role: ChatCompletionMessageRole,
}

#[derive(Debug, Deserialize, Clone, PartialEq)]
#[serde(untagged)]
enum ChatMessageContent {
    Text(String),
    Parts(Vec<ChatMessageContentPart>),
}

#[derive(Debug, Deserialize, Clone, PartialEq)]
struct ChatMessageContentPart {
    #[serde(rename = "type")]
    pub part_type: Option<String>,
    pub text: Option<String>,
    #[serde(rename = "image_url", alias = "imageUrl")]
    pub image_url: Option<ImageUrlRef>,
}

#[derive(Debug, Deserialize, Clone, PartialEq)]
struct ChatMessageImage {
    #[serde(rename = "type")]
    pub image_type: Option<String>,
    #[serde(rename = "image_url", alias = "imageUrl")]
    pub image_url: Option<ImageUrlRef>,
    pub url: Option<String>,
}

#[derive(Debug, Deserialize, Clone, PartialEq)]
#[serde(untagged)]
enum ImageUrlRef {
    Object { url: String },
    String(String),
}

impl ImageUrlRef {
    fn url(&self) -> &str {
        match self {
            Self::Object { url } | Self::String(url) => url,
        }
    }
}

#[derive(Debug, Deserialize, Clone, Default, PartialEq)]
#[serde(rename_all = "lowercase")]
enum ChatCompletionMessageRole {
    System,
    #[default]
    User,
    Assistant,
    Tool,
    Function,
}

#[derive(Debug, Deserialize, Clone, PartialEq)]
struct ChatChoiceLogprobs {
    pub content: Option<Vec<ChatCompletionTokenLogprob>>,
}

#[derive(Debug, Deserialize, Clone, PartialEq)]
struct ChatCompletionTokenLogprob {
    pub token: String,
    pub logprob: f32,
    pub bytes: Option<Vec<u8>>,
    pub top_logprobs: Vec<TopLogprobs>,
}

#[derive(Debug, Deserialize, Clone, PartialEq)]
struct TopLogprobs {
    pub token: String,
    pub logprob: f32,
    pub bytes: Option<Vec<u8>>,
}

// == Parser ========================================================

/// Parse an OpenAI-compatible chat completion response body into a normalized `LlmProviderResponse`.
pub(in crate::parse_response) fn parse_openai_response(
    body: &str,
) -> Result<LlmProviderResponse, ParseResponseError> {
    let response: ChatCompletionResponse =
        serde_json::from_str(body).map_err(|e| ParseResponseError::Deserialize {
            provider: "openai",
            source: e,
            content: body.to_string(),
        })?;

    if response.choices.is_empty() {
        return Err(ParseResponseError::NoContent {
            provider: "openai",
            detail: "response has no choices".into(),
        });
    }

    if response.choices.len() > 1 {
        return Err(ParseResponseError::UnsupportedResponseFormat {
            provider: "openai",
            detail: format!(
                "response contains {} choices but we can only parse a single choice; \
                 dropping {} choice(s) would lose data",
                response.choices.len(),
                response.choices.len() - 1
            ),
        });
    }

    let choice = &response.choices[0];

    let mut output = crate::parse_response::LlmOutput::default();
    push_chat_message_content(&mut output, choice.message.content.as_ref());
    push_chat_message_images(&mut output, &choice.message.images);
    let content = output.text_content();

    let finish_reason = match choice.finish_reason.as_deref() {
        Some("stop") => FinishReason::Stop,
        Some("length") => FinishReason::Length,
        Some("tool_calls") => FinishReason::ToolUse,
        Some(other) => FinishReason::Other(other.to_string()),
        None => FinishReason::Unknown,
    };

    let usage = response
        .usage
        .as_ref()
        .map(|u| TokenUsage {
            input_tokens: Some(u.prompt_tokens),
            output_tokens: Some(u.completion_tokens),
            total_tokens: Some(u.total_tokens),
            cached_input_tokens: u
                .input_tokens_details
                .as_ref()
                .and_then(|details| details.get("cached_tokens"))
                .and_then(serde_json::Value::as_u64),
        })
        .unwrap_or_default();

    Ok(LlmProviderResponse {
        output,
        content,
        model: Some(response.model),
        finish_reason,
        finish_reason_raw: choice.finish_reason.clone(),
        usage,
    })
}

fn push_chat_message_content(
    output: &mut crate::parse_response::LlmOutput,
    content: Option<&ChatMessageContent>,
) {
    match content {
        Some(ChatMessageContent::Text(text)) => output.push_text(text.clone()),
        Some(ChatMessageContent::Parts(parts)) => {
            for part in parts {
                if let Some(text) = &part.text {
                    output.push_text(text.clone());
                }
                if is_image_part_type(part.part_type.as_deref()) {
                    if let Some(image_url) = &part.image_url {
                        output.push_media(
                            media_from_image_url(image_url.url()),
                            None,
                            serde_json::json!({
                                "provider": "openai-chat",
                                "source": "message.content"
                            }),
                        );
                    }
                }
            }
        }
        None => {}
    }
}

fn push_chat_message_images(
    output: &mut crate::parse_response::LlmOutput,
    images: &[ChatMessageImage],
) {
    for image in images {
        if !is_image_part_type(image.image_type.as_deref()) {
            continue;
        }

        let url = image
            .image_url
            .as_ref()
            .map(ImageUrlRef::url)
            .or(image.url.as_deref());
        let Some(url) = url else {
            continue;
        };

        output.push_media(
            media_from_image_url(url),
            None,
            serde_json::json!({
                "provider": "openai-chat",
                "source": "message.images"
            }),
        );
    }
}

fn is_image_part_type(part_type: Option<&str>) -> bool {
    matches!(
        part_type,
        None | Some("image" | "image_url" | "output_image")
    )
}

fn media_from_image_url(url: &str) -> Arc<MediaValue> {
    if let Some((mime_type, base64_data)) = parse_image_data_url(url) {
        return MediaValue::from_base64(baml_base::MediaKind::Image, base64_data, Some(mime_type));
    }

    let mime_type = image_mime_type_from_url(url);
    MediaValue::from_url(baml_base::MediaKind::Image, url, mime_type.as_deref())
}

fn parse_image_data_url(url: &str) -> Option<(&str, &str)> {
    let rest = url.strip_prefix("data:")?;
    let (meta, data) = rest.split_once(',')?;
    if !meta.ends_with(";base64") {
        return None;
    }
    let mime_type = meta.strip_suffix(";base64")?.split(';').next()?.trim();
    if !mime_type.starts_with("image/") || data.is_empty() {
        return None;
    }
    Some((mime_type, data))
}

fn image_mime_type_from_url(url: &str) -> Option<String> {
    let path = url.split(['?', '#']).next().unwrap_or(url);
    let ext = path.rsplit('.').next()?.to_ascii_lowercase();
    match ext.as_str() {
        "png" => Some("image/png".to_string()),
        "jpg" | "jpeg" => Some("image/jpeg".to_string()),
        "webp" => Some("image/webp".to_string()),
        "gif" => Some("image/gif".to_string()),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{LlmProvider, parse_response::parse_response};

    #[test]
    fn test_parse_basic_response() {
        let body = r#"{
            "id": "chatcmpl-123",
            "object": "chat.completion",
            "created": 1677652288,
            "model": "gpt-4o",
            "system_fingerprint": "fp_44709d6fcb",
            "choices": [{
                "index": 0,
                "message": {
                    "role": "assistant",
                    "content": "Hello! How can I help you today?"
                },
                "logprobs": null,
                "finish_reason": "stop"
            }],
            "usage": {
                "prompt_tokens": 9,
                "completion_tokens": 12,
                "total_tokens": 21
            }
        }"#;

        let resp = parse_openai_response(body).unwrap();
        assert_eq!(resp.content, "Hello! How can I help you today?");
        assert_eq!(resp.model.as_deref(), Some("gpt-4o"));
        assert_eq!(resp.finish_reason, FinishReason::Stop);
        assert!(resp.finish_reason.is_complete());
        assert_eq!(resp.usage.input_tokens, Some(9));
        assert_eq!(resp.usage.output_tokens, Some(12));
        assert_eq!(resp.usage.total_tokens, Some(21));
        assert_eq!(resp.usage.cached_input_tokens, None);
    }

    #[test]
    fn test_parse_minimal_response() {
        let body = r#"{
            "model": "basic-model",
            "choices": [{
                "index": 0,
                "message": {
                    "role": "assistant",
                    "content": "Minimal"
                }
            }]
        }"#;

        let resp = parse_openai_response(body).unwrap();
        assert_eq!(resp.content, "Minimal");
        assert_eq!(resp.model.as_deref(), Some("basic-model"));
        assert_eq!(resp.finish_reason, FinishReason::Unknown);
        assert!(!resp.finish_reason.is_complete());
        assert_eq!(resp.usage.input_tokens, None);
        assert_eq!(resp.usage.output_tokens, None);
        assert_eq!(resp.usage.total_tokens, None);
        assert_eq!(resp.usage.cached_input_tokens, None);
    }

    #[test]
    fn test_parse_null_content() {
        let body = r#"{
            "model": "gpt-4o",
            "choices": [{
                "index": 0,
                "message": {
                    "role": "assistant",
                    "content": null
                },
                "finish_reason": "tool_calls"
            }]
        }"#;

        let resp = parse_openai_response(body).unwrap();
        assert_eq!(resp.content, "");
        assert_eq!(resp.finish_reason, FinishReason::ToolUse);
    }

    #[test]
    fn test_parse_openai_compatible_message_images() {
        let body = r#"{
            "model": "google/gemini-2.5-flash-image-preview",
            "choices": [{
                "index": 0,
                "message": {
                    "role": "assistant",
                    "content": "I've generated an image.",
                    "images": [{
                        "type": "image_url",
                        "image_url": {
                            "url": "data:image/png;base64,aW1hZ2U="
                        }
                    }]
                },
                "finish_reason": "stop"
            }],
            "usage": {
                "prompt_tokens": 10,
                "completion_tokens": 12,
                "total_tokens": 22
            }
        }"#;

        let resp = parse_openai_response(body).unwrap();
        assert_eq!(resp.content, "I've generated an image.");
        assert_eq!(resp.output.parts.len(), 2);

        let crate::parse_response::LlmOutputPart::Text { text } = &resp.output.parts[0] else {
            panic!("expected text output");
        };
        assert_eq!(text, "I've generated an image.");

        let crate::parse_response::LlmOutputPart::Media {
            media, metadata, ..
        } = &resp.output.parts[1]
        else {
            panic!("expected image output");
        };
        assert_eq!(media.kind, baml_base::MediaKind::Image);
        assert_eq!(media.base64(), "aW1hZ2U=");
        assert_eq!(media.mime_type().as_deref(), Some("image/png"));
        assert_eq!(metadata["source"], "message.images");
    }

    #[test]
    fn test_parse_openai_compatible_content_array_images() {
        let body = r#"{
            "model": "image-model",
            "choices": [{
                "index": 0,
                "message": {
                    "role": "assistant",
                    "content": [
                        {"type": "text", "text": "caption"},
                        {
                            "type": "image_url",
                            "image_url": {
                                "url": "https://example.com/generated.webp"
                            }
                        }
                    ]
                },
                "finish_reason": "stop"
            }]
        }"#;

        let resp = parse_openai_response(body).unwrap();
        assert_eq!(resp.content, "caption");
        assert_eq!(resp.output.parts.len(), 2);

        let crate::parse_response::LlmOutputPart::Media {
            media, metadata, ..
        } = &resp.output.parts[1]
        else {
            panic!("expected image output");
        };
        assert_eq!(
            media.url().as_deref(),
            Some("https://example.com/generated.webp")
        );
        assert_eq!(media.mime_type().as_deref(), Some("image/webp"));
        assert_eq!(metadata["source"], "message.content");
    }

    #[test]
    fn test_parse_length_finish_reason() {
        let body = r#"{
            "model": "gpt-4o",
            "choices": [{
                "index": 0,
                "message": {
                    "role": "assistant",
                    "content": "Truncated..."
                },
                "finish_reason": "length"
            }]
        }"#;

        let resp = parse_openai_response(body).unwrap();
        assert_eq!(resp.finish_reason, FinishReason::Length);
        assert!(!resp.finish_reason.is_complete());
    }

    #[test]
    fn test_parse_no_choices() {
        let body = r#"{
            "model": "gpt-4o",
            "choices": []
        }"#;

        let err = parse_openai_response(body).unwrap_err();
        assert!(matches!(err, ParseResponseError::NoContent { .. }));
    }

    #[test]
    fn test_parse_multiple_choices() {
        let body = r#"{
            "model": "gpt-4o",
            "choices": [
                {
                    "index": 0,
                    "message": { "role": "assistant", "content": "Answer A" },
                    "finish_reason": "stop"
                },
                {
                    "index": 1,
                    "message": { "role": "assistant", "content": "Answer B" },
                    "finish_reason": "stop"
                }
            ]
        }"#;

        let err = parse_openai_response(body).unwrap_err();
        assert!(matches!(
            err,
            ParseResponseError::UnsupportedResponseFormat { .. }
        ));
        let msg = err.to_string();
        assert!(msg.contains("2 choices"), "error message: {msg}");
    }

    #[test]
    fn test_parse_invalid_json() {
        let err = parse_openai_response("not json").unwrap_err();
        assert!(matches!(err, ParseResponseError::Deserialize { .. }));
    }

    #[test]
    fn test_azure_and_openai_produce_same_result() {
        let body = r#"{
            "id": "chatcmpl-abc",
            "object": "chat.completion",
            "created": 1677652288,
            "model": "gpt-35-turbo",
            "choices": [{
                "index": 0,
                "message": {
                    "role": "assistant",
                    "content": "Hello from Azure!"
                },
                "finish_reason": "stop"
            }],
            "usage": {
                "prompt_tokens": 10,
                "completion_tokens": 5,
                "total_tokens": 15
            }
        }"#;

        let openai_resp = parse_response(LlmProvider::OpenAi, body).unwrap();
        let azure_resp = parse_response(LlmProvider::AzureOpenAi, body).unwrap();

        assert_eq!(openai_resp.content, azure_resp.content);
        assert_eq!(openai_resp.model, azure_resp.model);
        assert_eq!(openai_resp.finish_reason, azure_resp.finish_reason);
    }

    #[test]
    fn test_parse_cached_tokens() {
        let body = r#"{
            "model": "gpt-4o",
            "choices": [{
                "index": 0,
                "message": { "role": "assistant", "content": "hi" },
                "finish_reason": "stop"
            }],
            "usage": {
                "prompt_tokens": 100,
                "completion_tokens": 10,
                "total_tokens": 110,
                "prompt_tokens_details": { "cached_tokens": 50 }
            }
        }"#;

        let resp = parse_openai_response(body).unwrap();
        assert_eq!(resp.usage.cached_input_tokens, Some(50));
    }

    #[test]
    fn test_parse_no_cached_tokens() {
        let body = r#"{
            "model": "gpt-4o",
            "choices": [{
                "index": 0,
                "message": { "role": "assistant", "content": "hi" },
                "finish_reason": "stop"
            }],
            "usage": {
                "prompt_tokens": 100,
                "completion_tokens": 10,
                "total_tokens": 110
            }
        }"#;

        let resp = parse_openai_response(body).unwrap();
        assert_eq!(resp.usage.cached_input_tokens, None);
    }
}
