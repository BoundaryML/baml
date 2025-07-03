use anyhow::{Context, Result};
use baml_types::BamlMap;
use serde::Deserialize;
use serde_json::{json, Value};

use super::types::{
    ChatCompletionResponse, ChatCompletionResponseDelta, ResponseOutputType, ResponsesApiResponse,
    ResponsesApiStreamEvent,
};
use crate::internal::llm_client::{
    primitive::request::RequestBuilder, traits::WithClient, ErrorCode, LLMCompleteResponse,
    LLMCompleteResponseMetadata, LLMErrorResponse, LLMResponse,
};

fn to_prompt(
    prompt: either::Either<&String, &[internal_baml_jinja::RenderedChatMessage]>,
) -> internal_baml_jinja::RenderedPrompt {
    match prompt {
        either::Left(prompt) => internal_baml_jinja::RenderedPrompt::Completion(prompt.clone()),
        either::Right(prompt) => internal_baml_jinja::RenderedPrompt::Chat(prompt.to_vec()),
    }
}

pub fn parse_openai_response<C: WithClient + RequestBuilder>(
    client: &C,
    prompt: either::Either<&String, &[internal_baml_jinja::RenderedChatMessage]>,
    response_body: serde_json::Value,
    system_now: web_time::SystemTime,
    instant_now: web_time::Instant,
    model_name: Option<String>,
) -> LLMResponse {
    let response = match ChatCompletionResponse::deserialize(&response_body)
        .context(format!(
            "Failed to parse into a response accepted by {}: {}",
            std::any::type_name::<ChatCompletionResponse>(),
            response_body
        ))
        .map_err(|e| LLMErrorResponse {
            client: client.context().name.to_string(),
            model: model_name.clone(),
            prompt: to_prompt(prompt),
            start_time: system_now,
            request_options: client.request_options().clone(),
            latency: instant_now.elapsed(),
            message: format!("{e:?}"),
            code: ErrorCode::Other(2),
        }) {
        Ok(response) => response,
        Err(e) => return LLMResponse::LLMFailure(e),
    };

    if response.choices.len() != 1 {
        return LLMResponse::LLMFailure(LLMErrorResponse {
            client: client.context().name.to_string(),
            model: model_name.clone(),
            prompt: to_prompt(prompt),
            start_time: system_now,
            latency: instant_now.elapsed(),
            request_options: client.request_options().clone(),
            message: format!(
                "Expected exactly one choices block, got {}",
                response.choices.len()
            ),
            code: ErrorCode::Other(200),
        });
    }

    let usage = response.usage.as_ref();

    LLMResponse::Success(LLMCompleteResponse {
        client: client.context().name.to_string(),
        prompt: to_prompt(prompt),
        content: response.choices[0]
            .message
            .content
            .as_ref()
            .map_or("", |s| s.as_str())
            .to_string(),
        start_time: system_now,
        latency: instant_now.elapsed(),
        model: response.model,
        request_options: client.request_options().clone(),
        metadata: LLMCompleteResponseMetadata {
            baml_is_complete: match response.choices.first() {
                Some(c) => c.finish_reason.as_ref().is_some_and(|f| f == "stop"),
                None => false,
            },
            finish_reason: match response.choices.first() {
                Some(c) => c.finish_reason.clone(),
                None => None,
            },
            prompt_tokens: usage.map(|u| u.prompt_tokens),
            output_tokens: usage.map(|u| u.completion_tokens),
            total_tokens: usage.map(|u| u.total_tokens),
        },
    })
}

pub fn scan_openai_chat_completion_stream(
    client_name: &str,
    request_options: &BamlMap<String, serde_json::Value>,
    prompt: &internal_baml_jinja::RenderedPrompt,
    system_now: &web_time::SystemTime,
    instant_now: &web_time::Instant,
    model_name: &Option<String>,
    accumulated: &mut Result<LLMCompleteResponse>,
    event_body: serde_json::Value,
) -> Result<(), LLMErrorResponse> {
    let inner = match accumulated {
        Ok(accumulated) => accumulated,
        // We'll just keep the first error and return it
        Err(e) => return Ok(()),
    };

    let event = ChatCompletionResponseDelta::deserialize(&event_body)
        .context(format!(
            "Failed to parse into a response accepted by {}: {}",
            std::any::type_name::<ChatCompletionResponseDelta>(),
            event_body
        ))
        .map_err(|e| LLMErrorResponse {
            client: client_name.to_string(),
            model: model_name.clone(),
            prompt: prompt.clone(),
            start_time: *system_now,
            request_options: request_options.clone(),
            latency: instant_now.elapsed(),
            message: format!("{e:?}"),
            code: ErrorCode::Other(2),
        })?;

    if let Some(choice) = event.choices.first() {
        if let Some(content) = choice.delta.content.as_ref() {
            inner.content += content.as_str();
        }
        inner.model = event.model;
        inner.metadata.finish_reason = choice.finish_reason.clone();
        inner.metadata.baml_is_complete =
            choice.finish_reason.as_ref().is_some_and(|s| s == "stop");
    }
    inner.latency = instant_now.elapsed();
    if let Some(usage) = event.usage.as_ref() {
        inner.metadata.prompt_tokens = Some(usage.prompt_tokens);
        inner.metadata.output_tokens = Some(usage.completion_tokens);
        inner.metadata.total_tokens = Some(usage.total_tokens);
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;
    use web_time::Duration;

    use super::*;
    use crate::internal::llm_client::primitive::tests::MockClient;
    const RESPONSE: &str = r#"
{
  "id": "chatcmpl-B7rcnRIX2lh1okEeeIrCtzLppkaSw",
  "object": "chat.completion",
  "created": 1741214129,
  "model": "gpt-4o-2024-08-06",
  "choices": [
    {
      "index": 0,
      "message": {
        "role": "assistant",
        "content": "```json\n{\n  \"name\": \"John Doe\",\n  \"education\": [\n    {\n      \"school\": \"University of California, Berkeley\",\n      \"degree\": \"B.S. in Computer Science\",\n      \"year\": 2020\n    }\n  ],\n  \"skills\": [\"Python\", \"Java\", \"C++\"]\n}\n```",
        "refusal": null
      },
      "logprobs": null,
      "finish_reason": "stop"
    }
  ],
  "usage": {
    "prompt_tokens": 128,
    "completion_tokens": 71,
    "total_tokens": 199,
    "prompt_tokens_details": {
      "cached_tokens": 0,
      "audio_tokens": 0
    },
    "completion_tokens_details": {
      "reasoning_tokens": 0,
      "audio_tokens": 0,
      "accepted_prediction_tokens": 0,
      "rejected_prediction_tokens": 0
    }
  },
  "service_tier": "default",
  "system_fingerprint": "fp_eb9dce56a8"
}

    "#;

    #[test]
    fn test_parse_openai_response() {
        let client = MockClient::new();
        let prompt = vec![];
        let response_body = serde_json::from_str(RESPONSE.trim()).unwrap();
        let system_now = web_time::SystemTime::now();
        let instant_now = web_time::Instant::now();
        let model_name = "gpt-4o-mini".to_string();

        let result = parse_openai_response(
            &client,
            either::Right(prompt.as_slice()),
            response_body,
            system_now,
            instant_now,
            Some(model_name.clone()),
        );

        let expected = LLMCompleteResponse {
            client: "mock".to_string(),
            prompt: internal_baml_jinja::RenderedPrompt::Chat(vec![]),
            content: "```json\n{\n  \"name\": \"John Doe\",\n  \"education\": [\n    {\n      \"school\": \"University of California, Berkeley\",\n      \"degree\": \"B.S. in Computer Science\",\n      \"year\": 2020\n    }\n  ],\n  \"skills\": [\"Python\", \"Java\", \"C++\"]\n}\n```".to_string(),
            start_time: system_now,
            latency: Duration::ZERO,
            model: "gpt-4o-2024-08-06".to_string(),
            request_options: client.request_options().clone(),
            metadata: LLMCompleteResponseMetadata {
                baml_is_complete: true,
                finish_reason: Some("stop".to_string()),
                prompt_tokens: Some(128),
                output_tokens: Some(71),
                total_tokens: Some(199),
            },
        };

        if let LLMResponse::Success(mut actual_result) = result {
            actual_result.latency = Duration::ZERO;
            assert_eq!(actual_result, expected);
        } else {
            panic!("Expected LLMResponse::Success, got {result:?}");
        }
    }
}

pub fn parse_openai_responses_response<C: WithClient + RequestBuilder>(
    client: &C,
    prompt: either::Either<&String, &[internal_baml_jinja::RenderedChatMessage]>,
    response_body: serde_json::Value,
    system_now: web_time::SystemTime,
    instant_now: web_time::Instant,
    model_name: Option<String>,
) -> LLMResponse {
    let response = match ResponsesApiResponse::deserialize(&response_body)
        .context(format!(
            "Failed to parse into a responses API response: {response_body}"
        ))
        .map_err(|e| LLMErrorResponse {
            client: client.context().name.to_string(),
            model: model_name.clone(),
            prompt: to_prompt(prompt),
            start_time: system_now,
            request_options: client.request_options().clone(),
            latency: instant_now.elapsed(),
            message: format!("{e:?}"),
            code: ErrorCode::Other(2),
        }) {
        Ok(response) => response,
        Err(e) => return LLMResponse::LLMFailure(e),
    };

    // Extract text content from the responses API format
    // Handle messages, web search results, and function calls
    let content = response
        .output
        .iter()
        .find_map(|output| {
            match output.output_type {
                ResponseOutputType::Message => {
                    // Regular message with text content
                    if !output.content.is_empty() {
                        output.content.first()?.text.as_ref().map(|s| s.to_string())
                    } else {
                        None
                    }
                }
                ResponseOutputType::FunctionCall => {
                    // Function call - return the function call as JSON
                    if let (Some(name), Some(arguments)) = (&output.name, &output.arguments) {
                        Some(
                            json!({
                                "type": "function_call",
                                "name": name,
                                "arguments": arguments,
                                "call_id": output.call_id
                            })
                            .to_string(),
                        )
                    } else {
                        None
                    }
                }
                ResponseOutputType::WebSearchCall
                | ResponseOutputType::FileSearchCall
                | ResponseOutputType::Reasoning => {
                    // Tool calls and reasoning outputs don't have text content, skip them
                    None
                }
            }
        })
        .unwrap_or_default();

    let usage = response.usage.as_ref();

    LLMResponse::Success(LLMCompleteResponse {
        client: client.context().name.to_string(),
        prompt: to_prompt(prompt),
        content,
        start_time: system_now,
        latency: instant_now.elapsed(),
        model: response.model,
        request_options: client.request_options().clone(),
        metadata: LLMCompleteResponseMetadata {
            baml_is_complete: response.status == "completed",
            finish_reason: Some(response.status),
            prompt_tokens: usage.map(|u| u.prompt_tokens),
            output_tokens: usage.map(|u| u.completion_tokens),
            total_tokens: usage.map(|u| u.total_tokens),
        },
    })
}

pub fn scan_openai_responses_stream(
    client_name: &str,
    request_options: &BamlMap<String, serde_json::Value>,
    prompt: &internal_baml_jinja::RenderedPrompt,
    system_now: &web_time::SystemTime,
    instant_now: &web_time::Instant,
    model_name: &Option<String>,
    accumulated: &mut Result<LLMCompleteResponse>,
    event_body: serde_json::Value,
) -> Result<(), LLMErrorResponse> {
    let inner = match accumulated {
        Ok(accumulated) => accumulated,
        // We'll just keep the first error and return it
        Err(e) => return Ok(()),
    };

    let event = ResponsesApiStreamEvent::deserialize(&event_body)
        .context(format!(
            "Failed to parse into a responses API stream event: {event_body}"
        ))
        .map_err(|e| LLMErrorResponse {
            client: client_name.to_string(),
            model: model_name.clone(),
            prompt: prompt.clone(),
            start_time: *system_now,
            request_options: request_options.clone(),
            latency: instant_now.elapsed(),
            message: format!("{e:?}"),
            code: ErrorCode::Other(2),
        })?;

    use super::types::ResponsesApiStreamEvent::*;

    match event {
        ResponseCreated { response, .. } | ResponseInProgress { response, .. } => {
            // Update model information
            inner.model = response.model;
        }
        ResponseCompleted { response, .. } => {
            // Final response with usage information and content
            inner.model = response.model;
            inner.metadata.finish_reason = Some(response.status.clone());
            inner.metadata.baml_is_complete = true;

            // Extract content from the final response
            let content = response
                .output
                .first()
                .and_then(|output| output.content.first())
                .and_then(|content| content.text.as_ref())
                .map_or_else(String::new, |s| s.to_string());

            // If we got content in the final response, use it (overwrite any accumulated content)
            if !content.is_empty() {
                inner.content = content;
            }

            if let Some(usage) = response.usage.as_ref() {
                inner.metadata.prompt_tokens = Some(usage.prompt_tokens);
                inner.metadata.output_tokens = Some(usage.completion_tokens);
                inner.metadata.total_tokens = Some(usage.total_tokens);
            }
        }
        ResponseFailed { response, .. } => {
            // Handle failure
            inner.metadata.finish_reason = Some(response.status.clone());
            inner.metadata.baml_is_complete = false;

            // If there's an error, we might want to add it to the content or handle it differently
            if let Some(error) = response.error {
                return Err(LLMErrorResponse {
                    client: client_name.to_string(),
                    model: Some(response.model),
                    prompt: prompt.clone(),
                    start_time: *system_now,
                    request_options: request_options.clone(),
                    latency: instant_now.elapsed(),
                    message: format!("Response failed with error: {error}"),
                    code: ErrorCode::Other(2),
                });
            }
        }
        ResponseIncomplete { response, .. } => {
            // Handle incomplete response (e.g., hit token limit)
            inner.model = response.model;
            inner.metadata.finish_reason = Some(response.status.clone());
            inner.metadata.baml_is_complete = false; // Mark as incomplete

            // Extract any partial content that was generated
            let content = response
                .output
                .first()
                .and_then(|output| output.content.first())
                .and_then(|content| content.text.as_ref())
                .map_or_else(String::new, |s| s.to_string());

            // If we got partial content, use it
            if !content.is_empty() {
                inner.content = content;
            }

            // Include usage information if available
            if let Some(usage) = response.usage.as_ref() {
                inner.metadata.prompt_tokens = Some(usage.prompt_tokens);
                inner.metadata.output_tokens = Some(usage.completion_tokens);
                inner.metadata.total_tokens = Some(usage.total_tokens);
            }
        }
        OutputTextDelta { delta, .. } => {
            // This is where incremental text content comes through during streaming
            inner.content += &delta;
        }
        OutputTextDone { text, .. } => {
            // Final complete text - use this if we don't have accumulated content
            if inner.content.is_empty() {
                inner.content = text;
            }
        }
        ContentPartAdded { .. } => {
            // Content part was added - this is informational, actual content comes via deltas
        }
        ContentPartDone { part, .. } => {
            // Content part is done - use this as fallback if we don't have accumulated content
            if inner.content.is_empty() && part.part_type == "output_text" {
                inner.content = part.text;
            }
        }
    }

    inner.latency = instant_now.elapsed();
    Ok(())
}

#[cfg(test)]
mod responses_tests {
    use std::time::Duration;

    use super::*;
    use crate::internal::llm_client::primitive::tests::MockClient;

    #[test]
    fn test_parse_openai_responses_response() {
        let client = MockClient::new();
        let prompt = vec![];
        let response_body = serde_json::from_str(RESPONSES_API_RESPONSE.trim()).unwrap();
        let system_now = web_time::SystemTime::now();
        let instant_now = web_time::Instant::now();
        let model_name = "gpt-4.1".to_string();

        let result = parse_openai_responses_response(
            &client,
            either::Right(prompt.as_slice()),
            response_body,
            system_now,
            instant_now,
            Some(model_name.clone()),
        );

        let expected = LLMCompleteResponse {
            client: "mock".to_string(),
            prompt: internal_baml_jinja::RenderedPrompt::Chat(vec![]),
            content: "In a peaceful grove beneath a silver moon, a unicorn named Lumina discovered a hidden pool that reflected the stars. As she dipped her horn into the water, the pool began to shimmer, revealing a pathway to a magical realm of endless night skies. Filled with wonder, Lumina whispered a wish for all who dream to find their own hidden magic, and as she glanced back, her hoofprints sparkled like stardust.".to_string(),
            start_time: system_now,
            latency: Duration::ZERO,
            model: "gpt-4.1-2025-04-14".to_string(),
            request_options: client.request_options().clone(),
            metadata: LLMCompleteResponseMetadata {
                baml_is_complete: true,
                finish_reason: Some("completed".to_string()),
                prompt_tokens: Some(36),
                output_tokens: Some(87),
                total_tokens: Some(123),
            },
        };

        if let LLMResponse::Success(mut actual_result) = result {
            actual_result.latency = Duration::ZERO;
            assert_eq!(actual_result, expected);
        } else {
            panic!("Expected LLMResponse::Success, got {result:?}");
        }
    }

    #[test]
    fn test_parse_openai_responses_with_empty_output() {
        let client = MockClient::new();
        let prompt = vec![];
        let response_body = serde_json::json!({
            "id": "resp_123",
            "object": "response",
            "created_at": 1741476542,
            "status": "completed",
            "model": "gpt-4.1",
            "output": [],
            "usage": {
                "prompt_tokens": 10,
                "completion_tokens": 0,
                "total_tokens": 10
            }
        });
        let system_now = web_time::SystemTime::now();
        let instant_now = web_time::Instant::now();

        let result = parse_openai_responses_response(
            &client,
            either::Right(prompt.as_slice()),
            response_body,
            system_now,
            instant_now,
            Some("gpt-4.1".to_string()),
        );

        if let LLMResponse::Success(actual_result) = result {
            assert_eq!(actual_result.content, "");
            assert_eq!(actual_result.metadata.baml_is_complete, true);
            assert_eq!(
                actual_result.metadata.finish_reason,
                Some("completed".to_string())
            );
        } else {
            panic!("Expected LLMResponse::Success, got {result:?}");
        }
    }

    #[test]
    fn test_parse_openai_responses_with_error_status() {
        let client = MockClient::new();
        let prompt = vec![];
        let response_body = serde_json::json!({
            "id": "resp_123",
            "object": "response",
            "created_at": 1741476542,
            "status": "failed",
            "model": "gpt-4.1",
            "output": [],
            "usage": null
        });
        let system_now = web_time::SystemTime::now();
        let instant_now = web_time::Instant::now();

        let result = parse_openai_responses_response(
            &client,
            either::Right(prompt.as_slice()),
            response_body,
            system_now,
            instant_now,
            Some("gpt-4.1".to_string()),
        );

        if let LLMResponse::Success(actual_result) = result {
            assert_eq!(actual_result.metadata.baml_is_complete, false);
            assert_eq!(
                actual_result.metadata.finish_reason,
                Some("failed".to_string())
            );
        } else {
            panic!("Expected LLMResponse::Success, got {result:?}");
        }
    }

    const RESPONSES_API_RESPONSE: &str = r#"
{
  "id": "resp_67ccd2bed1ec8190b14f964abc0542670bb6a6b452d3795b",
  "object": "response",
  "created_at": 1741476542,
  "status": "completed",
  "error": null,
  "incomplete_details": null,
  "instructions": null,
  "max_output_tokens": null,
  "model": "gpt-4.1-2025-04-14",
  "output": [
    {
      "type": "message",
      "id": "msg_67ccd2bf17f0819081ff3bb2cf6508e60bb6a6b452d3795b",
      "status": "completed",
      "role": "assistant",
      "content": [
        {
          "type": "output_text",
          "text": "In a peaceful grove beneath a silver moon, a unicorn named Lumina discovered a hidden pool that reflected the stars. As she dipped her horn into the water, the pool began to shimmer, revealing a pathway to a magical realm of endless night skies. Filled with wonder, Lumina whispered a wish for all who dream to find their own hidden magic, and as she glanced back, her hoofprints sparkled like stardust.",
          "annotations": []
        }
      ]
    }
  ],
  "parallel_tool_calls": true,
  "previous_response_id": null,
  "reasoning": {
    "effort": null,
    "summary": null
  },
  "store": true,
  "temperature": 1.0,
  "text": {
    "format": {
      "type": "text"
    }
  },
  "tool_choice": "auto",
  "tools": [],
  "top_p": 1.0,
  "truncation": "disabled",
  "usage": {
    "input_tokens": 36,
    "input_tokens_details": {
      "cached_tokens": 0
    },
    "output_tokens": 87,
    "output_tokens_details": {
      "reasoning_tokens": 0
    },
    "total_tokens": 123
  },
  "user": null,
  "metadata": {}
}
    "#;
}
