use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AnthropicMessageContent {
    Text {
        text: String,
    },
    ToolUse {
        id: Option<String>,
        input: serde_json::Value,
        name: String,
    },
    RedactedThinking {
        data: String,
    },
    #[serde(other)]
    Other,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct AnthropicUsage {
    pub input_tokens: u64,
    pub output_tokens: u64,
    #[serde(default)]
    pub cache_creation_input_tokens: u64,
    #[serde(default)]
    pub cache_read_input_tokens: u64,
    #[serde(default)]
    pub service_tier: String,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct AnthropicMessageResponse {
    pub id: String,
    pub role: String,
    pub r#type: String,
    pub content: Vec<AnthropicMessageContent>,
    pub model: String,
    pub stop_reason: Option<String>,
    pub stop_sequence: Option<StopSequence>,
    pub usage: AnthropicUsage,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(transparent)]
pub struct StopSequence {
    value: String,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ContentType {
    Text,
    Image,
    TextDelta,
    ToolUse,
    ToolResult,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct TextContentBlock {
    #[serde(rename = "type")]
    pub _type: ContentType,
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

#[derive(Debug, Deserialize, Clone, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum MessageChunk {
    MessageStart(MessageStartChunk),
    ContentBlockStart(ContentBlockStartChunk),
    Ping,
    ContentBlockDelta(ContentBlockDeltaChunk),
    ContentBlockStop(ContentBlockStopChunk),
    MessageDelta(MessageDeltaChunk),
    MessageStop,
    Error {
        error: AnthropicErrorInner,
    },
    #[serde(other)]
    Other,
}

#[derive(Debug, Deserialize, Clone, PartialEq, Serialize)]
pub struct MessageStartChunk {
    pub message: AnthropicMessageResponse,
}

#[derive(Debug, Deserialize, Clone, PartialEq, Serialize)]
pub struct ContentBlockStartChunk {
    pub index: u32,
    pub content_block: TextContentBlock,
}

#[derive(Debug, Deserialize, Clone, PartialEq, Serialize)]
pub struct ContentBlockDeltaChunk {
    pub index: u32,
    pub delta: ContentBlockDelta,
}

#[derive(Debug, Deserialize, Clone, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ContentBlockDelta {
    TextDelta { text: String },
    SignatureDelta { signature: String },
    ThinkingDelta { thinking: String },
    Other,
}

#[derive(Debug, Deserialize, Clone, PartialEq, Serialize)]
pub struct ContentBlockStopChunk {
    pub index: u32,
}

#[derive(Debug, Deserialize, Clone, PartialEq, Serialize)]
pub struct MessageDeltaChunk {
    pub delta: StreamStop,
    pub usage: DeltaUsage,
}

#[derive(Debug, Deserialize, Clone, PartialEq, Serialize)]
pub struct TextDeltaContentBlock {
    #[serde(rename = "type")]
    pub _type: ContentType,
    pub text: String,
}

#[derive(Debug, Deserialize, Clone, PartialEq, Serialize)]
pub struct StreamStop {
    pub stop_reason: Option<String>,
    pub stop_sequence: Option<StopSequence>,
}

#[derive(Debug, Deserialize, Clone, PartialEq, Serialize)]
pub struct DeltaUsage {
    pub output_tokens: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_deserialize_message_response() {
        let json = r#"{
            "id": "msg_013QyXSmCitiepWfcCMHPTsQ",
            "type": "message",
            "role": "assistant",
            "model": "claude-3-haiku-20240307",
            "content": [
                {
                    "type": "text",
                    "text": "Hello! How can I help you today?"
                }
            ],
            "stop_reason": "end_turn",
            "stop_sequence": null,
            "usage": {
                "input_tokens": 9,
                "cache_creation_input_tokens": 51,
                "cache_read_input_tokens": 2258,
                "output_tokens": 8,
                "service_tier": "standard"
            }
        }"#;

        let response: AnthropicMessageResponse = serde_json::from_str(json).unwrap();
        assert_eq!(response.id, "msg_013QyXSmCitiepWfcCMHPTsQ");
        assert_eq!(response.role, "assistant");
        assert_eq!(response.model, "claude-3-haiku-20240307");
        assert_eq!(response.content.len(), 1);
        match &response.content[0] {
            AnthropicMessageContent::Text { text } => {
                assert_eq!(text, "Hello! How can I help you today?");
            }
            _ => panic!("Expected text content"),
        }
        assert_eq!(response.stop_reason, Some("end_turn".to_string()));
        assert_eq!(response.usage.input_tokens, 9);
        assert_eq!(response.usage.output_tokens, 8);
        assert_eq!(response.usage.cache_creation_input_tokens, 51);
        assert_eq!(response.usage.cache_read_input_tokens, 2258);
        assert_eq!(response.usage.service_tier, "standard");
    }

    #[test]
    fn test_deserialize_streaming_chunks() {
        let message_start = r#"{
            "type": "message_start",
            "message": {
                "id": "msg_1nZdL29xx5MUA1yADyHTEsnR8uuvGzszyY",
                "type": "message",
                "role": "assistant",
                "content": [],
                "model": "claude-3-sonnet-20240229",
                "stop_reason": null,
                "stop_sequence": null,
                "usage": {
                    "input_tokens": 25,
                    "output_tokens": 1
                }
            }
        }"#;

        let chunk: MessageChunk = serde_json::from_str(message_start).unwrap();
        match chunk {
            MessageChunk::MessageStart(start) => {
                assert_eq!(start.message.id, "msg_1nZdL29xx5MUA1yADyHTEsnR8uuvGzszyY");
                assert_eq!(start.message.usage.input_tokens, 25);
            }
            _ => panic!("Expected MessageStart chunk"),
        }

        let content_delta = r#"{
            "type": "content_block_delta",
            "index": 0,
            "delta": {
                "type": "text_delta",
                "text": "Hello"
            }
        }"#;

        let chunk: MessageChunk = serde_json::from_str(content_delta).unwrap();
        match chunk {
            MessageChunk::ContentBlockDelta(delta) => {
                assert_eq!(delta.index, 0);
                match &delta.delta {
                    ContentBlockDelta::TextDelta { text } => {
                        assert_eq!(text, "Hello");
                    }
                    _ => panic!("Expected TextDelta"),
                }
            }
            _ => panic!("Expected ContentBlockDelta chunk"),
        }
    }

    #[test]
    fn test_deserialize_error_response() {
        let json = r#"{
            "type": "error",
            "error": {
                "type": "invalid_request_error",
                "message": "Invalid API key"
            }
        }"#;

        let response: AnthropicErrorResponse = serde_json::from_str(json).unwrap();
        assert_eq!(response.r#type, "error");
        assert_eq!(response.error.r#type, "invalid_request_error");
        assert_eq!(response.error.message, Some("Invalid API key".to_string()));
    }

    #[test]
    fn test_deserialize_tool_use_content() {
        let json = r#"{
            "type": "tool_use",
            "id": "toolu_01A09q90qw90lq917835lq9",
            "name": "get_weather",
            "input": {"location": "San Francisco, CA", "unit": "celsius"}
        }"#;

        let content: AnthropicMessageContent = serde_json::from_str(json).unwrap();
        match content {
            AnthropicMessageContent::ToolUse { id, name, input } => {
                assert_eq!(id, Some("toolu_01A09q90qw90lq917835lq9".to_string()));
                assert_eq!(name, "get_weather");
                assert_eq!(input["location"], "San Francisco, CA");
                assert_eq!(input["unit"], "celsius");
            }
            _ => panic!("Expected ToolUse content"),
        }
    }
}
