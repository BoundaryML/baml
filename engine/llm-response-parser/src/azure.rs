// Azure OpenAI Service uses the same response format as OpenAI
// Re-export OpenAI types for Azure compatibility

pub use crate::openai::{
    ChatChoiceLogprobs, ChatCompletionChoice, ChatCompletionChoiceDelta, ChatCompletionGeneric,
    ChatCompletionMessageDelta, ChatCompletionMessageRole, ChatCompletionResponse,
    ChatCompletionResponseDelta, ChatCompletionResponseMessage, ChatCompletionTokenLogprob,
    CompletionChoice, CompletionResponse, CompletionUsage, OpenAIError, OpenAIErrorResponse,
    TopLogprobs,
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_azure_chat_completion() {
        // Azure-specific response with deployment name in model field
        let json = r#"{
            "id": "chatcmpl-8nYnfSWFRBq8d9Qxk1XyMWCkGjzYG",
            "object": "chat.completion",
            "created": 1677652288,
            "model": "gpt-35-turbo",
            "system_fingerprint": null,
            "choices": [{
                "index": 0,
                "message": {
                    "role": "assistant",
                    "content": "Hello from Azure OpenAI!"
                },
                "logprobs": null,
                "finish_reason": "stop"
            }],
            "usage": {
                "prompt_tokens": 10,
                "completion_tokens": 5,
                "total_tokens": 15
            }
        }"#;

        let response: ChatCompletionResponse = serde_json::from_str(json).unwrap();
        assert_eq!(
            response.id,
            Some("chatcmpl-8nYnfSWFRBq8d9Qxk1XyMWCkGjzYG".to_string())
        );
        assert_eq!(response.model, "gpt-35-turbo"); // Azure uses different model names
        assert_eq!(response.choices.len(), 1);
        assert_eq!(
            response.choices[0].message.content,
            Some("Hello from Azure OpenAI!".to_string())
        );
        assert_eq!(response.choices[0].finish_reason, Some("stop".to_string()));
    }

    #[test]
    fn test_azure_streaming_response() {
        let json = r#"{
            "id": "chatcmpl-123",
            "object": "chat.completion.chunk",
            "created": 1677652288,
            "model": "gpt-35-turbo",
            "choices": [{
                "index": 0,
                "delta": {
                    "role": "assistant"
                },
                "finish_reason": null
            }]
        }"#;

        let response: ChatCompletionResponseDelta = serde_json::from_str(json).unwrap();
        assert_eq!(response.id, Some("chatcmpl-123".to_string()));
        assert_eq!(response.model, "gpt-35-turbo");
        assert_eq!(
            response.choices[0].delta.role,
            Some(ChatCompletionMessageRole::Assistant)
        );
    }

    #[test]
    fn test_azure_error_response() {
        // Azure might include additional error details
        let json = r#"{
            "error": {
                "message": "The API deployment for this resource does not exist.",
                "type": "invalid_request_error",
                "code": "DeploymentNotFound"
            }
        }"#;

        let response: OpenAIErrorResponse = serde_json::from_str(json).unwrap();
        assert_eq!(
            response.error.message,
            "The API deployment for this resource does not exist."
        );
        assert_eq!(response.error.code, Some("DeploymentNotFound".to_string()));
    }
}
