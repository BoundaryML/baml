use serde::{Deserialize, Deserializer, Serialize};

pub type CompletionResponse = ChatCompletionGeneric<CompletionChoice>;
pub type ChatCompletionResponse = ChatCompletionGeneric<ChatCompletionChoice>;
pub type ChatCompletionResponseDelta = ChatCompletionGeneric<ChatCompletionChoiceDelta>;

#[derive(Debug, Deserialize, Clone, PartialEq)]
pub struct ChatCompletionGeneric<C> {
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
        Some(FloatOrInt::Float(f)) => Ok(Some(f.floor() as u32)),
        None => Ok(None),
    }
}

#[derive(Debug, Deserialize, Clone, PartialEq)]
pub struct CompletionChoice {
    pub finish_reason: Option<String>,
    pub index: u32,
    pub text: String,
}

#[derive(Debug, Deserialize, Clone, PartialEq)]
pub struct ChatCompletionChoice {
    pub index: u32,
    pub message: ChatCompletionResponseMessage,
    pub finish_reason: Option<String>,
    pub logprobs: Option<ChatChoiceLogprobs>,
}

#[derive(Debug, Deserialize, Serialize, Clone, PartialEq)]
pub struct CompletionUsage {
    pub prompt_tokens: u64,
    pub completion_tokens: u64,
    pub total_tokens: u64,
}

#[derive(Debug, Deserialize, Clone, PartialEq)]
pub struct ChatCompletionResponseMessage {
    pub content: Option<String>,
    pub role: ChatCompletionMessageRole,
}

#[derive(Debug, Deserialize, Clone, PartialEq)]
pub struct ChatCompletionChoiceDelta {
    pub index: u64,
    pub finish_reason: Option<String>,
    pub delta: ChatCompletionMessageDelta,
}

#[derive(Debug, Deserialize, Clone, PartialEq)]
pub struct ChatCompletionMessageDelta {
    pub role: Option<ChatCompletionMessageRole>,
    pub content: Option<String>,
}

#[derive(Debug, Deserialize, Clone, Default, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum ChatCompletionMessageRole {
    System,
    #[default]
    User,
    Assistant,
    Tool,
    Function,
}

#[derive(Debug, Deserialize, Clone, PartialEq)]
pub struct ChatChoiceLogprobs {
    pub content: Option<Vec<ChatCompletionTokenLogprob>>,
}

#[derive(Debug, Deserialize, Clone, PartialEq)]
pub struct ChatCompletionTokenLogprob {
    pub token: String,
    pub logprob: f32,
    pub bytes: Option<Vec<u8>>,
    pub top_logprobs: Vec<TopLogprobs>,
}

#[derive(Debug, Deserialize, Clone, PartialEq)]
pub struct TopLogprobs {
    pub token: String,
    pub logprob: f32,
    pub bytes: Option<Vec<u8>>,
}

#[derive(Debug, Deserialize)]
pub struct OpenAIErrorResponse {
    pub error: OpenAIError,
}

#[derive(Debug, Deserialize)]
pub struct OpenAIError {
    pub message: String,
    pub r#type: String,
    pub code: Option<String>,
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
    fn test_deserialize_completion_response() {
        let json = r#"{
            "id": "cmpl-123",
            "object": "text_completion",
            "created": 1677652288,
            "model": "gpt-3.5-turbo-instruct",
            "choices": [{
                "text": "This is a test completion.",
                "index": 0,
                "finish_reason": "stop"
            }],
            "usage": {
                "prompt_tokens": 5,
                "completion_tokens": 7,
                "total_tokens": 12
            }
        }"#;

        let response: CompletionResponse = serde_json::from_str(json).unwrap();
        assert_eq!(response.id, Some("cmpl-123".to_string()));
        assert_eq!(response.model, "gpt-3.5-turbo-instruct");
        assert_eq!(response.choices.len(), 1);
        assert_eq!(response.choices[0].text, "This is a test completion.");
        assert_eq!(response.choices[0].finish_reason, Some("stop".to_string()));
    }

    #[test]
    fn test_deserialize_streaming_response() {
        let json = r#"{
            "id": "chatcmpl-123",
            "object": "chat.completion.chunk",
            "created": 1677652288,
            "model": "gpt-3.5-turbo",
            "system_fingerprint": "fp_44709d6fcb",
            "choices": [{
                "index": 0,
                "delta": {
                    "content": "Hello"
                },
                "finish_reason": null
            }]
        }"#;

        let response: ChatCompletionResponseDelta = serde_json::from_str(json).unwrap();
        assert_eq!(response.id, Some("chatcmpl-123".to_string()));
        assert_eq!(response.choices.len(), 1);
        assert_eq!(response.choices[0].delta.content, Some("Hello".to_string()));
        assert!(response.choices[0].finish_reason.is_none());
    }

    #[test]
    fn test_deserialize_error_response() {
        let json = r#"{
            "error": {
                "message": "Invalid API key provided",
                "type": "invalid_request_error",
                "code": "invalid_api_key"
            }
        }"#;

        let response: OpenAIErrorResponse = serde_json::from_str(json).unwrap();
        assert_eq!(response.error.message, "Invalid API key provided");
        assert_eq!(response.error.r#type, "invalid_request_error");
        assert_eq!(response.error.code, Some("invalid_api_key".to_string()));
    }
}
