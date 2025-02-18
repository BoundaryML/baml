use anyhow::Result;

use super::types::{FinishReason, GoogleResponse};
use crate::internal::llm_client::{primitive::request::RequestBuilder, traits::WithClient, ErrorCode, LLMCompleteResponse, LLMCompleteResponseMetadata, LLMErrorResponse, LLMResponse};
use anyhow::Context;
use serde::Deserialize;
use serde_json::Value;

fn to_prompt(
    prompt: either::Either<&String, &[internal_baml_jinja::RenderedChatMessage]>,
) -> internal_baml_jinja::RenderedPrompt {
    match prompt {
        either::Left(prompt) => internal_baml_jinja::RenderedPrompt::Completion(prompt.clone()),
        either::Right(prompt) => internal_baml_jinja::RenderedPrompt::Chat(prompt.to_vec()),
    }
}

pub fn parse_google_response<C: WithClient + RequestBuilder>(
    client: &C,
    prompt: either::Either<&String, &[internal_baml_jinja::RenderedChatMessage]>,
    response_body: serde_json::Value,
    system_now: web_time::SystemTime,
    instant_now: web_time::Instant,
    model_name: Option<String>,
) -> LLMResponse {
    let response = match GoogleResponse::deserialize(&response_body).context(format!(
        "Failed to parse into a response accepted by {}: {}",
        std::any::type_name::<GoogleResponse>(),
        response_body
    )).map_err(|e| LLMErrorResponse {
            client: client.context().name.to_string(),
            model: model_name.clone(),
            prompt: to_prompt(prompt),
            start_time: system_now,
            request_options: client.request_options().clone(),
            latency: instant_now.elapsed(),
            message: format!("{:?}", e),
            code: ErrorCode::Other(2),
        })
    {
        Ok(response) => response,
        Err(e) => return LLMResponse::LLMFailure(e),
    };


    if response.candidates.len() != 1 {
        return LLMResponse::LLMFailure(LLMErrorResponse {
            client: client.context().name.to_string(),
            model: None,
            prompt: to_prompt(prompt),
            start_time: system_now,
            request_options: client.request_options().clone(),
            latency: instant_now.elapsed(),
            message: format!(
                "Expected exactly one content block, got {}",
                response.candidates.len()
            ),
            code: ErrorCode::Other(200),
        });
    }

    let Some(content) = response.candidates[0].content.as_ref() else {
        return LLMResponse::LLMFailure(LLMErrorResponse {
            client: client.context().name.to_string(),
            model: None,
            prompt: to_prompt(prompt),
            start_time: system_now,
            request_options: client.request_options().clone(),
            latency: instant_now.elapsed(),
            message: "No content returned".to_string(),
            code: ErrorCode::Other(200),
        });
    };

    let model_name = model_name.unwrap_or("<unknown>".to_string());
    let part_index = content_part(&model_name);
    LLMResponse::Success(LLMCompleteResponse {
        client: client.context().name.to_string(),
        prompt: to_prompt(prompt),
        content: content.parts[part_index].text.clone(),
        start_time: system_now,
        latency: instant_now.elapsed(),
        request_options: client.request_options().clone(),
        model: model_name,
        metadata: LLMCompleteResponseMetadata {
            baml_is_complete: matches!(
                response.candidates[0].finish_reason,
                Some(FinishReason::Stop)
            ),
            finish_reason: response.candidates[0]
                .finish_reason
                .as_ref()
                .map(|r| serde_json::to_string(r).unwrap_or("".into())),
            prompt_tokens: response.usage_metadata.prompt_token_count,
            output_tokens: response.usage_metadata.candidates_token_count,
            total_tokens: response.usage_metadata.total_token_count,
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

pub fn scan_google_response_stream(
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
        Err(e) => return Ok(())
    };

    let event = match GoogleResponse::deserialize(&event_body).context(format!(
        "Failed to parse into a response accepted by {}: {}",
        std::any::type_name::<GoogleResponse>(),
        event_body
    )).map_err(|e| LLMErrorResponse {
            client: client_name.to_string(),
            model: model_name.clone(),
            prompt: prompt.clone(),
            start_time: system_now.clone(),
            request_options: request_options.clone(),
            latency: instant_now.elapsed(),
            message: format!("{:?}", e),
            code: ErrorCode::Other(2),
        })
    {
        Ok(response) => response,
        Err(e) => return Err(e),
    };
    if let Some(choice) = event.candidates.get(0) {
        let part_index = content_part(model_name.as_ref().map(|s| s.as_str()).unwrap_or("<unknown>"));
        if let Some(content) = choice
            .content
            .as_ref()
            .and_then(|c| c.parts.get(part_index))
        {
            inner.content += &content.text;
        }
        inner.metadata.finish_reason = choice.finish_reason.as_ref().map(|r| r.to_string());
        if let Some(FinishReason::Stop) = choice.finish_reason.as_ref() {
            inner.metadata.baml_is_complete = true;
        }
    }

    inner.latency = instant_now.elapsed();
    Ok(())
}
