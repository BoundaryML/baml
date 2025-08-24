use std::sync::Arc;

use anyhow::Result;
use async_std::stream::StreamExt;
use baml_ids::HttpRequestId;
use baml_types::{BamlValue, BamlValueWithMeta};
use internal_baml_core::ir::repr::IntermediateRepr;
use jsonish::BamlValueWithFlags;
use serde_json::json;
use stream_cancel::Tripwire;
use web_time::Duration;

use super::{call::CtxWithHttpRequestId, OrchestrationScope, OrchestratorNodeIterator};
use crate::{
    internal::{
        llm_client::{
            orchestrator::ExecutionScope,
            parsed_value_to_response,
            traits::{HttpContext, WithClientProperties, WithPrompt, WithStreamable},
            LLMErrorResponse, LLMResponse, ResponseBamlValue,
        },
        prompt_renderer::PromptRenderer,
    },
    tracingv2::storage::{make_trace_event_for_response, storage::BAML_TRACER},
    FunctionResult, RuntimeContext,
};

pub async fn orchestrate_stream<F, G>(
    iter: OrchestratorNodeIterator,
    ir: &IntermediateRepr,
    ctx: &RuntimeContext,
    prompt: &PromptRenderer,
    params: &BamlValue,
    on_tick_fn: Option<G>,
    partial_parse_fn: impl Fn(&str) -> Result<ResponseBamlValue>,
    parse_fn: impl Fn(&str) -> Result<ResponseBamlValue>,
    on_event: Option<F>,
    cancel_tripwire: Option<Tripwire>,
) -> (
    Vec<(
        OrchestrationScope,
        LLMResponse,
        Option<Result<ResponseBamlValue>>,
    )>,
    Duration,
)
where
    F: Fn(FunctionResult),
    G: Fn(),
{
    let mut results = Vec::new();
    let mut total_sleep_duration = std::time::Duration::from_secs(0);

    // Create a future that either waits for cancellation or never completes
    let cancel_future = match cancel_tripwire {
        Some(tripwire) => Box::pin(async move {
            tripwire.await;
        })
            as std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send>>,
        None => Box::pin(futures::future::pending()),
    };
    tokio::pin!(cancel_future);

    //advanced curl viewing, use render_raw_curl on each node. TODO
    for node in iter {
        // Check for cancellation at the start of each iteration
        let cancel_scope = node.scope.clone();
        tokio::select! {
            biased;

            _ = &mut cancel_future => {
                results.push((
                    cancel_scope,
                    LLMResponse::Cancelled("Operation cancelled".to_string()),
                    None,
                ));
                break;
            }
            result = async {
                let prompt = match node.render_prompt(ir, prompt, ctx, params).await {
                    Ok(p) => p,
                    Err(e) => {
                        return Some((
                            node.scope,
                            LLMResponse::InternalFailure(e.to_string()),
                            Some(Err(anyhow::anyhow!(e.to_string()))),
                        ));
                    }
                };

                let (system_start, instant_start) = (web_time::SystemTime::now(), web_time::Instant::now());
                let ctx = CtxWithHttpRequestId::from(ctx);
                let stream_res = node.stream(&ctx, &prompt).await;
                let final_response = match stream_res {
                    Ok(response) => response
                        .map(|stream_part| {
                            if let Some(on_tick) = on_tick_fn.as_ref() {
                                on_tick();
                            }
                            if let Some(on_event) = on_event.as_ref() {
                                if let LLMResponse::Success(s) = &stream_part {
                                    let response_value = partial_parse_fn(&s.content);
                                    // Flags seem to use a ton of memory, so we strip them here.
                                    if let Ok(baml_value) = response_value {
                                        // only success events are sent to the stream
                                        on_event(FunctionResult::new(
                                            node.scope.clone(),
                                            LLMResponse::Success(s.clone()),
                                            Some(Ok(ResponseBamlValue(baml_value.0.map_meta_owned(|m| {
                                                jsonish::ResponseValueMeta(vec![], m.1, m.2, m.3)
                                            })))),
                                        ));
                                    }
                                }
                            }
                            stream_part
                        })
                        .fold(None, |_, current| Some(current))
                        .await
                        .unwrap_or_else(|| {
                            LLMResponse::LLMFailure(LLMErrorResponse {
                                client: node.provider.name().into(),
                                model: None,
                                prompt,
                                start_time: system_start,
                                latency: instant_start.elapsed(),
                                request_options: node.provider.request_options().clone(),
                                message: "Stream ended without response".to_string(),
                                code: crate::internal::llm_client::ErrorCode::from_u16(2),
                            })
                        }),
                    Err(response) => response,
                };

                let response_value = match &final_response {
                    LLMResponse::Success(s) => {
                        if !node
                            .finish_reason_filter()
                            .is_allowed(s.metadata.finish_reason.as_ref())
                        {
                            Some(Err(anyhow::anyhow!(
                                crate::errors::ExposedError::FinishReasonError {
                                    prompt: s.prompt.to_string(),
                                    raw_output: s.content.clone(),
                                    message: "Finish reason not allowed".to_string(),
                                    finish_reason: s.metadata.finish_reason.clone(),
                                }
                            )))
                        } else {
                            Some(parse_fn(&s.content))
                        }
                    }
                    LLMResponse::LLMFailure(LLMErrorResponse {
                        code,
                        client,
                        message,
                        ..
                    }) => {
                        match code {
                            // This is some internal BAML error, so handle it like any other error
                            crate::internal::llm_client::ErrorCode::Other(2) => {
                                Some(Err(anyhow::anyhow!(message.clone())))
                            }
                            _ => Some(Err(anyhow::anyhow!(
                                crate::errors::ExposedError::ClientHttpError {
                                    client_name: client.clone(),
                                    message: message.clone(),
                                    status_code: code.clone(),
                                }
                            ))),
                        }
                    }
                    _ => None,
                };

                // parsed_response.map(|r| r.and_then(|v| parsed_value_to_response(v)));
                let node_name = node.scope.name();
                let sleep_duration = node.error_sleep_duration().cloned();

                {
                    let trace_event = make_trace_event_for_response(
                        &final_response,
                        ctx.runtime_context().call_id_stack.clone(),
                        ctx.http_request_id(),
                        node.scope
                            .scope
                            .iter()
                            .map(ExecutionScope::to_string)
                            .collect(),
                    );
                    BAML_TRACER.lock().unwrap().put(Arc::new(trace_event));
                }
                // Don't include flags in final resopnse either until we
                // figure out how to reduce memory usage.
                let response_value_without_flags = match response_value {
                    Some(Ok(baml_value)) => {
                        Some(Ok(ResponseBamlValue(baml_value.0.map_meta_owned(|m| {
                            jsonish::ResponseValueMeta(vec![], m.1, m.2, m.3)
                        }))))
                    }
                    Some(Err(e)) => Some(Err(e)),
                    None => None,
                };
                let result = (node.scope, final_response, response_value_without_flags);

                // Return to signal completion
                if matches!(result.1, LLMResponse::Success(_)) {
                    return Some(result); // Will break after pushing
                }

                // Sleep if needed
                if let Some(duration) = sleep_duration {
                    total_sleep_duration += duration;
                    async_std::task::sleep(duration).await;
                }

                Some(result)
            } => {
                if let Some(result) = result {
                    results.push(result);
                    // Check if we should break
                    if results.last().is_some_and(|(_, r, _)| matches!(r, LLMResponse::Success(_))) {
                        break;
                    }
                }
            }
        }
    }

    (results, total_sleep_duration)
}
