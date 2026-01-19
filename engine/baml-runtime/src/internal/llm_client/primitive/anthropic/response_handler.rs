use anyhow::{Context, Result};
use baml_types::BamlMap;
use serde::Deserialize;
use serde_json::Value;

use super::types::{AnthropicMessageContent, AnthropicMessageResponse, MessageChunk};
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

fn is_done(stop_reason: &Option<String>) -> bool {
    stop_reason.as_ref().is_some_and(|r| {
        r.eq_ignore_ascii_case("end_turn") || r.eq_ignore_ascii_case("stop_sequence")
    })
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
            message: format!("{e:?}"),
            code: ErrorCode::UnsupportedResponse(2),
            raw_response: Some(response_body.to_string()),
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
            raw_response: Some(response_body.to_string()),
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
            baml_is_complete: is_done(&response.stop_reason),
            finish_reason: response.stop_reason.clone(),
            prompt_tokens: Some(response.usage.input_tokens),
            output_tokens: Some(response.usage.output_tokens),
            total_tokens: Some(response.usage.input_tokens + response.usage.output_tokens),
            cached_input_tokens: response.usage.cache_read_input_tokens,
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

    let event = MessageChunk::deserialize(&event_body)
        .context(format!(
            "Failed to parse into a response accepted by {}: {}",
            std::any::type_name::<MessageChunk>(),
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

    match event {
        MessageChunk::MessageStart(chunk) => {
            let body = chunk.message;
            inner.model = body.model;
            let inner = &mut inner.metadata;
            inner.baml_is_complete = is_done(&body.stop_reason);
            inner.finish_reason = body.stop_reason.clone();
            inner.prompt_tokens = Some(body.usage.input_tokens);
            inner.output_tokens = Some(body.usage.output_tokens);
            inner.total_tokens = Some(body.usage.input_tokens + body.usage.output_tokens);
            inner.cached_input_tokens = body.usage.cache_read_input_tokens;
        }
        MessageChunk::ContentBlockDelta(event) => {
            if let super::types::ContentBlockDelta::TextDelta { text } = event.delta {
                inner.content += &text;
            }
        }
        MessageChunk::ContentBlockStart(_) => (),
        MessageChunk::ContentBlockStop(_) => (),
        MessageChunk::Ping => (),
        MessageChunk::MessageDelta(body) => {
            let inner = &mut inner.metadata;

            inner.baml_is_complete = is_done(&body.delta.stop_reason);
            inner.finish_reason = body.delta.stop_reason.clone();
            inner.output_tokens = Some(body.usage.output_tokens);
            inner.total_tokens = Some(inner.prompt_tokens.unwrap_or(0) + body.usage.output_tokens);
            // Only update cached_input_tokens if the new value is Some, as message_delta
            // events often have null for cache tokens (the correct value is in message_start)
            if body.usage.cache_read_input_tokens.is_some() {
                inner.cached_input_tokens = body.usage.cache_read_input_tokens;
            }
        }
        MessageChunk::MessageStop => (),
        MessageChunk::Error { error } => {
            return Err(LLMErrorResponse {
                client: client_name.to_string(),
                model: model_name.clone(),
                prompt: prompt.clone(),
                request_options: request_options.clone(),
                start_time: *system_now,
                latency: instant_now.elapsed(),
                message: error.message.unwrap_or_default(),
                code: ErrorCode::Other(2),
                raw_response: Some(event_body.to_string()),
            });
        }
        MessageChunk::Other => (),
    };

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
{"id":"msg_01TmchHftFKihgeV7Zy9vCvZ","type":"message","role":"assistant","model":"claude-3-5-sonnet-20241022","content":[{"type":"text","text":"{\n  \"isCompanyPost\": false,\n  \"companyName\": null,\n  \"stage\": null,\n  \"engineeringAssessment\": \"UNKNOWN\",\n  \"teamMembers\": [],\n  \"technicalHighlights\": []\n}\n\nNotes:\n- This is a general discussion post asking for programming book recommendations\n- Not a company/startup related post\n- No technical signals or team information can be extracted\n- Cannot make engineering assessment as this is just a question\n\nThis appears to be a learning/discussion oriented post rather than a startup/company related post, so most of the structured fields are null or empty. The post doesn't contain any analyzable technical due diligence information."}],"stop_reason":"end_turn","stop_sequence":null,"usage":{"input_tokens":321,"cache_creation_input_tokens":0,"cache_read_input_tokens":0,"output_tokens":158}}
    "#;

    #[test]
    fn test_parse_anthropic_response() {
        let client = MockClient::new();
        let prompt = vec![];
        let response_body = serde_json::from_str(RESPONSE.trim()).unwrap();
        let system_now = web_time::SystemTime::now();
        let instant_now = web_time::Instant::now();
        let model_name = "claude-3-5-sonnet-20241022".to_string();

        let result = parse_anthropic_response(
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
            content: "{\n  \"isCompanyPost\": false,\n  \"companyName\": null,\n  \"stage\": null,\n  \"engineeringAssessment\": \"UNKNOWN\",\n  \"teamMembers\": [],\n  \"technicalHighlights\": []\n}\n\nNotes:\n- This is a general discussion post asking for programming book recommendations\n- Not a company/startup related post\n- No technical signals or team information can be extracted\n- Cannot make engineering assessment as this is just a question\n\nThis appears to be a learning/discussion oriented post rather than a startup/company related post, so most of the structured fields are null or empty. The post doesn't contain any analyzable technical due diligence information.".to_string(),
            start_time: system_now,
            latency: Duration::ZERO,
            model: model_name,
            request_options: client.request_options().clone(),
            metadata: LLMCompleteResponseMetadata {
                baml_is_complete: true,
                finish_reason: Some("end_turn".to_string()),
                prompt_tokens: Some(321),
                output_tokens: Some(158),
                total_tokens: Some(479),
                cached_input_tokens: Some(0),
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
