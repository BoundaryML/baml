use serde::Deserialize;

use super::{
    ParseResponseError,
    types::{FinishReason, LlmProviderResponse, TokenUsage},
};

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// Top-level response from the Gemini API (Google AI and Vertex AI).
#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
struct GeminiResponse {
    #[serde(default)]
    candidates: Vec<Candidate>,
    usage_metadata: Option<UsageMetaData>,
    model_version: Option<String>,
}

#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
struct Candidate {
    content: Option<Content>,
    finish_reason: Option<String>,
}

#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
struct Content {
    #[serde(default)]
    parts: Vec<Part>,
}

#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
struct Part {
    #[serde(default)]
    text: String,
    /// When `true`, this part is a "thinking" / chain-of-thought block
    /// emitted by reasoning models (e.g. gemini-2.5-*). Non-thought parts
    /// have this field absent or `false`.
    thought: Option<bool>,
}

#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
#[allow(clippy::struct_field_names)]
struct UsageMetaData {
    prompt_token_count: Option<u64>,
    candidates_token_count: Option<u64>,
    total_token_count: Option<u64>,
    cached_content_token_count: Option<u64>,
}

// ---------------------------------------------------------------------------
// Parser
// ---------------------------------------------------------------------------

/// Parse a Gemini API response body (Google AI or Vertex AI) into a normalized
/// `LlmProviderResponse`.
pub(super) fn parse_gemini_response(body: &str) -> Result<LlmProviderResponse, ParseResponseError> {
    let response: GeminiResponse =
        serde_json::from_str(body).map_err(|e| ParseResponseError::Deserialize {
            provider: "google",
            source: e,
            content: body.to_string(),
        })?;

    if response.candidates.is_empty() {
        return Err(ParseResponseError::NoContent {
            provider: "google",
            detail: "response has no candidates".into(),
        });
    }

    if response.candidates.len() > 1 {
        return Err(ParseResponseError::UnsupportedResponseFormat {
            provider: "google",
            detail: format!(
                "response contains {} candidates but we can only parse a single candidate; \
                 dropping {} candidate(s) would lose data",
                response.candidates.len(),
                response.candidates.len() - 1
            ),
        });
    }

    let candidate = &response.candidates[0];

    let content = candidate
        .content
        .as_ref()
        .map(|c| text_content_parts(&c.parts))
        .unwrap_or_default();

    let finish_reason = match candidate.finish_reason.as_deref() {
        Some("STOP") => FinishReason::Stop,
        Some("MAX_TOKENS") => FinishReason::Length,
        Some("SAFETY" | "RECITATION" | "BLOCKLIST" | "PROHIBITED_CONTENT" | "SPII") => {
            FinishReason::Other(candidate.finish_reason.clone().unwrap_or_default())
        }
        Some(other) => FinishReason::Other(other.to_string()),
        None => FinishReason::Unknown,
    };

    let usage = response
        .usage_metadata
        .as_ref()
        .map(|u| TokenUsage {
            input_tokens: u.prompt_token_count,
            output_tokens: u.candidates_token_count,
            total_tokens: u.total_token_count,
            cached_input_tokens: u.cached_content_token_count,
        })
        .unwrap_or_default();

    Ok(LlmProviderResponse {
        content,
        model: response
            .model_version
            .unwrap_or_else(|| "<unknown>".to_string()),
        finish_reason,
        finish_reason_raw: candidate.finish_reason.clone(),
        usage,
    })
}

/// Concatenate all non-thought text parts.
///
/// Reasoning models (e.g. gemini-2.5-*) emit `thought: true` parts that
/// contain chain-of-thought reasoning. We filter those out and join the
/// remaining text parts, matching the behavior of the engine's
/// `google/response_handler.rs`.
fn text_content_parts(parts: &[Part]) -> String {
    parts
        .iter()
        .filter(|p| !p.thought.unwrap_or(false))
        .map(|p| p.text.as_str())
        .collect::<Vec<_>>()
        .join("")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{LlmProvider, parse_response::parse_response};

    #[test]
    fn test_deserialize_basic_response() {
        let json = r#"{
            "candidates": [{
                "content": {
                    "parts": [{"text": "Hello world"}],
                    "role": "model"
                },
                "finishReason": "STOP"
            }],
            "usageMetadata": {
                "promptTokenCount": 10,
                "candidatesTokenCount": 5,
                "totalTokenCount": 15
            }
        }"#;

        let resp: GeminiResponse = serde_json::from_str(json).unwrap();
        assert_eq!(resp.candidates.len(), 1);
        assert_eq!(
            resp.candidates[0].content.as_ref().unwrap().parts[0].text,
            "Hello world"
        );
        assert_eq!(resp.candidates[0].finish_reason.as_deref(), Some("STOP"));
        let usage = resp.usage_metadata.unwrap();
        assert_eq!(usage.prompt_token_count, Some(10));
        assert_eq!(usage.candidates_token_count, Some(5));
        assert_eq!(usage.total_token_count, Some(15));
    }

    #[test]
    fn test_deserialize_with_thought_parts() {
        let json = r#"{
            "candidates": [{
                "content": {
                    "parts": [
                        {"text": "thinking...", "thought": true},
                        {"text": "actual answer"}
                    ],
                    "role": "model"
                },
                "finishReason": "STOP"
            }],
            "usageMetadata": {
                "promptTokenCount": 10,
                "candidatesTokenCount": 20,
                "totalTokenCount": 30
            }
        }"#;

        let resp: GeminiResponse = serde_json::from_str(json).unwrap();
        let parts = &resp.candidates[0].content.as_ref().unwrap().parts;
        assert_eq!(parts.len(), 2);
        assert_eq!(parts[0].thought, Some(true));
        assert_eq!(parts[1].thought, None);
    }

    #[test]
    fn test_parse_basic_response() {
        let body = r#"{
            "candidates": [{
                "content": {
                    "parts": [{"text": "Hello! How can I help you today?"}],
                    "role": "model"
                },
                "finishReason": "STOP"
            }],
            "usageMetadata": {
                "promptTokenCount": 10,
                "candidatesTokenCount": 8,
                "totalTokenCount": 18
            },
            "modelVersion": "gemini-1.5-flash"
        }"#;

        let resp = parse_gemini_response(body).unwrap();
        assert_eq!(resp.content, "Hello! How can I help you today?");
        assert_eq!(resp.model, "gemini-1.5-flash");
        assert_eq!(resp.finish_reason, FinishReason::Stop);
        assert!(resp.finish_reason.is_complete());
        assert_eq!(resp.usage.input_tokens, Some(10));
        assert_eq!(resp.usage.output_tokens, Some(8));
        assert_eq!(resp.usage.total_tokens, Some(18));
        assert_eq!(resp.model, "gemini-1.5-flash");
    }

    #[test]
    fn test_parse_with_thinking_parts() {
        let body = r#"{
            "candidates": [{
                "content": {
                    "parts": [
                        {"text": "Let me think about this...", "thought": true},
                        {"text": "More reasoning here.", "thought": true},
                        {"text": "The answer is 42."}
                    ],
                    "role": "model"
                },
                "finishReason": "STOP"
            }],
            "usageMetadata": {
                "promptTokenCount": 20,
                "candidatesTokenCount": 30,
                "totalTokenCount": 50
            },
            "modelVersion": "gemini-2.5-pro"
        }"#;

        let resp = parse_gemini_response(body).unwrap();
        assert_eq!(resp.content, "The answer is 42.");
    }

    #[test]
    fn test_parse_multiple_non_thought_parts() {
        let body = r#"{
            "candidates": [{
                "content": {
                    "parts": [
                        {"text": "Reasoning...", "thought": true},
                        {"text": "Part one. "},
                        {"text": "Part two."}
                    ],
                    "role": "model"
                },
                "finishReason": "STOP"
            }],
            "usageMetadata": {
                "promptTokenCount": 10,
                "candidatesTokenCount": 20,
                "totalTokenCount": 30
            }
        }"#;

        let resp = parse_gemini_response(body).unwrap();
        assert_eq!(resp.content, "Part one. Part two.");
    }

    #[test]
    fn test_parse_no_content_candidate() {
        let body = r#"{
            "candidates": [{
                "finishReason": "SAFETY",
                "finishMessage": "Content blocked by safety filters"
            }],
            "usageMetadata": {
                "promptTokenCount": 10,
                "candidatesTokenCount": 0,
                "totalTokenCount": 10
            }
        }"#;

        let resp = parse_gemini_response(body).unwrap();
        assert_eq!(resp.content, "");
        assert_eq!(resp.finish_reason, FinishReason::Other("SAFETY".into()));
    }

    #[test]
    fn test_parse_no_candidates() {
        let body = r#"{
            "candidates": [],
            "usageMetadata": {
                "promptTokenCount": 10,
                "candidatesTokenCount": 0,
                "totalTokenCount": 10
            }
        }"#;

        let err = parse_gemini_response(body).unwrap_err();
        assert!(matches!(err, ParseResponseError::NoContent { .. }));
    }

    #[test]
    fn test_parse_multiple_candidates() {
        let body = r#"{
            "candidates": [
                {
                    "content": {"parts": [{"text": "A"}], "role": "model"},
                    "finishReason": "STOP"
                },
                {
                    "content": {"parts": [{"text": "B"}], "role": "model"},
                    "finishReason": "STOP"
                }
            ],
            "usageMetadata": {
                "promptTokenCount": 10,
                "candidatesTokenCount": 5,
                "totalTokenCount": 15
            }
        }"#;

        let err = parse_gemini_response(body).unwrap_err();
        assert!(matches!(
            err,
            ParseResponseError::UnsupportedResponseFormat { .. }
        ));
    }

    #[test]
    fn test_parse_max_tokens_finish() {
        let body = r#"{
            "candidates": [{
                "content": {
                    "parts": [{"text": "Truncated..."}],
                    "role": "model"
                },
                "finishReason": "MAX_TOKENS"
            }],
            "usageMetadata": {
                "promptTokenCount": 10,
                "candidatesTokenCount": 100,
                "totalTokenCount": 110
            }
        }"#;

        let resp = parse_gemini_response(body).unwrap();
        assert_eq!(resp.finish_reason, FinishReason::Length);
        assert!(!resp.finish_reason.is_complete());
    }

    #[test]
    fn test_parse_cached_tokens_in_metadata() {
        let body = r#"{
            "candidates": [{
                "content": {
                    "parts": [{"text": "Cached response"}],
                    "role": "model"
                },
                "finishReason": "STOP"
            }],
            "usageMetadata": {
                "promptTokenCount": 100,
                "candidatesTokenCount": 10,
                "totalTokenCount": 110,
                "cachedContentTokenCount": 80
            }
        }"#;

        let resp = parse_gemini_response(body).unwrap();
        assert_eq!(resp.usage.cached_input_tokens, Some(80));
    }

    #[test]
    fn test_parse_zero_cached_tokens_not_in_metadata() {
        let body = r#"{
            "candidates": [{
                "content": {
                    "parts": [{"text": "Response"}],
                    "role": "model"
                },
                "finishReason": "STOP"
            }],
            "usageMetadata": {
                "promptTokenCount": 100,
                "candidatesTokenCount": 10,
                "totalTokenCount": 110,
                "cachedContentTokenCount": 0
            }
        }"#;

        let resp = parse_gemini_response(body).unwrap();
        assert_eq!(resp.usage.cached_input_tokens, Some(0));
    }

    #[test]
    fn test_parse_optional_usage_metadata() {
        let body = r#"{
            "candidates": [{
                "content": {
                    "parts": [{"text": "No usage info"}],
                    "role": "model"
                },
                "finishReason": "STOP"
            }]
        }"#;

        let resp = parse_gemini_response(body).unwrap();
        assert_eq!(resp.content, "No usage info");
        assert_eq!(resp.usage.input_tokens, None);
        assert_eq!(resp.usage.output_tokens, None);
        assert_eq!(resp.usage.total_tokens, None);
    }

    #[test]
    fn test_parse_invalid_json() {
        let err = parse_gemini_response("not json").unwrap_err();
        assert!(matches!(err, ParseResponseError::Deserialize { .. }));
    }

    #[test]
    fn test_google_and_vertex_provider_dispatch() {
        let body = r#"{
            "candidates": [{
                "content": {
                    "parts": [{"text": "hi"}],
                    "role": "model"
                },
                "finishReason": "STOP"
            }],
            "usageMetadata": {
                "promptTokenCount": 5,
                "candidatesTokenCount": 2,
                "totalTokenCount": 7
            }
        }"#;

        let google_resp = parse_response(LlmProvider::GoogleAi, body).unwrap();
        assert_eq!(google_resp.content, "hi");

        let vertex_resp = parse_response(LlmProvider::VertexAi, body).unwrap();
        assert_eq!(vertex_resp.content, "hi");
    }

    #[test]
    fn test_parse_model_version_used_as_model() {
        let body = r#"{
            "candidates": [{
                "content": {
                    "parts": [{"text": "hi"}],
                    "role": "model"
                },
                "finishReason": "STOP"
            }],
            "usageMetadata": {
                "promptTokenCount": 5,
                "candidatesTokenCount": 2,
                "totalTokenCount": 7
            },
            "modelVersion": "gemini-2.5-flash"
        }"#;

        let resp = parse_gemini_response(body).unwrap();
        assert_eq!(resp.model, "gemini-2.5-flash");
    }

    #[test]
    fn test_parse_no_model_version() {
        let body = r#"{
            "candidates": [{
                "content": {
                    "parts": [{"text": "hi"}],
                    "role": "model"
                },
                "finishReason": "STOP"
            }],
            "usageMetadata": {
                "promptTokenCount": 5,
                "candidatesTokenCount": 2,
                "totalTokenCount": 7
            }
        }"#;

        let resp = parse_gemini_response(body).unwrap();
        assert_eq!(resp.model, "<unknown>");
    }
}
