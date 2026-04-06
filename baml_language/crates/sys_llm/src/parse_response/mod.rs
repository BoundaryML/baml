mod anthropic;
mod openai;

use crate::LlmProvider;

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

// ── Shared types ──────────────────────────────────────────────────

// Allows dead_code while consumer only reads content + finish_reason_raw
#[allow(dead_code)]
/// Normalized response from any LLM provider.
#[derive(Debug, Clone)]
pub(crate) struct LlmProviderResponse {
    /// Text content extracted from the LLM response.
    pub content: String,
    /// Model identifier returned by the provider.
    pub model: String,
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

// ── Dispatcher ────────────────────────────────────────────────────

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

        LlmProvider::Anthropic | LlmProvider::AwsBedrock => {
            anthropic::parse_anthropic_response(body)
        }

        LlmProvider::OpenAiResponses => Err(ParseResponseError::UnsupportedProvider(
            "openai-responses".into(),
        )),
        LlmProvider::GoogleAi | LlmProvider::VertexAi => Err(
            ParseResponseError::UnsupportedProvider(format!("{provider:?}")),
        ),
        LlmProvider::BamlFallback | LlmProvider::BamlRoundRobin => Err(
            ParseResponseError::UnsupportedProvider(format!("{provider:?}")),
        ),
    }
}
