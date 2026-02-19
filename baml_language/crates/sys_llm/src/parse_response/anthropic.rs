use super::{
    ParseResponseError,
    anthropic_types::{AnthropicMessageContent, AnthropicMessageResponse},
    types::{FinishReason, LlmProviderResponse, TokenUsage},
};

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
    };

    // Build metadata map with provider-specific fields.
    let mut metadata = serde_json::Map::new();
    metadata.insert("id".into(), serde_json::Value::String(response.id.clone()));
    if let Some(stop_seq) = &response.stop_sequence {
        metadata.insert(
            "stop_sequence".into(),
            serde_json::Value::String(stop_seq.value.clone()),
        );
    }
    if response.usage.cache_creation_input_tokens > 0 {
        metadata.insert(
            "cache_creation_input_tokens".into(),
            serde_json::Value::Number(response.usage.cache_creation_input_tokens.into()),
        );
    }
    if response.usage.cache_read_input_tokens > 0 {
        metadata.insert(
            "cache_read_input_tokens".into(),
            serde_json::Value::Number(response.usage.cache_read_input_tokens.into()),
        );
    }
    if !response.usage.service_tier.is_empty() {
        metadata.insert(
            "service_tier".into(),
            serde_json::Value::String(response.usage.service_tier.clone()),
        );
    }

    Ok(LlmProviderResponse {
        content,
        model: response.model,
        finish_reason,
        finish_reason_raw: response.stop_reason,
        usage,
        metadata,
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

        // Metadata
        assert_eq!(resp.metadata["id"], "msg_013QyXSmCitiepWfcCMHPTsQ");
        assert_eq!(resp.metadata["cache_creation_input_tokens"], 51);
        assert_eq!(resp.metadata["cache_read_input_tokens"], 2258);
        assert_eq!(resp.metadata["service_tier"], "standard");
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
        assert_eq!(resp.metadata["stop_sequence"], "END");
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
    fn test_cache_tokens_zero_not_in_metadata() {
        let body = r#"{
            "id": "msg_test",
            "type": "message",
            "role": "assistant",
            "model": "claude-3-haiku-20240307",
            "content": [{"type": "text", "text": "hi"}],
            "stop_reason": "end_turn",
            "stop_sequence": null,
            "usage": {
                "input_tokens": 5,
                "output_tokens": 2,
                "cache_creation_input_tokens": 0,
                "cache_read_input_tokens": 0
            }
        }"#;

        let resp = parse_anthropic_response(body).unwrap();
        // Zero cache tokens should not appear in metadata
        assert!(!resp.metadata.contains_key("cache_creation_input_tokens"));
        assert!(!resp.metadata.contains_key("cache_read_input_tokens"));
    }
}
