use serde::Deserialize;

use super::{FinishReason, LlmOutput, LlmProviderResponse, ParseResponseError, TokenUsage};

// == Serde types ===================================================

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
enum AnthropicMessageContent {
    Text {
        text: String,
    },
    ToolUse {
        id: Option<String>,
        input: serde_json::Value,
        name: String,
    },
    CodeExecutionToolResult {
        content: serde_json::Value,
    },
    TextEditorCodeExecutionToolResult {
        content: serde_json::Value,
    },
    RedactedThinking {
        data: String,
    },
    #[serde(other)]
    Other,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[allow(clippy::struct_field_names)]
struct AnthropicUsage {
    pub input_tokens: u64,
    pub output_tokens: u64,
    #[serde(default)]
    pub cache_creation_input_tokens: Option<u64>,
    #[serde(default)]
    pub cache_read_input_tokens: Option<u64>,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
struct AnthropicMessageResponse {
    pub id: String,
    pub role: String,
    pub r#type: String,
    pub content: Vec<AnthropicMessageContent>,
    pub model: String,
    pub stop_reason: Option<String>,
    pub stop_sequence: Option<serde_json::Value>,
    pub usage: AnthropicUsage,
}

// == Parser ========================================================

/// Parse an Anthropic message response body into a normalized `LlmProviderResponse`.
pub(super) fn parse_anthropic_response(
    body: &str,
) -> Result<LlmProviderResponse, ParseResponseError> {
    let response: AnthropicMessageResponse =
        serde_json::from_str(body).map_err(|e| ParseResponseError::Deserialize {
            provider: "anthropic",
            source: e,
            content: body.to_string(),
        })?;

    let output = extract_anthropic_output(&response.content);
    let content = output.text_content();

    let finish_reason = match response.stop_reason.as_deref() {
        Some("end_turn" | "stop_sequence") => FinishReason::Stop,
        Some("max_tokens") => FinishReason::Length,
        Some("tool_use") => FinishReason::ToolUse,
        Some(other) => FinishReason::Other(other.to_string()),
        None => FinishReason::Unknown,
    };

    let input = response.usage.input_tokens;
    let output_tokens = response.usage.output_tokens;
    let usage = TokenUsage {
        input_tokens: Some(input),
        output_tokens: Some(output_tokens),
        total_tokens: Some(input + output_tokens),
        cached_input_tokens: response.usage.cache_read_input_tokens,
    };

    Ok(LlmProviderResponse {
        output,
        content,
        model: Some(response.model),
        finish_reason,
        finish_reason_raw: response.stop_reason,
        usage,
    })
}

fn extract_anthropic_output(content: &[AnthropicMessageContent]) -> LlmOutput {
    let mut output = LlmOutput::default();
    for block in content {
        match block {
            AnthropicMessageContent::Text { text } => output.push_text(text.clone()),
            AnthropicMessageContent::CodeExecutionToolResult { content }
            | AnthropicMessageContent::TextEditorCodeExecutionToolResult { content } => {
                for file in extract_code_execution_files(content) {
                    output.push_media(
                        file.media,
                        Some(file.file_id.clone()),
                        serde_json::json!({
                            "provider": "anthropic",
                            "file_id": file.file_id,
                            "filename": file.filename,
                            "mime_type": file.mime_type,
                        }),
                    );
                }
            }
            _ => {}
        }
    }
    output
}

struct AnthropicOutputFile {
    file_id: String,
    filename: Option<String>,
    mime_type: Option<String>,
    media: std::sync::Arc<baml_builtins2::MediaValue>,
}

fn extract_code_execution_files(content: &serde_json::Value) -> Vec<AnthropicOutputFile> {
    let Some(files) = content.get("content").and_then(serde_json::Value::as_array) else {
        return Vec::new();
    };

    files
        .iter()
        .filter_map(|file| {
            let file_id = file.get("file_id")?.as_str()?.to_string();
            let filename = file
                .get("filename")
                .and_then(serde_json::Value::as_str)
                .map(ToString::to_string);
            let mime_type = file
                .get("mime_type")
                .and_then(serde_json::Value::as_str)
                .map(ToString::to_string)
                .or_else(|| image_mime_type_from_filename(filename.as_deref()));

            if !is_image_mime_type(mime_type.as_deref()) {
                return None;
            }

            let url = format!("anthropic://files/{file_id}");
            let media = baml_builtins2::MediaValue::from_url(
                baml_base::MediaKind::Image,
                &url,
                mime_type.as_deref(),
            );

            Some(AnthropicOutputFile {
                file_id,
                filename,
                mime_type,
                media,
            })
        })
        .collect()
}

fn image_mime_type_from_filename(filename: Option<&str>) -> Option<String> {
    let extension = filename?.rsplit('.').next()?.to_ascii_lowercase();
    match extension.as_str() {
        "png" => Some("image/png".to_string()),
        "jpg" | "jpeg" => Some("image/jpeg".to_string()),
        "gif" => Some("image/gif".to_string()),
        "webp" => Some("image/webp".to_string()),
        _ => None,
    }
}

fn is_image_mime_type(mime_type: Option<&str>) -> bool {
    mime_type
        .map(|mime_type| mime_type.starts_with("image/"))
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{LlmProvider, parse_response::parse_response};

    #[test]
    fn test_parse_basic_response() {
        let body = r#"{
            "id": "msg_013QyXSmCitiepWfcCMHPTsQ",
            "type": "message",
            "role": "assistant",
            "model": "claude-3-haiku-20240307",
            "content": [
                {
                    "type": "text",
                    "text": "Hello! How can I help you today?"
                }
            ],
            "stop_reason": "end_turn",
            "stop_sequence": null,
            "usage": {
                "input_tokens": 9,
                "cache_creation_input_tokens": 51,
                "cache_read_input_tokens": 2258,
                "output_tokens": 8,
                "service_tier": "standard"
            }
        }"#;

        let resp = parse_anthropic_response(body).unwrap();
        assert_eq!(resp.content, "Hello! How can I help you today?");
        assert_eq!(resp.model.as_deref(), Some("claude-3-haiku-20240307"));
        assert_eq!(resp.finish_reason, FinishReason::Stop);
        assert!(resp.finish_reason.is_complete());
        assert_eq!(resp.usage.input_tokens, Some(9));
        assert_eq!(resp.usage.output_tokens, Some(8));
        assert_eq!(resp.usage.total_tokens, Some(17));
        assert_eq!(resp.usage.cached_input_tokens, Some(2258));
    }

    #[test]
    fn test_parse_stop_sequence_response() {
        let body = r#"{
            "id": "msg_abc",
            "type": "message",
            "role": "assistant",
            "model": "claude-3-opus-20240229",
            "content": [
                {
                    "type": "text",
                    "text": "The answer is 42"
                }
            ],
            "stop_reason": "stop_sequence",
            "stop_sequence": "END",
            "usage": {
                "input_tokens": 20,
                "output_tokens": 5
            }
        }"#;

        let resp = parse_anthropic_response(body).unwrap();
        assert_eq!(resp.finish_reason, FinishReason::Stop);
    }

    #[test]
    fn test_parse_tool_use_response() {
        let body = r#"{
            "id": "msg_tools",
            "type": "message",
            "role": "assistant",
            "model": "claude-3-sonnet-20240229",
            "content": [
                {
                    "type": "tool_use",
                    "id": "toolu_abc",
                    "name": "get_weather",
                    "input": {"location": "SF"}
                }
            ],
            "stop_reason": "tool_use",
            "stop_sequence": null,
            "usage": {
                "input_tokens": 30,
                "output_tokens": 15
            }
        }"#;

        let resp = parse_anthropic_response(body).unwrap();
        // No text content block, so content is empty
        assert_eq!(resp.content, "");
        assert_eq!(resp.finish_reason, FinishReason::ToolUse);
    }

    #[test]
    fn test_parse_max_tokens_response() {
        let body = r#"{
            "id": "msg_trunc",
            "type": "message",
            "role": "assistant",
            "model": "claude-3-haiku-20240307",
            "content": [
                {
                    "type": "text",
                    "text": "This response was truncated..."
                }
            ],
            "stop_reason": "max_tokens",
            "stop_sequence": null,
            "usage": {
                "input_tokens": 50,
                "output_tokens": 4096
            }
        }"#;

        let resp = parse_anthropic_response(body).unwrap();
        assert_eq!(resp.finish_reason, FinishReason::Length);
        assert!(!resp.finish_reason.is_complete());
    }

    #[test]
    fn test_parse_multiple_content_blocks_takes_first_text() {
        let body = r#"{
            "id": "msg_multi",
            "type": "message",
            "role": "assistant",
            "model": "claude-3-haiku-20240307",
            "content": [
                { "type": "text", "text": "Let me check the weather." },
                { "type": "tool_use", "id": "toolu_1", "name": "get_weather", "input": {"city": "SF"} }
            ],
            "stop_reason": "tool_use",
            "stop_sequence": null,
            "usage": { "input_tokens": 10, "output_tokens": 20 }
        }"#;

        let resp = parse_anthropic_response(body).unwrap();
        assert_eq!(resp.content, "Let me check the weather.");
        assert_eq!(resp.finish_reason, FinishReason::ToolUse);
    }

    #[test]
    fn test_parse_tool_use_before_text_skips_to_text() {
        let body = r#"{
            "id": "msg_multi",
            "type": "message",
            "role": "assistant",
            "model": "claude-3-haiku-20240307",
            "content": [
                { "type": "tool_use", "id": "toolu_1", "name": "get_weather", "input": {"city": "SF"} },
                { "type": "text", "text": "Here's the weather." }
            ],
            "stop_reason": "tool_use",
            "stop_sequence": null,
            "usage": { "input_tokens": 10, "output_tokens": 20 }
        }"#;

        let resp = parse_anthropic_response(body).unwrap();
        assert_eq!(resp.content, "Here's the weather.");
    }

    #[test]
    fn test_parse_code_execution_image_file_output() {
        let body = r#"{
            "id": "msg_image",
            "type": "message",
            "role": "assistant",
            "model": "claude-3-7-sonnet-latest",
            "content": [
                { "type": "text", "text": "Here is the chart." },
                {
                    "type": "code_execution_tool_result",
                    "tool_use_id": "srvtoolu_123",
                    "content": {
                        "type": "code_execution_result",
                        "content": [
                            {
                                "type": "file",
                                "file_id": "file_123",
                                "filename": "chart.png",
                                "mime_type": "image/png"
                            }
                        ]
                    }
                }
            ],
            "stop_reason": "end_turn",
            "stop_sequence": null,
            "usage": { "input_tokens": 10, "output_tokens": 20 }
        }"#;

        let resp = parse_anthropic_response(body).unwrap();
        assert_eq!(resp.content, "Here is the chart.");
        assert_eq!(resp.output.parts.len(), 2);

        let crate::parse_response::LlmOutputPart::Media {
            media, provider_id, ..
        } = &resp.output.parts[1]
        else {
            panic!("expected image media output");
        };
        assert_eq!(media.kind, baml_base::MediaKind::Image);
        assert_eq!(media.url().as_deref(), Some("anthropic://files/file_123"));
        assert_eq!(media.mime_type().as_deref(), Some("image/png"));
        assert_eq!(provider_id.as_deref(), Some("file_123"));
    }

    #[test]
    fn test_parse_invalid_json() {
        let err = parse_anthropic_response("not json").unwrap_err();
        assert!(matches!(err, ParseResponseError::Deserialize { .. }));
    }

    #[test]
    fn test_anthropic_provider_dispatch() {
        let body = r#"{
            "id": "msg_test",
            "type": "message",
            "role": "assistant",
            "model": "claude-3-haiku-20240307",
            "content": [{"type": "text", "text": "hi"}],
            "stop_reason": "end_turn",
            "stop_sequence": null,
            "usage": {"input_tokens": 5, "output_tokens": 2}
        }"#;

        let resp = parse_response(LlmProvider::Anthropic, body).unwrap();
        assert_eq!(resp.content, "hi");
    }

    #[test]
    fn test_parse_cached_tokens() {
        let body = r#"{
            "id": "msg_test",
            "type": "message",
            "role": "assistant",
            "model": "claude-3-haiku-20240307",
            "content": [{"type": "text", "text": "hi"}],
            "stop_reason": "end_turn",
            "stop_sequence": null,
            "usage": {
                "input_tokens": 100,
                "output_tokens": 10,
                "cache_read_input_tokens": 50
            }
        }"#;

        let resp = parse_anthropic_response(body).unwrap();
        assert_eq!(resp.usage.cached_input_tokens, Some(50));
    }

    #[test]
    fn test_parse_no_cached_tokens() {
        let body = r#"{
            "id": "msg_test",
            "type": "message",
            "role": "assistant",
            "model": "claude-3-haiku-20240307",
            "content": [{"type": "text", "text": "hi"}],
            "stop_reason": "end_turn",
            "stop_sequence": null,
            "usage": { "input_tokens": 100, "output_tokens": 10 }
        }"#;

        let resp = parse_anthropic_response(body).unwrap();
        assert_eq!(resp.usage.cached_input_tokens, None);
    }
}
