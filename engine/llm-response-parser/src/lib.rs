pub mod anthropic;
pub mod azure;
pub mod google;
pub mod openai;
pub mod openai_generic;
pub mod provider;
pub mod vertex;

pub use provider::LLMProvider;

#[cfg(test)]
mod integration_tests {
    use super::*;

    #[test]
    fn test_cross_provider_compatibility() {
        // Test that Azure and OpenAI generic can parse the same OpenAI response
        let openai_response = r#"{
            "id": "chatcmpl-123",
            "object": "chat.completion",
            "created": 1677652288,
            "model": "gpt-3.5-turbo",
            "choices": [{
                "index": 0,
                "message": {
                    "role": "assistant",
                    "content": "Hello!"
                },
                "finish_reason": "stop"
            }],
            "usage": {
                "prompt_tokens": 5,
                "completion_tokens": 1,
                "total_tokens": 6
            }
        }"#;

        // All three should be able to parse the same response
        let _openai: openai::ChatCompletionResponse =
            serde_json::from_str(openai_response).unwrap();
        let _azure: azure::ChatCompletionResponse = serde_json::from_str(openai_response).unwrap();
        let _generic: openai_generic::ChatCompletionResponse =
            serde_json::from_str(openai_response).unwrap();
    }
}
