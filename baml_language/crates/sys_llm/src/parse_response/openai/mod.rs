pub(super) mod chat_completions;

use serde::Deserialize;

/// Shared token usage struct for both `OpenAI` Chat and Responses APIs.
#[derive(Debug, Deserialize, Clone, PartialEq)]
pub(super) struct CompletionUsage {
    pub prompt_tokens: u64,
    pub completion_tokens: u64,
    pub total_tokens: u64,
    pub input_tokens_details: Option<serde_json::Value>,
}
