use anyhow::Result;
use baml_types::BamlMap;

use super::types::{AnthropicMessageContent, AnthropicMessageResponse, MessageChunk, StopReason};
use crate::internal::llm_client::{
    primitive::request::RequestBuilder, traits::WithClient, ErrorCode, LLMCompleteResponse,
    LLMCompleteResponseMetadata, LLMErrorResponse, LLMResponse,
};
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

pub fn parse_anthropic_response<C: WithClient + RequestBuilder>(
    client: &C,
    prompt: either::Either<&String, &[internal_baml_jinja::RenderedChatMessage]>,
    response_body: serde_json::Value,
    system_now: web_time::SystemTime,
    instant_now: web_time::Instant,
    model_name: Option<String>,
) -> LLMResponse {
    let response = match AnthropicMessageResponse::deserialize(&response_body)
        .context(format!(
            "Failed to parse into a response accepted by {}: {}",
            std::any::type_name::<AnthropicMessageResponse>(),
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

    let content = response
        .content
        .iter()
        .filter_map(|v| match v {
            AnthropicMessageContent::Text { text } => Some(text),
            _ => None,
        })
        .next();

    let content = if let Some(content) = content {
        content
    } else {
        return LLMResponse::LLMFailure(LLMErrorResponse {
            client: client.context().name.to_string(),
            model: model_name.clone(),
            prompt: to_prompt(prompt),
            start_time: system_now,
            request_options: client.request_options().clone(),
            latency: instant_now.elapsed(),
            message: "Anthropic response contains no text".to_string(),
            code: ErrorCode::Other(2),
        });
    };

    LLMResponse::Success(LLMCompleteResponse {
        client: client.context().name.to_string(),
        prompt: to_prompt(prompt),
        content: content.to_string(),
        start_time: system_now,
        latency: instant_now.elapsed(),
        request_options: client.request_options().clone(),
        model: response.model,
        metadata: LLMCompleteResponseMetadata {
            baml_is_complete: matches!(
                response.stop_reason,
                Some(StopReason::StopSequence) | Some(StopReason::EndTurn)
            ),
            finish_reason: response
                .stop_reason
                .as_ref()
                .map(|r| serde_json::to_string(r).unwrap_or("".into())),
            prompt_tokens: Some(response.usage.input_tokens),
            output_tokens: Some(response.usage.output_tokens),
            total_tokens: Some(response.usage.input_tokens + response.usage.output_tokens),
        },
    })
}

pub fn scan_anthropic_response_stream(
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

    let event = match MessageChunk::deserialize(&event_body)
        .context(format!(
            "Failed to parse into a response accepted by {}: {}",
            std::any::type_name::<MessageChunk>(),
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
    match event {
        MessageChunk::MessageStart(chunk) => {
            let body = chunk.message;
            inner.model = body.model;
            let inner = &mut inner.metadata;
            inner.baml_is_complete = matches!(
                body.stop_reason,
                Some(StopReason::StopSequence) | Some(StopReason::EndTurn)
            );
            inner.finish_reason = body.stop_reason.as_ref().map(ToString::to_string);
            inner.prompt_tokens = Some(body.usage.input_tokens);
            inner.output_tokens = Some(body.usage.output_tokens);
            inner.total_tokens = Some(body.usage.input_tokens + body.usage.output_tokens);
        }
        MessageChunk::ContentBlockDelta(event) => match event.delta {
            super::types::ContentBlockDelta::TextDelta { text } => {
                inner.content += &text;
            }
            _ => (),
        },
        MessageChunk::ContentBlockStart(_) => (),
        MessageChunk::ContentBlockStop(_) => (),
        MessageChunk::Ping => (),
        MessageChunk::MessageDelta(body) => {
            let inner = &mut inner.metadata;

            inner.baml_is_complete = matches!(
                body.delta.stop_reason,
                Some(StopReason::StopSequence) | Some(StopReason::EndTurn)
            );
            inner.finish_reason = body
                .delta
                .stop_reason
                .as_ref()
                .map(|r| serde_json::to_string(r).unwrap_or("".into()));
            inner.output_tokens = Some(body.usage.output_tokens);
            inner.total_tokens = Some(inner.prompt_tokens.unwrap_or(0) + body.usage.output_tokens);
        }
        MessageChunk::MessageStop => (),
        MessageChunk::Error(err) => {
            return Err(LLMErrorResponse {
                client: client_name.to_string(),
                model: model_name.clone(),
                prompt: prompt.clone(),
                request_options: request_options.clone(),
                start_time: system_now.clone(),
                latency: instant_now.elapsed(),
                message: err.message,
                code: ErrorCode::Other(2),
            });
        }
        MessageChunk::Other => (),
    };

    inner.latency = instant_now.elapsed();
    Ok(())
}
