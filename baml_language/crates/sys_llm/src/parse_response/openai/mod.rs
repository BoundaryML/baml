pub(super) mod chat_completions;
pub(super) mod images;
pub(super) mod responses;

use serde::Deserialize;

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
