use serde::{Deserialize, Serialize};
// https://docs.anthropic.com/claude/reference/messages_post
#[derive(Debug, Deserialize, Serialize, Clone, PartialEq)]
pub struct AnthropicMessageContent {
    pub r#type: String,
    pub text: String,
}

#[derive(Debug, Deserialize, Serialize, Clone, PartialEq)]
pub struct AnthropicUsage {
    pub input_tokens: u32,
    pub output_tokens: u32,
}

#[derive(Debug, Deserialize, Serialize, Clone, PartialEq)]
pub struct AnthropicMessageResponse {
    pub id: String,
    pub role: String,
    pub r#type: String,
    pub content: Vec<AnthropicMessageContent>,
    pub model: String,
    pub stop_reason: Option<StopReason>, // can be null when streaming
    pub stop_sequence: Option<String>,
    pub usage: AnthropicUsage,
}

#[derive(Debug, Deserialize, Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum StopReason {
    MAX_TOKENS,
    STOP_SEQUENCE,
    END_TURN,
    #[serde(other)]
    UNKNOWN,
}

#[derive(Debug, Deserialize, Serialize, Clone, PartialEq)]
pub struct AnthropicErrorResponse {
    pub r#type: String,
    pub error: AnthropicErrorInner,
}

#[derive(Debug, Deserialize, Serialize, Clone, PartialEq)]
pub struct AnthropicErrorInner {
    pub r#type: String,
    pub message: String,
}
