use serde::Deserialize;

use super::CompletionUsage;
use crate::parse_response::{
    FinishReason, LlmOutput, LlmProviderResponse, ParseResponseError, TokenUsage,
};

// == Serde types ===================================================

#[derive(Debug, Deserialize)]
struct ResponsesApiResponse {
    status: String,
    model: String,
    output: Vec<ResponseOutput>,
    usage: Option<CompletionUsage>,
}

#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
enum ResponseOutputType {
    Message,
    FunctionCall,
    ImageGenerationCall,
    #[serde(other)]
    Unknown,
}

#[derive(Debug, Deserialize)]
struct ResponseOutput {
    #[serde(rename = "type")]
    output_type: ResponseOutputType,
    #[serde(default)]
    content: Vec<OutputContent>,
    // Function call fields
    name: Option<String>,
    arguments: Option<String>,
    call_id: Option<String>,
    // Image generation fields
    result: Option<String>,
    revised_prompt: Option<String>,
    output_format: Option<String>,
}

#[derive(Debug, Deserialize)]
struct OutputContent {
    #[serde(default)]
    text: Option<String>,
}

// == Parser ========================================================

/// Parse an `OpenAI` Responses API response body into a normalized [`LlmProviderResponse`].
pub(in crate::parse_response) fn parse_openai_responses_response(
    body: &str,
) -> Result<LlmProviderResponse, ParseResponseError> {
    let response: ResponsesApiResponse =
        serde_json::from_str(body).map_err(|e| ParseResponseError::Deserialize {
            provider: "openai-responses",
            source: e,
            content: body.to_string(),
        })?;

    let mut output = LlmOutput::default();

    for item in &response.output {
        match item.output_type {
            ResponseOutputType::Message => {
                for content in &item.content {
                    if let Some(text) = &content.text {
                        output.push_text(text.clone());
                    }
                }
            }
            ResponseOutputType::FunctionCall => {
                if let (Some(name), Some(arguments)) = (&item.name, &item.arguments) {
                    output.push_text(
                        serde_json::json!({
                            "type": "function_call",
                            "name": name,
                            "arguments": arguments,
                            "call_id": item.call_id.clone()
                        })
                        .to_string(),
                    );
                }
            }
            ResponseOutputType::ImageGenerationCall => {
                if let Some(result) = &item.result {
                    let mime_type = image_output_format_to_mime_type(item.output_format.as_deref())
                        .unwrap_or("image/png");
                    output.push_media(
                        baml_builtins2::MediaValue::from_base64(
                            baml_base::MediaKind::Image,
                            result,
                            Some(mime_type),
                        ),
                        None,
                        serde_json::json!({
                            "provider": "openai-responses",
                            "revised_prompt": item.revised_prompt.clone(),
                            "output_format": item.output_format.clone(),
                        }),
                    );
                }
            }
            ResponseOutputType::Unknown => {}
        }
    }

    let content = output.text_content();

    // Finish reason: status field, not a separate finish_reason
    let finish_reason = match response.status.as_str() {
        "completed" => FinishReason::Stop,
        "incomplete" => FinishReason::Length,
        other => FinishReason::Other(other.to_string()),
    };

    let usage = response
        .usage
        .as_ref()
        .map(|u| TokenUsage {
            input_tokens: Some(u.prompt_tokens),
            output_tokens: Some(u.completion_tokens),
            total_tokens: Some(u.total_tokens),
            cached_input_tokens: u
                .input_tokens_details
                .as_ref()
                .and_then(|details| details.get("cached_tokens"))
                .and_then(serde_json::Value::as_u64),
        })
        .unwrap_or_default();

    Ok(LlmProviderResponse {
        output,
        content,
        model: Some(response.model),
        finish_reason,
        finish_reason_raw: Some(response.status),
        usage,
    })
}

fn image_output_format_to_mime_type(output_format: Option<&str>) -> Option<&'static str> {
    match output_format?.trim().to_ascii_lowercase().as_str() {
        "png" => Some("image/png"),
        "jpeg" | "jpg" => Some("image/jpeg"),
        "webp" => Some("image/webp"),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_completed_response() {
        let body = r#"{
            "id": "resp_123",
            "object": "response",
            "created_at": 1700000000,
            "status": "completed",
            "model": "gpt-4o",
            "output": [{
                "type": "message",
                "id": "msg_1",
                "status": "completed",
                "role": "assistant",
                "content": [{"type": "output_text", "text": "Hello!"}]
            }],
            "usage": {
                "prompt_tokens": 10,
                "completion_tokens": 5,
                "total_tokens": 15
            }
        }"#;

        let resp = parse_openai_responses_response(body).unwrap();
        assert_eq!(resp.content, "Hello!");
        assert_eq!(resp.model.as_deref(), Some("gpt-4o"));
        assert_eq!(resp.finish_reason, FinishReason::Stop);
        assert_eq!(resp.finish_reason_raw, Some("completed".to_string()));
        assert_eq!(resp.usage.input_tokens, Some(10));
    }

    #[test]
    fn test_parse_text_and_multiple_images_response() {
        let body = r#"{
            "id": "resp_images",
            "object": "response",
            "created_at": 1700000000,
            "status": "completed",
            "model": "gpt-4.1",
            "output": [
                {
                    "type": "message",
                    "id": "msg_1",
                    "status": "completed",
                    "role": "assistant",
                    "content": [{"type": "output_text", "text": "caption"}]
                },
                {
                    "type": "image_generation_call",
                    "id": "ig_1",
                    "status": "completed",
                    "result": "aW1hZ2Ux",
                    "output_format": "png"
                },
                {
                    "type": "image_generation_call",
                    "id": "ig_2",
                    "status": "completed",
                    "result": "aW1hZ2Uy",
                    "output_format": "webp"
                }
            ],
            "usage": {
                "prompt_tokens": 10,
                "completion_tokens": 5,
                "total_tokens": 15
            }
        }"#;

        let resp = parse_openai_responses_response(body).unwrap();
        assert_eq!(resp.content, "caption");
        assert_eq!(resp.output.parts.len(), 3);

        let crate::parse_response::LlmOutputPart::Text { text } = &resp.output.parts[0] else {
            panic!("expected text output");
        };
        assert_eq!(text, "caption");

        let crate::parse_response::LlmOutputPart::Media { media, .. } = &resp.output.parts[1]
        else {
            panic!("expected first image output");
        };
        assert_eq!(media.kind, baml_base::MediaKind::Image);
        assert_eq!(media.base64(), "aW1hZ2Ux");
        assert_eq!(media.mime_type().as_deref(), Some("image/png"));

        let crate::parse_response::LlmOutputPart::Media { media, .. } = &resp.output.parts[2]
        else {
            panic!("expected second image output");
        };
        assert_eq!(media.base64(), "aW1hZ2Uy");
        assert_eq!(media.mime_type().as_deref(), Some("image/webp"));
    }

    #[test]
    fn test_parse_function_call_output() {
        let body = r#"{
            "id": "resp_456",
            "object": "response",
            "status": "completed",
            "model": "gpt-4o",
            "output": [{
                "type": "function_call",
                "id": "fc_1",
                "call_id": "call_abc",
                "name": "get_weather",
                "arguments": "{\"location\": \"SF\"}"
            }],
            "usage": {
                "prompt_tokens": 20,
                "completion_tokens": 10,
                "total_tokens": 30
            }
        }"#;

        let resp = parse_openai_responses_response(body).unwrap();
        assert!(resp.content.contains("get_weather"));
        assert!(resp.content.contains("function_call"));
    }

    #[test]
    fn test_parse_incomplete_status() {
        let body = r#"{
            "id": "resp_789",
            "object": "response",
            "status": "incomplete",
            "model": "gpt-4o",
            "output": [{
                "type": "message",
                "id": "msg_1",
                "role": "assistant",
                "content": [{"type": "output_text", "text": "Partial..."}]
            }],
            "incomplete_details": {"reason": "max_output_tokens"},
            "usage": {
                "prompt_tokens": 10,
                "completion_tokens": 100,
                "total_tokens": 110
            }
        }"#;

        let resp = parse_openai_responses_response(body).unwrap();
        assert_eq!(resp.finish_reason, FinishReason::Length);
        assert_eq!(resp.finish_reason_raw, Some("incomplete".to_string()));
    }

    #[test]
    fn test_parse_empty_output() {
        let body = r#"{
            "id": "resp_empty",
            "object": "response",
            "status": "completed",
            "model": "gpt-4o",
            "output": [],
            "usage": {
                "prompt_tokens": 5,
                "completion_tokens": 0,
                "total_tokens": 5
            }
        }"#;

        let resp = parse_openai_responses_response(body).unwrap();
        assert_eq!(resp.content, "");
        assert_eq!(resp.finish_reason, FinishReason::Stop);
    }

    #[test]
    fn test_parse_cached_tokens() {
        let body = r#"{
            "id": "resp_123",
            "object": "response",
            "status": "completed",
            "model": "gpt-4o",
            "output": [{
                "type": "message",
                "id": "msg_1",
                "role": "assistant",
                "content": [{"type": "output_text", "text": "hi"}]
            }],
            "usage": {
                "prompt_tokens": 100,
                "completion_tokens": 10,
                "total_tokens": 110,
                "input_tokens_details": { "cached_tokens": 50 }
            }
        }"#;

        let resp = parse_openai_responses_response(body).unwrap();
        assert_eq!(resp.usage.cached_input_tokens, Some(50));
    }
}
