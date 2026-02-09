// TODO: Remove this once we consume all the fields in this struct via collector etc
#![allow(dead_code)]

use serde_json::Value as JsonValue;

/// Normalized response from any LLM provider.
///
/// This is the result of parsing a provider-specific HTTP response body
/// (e.g. `OpenAI` `ChatCompletion` JSON, Anthropic Message JSON) into a
/// common shape.
///
/// Provider-specific data that doesn't fit the common fields is
/// preserved in `metadata` as a JSON map.
#[derive(Debug, Clone)]
pub(crate) struct LlmProviderResponse {
    /// The text content extracted from the LLM response.
    /// For chat completions this is typically `choices[0].message.content`.
    pub content: String,

    /// The model identifier returned by the provider.
    pub model: String,

    /// Whether the response represents a complete generation
    /// (i.e. the model stopped naturally, not due to token limits).
    pub finish_reason: FinishReason,

    /// Token usage information, if the provider reported it.
    pub usage: TokenUsage,

    /// Arbitrary provider-specific metadata as a JSON map.
    ///
    /// Examples of what ends up here:
    /// - `OpenAI`: `id`, `system_fingerprint`, `created`, `logprobs`
    /// - Anthropic: `id`, `cache_creation_input_tokens`, `cache_read_input_tokens`, `service_tier`, `stop_sequence`
    /// - Google: `safety_ratings`, `citation_metadata`, `grounding_metadata`
    pub metadata: serde_json::Map<String, JsonValue>,
}

/// Normalized finish reason.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum FinishReason {
    /// The model finished generating naturally (`OpenAI`: `stop`, Anthropic: `end_turn`/`stop_sequence`, Google: `STOP`).
    Stop,
    /// The model hit the maximum token limit.
    Length,
    /// The model wants to call a tool/function.
    ToolUse,
    /// Some other provider-specific reason.
    Other(String),
    /// The provider did not report a finish reason.
    Unknown,
}

impl FinishReason {
    /// Whether this finish reason indicates a complete response.
    pub(crate) fn is_complete(&self) -> bool {
        matches!(self, FinishReason::Stop)
    }
}

/// Token usage reported by the provider.
#[allow(clippy::struct_field_names)]
#[derive(Debug, Clone, Default)]
pub(crate) struct TokenUsage {
    pub input_tokens: Option<u64>,
    pub output_tokens: Option<u64>,
    pub total_tokens: Option<u64>,
}
