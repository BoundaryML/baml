use std::{collections::HashMap, ops::Deref};

use anyhow::{Context, Result};
use baml_types::{
    tracing::events::{HTTPRequest, HTTPResponse, HTTPResponseStream, SSEEvent, TraceEvent},
    BamlMap,
};
use eventsource_stream::Eventsource;
use futures::{StreamExt, TryStreamExt};
use internal_baml_jinja::RenderedChatMessage;
use reqwest::Response;
use serde::de::DeserializeOwned;

use super::{
    anthropic::response_handler::scan_anthropic_response_stream,
    google::response_handler::scan_google_response_stream,
    openai::response_handler::scan_openai_chat_completion_stream,
    request::{
        build_and_log_outbound_request, execute_request, to_prompt, EitherResponse, RequestBuilder,
        ResponseType,
    },
    vertex::response_handler::scan_vertex_response_stream,
};
use crate::{
    internal::llm_client::{
        traits::{HttpContext, StreamResponse, WithClient},
        ErrorCode, LLMCompleteResponse, LLMCompleteResponseMetadata, LLMErrorResponse, LLMResponse,
    },
    tracingv2::storage::storage::BAML_TRACER,
    RuntimeContext,
};

pub async fn make_stream_request(
    client: &(impl WithClient + RequestBuilder),
    prompt: either::Either<&String, &[RenderedChatMessage]>,
    model_name: Option<String>,
    response_type: ResponseType,
    runtime_context: &impl HttpContext,
) -> StreamResponse {
    let (start_time_system, start_time_instant, built_req) =
        build_and_log_outbound_request(client, prompt, true, true, runtime_context).await?;

    let resp = match execute_request(
        client,
        built_req,
        prompt,
        start_time_system,
        start_time_instant,
        runtime_context,
        false,
    )
    .await?
    {
        (EitherResponse::Raw(resp), sys, inst) => Ok(resp),
        (EitherResponse::Consumed(_), _, _) => {
            unreachable!("We never consume the body in streaming mode unless an error is returned.")
        }
    };

    let resp = match resp {
        Ok(v) => v,
        Err(e) => return Err(e),
    };

    let call_id_stack = runtime_context.runtime_context().call_id_stack.clone();
    let http_request_id = std::sync::Arc::new(runtime_context.http_request_id().clone());

    let client_name = client.context().name.clone();
    let params = client.request_options().clone();
    let prompt = to_prompt(prompt);
    Ok(Box::pin(
        resp.bytes_stream()
            .eventsource()
            .take_while(move |event| {
                if let Ok(event) = event {
                    let trace_event = TraceEvent::new_raw_llm_response_stream(
                        call_id_stack.clone(),
                        std::sync::Arc::new(HTTPResponseStream::new(
                            http_request_id.deref().clone(),
                            SSEEvent::new(
                                event.event.clone(),
                                event.data.clone(),
                                event.id.clone(),
                            ),
                        )),
                    );
                    BAML_TRACER
                        .lock()
                        .unwrap()
                        .put(std::sync::Arc::new(trace_event));
                }
                std::future::ready(event.as_ref().is_ok_and(|e| e.data != "[DONE]"))
            })
            .map(|event| -> Result<serde_json::Value> { Ok(serde_json::from_str(&event?.data)?) })
            .inspect(|event| log::trace!("{event:#?}"))
            .scan(
                Ok(LLMCompleteResponse {
                    client: client_name.clone(),
                    prompt: prompt.clone(),
                    content: "".to_string(),
                    start_time: start_time_system,
                    latency: start_time_instant.elapsed(),
                    model: model_name.clone().unwrap_or("<unknown>".to_string()),
                    request_options: params.clone(),
                    metadata: LLMCompleteResponseMetadata {
                        baml_is_complete: false,
                        finish_reason: None,
                        prompt_tokens: None,
                        output_tokens: None,
                        total_tokens: None,
                        cached_input_tokens: None,
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
                                    start_time: start_time_system,
                                    request_options: params.clone(),
                                    latency: start_time_instant.elapsed(),
                                    message: format!("Failed to parse event: {e:#?}"),
                                    code: ErrorCode::UnsupportedResponse(2),
                                },
                            )));
                        }
                    };
                    let update = match response_type {
                        ResponseType::OpenAI => scan_openai_chat_completion_stream(
                            &client_name,
                            &params,
                            &prompt,
                            &start_time_system,
                            &start_time_instant,
                            &model_name,
                            accumulated,
                            event_body,
                        ),
                        ResponseType::OpenAIResponses => {
                            super::openai::response_handler::scan_openai_responses_stream(
                                &client_name,
                                &params,
                                &prompt,
                                &start_time_system,
                                &start_time_instant,
                                &model_name,
                                accumulated,
                                event_body,
                            )
                        }
                        ResponseType::Anthropic => scan_anthropic_response_stream(
                            &client_name,
                            &params,
                            &prompt,
                            &start_time_system,
                            &start_time_instant,
                            &model_name,
                            accumulated,
                            event_body,
                        ),
                        ResponseType::Google => scan_google_response_stream(
                            &client_name,
                            &params,
                            &prompt,
                            &start_time_system,
                            &start_time_instant,
                            &model_name,
                            accumulated,
                            event_body,
                        ),
                        ResponseType::Vertex => scan_vertex_response_stream(
                            &client_name,
                            &params,
                            &prompt,
                            &start_time_system,
                            &start_time_instant,
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
