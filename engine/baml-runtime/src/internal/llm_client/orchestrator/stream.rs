use std::sync::Arc;

use anyhow::Result;
use async_std::stream::StreamExt;
use baml_ids::HttpRequestId;
use baml_types::BamlValue;
use futures::StreamExt as FuturesStreamExt;
use internal_baml_core::ir::repr::IntermediateRepr;
use jsonish::BamlValueWithFlags;
use serde_json::json;
use stream_cancel::Tripwire;
use tokio::sync::{watch, Mutex};
#[cfg(not(target_family = "wasm"))]
use tokio::time::*;
#[cfg(target_family = "wasm")]
use wasmtimer::tokio::*;
use web_time::Duration;

use super::{call::CtxWithHttpRequestId, OrchestrationScope, OrchestratorNodeIterator};
use crate::{
    internal::{
        llm_client::{
            orchestrator::ExecutionScope,
            parsed_value_to_response,
            traits::{HttpContext, WithClientProperties, WithPrompt, WithStreamable},
            ErrorCode, LLMCompleteResponse, LLMErrorResponse, LLMResponse, ResponseBamlValue,
        },
        prompt_renderer::PromptRenderer,
    },
    tracingv2::storage::{make_trace_event_for_response, storage::BAML_TRACER},
    FunctionResult, RuntimeContext,
};

// Shared state between the SSE consumer and the throttled parser.
#[derive(Default)]
struct ParserState {
    last_sent_partial_serialized: Option<String>,
    last_processed_snapshot_ptr: Option<usize>,
}

// Attempts to parse the latest SSE snapshot. We split this out in case parsing takes longer than the SSE interval.
async fn run_parser_loop<'a, ParseFn, EventFn>(
    scope: OrchestrationScope,
    parse_state: Arc<Mutex<ParserState>>,
    partial_parse_fn: &'a ParseFn,
    on_event: &'a EventFn,
    mut snapshot_rx: watch::Receiver<Option<Arc<LLMCompleteResponse>>>,
) where
    ParseFn: Fn(&str) -> Result<ResponseBamlValue> + 'a,
    EventFn: Fn(FunctionResult) + 'a,
{
    let mut parse_interval = interval(web_time::Duration::from_millis(50));
    parse_interval.set_missed_tick_behavior(MissedTickBehavior::Skip);

    loop {
        tokio::select! {
            _ = parse_interval.tick() => {
                process_latest_snapshot(
                    &scope,
                    &parse_state,
                    partial_parse_fn,
                    on_event,
                    &mut snapshot_rx,
                ).await;
            }
            changed = snapshot_rx.changed() => {

                if changed.is_err() {
                    process_latest_snapshot(
                        &scope,
                        &parse_state,
                        partial_parse_fn,
                        on_event,
                        &mut snapshot_rx,
                    ).await;
                    break;
                }
                // we purposefully dont process snpashot here -- only strictly every 50ms in case parsing takes long.
            }
        }
    }
}

async fn process_latest_snapshot<'a, ParseFn, EventFn>(
    scope: &OrchestrationScope,
    parse_state: &Arc<Mutex<ParserState>>,
    partial_parse_fn: &'a ParseFn,
    on_event: &'a EventFn,
    snapshot_rx: &mut watch::Receiver<Option<Arc<LLMCompleteResponse>>>,
) where
    ParseFn: Fn(&str) -> Result<ResponseBamlValue> + 'a,
    EventFn: Fn(FunctionResult) + 'a,
{
    let Some(snapshot) = snapshot_rx.borrow().clone() else {
        return;
    };

    let snapshot_ptr = Arc::as_ptr(&snapshot) as usize;
    let should_attempt = {
        let state = parse_state.lock().await;
        state.last_processed_snapshot_ptr != Some(snapshot_ptr)
    };

    if !should_attempt {
        return;
    }

    match partial_parse_fn(&snapshot.content) {
        Ok(baml_value) => {
            let parsed = ResponseBamlValue(
                baml_value
                    .0
                    .map_meta_owned(|m| jsonish::ResponseValueMeta(vec![], m.1, m.2, m.3)),
            );
            let partial = parsed.serialize_partial();
            let serialized = serde_json::to_string(&partial).ok();

            let should_emit = {
                let mut state = parse_state.lock().await;
                let should_emit = match serialized.as_ref() {
                    Some(serialized_str) => {
                        state.last_sent_partial_serialized.as_deref()
                            != Some(serialized_str.as_str())
                    }
                    None => true,
                };
                state.last_processed_snapshot_ptr = Some(snapshot_ptr);
                if should_emit {
                    if let Some(serialized_str) = serialized.clone() {
                        state.last_sent_partial_serialized = Some(serialized_str);
                    }
                }
                should_emit
            };

            if should_emit {
                on_event(FunctionResult::new(
                    scope.clone(),
                    LLMResponse::Success((*snapshot).clone()),
                    Some(Ok(parsed)),
                ));
            }
        }
        Err(err) => {
            let mut state = parse_state.lock().await;
            state.last_processed_snapshot_ptr = None;
        }
    }
}

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
    let mut total_sleep_duration = web_time::Duration::from_secs(0);

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
                    Some(Err(anyhow::anyhow!(
                        crate::errors::ExposedError::AbortError {
                            detailed_message: String::new()
                        }
                    ))),
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
                    Ok(mut response_stream) => {

                        let mut last_response: Option<LLMResponse> = None;
                        let parse_state = Arc::new(Mutex::new(ParserState::default()));
                        let (snapshot_tx, snapshot_rx) = watch::channel::<Option<Arc<LLMCompleteResponse>>>(None);

                        let parser_future = on_event.as_ref().map(|on_event_cb| {
                            let scope = node.scope.clone();
                            let parse_state = parse_state.clone();
                            let partial_parse_fn = &partial_parse_fn;
                            let snapshot_rx = snapshot_rx.clone();
                            async move {
                                run_parser_loop(
                                    scope,
                                    parse_state,
                                    partial_parse_fn,
                                    on_event_cb,
                                    snapshot_rx,
                                )
                                .await;
                            }
                        });

                        let on_tick_cb = on_tick_fn.as_ref();
                        let parse_state_for_sse = parse_state.clone();

                        // Get streaming timeout config and clone what we need before moving into async block
                        let http_config = node.provider.http_config();
                        let time_to_first_token_timeout = http_config
                            .time_to_first_token_timeout_ms
                            .filter(|&ms| ms > 0)
                            .map(Duration::from_millis);
                        let idle_timeout = http_config
                            .idle_timeout_ms
                            .filter(|&ms| ms > 0)
                            .map(Duration::from_millis);

                        let client_name = node.provider.name().to_string();
                        let stream_prompt = prompt.clone();
                        let request_options_for_timeout = node.provider.request_options().clone();

                        let sse_future = async move {
                            let snapshot_sender = snapshot_tx;
                            let mut first_token_received = false;
                            let stream_start = web_time::Instant::now();

                            loop {
                                // Determine which timeout to use for this iteration
                                let timeout_duration = if !first_token_received {
                                    time_to_first_token_timeout
                                } else {
                                    idle_timeout
                                };

                                // Wait for next stream part with timeout
                                let next_result: Result<LLMResponse, ()> = if let Some(timeout_dur) = timeout_duration {
                                    match timeout(timeout_dur, FuturesStreamExt::next(&mut response_stream)).await {
                                        Ok(Some(part)) => Ok(part),
                                        Ok(None) => break, // Stream ended normally
                                        Err(_elapsed) => {
                                            // Timeout occurred
                                            let timeout_type = if !first_token_received {
                                                "time_to_first_token_timeout"
                                            } else {
                                                "idle_timeout"
                                            };
                                            let elapsed = stream_start.elapsed();
                                            last_response = Some(LLMResponse::LLMFailure(LLMErrorResponse {
                                                client: client_name.clone(),
                                                model: None,
                                                prompt: stream_prompt.clone(),
                                                start_time: system_start,
                                                latency: elapsed,
                                                request_options: request_options_for_timeout.clone(),
                                                message: format!(
                                                    "Timeout: No data received within {}ms ({})",
                                                    timeout_dur.as_millis(),
                                                    timeout_type
                                                ),
                                                code: ErrorCode::Timeout,
                    raw_response: None,
                                            }));
                                            // Explicitly drop the stream to abort the HTTP request
                                            drop(response_stream);
                                            break;
                                        }
                                    }
                                } else {
                                    // No timeout configured, wait indefinitely
                                    match FuturesStreamExt::next(&mut response_stream).await {
                                        Some(part) => Ok(part),
                                        None => break, // Stream ended normally
                                    }
                                };

                                if let Ok(stream_part) = next_result {
                                    if let Some(on_tick) = on_tick_cb {
                                        on_tick();
                                    }

                                    // Mark first token as received
                                    if !first_token_received {
                                        first_token_received = true;
                                    }

                                    match &stream_part {
                                        LLMResponse::Success(s) => {
                                            let snapshot = Arc::new(s.clone());
                                            let _ = snapshot_sender.send_replace(Some(snapshot.clone()));
                                            last_response = Some(LLMResponse::Success((*snapshot).clone()));

                                            let mut state = parse_state_for_sse.lock().await;
                                            state.last_processed_snapshot_ptr = None;
                                        }
                                        other => {
                                            last_response = Some(other.clone());
                                        }
                                    }
                                }
                            }

                            drop(snapshot_sender);

                            // Note: We no longer treat missing baml_is_complete as a timeout.
                            // The stream ending naturally (returning None) is a valid completion.
                            // Timeouts are only detected above when the timeout() call returns Err.
                            //
                            // If last_response is None here, it means we never received any events,
                            // which could be a connection issue but NOT necessarily a timeout.
                            if last_response.is_none() {
                                last_response = Some(LLMResponse::LLMFailure(LLMErrorResponse {
                                    client: client_name.clone(),
                                    model: None,
                                    prompt: stream_prompt.clone(),
                                    start_time: system_start,
                                    latency: stream_start.elapsed(),
                                    request_options: request_options_for_timeout.clone(),
                                    message: "Stream ended without receiving any events".to_string(),
                                    code: ErrorCode::Other(2),
                    raw_response: None,
                                }));
                            }

                            last_response
                        };

                        let final_last_response = if let Some(parser_future) = parser_future {
                            let (last_response_opt, _) = futures::future::join(sse_future, parser_future).await;
                            last_response_opt
                        } else {
                            sse_future.await
                        };

                        if let Some(response) = final_last_response {
                            response
                        } else {
                            // This should be unreachable - we handle the None case in sse_future.
                            // But keep as defensive fallback.
                            LLMResponse::LLMFailure(LLMErrorResponse {
                                client: node.provider.name().into(),
                                model: None,
                                prompt,
                                start_time: system_start,
                                latency: instant_start.elapsed(),
                                request_options: node.provider.request_options().clone(),
                                message: "Stream ended and no events were received".to_string(),
                                code: ErrorCode::Other(2),
                    raw_response: None,
                            })
                        }
                    }
                    Err(response) => response,
                };

                let response_value = match &final_response {
                    LLMResponse::Success(s) => {
                        if !node
                            .finish_reason_filter()
                            .is_allowed(s.metadata.finish_reason.as_ref())
                        {
                            let message = "Finish reason not allowed".to_string();
                            Some(Err(anyhow::anyhow!(
                                crate::errors::ExposedError::FinishReasonError {
                                    prompt: s.prompt.to_string(),
                                    raw_output: s.content.clone(),
                                    detailed_message: message.clone(),
                                    message,
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
                        raw_response,
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
                                    detailed_message: message.clone(),
                                    raw_response: raw_response.clone(),
                                }
                            ))),
                        }
                    }
                    other => {
                        None
                    },
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
                // Call on_event for the final response (success or failure)
                // We need to do this before moving response_value_without_flags into result
                if let Some(ref on_event_cb) = on_event {
                    // We can't clone anyhow::Error, so we need to check the response type
                    // and only send on_event for responses we can represent
                    let event_result = match &response_value_without_flags {
                        Some(Ok(val)) => Some(Ok(val.clone())),
                        Some(Err(e)) => {
                            // print the type of the error
                            Some(Err(anyhow::anyhow!(e.to_string())))
                        }
                        None => None,
                    };

                    on_event_cb(FunctionResult::new(
                        node.scope.clone(),
                        final_response.clone(),
                        event_result,
                    ));
                }

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
