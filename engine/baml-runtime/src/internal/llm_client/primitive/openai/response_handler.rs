use anyhow::{Context, Result};
use baml_types::BamlMap;
use serde::Deserialize;
use serde_json::Value;

use super::types::{ChatCompletionResponse, ChatCompletionResponseDelta};
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
            message: format!("{:?}", e),
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
            baml_is_complete: match response.choices.get(0) {
                Some(c) => c.finish_reason.as_ref().is_some_and(|f| f == "stop"),
                None => false,
            },
            finish_reason: match response.choices.get(0) {
                Some(c) => c.finish_reason.clone(),
                None => None,
            },
            prompt_tokens: usage.map(|u| u.prompt_tokens),
            output_tokens: usage.map(|u| u.completion_tokens),
            total_tokens: usage.map(|u| u.total_tokens),
        },
    })
}

pub fn scan_openai_response_stream(
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

    let event = match ChatCompletionResponseDelta::deserialize(&event_body)
        .context(format!(
            "Failed to parse into a response accepted by {}: {}",
            std::any::type_name::<ChatCompletionResponseDelta>(),
            event_body
        ))
        .map_err(|e| LLMErrorResponse {
            client: client_name.to_string(),
            model: model_name.clone(),
            prompt: prompt.clone(),
            start_time: system_now.clone(),
            request_options: request_options.clone(),
            latency: instant_now.elapsed(),
            message: format!("{:?}", e),
            code: ErrorCode::Other(2),
        }) {
        Ok(response) => response,
        Err(e) => return Err(e),
    };
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
            panic!("Expected LLMResponse::Success, got {:?}", result);
        }
    }
}
