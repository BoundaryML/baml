use serde::{Deserialize, Deserializer, Serialize};

#[derive(Debug, Deserialize, Clone, PartialEq)]
#[serde(untagged)]
pub(super) enum ChatCompletionResponse {
    Success(ChatCompletionGeneric<ChatCompletionChoice>),
    Error(OpenAiErrorWrapper),
}

#[derive(Debug, Deserialize, Clone, PartialEq)]
pub(super) struct OpenAiErrorWrapper {
    pub error: OpenAiErrorResponse,
}

#[derive(Debug, Deserialize, Clone, PartialEq)]
pub(super) struct OpenAiErrorResponse {
    pub message: String,
    #[serde(rename = "type")]
    pub r#type: String,
    pub param: Option<String>,
    pub code: Option<String>,
}

#[derive(Debug, Deserialize, Clone, PartialEq)]
pub(super) struct ChatCompletionGeneric<C> {
    pub id: Option<String>,
    pub choices: Vec<C>,
    #[serde(default, deserialize_with = "deserialize_float_to_u32")]
    pub created: Option<u32>,
    pub model: String,
    pub system_fingerprint: Option<String>,
    pub object: Option<String>,
    pub usage: Option<CompletionUsage>,
}

fn deserialize_float_to_u32<'de, D>(deserializer: D) -> Result<Option<u32>, D::Error>
where
    D: Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum FloatOrInt {
        Int(u32),
        Float(f64),
    }

    match Option::<FloatOrInt>::deserialize(deserializer)? {
        Some(FloatOrInt::Int(i)) => Ok(Some(i)),
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        Some(FloatOrInt::Float(f)) => Ok(Some(f.floor() as u32)),
        None => Ok(None),
    }
}

#[derive(Debug, Deserialize, Clone, PartialEq)]
pub(super) struct ChatCompletionChoice {
    pub index: u32,
    pub message: ChatCompletionResponseMessage,
    pub finish_reason: Option<String>,
    pub logprobs: Option<ChatChoiceLogprobs>,
}

#[allow(clippy::struct_field_names)]
#[derive(Debug, Deserialize, Serialize, Clone, PartialEq)]
pub(super) struct CompletionUsage {
    pub prompt_tokens: u64,
    pub completion_tokens: u64,
    pub total_tokens: u64,
}

#[derive(Debug, Deserialize, Clone, PartialEq)]
pub(super) struct ChatCompletionResponseMessage {
    pub content: Option<String>,
    pub role: ChatCompletionMessageRole,
}

#[derive(Debug, Deserialize, Clone, Default, PartialEq)]
#[serde(rename_all = "lowercase")]
pub(super) enum ChatCompletionMessageRole {
    System,
    #[default]
    User,
    Assistant,
    Tool,
    Function,
}

#[derive(Debug, Deserialize, Serialize, Clone, PartialEq)]
pub(super) struct ChatChoiceLogprobs {
    pub content: Option<Vec<ChatCompletionTokenLogprob>>,
}

#[derive(Debug, Deserialize, Serialize, Clone, PartialEq)]
pub(super) struct ChatCompletionTokenLogprob {
    pub token: String,
    pub logprob: f32,
    pub bytes: Option<Vec<u8>>,
    pub top_logprobs: Vec<TopLogprobs>,
}

#[derive(Debug, Deserialize, Serialize, Clone, PartialEq)]
pub(super) struct TopLogprobs {
    pub token: String,
    pub logprob: f32,
    pub bytes: Option<Vec<u8>>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_deserialize_chat_completion_response() {
        let json = r#"{
            "id": "chatcmpl-123",
            "object": "chat.completion",
            "created": 1677652288,
            "model": "gpt-3.5-turbo-0125",
            "system_fingerprint": "fp_44709d6fcb",
            "choices": [{
                "index": 0,
                "message": {
                    "role": "assistant",
                    "content": "Hello! How can I help you today?"
                },
                "logprobs": null,
                "finish_reason": "stop"
            }],
            "usage": {
                "prompt_tokens": 9,
                "completion_tokens": 12,
                "total_tokens": 21
            }
        }"#;

        let response: ChatCompletionResponse = serde_json::from_str(json).unwrap();
        let ChatCompletionResponse::Success(response) = response else {
            panic!("expected success");
        };
        assert_eq!(response.id, Some("chatcmpl-123".to_string()));
        assert_eq!(response.model, "gpt-3.5-turbo-0125");
        assert_eq!(response.choices.len(), 1);
        assert_eq!(
            response.choices[0].message.content,
            Some("Hello! How can I help you today?".to_string())
        );
        assert_eq!(response.choices[0].finish_reason, Some("stop".to_string()));
        assert!(response.usage.is_some());
        assert_eq!(response.usage.as_ref().unwrap().total_tokens, 21);
    }

    #[test]
    fn test_deserialize_float_created() {
        let json = r#"{
            "model": "custom-model",
            "created": 1677652288.5,
            "choices": [{
                "index": 0,
                "message": {
                    "role": "assistant",
                    "content": "Hello"
                },
                "finish_reason": "stop"
            }]
        }"#;

        let response: ChatCompletionResponse = serde_json::from_str(json).unwrap();
        let ChatCompletionResponse::Success(response) = response else {
            panic!("expected success");
        };
        assert_eq!(response.created, Some(1_677_652_288));
    }

    #[test]
    fn test_deserialize_error_response() {
        let json = r#"{
            "error": {
                "message": "Invalid request",
                "type": "invalid_request_error",
                "param": "model",
                "code": "invalid_model"
            }
        }"#;

        let response: ChatCompletionResponse = serde_json::from_str(json).unwrap();
        let ChatCompletionResponse::Error(wrapper) = response else {
            panic!("expected error");
        };
        assert_eq!(wrapper.error.message, "Invalid request");
        assert_eq!(wrapper.error.r#type, "invalid_request_error");
        assert_eq!(wrapper.error.param, Some("model".to_string()));
        assert_eq!(wrapper.error.code, Some("invalid_model".to_string()));
    }

    #[test]
    fn test_deserialize_error_response_minimal() {
        let json = r#"{
            "error": {
                "message": "Something went wrong",
                "type": "server_error"
            }
        }"#;

        let response: ChatCompletionResponse = serde_json::from_str(json).unwrap();
        let ChatCompletionResponse::Error(wrapper) = response else {
            panic!("expected error");
        };
        assert_eq!(wrapper.error.message, "Something went wrong");
        assert_eq!(wrapper.error.r#type, "server_error");
        assert_eq!(wrapper.error.param, None);
        assert_eq!(wrapper.error.code, None);
    }

    #[test]
    fn test_deserialize_minimal_response() {
        let json = r#"{
            "model": "basic-model",
            "choices": [{
                "index": 0,
                "message": {
                    "role": "assistant",
                    "content": "Minimal response"
                }
            }]
        }"#;

        let response: ChatCompletionResponse = serde_json::from_str(json).unwrap();
        let ChatCompletionResponse::Success(response) = response else {
            panic!("expected success");
        };
        assert!(response.id.is_none());
        assert!(response.created.is_none());
        assert!(response.usage.is_none());
        assert_eq!(response.model, "basic-model");
        assert_eq!(
            response.choices[0].message.content,
            Some("Minimal response".to_string())
        );
    }
}
