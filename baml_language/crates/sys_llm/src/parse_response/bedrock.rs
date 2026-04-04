use serde::Deserialize;

use super::{
    ParseResponseError,
    types::{FinishReason, LlmProviderResponse, TokenUsage},
};

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// Top-level response from the AWS Bedrock Converse API.
///
/// Reference: <https://docs.aws.amazon.com/bedrock/latest/APIReference/API_runtime_Converse.html>
#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
struct ConverseResponse {
    output: Option<ConverseOutput>,
    stop_reason: Option<String>,
    usage: Option<ConverseUsage>,
    metrics: Option<ConverseMetrics>,
}

#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
struct ConverseOutput {
    message: Option<ConverseMessage>,
}

#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
struct ConverseMessage {
    #[allow(dead_code)]
    role: Option<String>,
    #[serde(default)]
    content: Vec<ContentBlock>,
}

/// A content block in a Converse API response.
#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
struct ContentBlock {
    text: Option<String>,
    #[allow(dead_code)]
    tool_use: Option<ToolUseBlock>,
}

#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
#[allow(dead_code)]
struct ToolUseBlock {
    tool_use_id: Option<String>,
    name: Option<String>,
    input: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
#[allow(clippy::struct_field_names)]
struct ConverseUsage {
    input_tokens: Option<u64>,
    output_tokens: Option<u64>,
    total_tokens: Option<u64>,
}

#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
#[allow(dead_code)]
struct ConverseMetrics {
    latency_ms: Option<u64>,
}

// ---------------------------------------------------------------------------
// Parser
// ---------------------------------------------------------------------------

/// Parse an AWS Bedrock Converse API response body into a normalized
/// `LlmProviderResponse`.
pub(super) fn parse_bedrock_response(
    body: &str,
) -> Result<LlmProviderResponse, ParseResponseError> {
    let response: ConverseResponse =
        serde_json::from_str(body).map_err(|e| ParseResponseError::Deserialize {
            provider: "aws-bedrock",
            source: e,
            content: body.to_string(),
        })?;

    let message = response.output.as_ref().and_then(|o| o.message.as_ref());

    let content = message
        .map(|msg| {
            msg.content
                .iter()
                .filter_map(|block| block.text.as_deref())
                .collect::<Vec<_>>()
                .join("")
        })
        .unwrap_or_default();

    if content.is_empty() && message.is_none() {
        return Err(ParseResponseError::NoContent {
            provider: "aws-bedrock",
            detail: "response has no output message".into(),
        });
    }

    let finish_reason = match response.stop_reason.as_deref() {
        Some("end_turn" | "stop_sequence") => FinishReason::Stop,
        Some("max_tokens") => FinishReason::Length,
        Some("tool_use") => FinishReason::ToolUse,
        Some(other) => FinishReason::Other(other.to_string()),
        None => FinishReason::Unknown,
    };

    let usage = response
        .usage
        .as_ref()
        .map(|u| TokenUsage {
            input_tokens: u.input_tokens,
            output_tokens: u.output_tokens,
            total_tokens: u.total_tokens,
        })
        .unwrap_or_default();

    let mut metadata = serde_json::Map::new();
    if let Some(metrics) = &response.metrics {
        if let Some(latency) = metrics.latency_ms {
            metadata.insert(
                "latency_ms".into(),
                serde_json::Value::Number(latency.into()),
            );
        }
    }

    Ok(LlmProviderResponse {
        content,
        // Bedrock Converse API does not return a model name in the response.
        model: "<unknown>".to_string(),
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
    fn test_deserialize_basic_response() {
        let json = r#"{
            "output": {
                "message": {
                    "role": "assistant",
                    "content": [
                        {"text": "Hello! How can I help you today?"}
                    ]
                }
            },
            "stopReason": "end_turn",
            "usage": {
                "inputTokens": 10,
                "outputTokens": 8,
                "totalTokens": 18
            },
            "metrics": {
                "latencyMs": 234
            }
        }"#;

        let resp: ConverseResponse = serde_json::from_str(json).unwrap();
        let msg = resp.output.unwrap().message.unwrap();
        assert_eq!(
            msg.content[0].text.as_deref(),
            Some("Hello! How can I help you today?")
        );
        assert_eq!(resp.stop_reason.as_deref(), Some("end_turn"));
        let usage = resp.usage.unwrap();
        assert_eq!(usage.input_tokens, Some(10));
        assert_eq!(usage.output_tokens, Some(8));
        assert_eq!(usage.total_tokens, Some(18));
        assert_eq!(resp.metrics.unwrap().latency_ms, Some(234));
    }

    #[test]
    fn test_deserialize_tool_use_response() {
        let json = r#"{
            "output": {
                "message": {
                    "role": "assistant",
                    "content": [
                        {
                            "toolUse": {
                                "toolUseId": "tool_123",
                                "name": "get_weather",
                                "input": {"location": "SF"}
                            }
                        }
                    ]
                }
            },
            "stopReason": "tool_use",
            "usage": {
                "inputTokens": 20,
                "outputTokens": 15,
                "totalTokens": 35
            }
        }"#;

        let resp: ConverseResponse = serde_json::from_str(json).unwrap();
        let msg = resp.output.unwrap().message.unwrap();
        let tool = msg.content[0].tool_use.as_ref().unwrap();
        assert_eq!(tool.name.as_deref(), Some("get_weather"));
        assert_eq!(tool.tool_use_id.as_deref(), Some("tool_123"));
        assert_eq!(resp.stop_reason.as_deref(), Some("tool_use"));
    }

    #[test]
    fn test_parse_basic_response() {
        let body = r#"{
            "output": {
                "message": {
                    "role": "assistant",
                    "content": [
                        {"text": "Hello! How can I help you today?"}
                    ]
                }
            },
            "stopReason": "end_turn",
            "usage": {
                "inputTokens": 10,
                "outputTokens": 8,
                "totalTokens": 18
            },
            "metrics": {
                "latencyMs": 234
            }
        }"#;

        let resp = parse_bedrock_response(body).unwrap();
        assert_eq!(resp.content, "Hello! How can I help you today?");
        assert_eq!(resp.finish_reason, FinishReason::Stop);
        assert!(resp.finish_reason.is_complete());
        assert_eq!(resp.usage.input_tokens, Some(10));
        assert_eq!(resp.usage.output_tokens, Some(8));
        assert_eq!(resp.usage.total_tokens, Some(18));
        assert_eq!(resp.metadata["latency_ms"], 234);
    }

    #[test]
    fn test_parse_stop_sequence_response() {
        let body = r#"{
            "output": {
                "message": {
                    "role": "assistant",
                    "content": [{"text": "Done."}]
                }
            },
            "stopReason": "stop_sequence",
            "usage": {
                "inputTokens": 5,
                "outputTokens": 2,
                "totalTokens": 7
            }
        }"#;

        let resp = parse_bedrock_response(body).unwrap();
        assert_eq!(resp.finish_reason, FinishReason::Stop);
    }

    #[test]
    fn test_parse_max_tokens_response() {
        let body = r#"{
            "output": {
                "message": {
                    "role": "assistant",
                    "content": [{"text": "Truncated..."}]
                }
            },
            "stopReason": "max_tokens",
            "usage": {
                "inputTokens": 50,
                "outputTokens": 4096,
                "totalTokens": 4146
            }
        }"#;

        let resp = parse_bedrock_response(body).unwrap();
        assert_eq!(resp.finish_reason, FinishReason::Length);
        assert!(!resp.finish_reason.is_complete());
    }

    #[test]
    fn test_parse_tool_use_response() {
        let body = r#"{
            "output": {
                "message": {
                    "role": "assistant",
                    "content": [
                        {
                            "toolUse": {
                                "toolUseId": "tool_123",
                                "name": "get_weather",
                                "input": {"location": "SF"}
                            }
                        }
                    ]
                }
            },
            "stopReason": "tool_use",
            "usage": {
                "inputTokens": 20,
                "outputTokens": 15,
                "totalTokens": 35
            }
        }"#;

        let resp = parse_bedrock_response(body).unwrap();
        assert_eq!(resp.content, "");
        assert_eq!(resp.finish_reason, FinishReason::ToolUse);
    }

    #[test]
    fn test_parse_no_output() {
        let body = r#"{
            "stopReason": "guardrail_intervened"
        }"#;

        let err = parse_bedrock_response(body).unwrap_err();
        assert!(matches!(err, ParseResponseError::NoContent { .. }));
    }

    #[test]
    fn test_parse_no_usage() {
        let body = r#"{
            "output": {
                "message": {
                    "role": "assistant",
                    "content": [{"text": "No usage info"}]
                }
            },
            "stopReason": "end_turn"
        }"#;

        let resp = parse_bedrock_response(body).unwrap();
        assert_eq!(resp.content, "No usage info");
        assert_eq!(resp.usage.input_tokens, None);
        assert_eq!(resp.usage.output_tokens, None);
        assert_eq!(resp.usage.total_tokens, None);
    }

    #[test]
    fn test_parse_guardrail_intervened() {
        let body = r#"{
            "output": {
                "message": {
                    "role": "assistant",
                    "content": [{"text": "Sorry, I can't help with that."}]
                }
            },
            "stopReason": "guardrail_intervened",
            "usage": {
                "inputTokens": 10,
                "outputTokens": 8,
                "totalTokens": 18
            }
        }"#;

        let resp = parse_bedrock_response(body).unwrap();
        assert_eq!(
            resp.finish_reason,
            FinishReason::Other("guardrail_intervened".into())
        );
    }

    #[test]
    fn test_parse_invalid_json() {
        let err = parse_bedrock_response("not json").unwrap_err();
        assert!(matches!(err, ParseResponseError::Deserialize { .. }));
    }

    #[test]
    fn test_bedrock_provider_dispatch() {
        let body = r#"{
            "output": {
                "message": {
                    "role": "assistant",
                    "content": [{"text": "hi from bedrock"}]
                }
            },
            "stopReason": "end_turn",
            "usage": {
                "inputTokens": 5,
                "outputTokens": 3,
                "totalTokens": 8
            }
        }"#;

        let resp = parse_response(LlmProvider::AwsBedrock, body).unwrap();
        assert_eq!(resp.content, "hi from bedrock");
    }
}
