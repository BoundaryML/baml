use serde::Deserialize;

use super::CompletionUsage;
use crate::parse_response::{FinishReason, LlmProviderResponse, ParseResponseError, TokenUsage};

// ── Serde types ───────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct ResponsesApiResponse {
    status: String,
    model: String,
    output: Vec<ResponseOutput>,
    usage: Option<CompletionUsage>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
enum ResponseOutputType {
    Message,
    FunctionCall,
    #[serde(other)]
    Unknown,
}

#[derive(Debug, Deserialize)]
struct ResponseOutput {
    #[serde(rename = "type")]
    output_type: ResponseOutputType,
    #[serde(default)]
    content: Vec<OutputContent>,
    // Function call fields
    name: Option<String>,
    arguments: Option<String>,
    call_id: Option<String>,
}

#[derive(Debug, Deserialize)]
struct OutputContent {
    text: Option<String>,
}

// ── Parser ────────────────────────────────────────────────────────

/// Parse an `OpenAI` Responses API response body into a normalized [`LlmProviderResponse`].
pub(in crate::parse_response) fn parse_openai_responses_response(
    body: &str,
) -> Result<LlmProviderResponse, ParseResponseError> {
    let response: ResponsesApiResponse =
        serde_json::from_str(body).map_err(|e| ParseResponseError::Deserialize {
            provider: "openai-responses",
            source: e,
            content: body.to_string(),
        })?;

    // Extract content: first Message output's text, or FunctionCall serialized JSON
    let content = response
        .output
        .iter()
        .find_map(|output| match output.output_type {
            ResponseOutputType::Message => output.content.first()?.text.clone(),
            ResponseOutputType::FunctionCall => {
                if let (Some(name), Some(arguments)) = (&output.name, &output.arguments) {
                    Some(
                        serde_json::json!({
                            "type": "function_call",
                            "name": name,
                            "arguments": arguments,
                            "call_id": output.call_id
                        })
                        .to_string(),
                    )
                } else {
                    None
                }
            }
            ResponseOutputType::Unknown => None,
        })
        .unwrap_or_default();

    // Finish reason: status field, not a separate finish_reason
    let finish_reason = match response.status.as_str() {
        "completed" => FinishReason::Stop,
        "incomplete" => FinishReason::Length,
        other => FinishReason::Other(other.to_string()),
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
        content,
        model: Some(response.model),
        finish_reason,
        finish_reason_raw: Some(response.status),
        usage,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_completed_response() {
        let body = r#"{
            "id": "resp_123",
            "object": "response",
            "created_at": 1700000000,
            "status": "completed",
            "model": "gpt-4o",
            "output": [{
                "type": "message",
                "id": "msg_1",
                "status": "completed",
                "role": "assistant",
                "content": [{"type": "output_text", "text": "Hello!"}]
            }],
            "usage": {
                "prompt_tokens": 10,
                "completion_tokens": 5,
                "total_tokens": 15
            }
        }"#;

        let resp = parse_openai_responses_response(body).unwrap();
        assert_eq!(resp.content, "Hello!");
        assert_eq!(resp.model.as_deref(), Some("gpt-4o"));
        assert_eq!(resp.finish_reason, FinishReason::Stop);
        assert_eq!(resp.finish_reason_raw, Some("completed".to_string()));
        assert_eq!(resp.usage.input_tokens, Some(10));
    }

    #[test]
    fn test_parse_function_call_output() {
        let body = r#"{
            "id": "resp_456",
            "object": "response",
            "status": "completed",
            "model": "gpt-4o",
            "output": [{
                "type": "function_call",
                "id": "fc_1",
                "call_id": "call_abc",
                "name": "get_weather",
                "arguments": "{\"location\": \"SF\"}"
            }],
            "usage": {
                "prompt_tokens": 20,
                "completion_tokens": 10,
                "total_tokens": 30
            }
        }"#;

        let resp = parse_openai_responses_response(body).unwrap();
        assert!(resp.content.contains("get_weather"));
        assert!(resp.content.contains("function_call"));
    }

    #[test]
    fn test_parse_incomplete_status() {
        let body = r#"{
            "id": "resp_789",
            "object": "response",
            "status": "incomplete",
            "model": "gpt-4o",
            "output": [{
                "type": "message",
                "id": "msg_1",
                "role": "assistant",
                "content": [{"type": "output_text", "text": "Partial..."}]
            }],
            "incomplete_details": {"reason": "max_output_tokens"},
            "usage": {
                "prompt_tokens": 10,
                "completion_tokens": 100,
                "total_tokens": 110
            }
        }"#;

        let resp = parse_openai_responses_response(body).unwrap();
        assert_eq!(resp.finish_reason, FinishReason::Length);
        assert_eq!(resp.finish_reason_raw, Some("incomplete".to_string()));
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
                "prompt_tokens": 5,
                "completion_tokens": 0,
                "total_tokens": 5
            }
        }"#;

        let resp = parse_openai_responses_response(body).unwrap();
        assert_eq!(resp.content, "");
        assert_eq!(resp.finish_reason, FinishReason::Stop);
    }
}
