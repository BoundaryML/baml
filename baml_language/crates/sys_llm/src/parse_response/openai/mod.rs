pub(super) mod chat_completions;
pub(super) mod images;
pub(super) mod responses;

use serde::Deserialize;

use super::TokenUsage;

/// Shared token usage struct for both `OpenAI` Chat and Responses APIs.
#[derive(Debug, Deserialize, Clone, PartialEq)]
pub(super) struct CompletionUsage {
    #[serde(alias = "input_tokens")]
    pub prompt_tokens: u64,
    #[serde(alias = "output_tokens")]
    pub completion_tokens: u64,
    pub total_tokens: u64,
    #[serde(alias = "prompt_tokens_details")]
    pub input_tokens_details: Option<serde_json::Value>,
}

pub(super) fn token_usage_from_completion(usage: &CompletionUsage) -> TokenUsage {
    TokenUsage {
        input_tokens: Some(usage.prompt_tokens),
        output_tokens: Some(usage.completion_tokens),
        total_tokens: Some(usage.total_tokens),
        cached_input_tokens: usage.input_tokens_details.as_ref().and_then(|details| {
            details
                .get("cached_tokens")
                .and_then(serde_json::Value::as_u64)
        }),
    }
}
