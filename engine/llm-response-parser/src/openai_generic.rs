// OpenAI-compatible providers (like Ollama, Groq, etc.) use the same response format
// Re-export OpenAI types for compatibility

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
    fn test_ollama_chat_completion() {
        // Ollama response example
        let json = r#"{
            "id": "cmpl-5e1a8d84-a0b1-4c23-9d67-8a0f3d1f5c2b",
            "object": "chat.completion",
            "created": 1677652288,
            "model": "llama2:latest",
            "choices": [{
                "index": 0,
                "message": {
                    "role": "assistant",
                    "content": "Hello! I'm running on Ollama."
                },
                "finish_reason": "stop"
            }],
            "usage": {
                "prompt_tokens": 8,
                "completion_tokens": 7,
                "total_tokens": 15
            }
        }"#;

        let response: ChatCompletionResponse = serde_json::from_str(json).unwrap();
        assert_eq!(
            response.id,
            Some("cmpl-5e1a8d84-a0b1-4c23-9d67-8a0f3d1f5c2b".to_string())
        );
        assert_eq!(response.model, "llama2:latest");
        assert_eq!(response.choices.len(), 1);
        assert_eq!(
            response.choices[0].message.content,
            Some("Hello! I'm running on Ollama.".to_string())
        );
    }

    #[test]
    fn test_groq_chat_completion() {
        // Groq response example - may have slight differences
        let json = r#"{
            "id": "chatcmpl-7a9b4f6e-3c1d-4e2f-8a5b-9c0d1e2f3a4b",
            "object": "chat.completion",
            "created": 1677652288,
            "model": "llama2-70b-4096",
            "system_fingerprint": "fp_groq_12345",
            "choices": [{
                "index": 0,
                "message": {
                    "role": "assistant",
                    "content": "Hello from Groq!"
                },
                "logprobs": null,
                "finish_reason": "stop"
            }],
            "usage": {
                "prompt_tokens": 12,
                "completion_tokens": 4,
                "total_tokens": 16
            }
        }"#;

        let response: ChatCompletionResponse = serde_json::from_str(json).unwrap();
        assert_eq!(response.model, "llama2-70b-4096");
        assert_eq!(
            response.system_fingerprint,
            Some("fp_groq_12345".to_string())
        );
        assert_eq!(
            response.choices[0].message.content,
            Some("Hello from Groq!".to_string())
        );
    }

    #[test]
    fn test_generic_provider_streaming() {
        // Generic OpenAI-compatible streaming response
        let json = r#"{
            "id": "chatcmpl-generic",
            "object": "chat.completion.chunk",
            "created": 1677652288.5,
            "model": "custom-model-7b",
            "choices": [{
                "index": 0,
                "delta": {
                    "content": "Streaming "
                },
                "finish_reason": null
            }]
        }"#;

        let response: ChatCompletionResponseDelta = serde_json::from_str(json).unwrap();
        assert_eq!(response.id, Some("chatcmpl-generic".to_string()));
        // Test float to u32 conversion
        assert_eq!(response.created, Some(1677652288));
        assert_eq!(
            response.choices[0].delta.content,
            Some("Streaming ".to_string())
        );
    }

    #[test]
    fn test_minimal_response() {
        // Some providers might return minimal responses
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
