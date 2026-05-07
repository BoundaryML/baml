mod anthropic;
mod bedrock;
mod google;
mod openai;

use crate::LlmProvider;

#[derive(Debug, Clone, Default)]
pub(crate) struct LlmOutput {
    pub parts: Vec<LlmOutputPart>,
}

#[derive(Debug, Clone)]
pub(crate) enum LlmOutputPart {
    Text {
        text: String,
    },
    Media {
        media: std::sync::Arc<baml_builtins2::MediaValue>,
        #[allow(dead_code)]
        provider_id: Option<String>,
        #[allow(dead_code)]
        metadata: serde_json::Value,
    },
}

impl LlmOutput {
    pub(crate) fn from_text(text: String) -> Self {
        let mut output = Self::default();
        output.push_text(text);
        output
    }

    pub(crate) fn push_text(&mut self, text: String) {
        if !text.is_empty() {
            self.parts.push(LlmOutputPart::Text { text });
        }
    }

    pub(crate) fn push_media(
        &mut self,
        media: std::sync::Arc<baml_builtins2::MediaValue>,
        provider_id: Option<String>,
        metadata: serde_json::Value,
    ) {
        self.parts.push(LlmOutputPart::Media {
            media,
            provider_id,
            metadata,
        });
    }

    pub(crate) fn text_content(&self) -> String {
        self.parts
            .iter()
            .filter_map(|part| match part {
                LlmOutputPart::Text { text } => Some(text.as_str()),
                LlmOutputPart::Media { .. } => None,
            })
            .collect::<Vec<_>>()
            .join("")
    }
}

#[derive(Debug, thiserror::Error)]
pub(crate) enum ParseResponseError {
    #[error("failed to deserialize {provider} response: {source}. Body:\n{content}")]
    Deserialize {
        provider: &'static str,
        #[source]
        source: serde_json::Error,
        content: String,
    },

    #[error("{provider} response has no content: {detail}")]
    NoContent {
        provider: &'static str,
        detail: String,
    },

    #[error("{provider} response has unsupported shape: {detail}")]
    UnsupportedResponseFormat {
        provider: &'static str,
        detail: String,
    },

    #[error("provider {0} is not yet supported for response parsing")]
    UnsupportedProvider(String),
}

// == Shared types ==================================================

// Allows dead_code while consumer only reads content + finish_reason_raw
#[allow(dead_code)]
/// Normalized response from any LLM provider.
#[derive(Debug, Clone)]
pub(crate) struct LlmProviderResponse {
    /// Text content extracted from the LLM response.
    pub content: String,
    /// Structured provider-native output parts, preserving text/media order.
    pub output: LlmOutput,
    /// Model identifier returned by the provider (absent for Google/Vertex).
    pub model: Option<String>,
    /// Normalized finish reason.
    pub finish_reason: FinishReason,
    /// Raw provider-reported finish reason string.
    pub finish_reason_raw: Option<String>,
    /// Token usage information.
    pub usage: TokenUsage,
}

/// Normalized finish reason.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum FinishReason {
    Stop,
    Length,
    ToolUse,
    Other(String),
    Unknown,
}

#[allow(dead_code)]
impl FinishReason {
    pub(crate) fn is_complete(&self) -> bool {
        matches!(self, FinishReason::Stop)
    }
}

/// Token usage reported by the provider.
#[allow(clippy::struct_field_names, dead_code)]
#[derive(Debug, Clone, Default)]
pub(crate) struct TokenUsage {
    pub input_tokens: Option<u64>,
    pub output_tokens: Option<u64>,
    pub total_tokens: Option<u64>,
    pub cached_input_tokens: Option<u64>,
}

// == Dispatcher ====================================================

/// Parse a raw HTTP response body into a normalized `LlmProviderResponse`.
pub(crate) fn parse_response(
    provider: LlmProvider,
    body: &str,
) -> Result<LlmProviderResponse, ParseResponseError> {
    match provider {
        LlmProvider::OpenAi
        | LlmProvider::OpenAiGeneric
        | LlmProvider::AzureOpenAi
        | LlmProvider::Ollama
        | LlmProvider::OpenRouter => openai::chat_completions::parse_openai_response(body),

        LlmProvider::Anthropic => anthropic::parse_anthropic_response(body),
        LlmProvider::AwsBedrock => bedrock::parse_bedrock_response(body),

        LlmProvider::OpenAiResponses => openai::responses::parse_openai_responses_response(body),
        LlmProvider::AiGatewayImages => openai::images::parse_openai_images_response(body),
        LlmProvider::GoogleAi => google::parse_google_response(body),
        LlmProvider::VertexAi => google::parse_vertex_response(body),
        LlmProvider::BamlFallback | LlmProvider::BamlRoundRobin => Err(
            ParseResponseError::UnsupportedProvider(format!("{provider:?}")),
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Minimal valid `OpenAI` Chat Completion response body.
    const OPENAI_CHAT_BODY: &str = r#"{
        "model": "gpt-4o",
        "choices": [{
            "index": 0,
            "message": { "role": "assistant", "content": "ok" },
            "finish_reason": "stop"
        }],
        "usage": { "prompt_tokens": 1, "completion_tokens": 1, "total_tokens": 2 }
    }"#;

    /// Minimal valid Anthropic Messages response body.
    const ANTHROPIC_BODY: &str = r#"{
        "id": "msg_1",
        "type": "message",
        "role": "assistant",
        "model": "claude-3-haiku-20240307",
        "content": [{"type": "text", "text": "ok"}],
        "stop_reason": "end_turn",
        "stop_sequence": null,
        "usage": { "input_tokens": 1, "output_tokens": 1 }
    }"#;

    /// Minimal valid `OpenAI` Responses API body.
    const RESPONSES_BODY: &str = r#"{
        "id": "resp_1",
        "object": "response",
        "status": "completed",
        "model": "gpt-4o",
        "output": [{
            "type": "message",
            "id": "msg_1",
            "role": "assistant",
            "content": [{"type": "output_text", "text": "ok"}]
        }],
        "usage": { "prompt_tokens": 1, "completion_tokens": 1, "total_tokens": 2 }
    }"#;

    /// Minimal valid Bedrock Converse API body.
    const BEDROCK_BODY: &str = r#"{
        "output": {
            "message": {
                "role": "assistant",
                "content": [{ "text": "ok" }]
            }
        },
        "stopReason": "end_turn",
        "usage": { "inputTokens": 1, "outputTokens": 1, "totalTokens": 2 }
    }"#;

    /// Minimal valid Google AI / Vertex AI body.
    const GOOGLE_BODY: &str = r#"{
        "candidates": [{
            "content": {
                "parts": [{"text": "ok"}],
                "role": "model"
            },
            "finishReason": "STOP"
        }],
        "usageMetadata": {
            "promptTokenCount": 1,
            "candidatesTokenCount": 1,
            "totalTokenCount": 2
        }
    }"#;

    // == OpenAI Chat variants all route to the same parser ========

    #[test]
    fn test_openai_variants_route_correctly() {
        for provider in [
            LlmProvider::OpenAi,
            LlmProvider::OpenAiGeneric,
            LlmProvider::AzureOpenAi,
            LlmProvider::Ollama,
            LlmProvider::OpenRouter,
        ] {
            let resp = parse_response(provider, OPENAI_CHAT_BODY).unwrap();
            assert_eq!(resp.content, "ok");
            assert_eq!(resp.finish_reason, FinishReason::Stop);
        }
    }

    // == Anthropic =================================================

    #[test]
    fn test_anthropic_routes_correctly() {
        let resp = parse_response(LlmProvider::Anthropic, ANTHROPIC_BODY).unwrap();
        assert_eq!(resp.content, "ok");
        assert_eq!(resp.finish_reason, FinishReason::Stop);
    }

    // == Bedrock Converse =========================================

    #[test]
    fn test_bedrock_routes_correctly() {
        let resp = parse_response(LlmProvider::AwsBedrock, BEDROCK_BODY).unwrap();
        assert_eq!(resp.content, "ok");
        assert_eq!(resp.finish_reason, FinishReason::Stop);
    }

    // == OpenAI Responses API =====================================

    #[test]
    fn test_openai_responses_routes_correctly() {
        let resp = parse_response(LlmProvider::OpenAiResponses, RESPONSES_BODY).unwrap();
        assert_eq!(resp.content, "ok");
        assert_eq!(resp.finish_reason, FinishReason::Stop);
    }

    // == Google AI ================================================

    #[test]
    fn test_google_ai_routes_correctly() {
        let resp = parse_response(LlmProvider::GoogleAi, GOOGLE_BODY).unwrap();
        assert_eq!(resp.content, "ok");
        assert_eq!(resp.finish_reason, FinishReason::Stop);
    }

    // == Vertex AI ================================================

    #[test]
    fn test_vertex_ai_routes_correctly() {
        let resp = parse_response(LlmProvider::VertexAi, GOOGLE_BODY).unwrap();
        assert_eq!(resp.content, "ok");
        assert_eq!(resp.finish_reason, FinishReason::Stop);
    }

    // == Meta-strategies remain unsupported =======================

    #[test]
    fn test_meta_strategies_unsupported() {
        for provider in [LlmProvider::BamlFallback, LlmProvider::BamlRoundRobin] {
            let err = parse_response(provider, "{}").unwrap_err();
            assert!(matches!(err, ParseResponseError::UnsupportedProvider(_)));
        }
    }

    // == cached_input_tokens round-trip per provider ==============

    #[test]
    fn test_openai_cached_tokens_round_trip() {
        let body = r#"{
            "model": "gpt-4o",
            "choices": [{
                "index": 0,
                "message": { "role": "assistant", "content": "ok" },
                "finish_reason": "stop"
            }],
            "usage": {
                "prompt_tokens": 100,
                "completion_tokens": 10,
                "total_tokens": 110,
                "input_tokens_details": { "cached_tokens": 50 }
            }
        }"#;
        let resp = parse_response(LlmProvider::OpenAi, body).unwrap();
        assert_eq!(resp.usage.cached_input_tokens, Some(50));
    }

    #[test]
    fn test_anthropic_cached_tokens_round_trip() {
        let body = r#"{
            "id": "msg_1",
            "type": "message",
            "role": "assistant",
            "model": "claude-3-haiku-20240307",
            "content": [{"type": "text", "text": "ok"}],
            "stop_reason": "end_turn",
            "stop_sequence": null,
            "usage": {
                "input_tokens": 100,
                "output_tokens": 10,
                "cache_read_input_tokens": 50
            }
        }"#;
        let resp = parse_response(LlmProvider::Anthropic, body).unwrap();
        assert_eq!(resp.usage.cached_input_tokens, Some(50));
    }

    #[test]
    fn test_google_cached_tokens_round_trip() {
        let body = r#"{
            "candidates": [{
                "content": {
                    "parts": [{"text": "ok"}],
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
        let resp = parse_response(LlmProvider::GoogleAi, body).unwrap();
        assert_eq!(resp.usage.cached_input_tokens, Some(80));
    }
}
