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
    pub value: String,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize, Serialize, Clone, PartialEq)]
pub struct AnthropicErrorResponse {
    pub r#type: String,
    pub error: AnthropicErrorInner,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize, Serialize, Clone, PartialEq)]
pub struct AnthropicErrorInner {
    pub r#type: String,
    pub message: Option<String>,
    pub details: Option<serde_json::Value>,
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
