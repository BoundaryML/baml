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

fn google_content(candidate: &Candidate) -> Result<String, &'static str> {
    let content = candidate
        .content
        .as_ref()
        .ok_or("candidate has no content")?;
    Ok(text_content_part(&content.parts).unwrap_or_default())
}

fn vertex_content(candidate: &Candidate) -> Result<String, &'static str> {
    // Vertex takes first part only, without filtering thought parts.
    // Matches engine/baml-runtime vertex/response_handler.rs behavior.
    candidate
        .content
        .as_ref()
        .and_then(|content| content.parts.first())
        .and_then(|part| part.text.clone())
        .ok_or("candidate has no content parts")
}

fn parse_google_family_response(
    body: &str,
    provider: &'static str,
    extract_content: fn(&Candidate) -> Result<String, &'static str>,
) -> Result<LlmProviderResponse, ParseResponseError> {
    let response: GoogleResponse =
        serde_json::from_str(body).map_err(|e| ParseResponseError::Deserialize {
            provider,
            source: e,
            content: body.to_string(),
        })?;

    if response.candidates.len() != 1 {
        return Err(ParseResponseError::NoContent {
            provider,
            detail: format!(
                "expected exactly 1 candidate, got {}",
                response.candidates.len()
            ),
        });
    }

    let candidate = &response.candidates[0];

    let content = extract_content(candidate).map_err(|detail| ParseResponseError::NoContent {
        provider,
        detail: detail.into(),
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

// == Google AI parser =============================================

pub(super) fn parse_google_response(body: &str) -> Result<LlmProviderResponse, ParseResponseError> {
    parse_google_family_response(body, "google-ai", google_content)
}

// == Vertex AI parser =============================================

pub(super) fn parse_vertex_response(body: &str) -> Result<LlmProviderResponse, ParseResponseError> {
    parse_google_family_response(body, "vertex-ai", vertex_content)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{LlmProvider, parse_response::parse_response};

    type ResponseParser = fn(&str) -> Result<LlmProviderResponse, ParseResponseError>;

    #[test]
    fn test_google_family_shared_fields_and_content_policies() {
        let body = r#"{
            "candidates": [{
                "content": {
                    "parts": [
                        {"text": "thinking", "thought": true},
                        {"text": "answer"}
                    ]
                },
                "finishReason": "SAFETY"
            }],
            "usageMetadata": {
                "promptTokenCount": 100,
                "candidatesTokenCount": 20,
                "totalTokenCount": 120,
                "cachedContentTokenCount": 80
            }
        }"#;

        for (parse, expected_content) in [
            (parse_google_response as ResponseParser, "answer"),
            (parse_vertex_response as ResponseParser, "thinking"),
        ] {
            let response = parse(body).unwrap();
            assert_eq!(response.content, expected_content);
            assert_eq!(response.output.text_content(), expected_content);
            assert_eq!(response.finish_reason, FinishReason::Other("SAFETY".into()));
            assert_eq!(response.finish_reason_raw.as_deref(), Some("SAFETY"));
            assert_eq!(response.model, None);
            assert_eq!(response.usage.input_tokens, Some(100));
            assert_eq!(response.usage.output_tokens, Some(20));
            assert_eq!(response.usage.total_tokens, Some(120));
            assert_eq!(response.usage.cached_input_tokens, Some(80));
        }
    }

    #[test]
    fn test_google_family_errors_keep_provider_details() {
        let parsers = [
            (parse_google_response as ResponseParser, "google-ai"),
            (parse_vertex_response as ResponseParser, "vertex-ai"),
        ];

        for (parse, expected_provider) in parsers {
            match parse("{").unwrap_err() {
                ParseResponseError::Deserialize {
                    provider, content, ..
                } => {
                    assert_eq!(provider, expected_provider);
                    assert_eq!(content, "{");
                }
                error => panic!("unexpected error: {error}"),
            }

            for (body, expected_detail) in [
                (
                    r#"{"candidates":[]}"#,
                    "expected exactly 1 candidate, got 0",
                ),
                (
                    r#"{"candidates":[{},{}]}"#,
                    "expected exactly 1 candidate, got 2",
                ),
            ] {
                match parse(body).unwrap_err() {
                    ParseResponseError::NoContent { provider, detail } => {
                        assert_eq!(provider, expected_provider);
                        assert_eq!(detail, expected_detail);
                    }
                    error => panic!("unexpected error: {error}"),
                }
            }
        }

        for (parse, expected_provider, expected_detail) in [
            (
                parse_google_response as ResponseParser,
                "google-ai",
                "candidate has no content",
            ),
            (
                parse_vertex_response as ResponseParser,
                "vertex-ai",
                "candidate has no content parts",
            ),
        ] {
            match parse(r#"{"candidates":[{}]}"#).unwrap_err() {
                ParseResponseError::NoContent { provider, detail } => {
                    assert_eq!(provider, expected_provider);
                    assert_eq!(detail, expected_detail);
                }
                error => panic!("unexpected error: {error}"),
            }
        }
    }

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
