use std::collections::HashMap;

use crate::internal::llm_client::{
    traits::{StreamResponse, WithClient},
    ErrorCode, LLMCompleteResponse, LLMCompleteResponseMetadata, LLMErrorResponse, LLMResponse,
};
use anyhow::{Context, Result};
use baml_types::BamlMap;
use eventsource_stream::Eventsource;
use futures::{StreamExt, TryStreamExt};
use internal_baml_jinja::RenderedChatMessage;
use reqwest::Response;
use serde::de::DeserializeOwned;

use super::{
    anthropic::response_handler::scan_anthropic_response_stream,
    google::response_handler::scan_google_response_stream,
    openai::response_handler::scan_openai_response_stream,
    request::{make_request, to_prompt, RequestBuilder, ResponseType},
    vertex::response_handler::scan_vertex_response_stream,
};

pub async fn make_stream_request(
    client: &(impl WithClient + RequestBuilder),
    prompt: either::Either<&String, &[RenderedChatMessage]>,
    model_name: Option<String>,
    response_type: ResponseType,
) -> StreamResponse {
    let (resp, system_start, instant_start) = match make_request(client, prompt, true).await {
        Ok(v) => v,
        Err(e) => return Err(e),
    };
    let client_name = client.context().name.clone();
    let params = client.request_options().clone();
    let prompt = to_prompt(prompt);
    Ok(Box::pin(
        resp.bytes_stream()
            .eventsource()
            .take_while(|event| {
                std::future::ready(event.as_ref().is_ok_and(|e| e.data != "[DONE]"))
            })
            .map(|event| -> Result<serde_json::Value> { Ok(serde_json::from_str(&event?.data)?) })
            .inspect(|event| log::trace!("{:#?}", event))
            .scan(
                Ok(LLMCompleteResponse {
                    client: client_name.clone(),
                    prompt: prompt.clone(),
                    content: "".to_string(),
                    start_time: system_start,
                    latency: instant_start.elapsed(),
                    model: model_name.clone().unwrap_or("<unknown>".to_string()),
                    request_options: params.clone(),
                    metadata: LLMCompleteResponseMetadata {
                        baml_is_complete: false,
                        finish_reason: None,
                        prompt_tokens: None,
                        output_tokens: None,
                        total_tokens: None,
                    },
                }),
                move |accumulated: &mut Result<LLMCompleteResponse>, event| {
                    let event_body = match event {
                        Ok(event) => event,
                        Err(e) => {
                            return std::future::ready(Some(LLMResponse::LLMFailure(
                                LLMErrorResponse {
                                    client: client_name.clone(),
                                    model: model_name.clone(),
                                    prompt: prompt.clone(),
                                    start_time: system_start,
                                    request_options: params.clone(),
                                    latency: instant_start.elapsed(),
                                    message: format!("Failed to parse event: {:#?}", e),
                                    code: ErrorCode::UnsupportedResponse(2),
                                },
                            )));
                        }
                    };
                    let update = match response_type {
                        ResponseType::OpenAI => scan_openai_response_stream(
                            &client_name,
                            &params,
                            &prompt,
                            &system_start,
                            &instant_start,
                            &model_name,
                            accumulated,
                            event_body,
                        ),
                        ResponseType::Anthropic => scan_anthropic_response_stream(
                            &client_name,
                            &params,
                            &prompt,
                            &system_start,
                            &instant_start,
                            &model_name,
                            accumulated,
                            event_body,
                        ),
                        ResponseType::Google => scan_google_response_stream(
                            &client_name,
                            &params,
                            &prompt,
                            &system_start,
                            &instant_start,
                            &model_name,
                            accumulated,
                            event_body,
                        ),
                        ResponseType::Vertex => scan_vertex_response_stream(
                            &client_name,
                            &params,
                            &prompt,
                            &system_start,
                            &instant_start,
                            &model_name,
                            accumulated,
                            event_body,
                        ),
                    };
                    if let Err(e) = update {
                        std::future::ready(Some(LLMResponse::LLMFailure(e)))
                    } else {
                        match accumulated {
                            Ok(v) => std::future::ready(Some(LLMResponse::Success(v.clone()))),
                            Err(e) => std::future::ready(None),
                        }
                    }
                },
            ),
    ))
}
