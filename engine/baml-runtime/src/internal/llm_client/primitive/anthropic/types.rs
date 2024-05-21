use serde::{Deserialize, Serialize};

// https://docs.anthropic.com/claude/reference/messages_post
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct AnthropicMessageContent {
    pub r#type: String,
    pub text: String,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct AnthropicUsage {
    pub input_tokens: u64,
    pub output_tokens: u64,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct AnthropicMessageResponse {
    pub id: String,
    pub role: String,
    pub r#type: String,
    pub content: Vec<AnthropicMessageContent>,
    pub model: String,
    pub stop_reason: Option<StopReason>, // can be null when streaming
    pub stop_sequence: Option<StopSequence>,
    pub usage: AnthropicUsage,
}

#[derive(Clone, Debug, Deserialize, strum_macros::Display, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
#[strum(serialize_all = "snake_case")]
pub enum StopReason {
    MaxTokens,
    StopSequence,
    EndTurn,
    #[serde(other)]
    Unknown,
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
    pub message: String,
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
    Error(AnthropicErrorInner),
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
    pub delta: TextDeltaContentBlock,
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
    pub usage: DeltaUsage,
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
    pub stop_reason: Option<StopReason>,
    /// The stop sequence of this stream.
    pub stop_sequence: Option<StopSequence>,
}

/// The delta usage of the stream.
#[derive(Debug, Deserialize, Clone, PartialEq, Serialize)]
pub struct DeltaUsage {
    /// The number of output tokens which were used.
    pub output_tokens: u64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use anyhow::Result;

    #[test]
    fn deserialize() -> Result<()> {
        env_logger::init();

        let chunk = MessageChunk::ContentBlockDelta(ContentBlockDeltaChunk {
            index: 0,
            delta: TextDeltaContentBlock {
                _type: ContentType::TextDelta,
                text: "Hello".to_string(),
            },
        });
        println!("serialized = {}", serde_json::to_string(&chunk)?);

        let deserialized: MessageChunk = serde_json::from_str(r#"{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"Hello"}    }"#).unwrap();
        println!("deserialized = {:?}", deserialized);

        Ok(())
    }
}
