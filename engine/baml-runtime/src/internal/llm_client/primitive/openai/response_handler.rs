use anyhow::Result;
use baml_types::BamlMap;

use super::types::{ChatCompletionResponse, ChatCompletionResponseDelta};
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

pub fn parse_openai_response<C: WithClient + RequestBuilder>(
    client: &C,
    prompt: either::Either<&String, &[internal_baml_jinja::RenderedChatMessage]>,
    response_body: serde_json::Value,
    system_now: web_time::SystemTime,
    instant_now: web_time::Instant,
    model_name: Option<String>,
) -> LLMResponse {
    let response = match ChatCompletionResponse::deserialize(&response_body).context(format!(
        "Failed to parse into a response accepted by {}: {}",
        std::any::type_name::<ChatCompletionResponse>(),
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
        Err(e) => return Ok(())
    };

    let event = match ChatCompletionResponseDelta::deserialize(&event_body).context(format!(
        "Failed to parse into a response accepted by {}: {}",
        std::any::type_name::<ChatCompletionResponseDelta>(),
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
    use crate::internal::llm_client::primitive::tests::MockClient;

    use super::*;

    const RESPONSE: &str = r#"
{"id":"msg_01TmchHftFKihgeV7Zy9vCvZ","type":"message","role":"assistant","model":"claude-3-5-sonnet-20241022","content":[{"type":"text","text":"{\n  \"isCompanyPost\": false,\n  \"companyName\": null,\n  \"stage\": null,\n  \"engineeringAssessment\": \"UNKNOWN\",\n  \"teamMembers\": [],\n  \"technicalHighlights\": []\n}\n\nNotes:\n- This is a general discussion post asking for programming book recommendations\n- Not a company/startup related post\n- No technical signals or team information can be extracted\n- Cannot make engineering assessment as this is just a question\n\nThis appears to be a learning/discussion oriented post rather than a startup/company related post, so most of the structured fields are null or empty. The post doesn't contain any analyzable technical due diligence information."}],"stop_reason":"end_turn","stop_sequence":null,"usage":{"input_tokens":321,"cache_creation_input_tokens":0,"cache_read_input_tokens":0,"output_tokens":158}}
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
        content: "{\n  \"isCompanyPost\": false,\n  \"companyName\": null,\n  \"stage\": null,\n  \"engineeringAssessment\": \"UNKNOWN\",\n  \"teamMembers\": [],\n  \"technicalHighlights\": []\n}\n\nNotes:\n- This is a general discussion post asking for programming book recommendations\n- Not a company/startup related post\n- No technical signals or team information can be extracted\n- Cannot make engineering assessment as this is just a question\n\nThis appears to be a learning/discussion oriented post rather than a startup/company related post, so most of the structured fields are null or empty. The post doesn't contain any analyzable technical due diligence information.".to_string(),
        start_time: system_now,
        latency: instant_now.elapsed(),
        model: model_name,
        request_options: client.request_options().clone(),
        metadata: LLMCompleteResponseMetadata {
            baml_is_complete: true,
            finish_reason: Some("end_turn".to_string()),
            prompt_tokens: Some(321),
            output_tokens: Some(158),
            total_tokens: Some(479),
        },
    };

        assert!(matches!(result, LLMResponse::Success(response)));
    }
}
