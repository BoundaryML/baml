use anyhow::{Context, Result};
use serde::Deserialize;
use serde_json::Value;

use super::types::VertexResponse;
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

pub fn parse_vertex_response<C: WithClient + RequestBuilder>(
    client: &C,
    prompt: either::Either<&String, &[internal_baml_jinja::RenderedChatMessage]>,
    response_body: serde_json::Value,
    system_now: web_time::SystemTime,
    instant_now: web_time::Instant,
    model_name: Option<String>,
) -> LLMResponse {
    let response = match VertexResponse::deserialize(&response_body)
        .context(format!(
            "Failed to parse into a response accepted by {}: {}",
            std::any::type_name::<VertexResponse>(),
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
            code: ErrorCode::UnsupportedResponse(2),
            raw_response: Some(response_body.to_string()),
        }) {
        Ok(response) => response,
        Err(e) => return LLMResponse::LLMFailure(e),
    };

    if response.candidates.len() != 1 {
        return LLMResponse::LLMFailure(LLMErrorResponse {
            client: client.context().name.to_string(),
            model: model_name.clone(),
            prompt: to_prompt(prompt),
            start_time: system_now,
            request_options: client.request_options().clone(),
            latency: instant_now.elapsed(),
            message: format!(
                "Expected exactly one content block, got {}",
                response.candidates.len()
            ),
            code: ErrorCode::Other(200),
            raw_response: Some(response_body.to_string()),
        });
    }

    let content = if let Some(content) = response.candidates.first().and_then(|c| {
        c.content
            .as_ref()
            .and_then(|c| c.parts.first().map(|p| p.text.clone()))
    }) {
        content
    } else {
        return LLMResponse::LLMFailure(LLMErrorResponse {
            client: client.context().name.to_string(),
            model: model_name.clone(),
            prompt: to_prompt(prompt),
            start_time: system_now,
            request_options: client.request_options().clone(),
            latency: instant_now.elapsed(),
            message: "No content".to_string(),
            code: ErrorCode::Other(200),
            raw_response: Some(response_body.to_string()),
        });
    };

    let usage_metadata = response.usage_metadata.clone().unwrap();

    LLMResponse::Success(LLMCompleteResponse {
        client: client.context().name.to_string(),
        prompt: to_prompt(prompt),
        content,
        start_time: system_now,
        latency: instant_now.elapsed(),
        request_options: client.request_options().clone(),
        model: model_name.unwrap_or("<unknown>".to_string()),
        metadata: LLMCompleteResponseMetadata {
            baml_is_complete: response.candidates[0].finish_reason == Some("STOP".to_string()),
            finish_reason: response.candidates[0]
                .finish_reason
                .as_ref()
                .map(|r| r.to_string()),
            prompt_tokens: usage_metadata.prompt_token_count,
            output_tokens: usage_metadata.candidates_token_count,
            total_tokens: usage_metadata.total_token_count,
            cached_input_tokens: usage_metadata.cached_content_token_count,
        },
    })
}

fn content_part(model_name: &str) -> usize {
    if model_name.contains("gemini-2.0-flash-thinking-exp-1219") {
        1
    } else {
        0
    }
}

pub fn scan_vertex_response_stream(
    client_name: &str,
    request_options: &baml_types::BamlMap<String, serde_json::Value>,
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
        Err(e) => {
            return Ok(());
        }
    };

    let event = VertexResponse::deserialize(&event_body)
        .context(format!(
            "Failed to parse into a response accepted by {}: {}",
            std::any::type_name::<VertexResponse>(),
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
            code: ErrorCode::UnsupportedResponse(2),
            raw_response: Some(event_body.to_string()),
        })?;

    if let Some(choice) = event.candidates.first() {
        let part_index = content_part(
            model_name
                .as_ref()
                .map(|s| s.as_str())
                .unwrap_or("<unknown>"),
        );
        if let Some(content) = choice
            .content
            .as_ref()
            .and_then(|c| c.parts.get(part_index))
        {
            inner.content += &content.text;
        }
        inner.metadata.finish_reason = choice.finish_reason.as_ref().map(|r| r.to_string());
        if choice.finish_reason == Some("STOP".to_string()) {
            inner.metadata.baml_is_complete = true;
        }
        inner.metadata.prompt_tokens = event
            .usage_metadata
            .as_ref()
            .and_then(|u| u.prompt_token_count);
        inner.metadata.output_tokens = event
            .usage_metadata
            .as_ref()
            .and_then(|u| u.candidates_token_count);
        inner.metadata.total_tokens = event
            .usage_metadata
            .as_ref()
            .and_then(|u| u.total_token_count);
        inner.metadata.cached_input_tokens = event
            .usage_metadata
            .as_ref()
            .and_then(|u| u.cached_content_token_count);
    }

    inner.latency = instant_now.elapsed();
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
  "candidates": [
    {
      "content": {
        "parts": [
          {
            "text": "**AccountIssue** \n\nThe input clearly describes a problem with accessing an account and a failure in the password reset process.  This falls squarely into account-related issues. \n"
          }
        ],
        "role": "model"
      },
      "finishReason": "STOP",
      "index": 0,
      "safetyRatings": [
        {
          "category": "HARM_CATEGORY_SEXUALLY_EXPLICIT",
          "probability": "NEGLIGIBLE"
        },
        {
          "category": "HARM_CATEGORY_HATE_SPEECH",
          "probability": "NEGLIGIBLE"
        },
        {
          "category": "HARM_CATEGORY_HARASSMENT",
          "probability": "NEGLIGIBLE"
        },
        {
          "category": "HARM_CATEGORY_DANGEROUS_CONTENT",
          "probability": "NEGLIGIBLE"
        }
      ]
    }
  ],
  "usageMetadata": {
    "promptTokenCount": 79,
    "candidatesTokenCount": 35,
    "totalTokenCount": 114,
    "promptTokensDetails": [
      {
        "modality": "TEXT",
        "tokenCount": 79
      }
    ]
  },
  "modelVersion": "gemini-1.5-pro"
}
    "#;

    #[test]
    fn test_parse_vertex_response() {
        let client = MockClient::new();
        let prompt = vec![];
        let response_body = serde_json::from_str(RESPONSE.trim()).unwrap();
        let system_now = web_time::SystemTime::now();
        let instant_now = web_time::Instant::now();
        let model_name = "gpt-4o-mini".to_string();

        let result = parse_vertex_response(
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
            content: "**AccountIssue** \n\nThe input clearly describes a problem with accessing an account and a failure in the password reset process.  This falls squarely into account-related issues. \n".to_string(),
            start_time: system_now,
            latency: Duration::ZERO,
            model: model_name,
            request_options: client.request_options().clone(),
            metadata: LLMCompleteResponseMetadata {
                baml_is_complete: true,
                finish_reason: Some("STOP".to_string()),
                prompt_tokens: Some(79),
                output_tokens: Some(35),
                total_tokens: Some(114),
                cached_input_tokens: None,
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
