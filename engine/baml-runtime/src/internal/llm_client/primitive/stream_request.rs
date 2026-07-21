use std::{collections::HashMap, ops::Deref};

use anyhow::{Context, Result};
use baml_types::{
    tracing::events::{HTTPRequest, HTTPResponse, HTTPResponseStream, SSEEvent, TraceEvent},
    BamlMap,
};
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

fn response_type_supports_streaming(response_type: &ResponseType) -> bool {
    match response_type {
        ResponseType::OpenAI
        | ResponseType::OpenAIResponses
        | ResponseType::Anthropic
        | ResponseType::Google
        | ResponseType::Vertex => true,
        ResponseType::OpenAITranscription => false,
    }
}

fn unsupported_streaming_error(
    client_name: &str,
    request_options: &BamlMap<String, serde_json::Value>,
    prompt: &internal_baml_jinja::RenderedPrompt,
    start_time: &web_time::SystemTime,
    start_instant: &web_time::Instant,
    model_name: &Option<String>,
) -> LLMErrorResponse {
    LLMErrorResponse {
        client: client_name.to_string(),
        model: model_name.clone(),
        prompt: prompt.clone(),
        start_time: *start_time,
        latency: start_instant.elapsed(),
        request_options: request_options.clone(),
        message: "OpenAI audio transcription does not support streaming".to_string(),
        code: ErrorCode::NotSupported,
        raw_response: None,
    }
}

/// Represents the result of processing a stream event
#[derive(Debug)]
enum StreamEventResult {
    /// Successfully parsed JSON data to process
    Data(serde_json::Value),
    /// Stream ended normally with [DONE]
    Done,
    /// Transport or parsing error occurred
    Error { message: String, is_timeout: bool },
}

pub async fn make_stream_request(
    client: &(impl WithClient + RequestBuilder),
    prompt: either::Either<&String, &[RenderedChatMessage]>,
    model_name: Option<String>,
    response_type: ResponseType,
    runtime_context: &impl HttpContext,
) -> StreamResponse {
    if !response_type_supports_streaming(&response_type) {
        let start_time_system = web_time::SystemTime::now();
        let start_time_instant = web_time::Instant::now();
        let rendered_prompt = to_prompt(prompt);
        return Err(LLMResponse::LLMFailure(unsupported_streaming_error(
            &client.context().name,
            client.request_options(),
            &rendered_prompt,
            &start_time_system,
            &start_time_instant,
            &model_name,
        )));
    }

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
        crate::sse::eventsource(resp.bytes_stream())
            // Convert eventsource events to our StreamEventResult enum
            // This allows errors to propagate through instead of stopping at take_while
            .map(move |event| -> StreamEventResult {
                match event {
                    Ok(e) => {
                        // Log trace event for successful events
                        let trace_event = TraceEvent::new_raw_llm_response_stream(
                            call_id_stack.clone(),
                            std::sync::Arc::new(HTTPResponseStream::new(
                                http_request_id.deref().clone(),
                                SSEEvent::new(e.event.clone(), e.data.clone(), e.id.clone()),
                            )),
                        );
                        BAML_TRACER
                            .lock()
                            .unwrap()
                            .put(std::sync::Arc::new(trace_event));

                        // Check for [DONE] signal
                        if e.data == "[DONE]" {
                            StreamEventResult::Done
                        } else {
                            // Try to parse JSON
                            match serde_json::from_str(&e.data) {
                                Ok(json) => StreamEventResult::Data(json),
                                Err(parse_err) => StreamEventResult::Error {
                                    message: format!(
                                        "Failed to parse SSE data as JSON: {parse_err}"
                                    ),
                                    is_timeout: false,
                                },
                            }
                        }
                    }
                    Err(e) => {
                        let error_str = format!("{e:?}");

                        // Check if this is a timeout error:
                        // - Native: reqwest returns errors containing "TimedOut" or "timeout"
                        // - WASM: timeouts are implemented via AbortController, so "abort" indicates timeout
                        let error_lower = error_str.to_lowercase();
                        let is_timeout = error_lower.contains("timedout")
                            || error_lower.contains("timed out")
                            || error_lower.contains("timeout")
                            || error_lower.contains("abort");

                        StreamEventResult::Error {
                            message: if is_timeout {
                                "Request timed out".to_string()
                            } else {
                                format!("Stream transport error: {error_str}")
                            },
                            is_timeout,
                        }
                    }
                }
            })
            // Stop on Done or Error, but emit Error events first
            .take_while(|event| std::future::ready(!matches!(event, StreamEventResult::Done)))
            .inspect(|event| log::debug!("{event:#?}"))
            .scan(
                (
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
                    false, // has_emitted_error - to stop after emitting error
                ),
                move |(accumulated, has_emitted_error): &mut (
                    Result<LLMCompleteResponse>,
                    bool,
                ),
                      event| {
                    // If we've already emitted an error, stop the stream
                    if *has_emitted_error {
                        return std::future::ready(None);
                    }

                    let event_body = match event {
                        StreamEventResult::Data(json) => json,
                        StreamEventResult::Done => {
                            // Should not reach here due to take_while, but handle gracefully
                            return std::future::ready(None);
                        }
                        StreamEventResult::Error {
                            message,
                            is_timeout,
                        } => {
                            // Only stop the stream for fatal errors (timeouts).
                            // Non-fatal errors (like JSON parse failures on individual events)
                            // should emit a failure but allow subsequent events to be processed.
                            // This matches the old behavior where parse errors didn't kill the stream.
                            if is_timeout {
                                *has_emitted_error = true;
                            }
                            let code = if is_timeout {
                                ErrorCode::Timeout
                            } else {
                                ErrorCode::UnsupportedResponse(2)
                            };
                            return std::future::ready(Some(LLMResponse::LLMFailure(
                                LLMErrorResponse {
                                    client: client_name.clone(),
                                    model: model_name.clone(),
                                    prompt: prompt.clone(),
                                    start_time: start_time_system,
                                    request_options: params.clone(),
                                    latency: start_time_instant.elapsed(),
                                    message,
                                    code,
                                    raw_response: None,
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
                        ResponseType::OpenAITranscription => Err(unsupported_streaming_error(
                            &client_name,
                            &params,
                            &prompt,
                            &start_time_system,
                            &start_time_instant,
                            &model_name,
                        )),
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

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use baml_ids::HttpRequestId;
    use baml_types::BamlMap;
    use indexmap::IndexMap;
    use internal_baml_jinja::{RenderContext_Client, RenderedChatMessage};

    use super::*;
    use crate::internal::llm_client::{
        traits::{HttpContext, WithClient},
        AllowedRoleMetadata, ErrorCode, LLMResponse, ModelFeatures, ResolveMediaUrls,
    };

    struct NeverBuildClient {
        context: RenderContext_Client,
        model_features: ModelFeatures,
        request_options: BamlMap<String, serde_json::Value>,
    }

    impl NeverBuildClient {
        fn new() -> Self {
            Self {
                context: RenderContext_Client {
                    name: "transcription-test".to_string(),
                    provider: "openai-transcriptions".to_string(),
                    default_role: "user".to_string(),
                    allowed_roles: vec![],
                    options: IndexMap::new(),
                    remap_role: HashMap::new(),
                },
                model_features: ModelFeatures {
                    completion: false,
                    chat: true,
                    max_one_system_prompt: false,
                    resolve_audio_urls: ResolveMediaUrls::SendBase64,
                    resolve_image_urls: ResolveMediaUrls::SendBase64,
                    resolve_pdf_urls: ResolveMediaUrls::SendBase64,
                    resolve_video_urls: ResolveMediaUrls::SendBase64,
                    allowed_metadata: AllowedRoleMetadata::All,
                },
                request_options: BamlMap::new(),
            }
        }
    }

    impl WithClient for NeverBuildClient {
        fn context(&self) -> &RenderContext_Client {
            &self.context
        }

        fn model_features(&self) -> &ModelFeatures {
            &self.model_features
        }
    }

    impl RequestBuilder for NeverBuildClient {
        async fn build_request(
            &self,
            _prompt: either::Either<&String, &[RenderedChatMessage]>,
            _allow_proxy: bool,
            _stream: bool,
            _expose_secrets: bool,
        ) -> Result<reqwest::RequestBuilder> {
            panic!("transcription streaming must fail before building an HTTP request")
        }

        fn request_options(&self) -> &BamlMap<String, serde_json::Value> {
            &self.request_options
        }

        fn http_client(&self) -> &reqwest::Client {
            panic!("transcription streaming must fail before accessing the HTTP client")
        }

        fn http_config(&self) -> &internal_llm_client::HttpConfig {
            panic!("transcription streaming must fail before reading HTTP config")
        }
    }

    struct UnusedHttpContext;

    impl HttpContext for UnusedHttpContext {
        fn http_request_id(&self) -> &HttpRequestId {
            panic!("transcription streaming must fail before reading HTTP request id")
        }

        fn runtime_context(&self) -> &RuntimeContext {
            panic!("transcription streaming must fail before reading runtime context")
        }
    }

    #[tokio::test]
    async fn transcription_streaming_unsupported_fails_before_http() {
        let client = NeverBuildClient::new();
        let messages: Vec<RenderedChatMessage> = vec![];

        let response = make_stream_request(
            &client,
            either::Right(messages.as_slice()),
            Some("gpt-4o-transcribe".to_string()),
            ResponseType::OpenAITranscription,
            &UnusedHttpContext,
        )
        .await;

        let failure = match response {
            Err(LLMResponse::LLMFailure(failure)) => failure,
            Err(other) => panic!("expected LLMFailure, got {other:?}"),
            Ok(_) => panic!("expected unsupported-streaming failure"),
        };

        assert_eq!(failure.client, "transcription-test");
        assert_eq!(failure.model.as_deref(), Some("gpt-4o-transcribe"));
        assert!(failure.message.contains("does not support streaming"));
        assert_eq!(failure.code, ErrorCode::NotSupported);
    }
}
