use serde::Deserialize;

use super::{FinishReason, LlmProviderResponse, ParseResponseError, TokenUsage};

// == Shared serde types (Google as superset) ======================

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct GoogleResponse {
    candidates: Vec<Candidate>,
    usage_metadata: Option<UsageMetaData>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Candidate {
    content: Option<Content>,
    finish_reason: Option<String>,
}

#[derive(Debug, Deserialize)]
struct Content {
    #[serde(default)]
    parts: Vec<Part>,
}

#[derive(Debug, Deserialize)]
struct Part {
    text: Option<String>,
    thought: Option<bool>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
#[allow(clippy::struct_field_names)]
struct UsageMetaData {
    prompt_token_count: Option<u64>,
    candidates_token_count: Option<u64>,
    total_token_count: Option<u64>,
    cached_content_token_count: Option<u64>,
}

// == Helpers =======================================================

fn map_finish_reason(raw: Option<&str>) -> FinishReason {
    match raw {
        Some("STOP") => FinishReason::Stop,
        Some("MAX_TOKENS") => FinishReason::Length,
        Some(other) => FinishReason::Other(other.to_string()),
        None => FinishReason::Unknown,
    }
}

/// Filter out thought parts and join remaining text.
/// Mirrors `engine/baml-runtime/.../google/response_handler.rs:107-119`.
fn text_content_part(parts: &[Part]) -> Option<String> {
    let non_thought_parts: Vec<&str> = parts
        .iter()
        .filter(|part| !part.thought.unwrap_or(false))
        .filter_map(|part| part.text.as_deref())
        .collect();

    if non_thought_parts.is_empty() {
        None
    } else {
        Some(non_thought_parts.join(""))
    }
}

// == Google AI parser =============================================

pub(super) fn parse_google_response(body: &str) -> Result<LlmProviderResponse, ParseResponseError> {
    let response: GoogleResponse =
        serde_json::from_str(body).map_err(|e| ParseResponseError::Deserialize {
            provider: "google-ai",
            source: e,
            content: body.to_string(),
        })?;

    if response.candidates.len() != 1 {
        return Err(ParseResponseError::NoContent {
            provider: "google-ai",
            detail: format!(
                "expected exactly 1 candidate, got {}",
                response.candidates.len()
            ),
        });
    }

    let candidate = &response.candidates[0];

    let Some(content_obj) = candidate.content.as_ref() else {
        return Err(ParseResponseError::NoContent {
            provider: "google-ai",
            detail: "candidate has no content".into(),
        });
    };

    let content = text_content_part(&content_obj.parts).unwrap_or_default();

    let finish_reason = map_finish_reason(candidate.finish_reason.as_deref());

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
        output: crate::parse_response::LlmOutput::from_text(content.clone()),
        content,
        model: None,
        finish_reason,
        finish_reason_raw: candidate.finish_reason.clone(),
        usage,
    })
}

// == Vertex AI parser =============================================

pub(super) fn parse_vertex_response(body: &str) -> Result<LlmProviderResponse, ParseResponseError> {
    let response: GoogleResponse =
        serde_json::from_str(body).map_err(|e| ParseResponseError::Deserialize {
            provider: "vertex-ai",
            source: e,
            content: body.to_string(),
        })?;

    if response.candidates.len() != 1 {
        return Err(ParseResponseError::NoContent {
            provider: "vertex-ai",
            detail: format!(
                "expected exactly 1 candidate, got {}",
                response.candidates.len()
            ),
        });
    }

    let candidate = &response.candidates[0];

    // Vertex takes first part only, no thought filtering.
    // Matches engine/baml-runtime vertex/response_handler.rs behavior.
    let content = candidate
        .content
        .as_ref()
        .and_then(|c| c.parts.first())
        .and_then(|p| p.text.clone())
        .ok_or_else(|| ParseResponseError::NoContent {
            provider: "vertex-ai",
            detail: "candidate has no content parts".into(),
        })?;

    let finish_reason = map_finish_reason(candidate.finish_reason.as_deref());

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
        output: crate::parse_response::LlmOutput::from_text(content.clone()),
        content,
        model: None,
        finish_reason,
        finish_reason_raw: candidate.finish_reason.clone(),
        usage,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{LlmProvider, parse_response::parse_response};

    // == Google AI tests ==========================================

    #[test]
    fn test_google_basic_response() {
        let body = r#"{
            "candidates": [{
                "content": {
                    "parts": [{"text": "Hello from Gemini!"}],
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

        let resp = parse_google_response(body).unwrap();
        assert_eq!(resp.content, "Hello from Gemini!");
        assert_eq!(resp.finish_reason, FinishReason::Stop);
        assert_eq!(resp.usage.input_tokens, Some(10));
        assert_eq!(resp.usage.output_tokens, Some(5));
        assert_eq!(resp.usage.total_tokens, Some(15));
    }

    #[test]
    fn test_google_thought_filtering() {
        let body = r#"{
            "candidates": [{
                "content": {
                    "parts": [
                        {"text": "thinking...", "thought": true},
                        {"text": "The answer is 42."}
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

        let resp = parse_google_response(body).unwrap();
        assert_eq!(resp.content, "The answer is 42.");
    }

    #[test]
    fn test_google_no_candidates() {
        let body = r#"{
            "candidates": [],
            "usageMetadata": {
                "promptTokenCount": 10,
                "candidatesTokenCount": 0,
                "totalTokenCount": 10
            }
        }"#;

        let err = parse_google_response(body).unwrap_err();
        assert!(matches!(err, ParseResponseError::NoContent { .. }));
    }

    #[test]
    fn test_google_no_content() {
        let body = r#"{
            "candidates": [{"finishReason": "SAFETY"}],
            "usageMetadata": {
                "promptTokenCount": 10,
                "candidatesTokenCount": 0,
                "totalTokenCount": 10
            }
        }"#;

        let err = parse_google_response(body).unwrap_err();
        assert!(matches!(err, ParseResponseError::NoContent { .. }));
    }

    #[test]
    fn test_google_max_tokens() {
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

        let resp = parse_google_response(body).unwrap();
        assert_eq!(resp.finish_reason, FinishReason::Length);
    }

    #[test]
    fn test_google_missing_usage_metadata() {
        let body = r#"{
            "candidates": [{
                "content": {
                    "parts": [{"text": "hi"}],
                    "role": "model"
                },
                "finishReason": "STOP"
            }]
        }"#;

        let resp = parse_google_response(body).unwrap();
        assert_eq!(resp.content, "hi");
        assert_eq!(resp.usage.input_tokens, None);
    }

    #[test]
    fn test_google_cached_tokens() {
        let body = r#"{
            "candidates": [{
                "content": {
                    "parts": [{"text": "cached response"}],
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

        let resp = parse_google_response(body).unwrap();
        assert_eq!(resp.usage.cached_input_tokens, Some(80));
    }

    #[test]
    fn test_google_provider_dispatch() {
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

        let resp = parse_response(LlmProvider::GoogleAi, body).unwrap();
        assert_eq!(resp.content, "hi");
    }

    // == Vertex AI tests ==========================================

    #[test]
    fn test_vertex_basic_response() {
        let body = r#"{
            "candidates": [{
                "content": {
                    "parts": [{"text": "Hello from Vertex!"}],
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

        let resp = parse_vertex_response(body).unwrap();
        assert_eq!(resp.content, "Hello from Vertex!");
        assert_eq!(resp.finish_reason, FinishReason::Stop);
    }

    #[test]
    fn test_vertex_first_part_only() {
        let body = r#"{
            "candidates": [{
                "content": {
                    "parts": [
                        {"text": "First part"},
                        {"text": "Second part (ignored)"}
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

        let resp = parse_vertex_response(body).unwrap();
        assert_eq!(resp.content, "First part");
    }

    #[test]
    fn test_vertex_missing_usage_metadata() {
        let body = r#"{
            "candidates": [{
                "content": {
                    "parts": [{"text": "hi"}],
                    "role": "model"
                },
                "finishReason": "STOP"
            }]
        }"#;

        let resp = parse_vertex_response(body).unwrap();
        assert_eq!(resp.content, "hi");
        assert_eq!(resp.usage.input_tokens, None);
    }

    #[test]
    fn test_vertex_no_content_parts() {
        let body = r#"{
            "candidates": [{
                "content": { "parts": [] },
                "finishReason": "STOP"
            }],
            "usageMetadata": {
                "promptTokenCount": 10,
                "candidatesTokenCount": 0,
                "totalTokenCount": 10
            }
        }"#;

        let err = parse_vertex_response(body).unwrap_err();
        assert!(matches!(err, ParseResponseError::NoContent { .. }));
    }

    #[test]
    fn test_vertex_provider_dispatch() {
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

        let resp = parse_response(LlmProvider::VertexAi, body).unwrap();
        assert_eq!(resp.content, "hi");
    }
}
