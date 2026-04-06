use serde::Deserialize;

use super::{
    ParseResponseError,
    types::{FinishReason, LlmProviderResponse, TokenUsage},
};

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize, Clone, PartialEq)]
#[serde(untagged)]
enum ChatCompletionResponse {
    Success(ChatCompletionSuccess),
    Error(OpenAiErrorWrapper),
}

#[derive(Debug, Deserialize, Clone, PartialEq)]
struct OpenAiErrorWrapper {
    error: OpenAiErrorResponse,
}

#[derive(Debug, Deserialize, Clone, PartialEq)]
struct OpenAiErrorResponse {
    message: String,
    param: Option<String>,
    code: Option<String>,
}

#[derive(Debug, Deserialize, Clone, PartialEq)]
struct ChatCompletionSuccess {
    choices: Vec<ChatCompletionChoice>,
    model: String,
    usage: Option<CompletionUsage>,
}

#[derive(Debug, Deserialize, Clone, PartialEq)]
struct ChatCompletionChoice {
    message: ChatCompletionResponseMessage,
    finish_reason: Option<String>,
}

#[derive(Debug, Deserialize, Clone, PartialEq)]
struct CompletionUsage {
    prompt_tokens: u64,
    completion_tokens: u64,
    total_tokens: u64,
    #[serde(alias = "prompt_tokens_details")]
    input_tokens_details: Option<InputTokensDetails>,
}

#[derive(Debug, Deserialize, Clone, PartialEq)]
struct InputTokensDetails {
    cached_tokens: Option<u64>,
}

#[derive(Debug, Deserialize, Clone, PartialEq)]
struct ChatCompletionResponseMessage {
    content: Option<String>,
}

// ---------------------------------------------------------------------------
// Parser
// ---------------------------------------------------------------------------

/// Parse an OpenAI-compatible chat completion response body into a normalized `LlmProviderResponse`.
pub(super) fn parse_openai_response(body: &str) -> Result<LlmProviderResponse, ParseResponseError> {
    let response: ChatCompletionResponse =
        serde_json::from_str(body).map_err(|e| ParseResponseError::Deserialize {
            provider: "openai",
            source: e,
            content: body.to_string(),
        })?;

    let response = match response {
        ChatCompletionResponse::Success(success) => success,
        ChatCompletionResponse::Error(wrapper) => {
            return Err(ParseResponseError::ApiError {
                provider: "openai",
                message: wrapper.error.message,
                code: wrapper.error.code,
                param: wrapper.error.param,
            });
        }
    };

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
            input_tokens: Some(u.prompt_tokens),
            output_tokens: Some(u.completion_tokens),
            total_tokens: Some(u.total_tokens),
            cached_input_tokens: u
                .input_tokens_details
                .as_ref()
                .and_then(|d| d.cached_tokens),
        })
        .unwrap_or_default();

    Ok(LlmProviderResponse {
        content,
        model: response.model,
        finish_reason,
        finish_reason_raw: choice.finish_reason.clone(),
        usage,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{LlmProvider, parse_response::parse_response};

    #[test]
    fn test_deserialize_chat_completion_response() {
        let json = r#"{
            "id": "chatcmpl-123",
            "object": "chat.completion",
            "created": 1677652288,
            "model": "gpt-3.5-turbo-0125",
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

        let response: ChatCompletionResponse = serde_json::from_str(json).unwrap();
        let ChatCompletionResponse::Success(response) = response else {
            panic!("expected success");
        };
        assert_eq!(response.model, "gpt-3.5-turbo-0125");
        assert_eq!(response.choices.len(), 1);
        assert_eq!(
            response.choices[0].message.content,
            Some("Hello! How can I help you today?".to_string())
        );
        assert_eq!(response.choices[0].finish_reason, Some("stop".to_string()));
        assert_eq!(response.usage.as_ref().unwrap().total_tokens, 21);
    }

    #[test]
    fn test_deserialize_error_response() {
        let json = r#"{
            "error": {
                "message": "Invalid request",
                "type": "invalid_request_error",
                "param": "model",
                "code": "invalid_model"
            }
        }"#;

        let response: ChatCompletionResponse = serde_json::from_str(json).unwrap();
        let ChatCompletionResponse::Error(wrapper) = response else {
            panic!("expected error");
        };
        assert_eq!(wrapper.error.message, "Invalid request");
        assert_eq!(wrapper.error.param, Some("model".to_string()));
        assert_eq!(wrapper.error.code, Some("invalid_model".to_string()));
    }

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
        assert_eq!(resp.model, "basic-model");
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
