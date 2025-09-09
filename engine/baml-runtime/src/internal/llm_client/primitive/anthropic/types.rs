use std::string;

use serde::{Deserialize, Serialize};

// https://docs.anthropic.com/claude/reference/messages_post
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AnthropicMessageContent {
    // type: text
    Text {
        text: String,
    },
    // // type: tool_use
    ToolUse {
        id: Option<String>,
        input: serde_json::Value,
        name: String,
    },
    // // type: thinking
    // Thinking {
    //     signature: Option<String>,
    //     thinking: String,
    // },
    // type: redacted_thinking
    RedactedThinking {
        data: String,
    },
    // fallback for unknown types
    #[serde(other)]
    Other,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct AnthropicUsage {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_creation_input_tokens: Option<u64>,
    pub cache_read_input_tokens: Option<u64>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct AnthropicMessageResponse {
    pub id: String,
    pub role: String,
    pub r#type: String,
    pub content: Vec<AnthropicMessageContent>,
    pub model: String,
    pub stop_reason: Option<String>, // can be null when streaming
    pub stop_sequence: Option<StopSequence>,
    pub usage: AnthropicUsage,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(transparent)]
pub struct StopSequence {
    value: String,
}

// TODO(sam): this is WRONG. this enum should use struct variants with tagged parsing
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ContentType {
    /// text
    Text,
    /// image
    Image,
    /// text_delta
    TextDelta,
    /// tool_use
    ToolUse,
    /// tool_result
    ToolResult,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct TextContentBlock {
    /// The content type. It is always `text`.
    #[serde(rename = "type")]
    pub _type: ContentType,
    /// The text content.
    pub text: String,
}

#[derive(Debug, Deserialize, Serialize, Clone, PartialEq)]
pub struct AnthropicErrorResponse {
    pub r#type: String,
    pub error: AnthropicErrorInner,
}

#[derive(Debug, Deserialize, Serialize, Clone, PartialEq)]
pub struct AnthropicErrorInner {
    pub r#type: String,
    pub message: Option<String>,
    pub details: Option<serde_json::Value>,
}

/// The stream chunk of messages.
#[derive(Debug, Deserialize, Clone, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum MessageChunk {
    /// Message start chunk.
    MessageStart(MessageStartChunk),
    /// Content block start chunk.
    ContentBlockStart(ContentBlockStartChunk),
    /// Ping chunk.
    Ping,
    /// Content block delta chunk.
    ContentBlockDelta(ContentBlockDeltaChunk),
    /// Content block stop chunk.
    ContentBlockStop(ContentBlockStopChunk),
    /// Message delta chunk.
    MessageDelta(MessageDeltaChunk),
    /// Message stop chunk.
    MessageStop,
    Error {
        error: AnthropicErrorInner,
    },
    /// Fallback for unknown types
    #[serde(other)]
    Other,
}

/// The message start chunk.
#[derive(Debug, Deserialize, Clone, PartialEq, Serialize)]
pub struct MessageStartChunk {
    /// The start message.
    pub message: AnthropicMessageResponse,
}

/// The content block start chunk.
#[derive(Debug, Deserialize, Clone, PartialEq, Serialize)]
pub struct ContentBlockStartChunk {
    /// The index.
    pub index: u32,
    /// The text content block of start.
    pub content_block: TextContentBlock,
}

/// The content block delta chunk.
#[derive(Debug, Deserialize, Clone, PartialEq, Serialize)]
pub struct ContentBlockDeltaChunk {
    /// The index.
    pub index: u32,
    /// The text delta content block.
    pub delta: ContentBlockDelta,
}

#[derive(Debug, Deserialize, Clone, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ContentBlockDelta {
    TextDelta {
        text: String,
    },
    SignatureDelta {
        signature: String,
    },
    ThinkingDelta {
        thinking: String,
    },
    ToolUse {
        id: String,
        input: serde_json::Value,
        name: String,
    },
    ServerToolUse {
        id: String,
        input: serde_json::Value,
        name: String,
    },
    WebSearchToolResult {
        content: String,
        tool_use_id: String,
    },
    CodeExecutionResult {
        content: serde_json::Value,
        tool_use_id: String,
    },
    MCPToolUse {
        id: String,
        input: serde_json::Value,
        name: String,
        server_name: String,
    },
    MCPToolResult {
        content: String,
        is_error: bool,
        tool_use_id: String,
    },
    Other,
}

/// The content block stop chunk.
#[derive(Debug, Deserialize, Clone, PartialEq, Serialize)]
pub struct ContentBlockStopChunk {
    /// The index.
    pub index: u32,
}

/// The message delta chunk.
#[derive(Debug, Deserialize, Clone, PartialEq, Serialize)]
pub struct MessageDeltaChunk {
    /// The result of this stream.
    pub delta: StreamStop,
    /// The billing and rate-limit usage of this stream.
    pub usage: AnthropicUsage,
}

/// The text delta content block.
#[derive(Debug, Deserialize, Clone, PartialEq, Serialize)]
pub struct TextDeltaContentBlock {
    /// The content type. It is always `text_delta`.
    #[serde(rename = "type")]
    pub _type: ContentType,
    /// The text delta content.
    pub text: String,
}

/// The stream stop information.
#[derive(Debug, Deserialize, Clone, PartialEq, Serialize)]
pub struct StreamStop {
    /// The stop reason of this stream.
    pub stop_reason: Option<String>,
    /// The stop sequence of this stream.
    pub stop_sequence: Option<StopSequence>,
}

#[cfg(test)]
mod tests {
    use anyhow::Result;

    use super::*;

    #[test]
    fn deserialize() -> Result<()> {
        env_logger::init();

        let chunk = MessageChunk::ContentBlockDelta(ContentBlockDeltaChunk {
            index: 0,
            delta: ContentBlockDelta::TextDelta {
                text: "Hello".to_string(),
            },
        });
        println!("serialized = {}", serde_json::to_string(&chunk)?);

        let deserialized: MessageChunk = serde_json::from_str(r#"{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"Hello"}    }"#).unwrap();
        println!("deserialized = {deserialized:?}");

        Ok(())
    }
}
