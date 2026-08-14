//! AWS Bedrock Converse API response parser.
//!
//! Parses JSON responses from the Bedrock Converse API. The response format
//! is different from Anthropic's native format — it uses camelCase and a
//! nested `output.message` structure.

use serde::Deserialize;

use super::{FinishReason, LlmProviderResponse, ParseResponseError, TokenUsage};

// == Serde types ===================================================

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ConverseResponse {
    output: ConverseOutput,
    stop_reason: Option<String>,
    usage: Option<ConverseUsage>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ConverseOutput {
    message: Option<ConverseMessage>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ConverseMessage {
    #[serde(default)]
    content: Vec<ContentBlock>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
#[allow(clippy::struct_field_names)]
struct ConverseUsage {
    input_tokens: Option<u64>,
    output_tokens: Option<u64>,
    total_tokens: Option<u64>,
}

/// Content block in a Converse response.
/// We only extract text; other block types (image, toolUse, etc.) are skipped.
#[derive(Debug, Deserialize)]
#[serde(untagged)]
#[allow(dead_code)]
enum ContentBlock {
    Text { text: String },
    Other(serde_json::Value),
}

// == Parser ========================================================

/// Parse a Bedrock Converse API response body into a normalized `LlmProviderResponse`.
pub(super) fn parse_bedrock_response(
    body: &str,
) -> Result<LlmProviderResponse, ParseResponseError> {
    let response: ConverseResponse =
        serde_json::from_str(body).map_err(|e| ParseResponseError::Deserialize {
            provider: "bedrock",
            source: e,
            content: body.to_string(),
        })?;

    let message = response
        .output
        .message
        .ok_or_else(|| ParseResponseError::NoContent {
            provider: "bedrock",
            detail: "response has no output.message".into(),
        })?;

    if message.content.is_empty() {
        return Err(ParseResponseError::NoContent {
            provider: "bedrock",
            detail: "message content is empty".into(),
        });
    }

    // Extract and join all text blocks (matching runtime behavior).
    let mut text_parts = Vec::new();
    for block in &message.content {
        if let ContentBlock::Text { text } = block {
            text_parts.push(text.as_str());
        }
    }

    if text_parts.is_empty() {
        return Err(ParseResponseError::NoContent {
            provider: "bedrock",
            detail: format!(
                "message contains no text blocks, found: {:?}",
                message
                    .content
                    .iter()
                    .map(|b| match b {
                        ContentBlock::Text { .. } => "text",
                        ContentBlock::Other(_) => "other",
                    })
                    .collect::<Vec<_>>()
            ),
        });
    }

    let content = text_parts.join("");

    // Map stop_reason to FinishReason.
    // Bedrock uses: end_turn, stop_sequence, max_tokens, tool_use, content_filtered, etc.
    let finish_reason = match response.stop_reason.as_deref() {
        Some("end_turn" | "stop_sequence") => FinishReason::Stop,
        Some("max_tokens") => FinishReason::Length,
        Some("tool_use") => FinishReason::ToolUse,
        Some(other) => FinishReason::Other(other.to_string()),
        None => FinishReason::Unknown,
    };

    let usage = response
        .usage
        .map(|u| TokenUsage {
            input_tokens: u.input_tokens,
            output_tokens: u.output_tokens,
            total_tokens: u.total_tokens,
            cached_input_tokens: None, // Bedrock Converse doesn't report cached tokens
        })
        .unwrap_or_default();

    Ok(LlmProviderResponse {
        output: crate::parse_response::LlmOutput::from_text(content.clone()),
        content,
        model: None, // Bedrock Converse responses don't include the model name
        finish_reason,
        finish_reason_raw: response.stop_reason,
        usage,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_basic_response() {
        let body = r#"{
            "output": {
                "message": {
                    "role": "assistant",
                    "content": [
                        { "text": "Hello! How can I help you today?" }
                    ]
                }
            },
            "stopReason": "end_turn",
            "usage": {
                "inputTokens": 10,
                "outputTokens": 15,
                "totalTokens": 25
            }
        }"#;

        let resp = parse_bedrock_response(body).unwrap();
        assert_eq!(resp.content, "Hello! How can I help you today?");
        assert_eq!(resp.finish_reason, FinishReason::Stop);
        assert_eq!(resp.finish_reason_raw, Some("end_turn".to_string()));
        assert_eq!(resp.usage.input_tokens, Some(10));
        assert_eq!(resp.usage.output_tokens, Some(15));
        assert_eq!(resp.usage.total_tokens, Some(25));
        assert_eq!(resp.model, None);
    }

    #[test]
    fn test_parse_multiple_text_blocks() {
        let body = r#"{
            "output": {
                "message": {
                    "role": "assistant",
                    "content": [
                        { "text": "First part. " },
                        { "text": "Second part." }
                    ]
                }
            },
            "stopReason": "end_turn",
            "usage": {
                "inputTokens": 5,
                "outputTokens": 10,
                "totalTokens": 15
            }
        }"#;

        let resp = parse_bedrock_response(body).unwrap();
        assert_eq!(resp.content, "First part. Second part.");
    }

    #[test]
    fn test_parse_stop_sequence() {
        let body = r#"{
            "output": {
                "message": {
                    "role": "assistant",
                    "content": [{ "text": "Stopped at sequence" }]
                }
            },
            "stopReason": "stop_sequence",
            "usage": { "inputTokens": 5, "outputTokens": 5, "totalTokens": 10 }
        }"#;

        let resp = parse_bedrock_response(body).unwrap();
        assert_eq!(resp.finish_reason, FinishReason::Stop);
    }

    #[test]
    fn test_parse_max_tokens() {
        let body = r#"{
            "output": {
                "message": {
                    "role": "assistant",
                    "content": [{ "text": "Truncated..." }]
                }
            },
            "stopReason": "max_tokens",
            "usage": { "inputTokens": 10, "outputTokens": 100, "totalTokens": 110 }
        }"#;

        let resp = parse_bedrock_response(body).unwrap();
        assert_eq!(resp.finish_reason, FinishReason::Length);
        assert!(!resp.finish_reason.is_complete());
    }

    #[test]
    fn test_parse_tool_use() {
        let body = r#"{
            "output": {
                "message": {
                    "role": "assistant",
                    "content": [
                        { "text": "Let me check that for you." },
                        { "toolUse": { "toolUseId": "tool_1", "name": "get_weather", "input": {} } }
                    ]
                }
            },
            "stopReason": "tool_use",
            "usage": { "inputTokens": 10, "outputTokens": 20, "totalTokens": 30 }
        }"#;

        let resp = parse_bedrock_response(body).unwrap();
        assert_eq!(resp.content, "Let me check that for you.");
        assert_eq!(resp.finish_reason, FinishReason::ToolUse);
    }

    #[test]
    fn test_parse_no_text_blocks_error() {
        let body = r#"{
            "output": {
                "message": {
                    "role": "assistant",
                    "content": [
                        { "toolUse": { "toolUseId": "tool_1", "name": "get_weather", "input": {} } }
                    ]
                }
            },
            "stopReason": "tool_use",
            "usage": { "inputTokens": 10, "outputTokens": 5, "totalTokens": 15 }
        }"#;

        let err = parse_bedrock_response(body).unwrap_err();
        assert!(matches!(err, ParseResponseError::NoContent { .. }));
        assert!(err.to_string().contains("no text blocks"));
    }

    #[test]
    fn test_parse_empty_content_error() {
        let body = r#"{
            "output": {
                "message": {
                    "role": "assistant",
                    "content": []
                }
            },
            "stopReason": "end_turn",
            "usage": { "inputTokens": 5, "outputTokens": 0, "totalTokens": 5 }
        }"#;

        let err = parse_bedrock_response(body).unwrap_err();
        assert!(matches!(err, ParseResponseError::NoContent { .. }));
    }

    #[test]
    fn test_parse_no_message_error() {
        let body = r#"{
            "output": {},
            "stopReason": "end_turn",
            "usage": { "inputTokens": 5, "outputTokens": 0, "totalTokens": 5 }
        }"#;

        let err = parse_bedrock_response(body).unwrap_err();
        assert!(matches!(err, ParseResponseError::NoContent { .. }));
    }

    #[test]
    fn test_parse_invalid_json() {
        let err = parse_bedrock_response("not json").unwrap_err();
        assert!(matches!(err, ParseResponseError::Deserialize { .. }));
    }

    #[test]
    fn test_parse_no_usage() {
        let body = r#"{
            "output": {
                "message": {
                    "role": "assistant",
                    "content": [{ "text": "hi" }]
                }
            },
            "stopReason": "end_turn"
        }"#;

        let resp = parse_bedrock_response(body).unwrap();
        assert_eq!(resp.content, "hi");
        assert_eq!(resp.usage.input_tokens, None);
        assert_eq!(resp.usage.output_tokens, None);
    }
}
