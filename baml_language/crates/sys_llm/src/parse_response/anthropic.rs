use serde::Deserialize;

use super::{FinishReason, LlmProviderResponse, ParseResponseError, TokenUsage};

// ── Serde types ───────────────────────────────────────────────────

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

// ── Parser ────────────────────────────────────────────────────────

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

    if response.content.len() > 1 {
        let block_types: Vec<&str> = response
            .content
            .iter()
            .map(|b| match b {
                AnthropicMessageContent::Text { .. } => "text",
                AnthropicMessageContent::ToolUse { .. } => "tool_use",
                AnthropicMessageContent::RedactedThinking { .. } => "redacted_thinking",
                AnthropicMessageContent::Other => "other",
            })
            .collect();
        return Err(ParseResponseError::UnsupportedResponseFormat {
            provider: "anthropic",
            detail: format!(
                "response contains {} content blocks ({}) but we can only parse a single block; \
                 dropping block(s) would lose data",
                response.content.len(),
                block_types.join(", ")
            ),
        });
    }

    // Extract the single content block (if any).
    let content = response
        .content
        .first()
        .and_then(|block| match block {
            AnthropicMessageContent::Text { text } => Some(text.clone()),
            _ => None,
        })
        .unwrap_or_default();

    let finish_reason = match response.stop_reason.as_deref() {
        Some("end_turn" | "stop_sequence") => FinishReason::Stop,
        Some("max_tokens") => FinishReason::Length,
        Some("tool_use") => FinishReason::ToolUse,
        Some(other) => FinishReason::Other(other.to_string()),
        None => FinishReason::Unknown,
    };

    let input = response.usage.input_tokens;
    let output = response.usage.output_tokens;
    let usage = TokenUsage {
        input_tokens: Some(input),
        output_tokens: Some(output),
        total_tokens: Some(input + output),
        cached_input_tokens: response.usage.cache_read_input_tokens,
    };

    Ok(LlmProviderResponse {
        content,
        model: response.model,
        finish_reason,
        finish_reason_raw: response.stop_reason,
        usage,
    })
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
        assert_eq!(resp.model, "claude-3-haiku-20240307");
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
    fn test_parse_multiple_content_blocks() {
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

        let err = parse_anthropic_response(body).unwrap_err();
        assert!(matches!(
            err,
            ParseResponseError::UnsupportedResponseFormat { .. }
        ));
        let msg = err.to_string();
        assert!(msg.contains("2 content blocks"), "error message: {msg}");
        assert!(msg.contains("text, tool_use"), "error message: {msg}");
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

        let resp2 = parse_response(LlmProvider::AwsBedrock, body).unwrap();
        assert_eq!(resp2.content, "hi");
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
