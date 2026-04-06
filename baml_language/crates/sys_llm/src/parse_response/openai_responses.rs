use serde::{Deserialize, Deserializer};

use super::{
    ParseResponseError,
    types::{FinishReason, LlmProviderResponse, TokenUsage},
};

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// Top-level response from `OpenAI`'s Responses API.
#[derive(Debug, Deserialize, Clone)]
struct ResponsesApiResponse {
    status: String,
    model: String,
    #[serde(default)]
    output: Vec<ResponseOutput>,
    usage: Option<ResponsesApiUsage>,
    error: Option<serde_json::Value>,
    incomplete_details: Option<IncompleteDetails>,
}

#[derive(Debug, Deserialize, Clone, PartialEq)]
#[serde(rename_all = "snake_case")]
enum ResponseOutputType {
    Message,
    FunctionCall,
    WebSearchCall,
    FileSearchCall,
    Reasoning,
    ComputerCall,
    McpListTools,
    McpCall,
    #[serde(other)]
    Unknown,
}

#[derive(Debug, Deserialize, Clone)]
struct ResponseOutput {
    #[serde(rename = "type")]
    output_type: ResponseOutputType,
    #[serde(default, deserialize_with = "deserialize_maybe_list_to_vec")]
    content: Vec<ResponseContent>,
    // Function call fields
    call_id: Option<String>,
    name: Option<String>,
    arguments: Option<String>,
}

#[derive(Debug, Deserialize, Clone)]
struct ResponseContent {
    text: Option<String>,
}

#[derive(Debug, Deserialize, Clone)]
struct ResponsesApiUsage {
    input_tokens: u64,
    output_tokens: u64,
    total_tokens: u64,
    input_tokens_details: Option<ResponsesInputTokensDetails>,
}

#[derive(Debug, Deserialize, Clone)]
struct ResponsesInputTokensDetails {
    cached_tokens: Option<u64>,
}

#[derive(Debug, Deserialize, Clone)]
struct IncompleteDetails {
    reason: String,
}

fn deserialize_maybe_list_to_vec<'de, D, I>(deserializer: D) -> Result<Vec<I>, D::Error>
where
    D: Deserializer<'de>,
    I: Deserialize<'de>,
{
    match Option::<Vec<I>>::deserialize(deserializer)? {
        Some(inner) => Ok(inner),
        None => Ok(vec![]),
    }
}

// ---------------------------------------------------------------------------
// Parser
// ---------------------------------------------------------------------------

/// Parse an `OpenAI` Responses API response body into a normalized
/// `LlmProviderResponse`.
pub(super) fn parse_openai_responses_response(
    body: &str,
) -> Result<LlmProviderResponse, ParseResponseError> {
    let response: ResponsesApiResponse =
        serde_json::from_str(body).map_err(|e| ParseResponseError::Deserialize {
            provider: "openai-responses",
            source: e,
            content: body.to_string(),
        })?;

    if let Some(error) = &response.error {
        let message = error
            .get("message")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown error")
            .to_string();
        let code = error.get("code").and_then(|v| v.as_str()).map(String::from);
        return Err(ParseResponseError::ApiError {
            provider: "openai-responses",
            message,
            code,
            param: None,
        });
    }

    // Extract text content from the response outputs.
    // We look for the first Message or FunctionCall output, matching engine behavior.
    let content = response
        .output
        .iter()
        .find_map(|output| match output.output_type {
            ResponseOutputType::Message => output
                .content
                .first()
                .and_then(|c| c.text.as_ref())
                .cloned(),
            ResponseOutputType::FunctionCall => match (&output.name, &output.arguments) {
                (Some(name), Some(arguments)) => Some(
                    serde_json::json!({
                        "type": "function_call",
                        "name": name,
                        "arguments": arguments,
                        "call_id": output.call_id,
                    })
                    .to_string(),
                ),
                _ => None,
            },
            // Web search, file search, reasoning, computer, MCP outputs are skipped.
            _ => None,
        })
        .unwrap_or_default();

    let finish_reason = match response.status.as_str() {
        "completed" => FinishReason::Stop,
        "incomplete" => {
            let reason = response
                .incomplete_details
                .as_ref()
                .map(|d| d.reason.as_str())
                .unwrap_or("incomplete");
            if reason.contains("max_output_tokens") || reason.contains("max_tokens") {
                FinishReason::Length
            } else {
                FinishReason::Other(reason.to_string())
            }
        }
        "failed" | "cancelled" => FinishReason::Other(response.status.clone()),
        other => FinishReason::Other(other.to_string()),
    };

    let usage = response
        .usage
        .as_ref()
        .map(|u| TokenUsage {
            input_tokens: Some(u.input_tokens),
            output_tokens: Some(u.output_tokens),
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
        finish_reason_raw: Some(response.status),
        usage,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{LlmProvider, parse_response::parse_response};

    #[test]
    fn test_deserialize_message_response() {
        let json = r#"{
            "id": "resp_abc123",
            "object": "response",
            "created_at": 1700000000,
            "status": "completed",
            "model": "gpt-4o",
            "output": [{
                "type": "message",
                "id": "msg_abc",
                "status": "completed",
                "role": "assistant",
                "content": [{
                    "type": "output_text",
                    "text": "Hello! How can I help you?"
                }]
            }],
            "usage": {
                "input_tokens": 10,
                "output_tokens": 8,
                "total_tokens": 18
            }
        }"#;

        let resp: ResponsesApiResponse = serde_json::from_str(json).unwrap();
        assert_eq!(resp.status, "completed");
        assert_eq!(resp.model, "gpt-4o");
        assert_eq!(resp.output.len(), 1);
        assert_eq!(resp.output[0].output_type, ResponseOutputType::Message);
        assert_eq!(
            resp.output[0].content[0].text.as_deref(),
            Some("Hello! How can I help you?")
        );
        let usage = resp.usage.unwrap();
        assert_eq!(usage.input_tokens, 10);
        assert_eq!(usage.output_tokens, 8);
        assert_eq!(usage.total_tokens, 18);
    }

    #[test]
    fn test_deserialize_function_call_response() {
        let json = r#"{
            "id": "resp_func",
            "object": "response",
            "status": "completed",
            "model": "gpt-4o",
            "output": [{
                "type": "function_call",
                "id": "fc_abc",
                "call_id": "call_123",
                "name": "get_weather",
                "arguments": "{\"location\": \"SF\"}"
            }],
            "usage": {
                "input_tokens": 20,
                "output_tokens": 15,
                "total_tokens": 35
            }
        }"#;

        let resp: ResponsesApiResponse = serde_json::from_str(json).unwrap();
        assert_eq!(resp.output[0].output_type, ResponseOutputType::FunctionCall);
        assert_eq!(resp.output[0].name.as_deref(), Some("get_weather"));
        assert_eq!(
            resp.output[0].arguments.as_deref(),
            Some("{\"location\": \"SF\"}")
        );
        assert_eq!(resp.output[0].call_id.as_deref(), Some("call_123"));
    }

    #[test]
    fn test_deserialize_with_cached_tokens() {
        let json = r#"{
            "id": "resp_cached",
            "object": "response",
            "status": "completed",
            "model": "gpt-4o",
            "output": [{
                "type": "message",
                "content": [{
                    "type": "output_text",
                    "text": "Cached response"
                }]
            }],
            "usage": {
                "input_tokens": 100,
                "output_tokens": 10,
                "total_tokens": 110,
                "input_tokens_details": {
                    "cached_tokens": 80
                },
                "output_tokens_details": {
                    "reasoning_tokens": 0
                }
            }
        }"#;

        let resp: ResponsesApiResponse = serde_json::from_str(json).unwrap();
        let usage = resp.usage.unwrap();
        let cached = usage.input_tokens_details.as_ref().unwrap().cached_tokens;
        assert_eq!(cached, Some(80));
    }

    #[test]
    fn test_deserialize_unknown_output_type() {
        let json = r#"{
            "id": "resp_unk",
            "object": "response",
            "status": "completed",
            "model": "gpt-4o",
            "output": [{
                "type": "some_future_type",
                "id": "unk_123"
            }],
            "usage": {
                "input_tokens": 5,
                "output_tokens": 2,
                "total_tokens": 7
            }
        }"#;

        let resp: ResponsesApiResponse = serde_json::from_str(json).unwrap();
        assert_eq!(resp.output[0].output_type, ResponseOutputType::Unknown);
    }

    #[test]
    fn test_deserialize_incomplete_response() {
        let json = r#"{
            "id": "resp_inc",
            "object": "response",
            "status": "incomplete",
            "model": "gpt-4o",
            "output": [],
            "incomplete_details": {
                "reason": "max_output_tokens"
            },
            "usage": {
                "input_tokens": 100,
                "output_tokens": 4096,
                "total_tokens": 4196
            }
        }"#;

        let resp: ResponsesApiResponse = serde_json::from_str(json).unwrap();
        assert_eq!(resp.status, "incomplete");
        assert_eq!(
            resp.incomplete_details.as_ref().unwrap().reason,
            "max_output_tokens"
        );
    }

    #[test]
    fn test_parse_basic_response() {
        let body = r#"{
            "id": "resp_abc123",
            "object": "response",
            "created_at": 1700000000,
            "status": "completed",
            "model": "gpt-4o",
            "output": [{
                "type": "message",
                "id": "msg_abc",
                "status": "completed",
                "role": "assistant",
                "content": [{
                    "type": "output_text",
                    "text": "Hello! How can I help you today?"
                }]
            }],
            "usage": {
                "input_tokens": 10,
                "output_tokens": 8,
                "total_tokens": 18
            }
        }"#;

        let resp = parse_openai_responses_response(body).unwrap();
        assert_eq!(resp.content, "Hello! How can I help you today?");
        assert_eq!(resp.model, "gpt-4o");
        assert_eq!(resp.finish_reason, FinishReason::Stop);
        assert!(resp.finish_reason.is_complete());
        assert_eq!(resp.usage.input_tokens, Some(10));
        assert_eq!(resp.usage.output_tokens, Some(8));
        assert_eq!(resp.usage.total_tokens, Some(18));
        assert_eq!(resp.usage.cached_input_tokens, None);
    }

    #[test]
    fn test_parse_function_call_response() {
        let body = r#"{
            "id": "resp_func",
            "object": "response",
            "status": "completed",
            "model": "gpt-4o",
            "output": [{
                "type": "function_call",
                "id": "fc_abc",
                "call_id": "call_123",
                "name": "get_weather",
                "arguments": "{\"location\": \"SF\"}"
            }],
            "usage": {
                "input_tokens": 20,
                "output_tokens": 15,
                "total_tokens": 35
            }
        }"#;

        let resp = parse_openai_responses_response(body).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&resp.content).unwrap();
        assert_eq!(parsed["type"], "function_call");
        assert_eq!(parsed["name"], "get_weather");
        assert_eq!(parsed["arguments"], "{\"location\": \"SF\"}");
        assert_eq!(parsed["call_id"], "call_123");
    }

    #[test]
    fn test_parse_incomplete_response() {
        let body = r#"{
            "id": "resp_inc",
            "object": "response",
            "status": "incomplete",
            "model": "gpt-4o",
            "output": [{
                "type": "message",
                "content": [{
                    "type": "output_text",
                    "text": "Truncated..."
                }]
            }],
            "incomplete_details": {
                "reason": "max_output_tokens"
            },
            "usage": {
                "input_tokens": 100,
                "output_tokens": 4096,
                "total_tokens": 4196
            }
        }"#;

        let resp = parse_openai_responses_response(body).unwrap();
        assert_eq!(resp.finish_reason, FinishReason::Length);
        assert!(!resp.finish_reason.is_complete());
        assert_eq!(resp.finish_reason, FinishReason::Length);
    }

    #[test]
    fn test_parse_failed_response() {
        let body = r#"{
            "id": "resp_fail",
            "object": "response",
            "status": "failed",
            "model": "gpt-4o",
            "output": [],
            "error": {
                "message": "Something went wrong",
                "code": "server_error"
            },
            "usage": {
                "input_tokens": 10,
                "output_tokens": 0,
                "total_tokens": 10
            }
        }"#;

        let err = parse_openai_responses_response(body).unwrap_err();
        assert!(matches!(err, ParseResponseError::ApiError { .. }));
        let msg = err.to_string();
        assert!(msg.contains("Something went wrong"), "error message: {msg}");
    }

    #[test]
    fn test_parse_with_cached_tokens() {
        let body = r#"{
            "id": "resp_cached",
            "object": "response",
            "status": "completed",
            "model": "gpt-4o",
            "output": [{
                "type": "message",
                "content": [{
                    "type": "output_text",
                    "text": "Cached response"
                }]
            }],
            "usage": {
                "input_tokens": 100,
                "output_tokens": 10,
                "total_tokens": 110,
                "input_tokens_details": {
                    "cached_tokens": 80
                },
                "output_tokens_details": {
                    "reasoning_tokens": 50
                }
            }
        }"#;

        let resp = parse_openai_responses_response(body).unwrap();
        assert_eq!(resp.usage.cached_input_tokens, Some(80));
    }

    #[test]
    fn test_parse_zero_cached_tokens() {
        let body = r#"{
            "id": "resp_no_cache",
            "object": "response",
            "status": "completed",
            "model": "gpt-4o",
            "output": [{
                "type": "message",
                "content": [{
                    "type": "output_text",
                    "text": "No cache"
                }]
            }],
            "usage": {
                "input_tokens": 10,
                "output_tokens": 5,
                "total_tokens": 15,
                "input_tokens_details": {
                    "cached_tokens": 0
                },
                "output_tokens_details": {
                    "reasoning_tokens": 0
                }
            }
        }"#;

        let resp = parse_openai_responses_response(body).unwrap();
        assert_eq!(resp.usage.cached_input_tokens, Some(0));
    }

    #[test]
    fn test_parse_empty_output() {
        let body = r#"{
            "id": "resp_empty",
            "object": "response",
            "status": "completed",
            "model": "gpt-4o",
            "output": [],
            "usage": {
                "input_tokens": 5,
                "output_tokens": 0,
                "total_tokens": 5
            }
        }"#;

        let resp = parse_openai_responses_response(body).unwrap();
        assert_eq!(resp.content, "");
    }

    #[test]
    fn test_parse_skips_web_search_output() {
        let body = r#"{
            "id": "resp_search",
            "object": "response",
            "status": "completed",
            "model": "gpt-4o",
            "output": [
                {
                    "type": "web_search_call",
                    "id": "ws_abc"
                },
                {
                    "type": "message",
                    "content": [{
                        "type": "output_text",
                        "text": "Search result summary"
                    }]
                }
            ],
            "usage": {
                "input_tokens": 20,
                "output_tokens": 10,
                "total_tokens": 30
            }
        }"#;

        let resp = parse_openai_responses_response(body).unwrap();
        assert_eq!(resp.content, "Search result summary");
    }

    #[test]
    fn test_parse_invalid_json() {
        let err = parse_openai_responses_response("not json").unwrap_err();
        assert!(matches!(err, ParseResponseError::Deserialize { .. }));
    }

    #[test]
    fn test_openai_responses_provider_dispatch() {
        let body = r#"{
            "id": "resp_test",
            "object": "response",
            "status": "completed",
            "model": "gpt-4o",
            "output": [{
                "type": "message",
                "content": [{
                    "type": "output_text",
                    "text": "hi"
                }]
            }],
            "usage": {
                "input_tokens": 5,
                "output_tokens": 2,
                "total_tokens": 7
            }
        }"#;

        let resp = parse_response(LlmProvider::OpenAiResponses, body).unwrap();
        assert_eq!(resp.content, "hi");
    }
}
