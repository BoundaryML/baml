use super::{
    ParseResponseError,
    openai_types::ChatCompletionResponse,
    types::{FinishReason, LlmProviderResponse, TokenUsage},
};

/// Parse an OpenAI-compatible chat completion response body into a normalized `LlmProviderResponse`.
pub(super) fn parse_openai_response(body: &str) -> Result<LlmProviderResponse, ParseResponseError> {
    let response: ChatCompletionResponse =
        serde_json::from_str(body).map_err(|e| ParseResponseError::Deserialize {
            provider: "openai",
            source: e,
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

    let content = choice.message.content.clone().unwrap_or_default();

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
            prompt_tokens: Some(u.prompt_tokens),
            output_tokens: Some(u.completion_tokens),
            total_tokens: Some(u.total_tokens),
        })
        .unwrap_or_default();

    // Build metadata map with provider-specific fields.
    let mut metadata = serde_json::Map::new();
    if let Some(id) = &response.id {
        metadata.insert("id".into(), serde_json::Value::String(id.clone()));
    }
    if let Some(fp) = &response.system_fingerprint {
        metadata.insert(
            "system_fingerprint".into(),
            serde_json::Value::String(fp.clone()),
        );
    }
    if let Some(created) = response.created {
        metadata.insert("created".into(), serde_json::Value::Number(created.into()));
    }
    if let Some(object) = &response.object {
        metadata.insert("object".into(), serde_json::Value::String(object.clone()));
    }
    if let Some(logprobs) = &choice.logprobs {
        if let Ok(val) = serde_json::to_value(logprobs) {
            metadata.insert("logprobs".into(), val);
        }
    }

    Ok(LlmProviderResponse {
        content,
        model: response.model,
        finish_reason,
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
        assert_eq!(resp.model, "gpt-4o");
        assert_eq!(resp.finish_reason, FinishReason::Stop);
        assert!(resp.finish_reason.is_complete());
        assert_eq!(resp.usage.prompt_tokens, Some(9));
        assert_eq!(resp.usage.output_tokens, Some(12));
        assert_eq!(resp.usage.total_tokens, Some(21));

        // Metadata
        assert_eq!(resp.metadata["id"], "chatcmpl-123");
        assert_eq!(resp.metadata["system_fingerprint"], "fp_44709d6fcb");
        assert_eq!(resp.metadata["created"], 1_677_652_288);
        assert_eq!(resp.metadata["object"], "chat.completion");
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
        assert_eq!(resp.model, "basic-model");
        assert_eq!(resp.finish_reason, FinishReason::Unknown);
        assert!(!resp.finish_reason.is_complete());
        assert_eq!(resp.usage.prompt_tokens, None);
        assert_eq!(resp.usage.output_tokens, None);
        assert_eq!(resp.usage.total_tokens, None);

        // No metadata when fields are absent
        assert!(!resp.metadata.contains_key("id"));
        assert!(!resp.metadata.contains_key("system_fingerprint"));
        assert!(!resp.metadata.contains_key("created"));
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
}
