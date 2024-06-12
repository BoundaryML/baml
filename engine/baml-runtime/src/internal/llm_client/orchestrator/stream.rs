use std::ops::Deref;

use anyhow::Result;
use async_std::stream::StreamExt;
use baml_types::BamlValue;
use instant::Duration;
use internal_baml_core::ir::repr::IntermediateRepr;
use jsonish::BamlValueWithFlags;

use crate::{
    internal::{
        llm_client::{
            primitive::request::RequestBuilder,
            traits::{WithPrompt, WithStreamable},
            LLMErrorResponse, LLMResponse,
        },
        prompt_renderer::PromptRenderer,
    },
    FunctionResult, RuntimeContext,
};

use super::{OrchestrationScope, OrchestratorNodeIterator};

pub async fn orchestrate_stream<F>(
    iter: OrchestratorNodeIterator,
    ir: &IntermediateRepr,
    ctx: &RuntimeContext,
    prompt: &PromptRenderer,
    params: &BamlValue,
    partial_parse_fn: impl Fn(&str) -> Result<BamlValueWithFlags>,
    parse_fn: impl Fn(&str) -> Result<BamlValueWithFlags>,
    on_event: Option<F>,
) -> (
    Vec<(
        OrchestrationScope,
        LLMResponse,
        Option<Result<BamlValueWithFlags>>,
    )>,
    Duration,
)
where
    F: Fn(FunctionResult) -> (),
{
    let mut results = Vec::new();
    let mut total_sleep_duration = std::time::Duration::from_secs(0);

    for node in iter {
        let prompt = match node.render_prompt(ir, prompt, ctx, params) {
            Ok(p) => p,
            Err(e) => {
                results.push((node.scope, LLMResponse::OtherFailure(e.to_string()), None));
                continue;
            }
        };

        let (system_start, instant_start) = (web_time::SystemTime::now(), web_time::Instant::now());
        let stream_res = node.stream(ctx, &prompt).await;

        match &stream_res {
            Ok(_) => log::info!("Stream response: Success"),
            Err(e) => log::info!("Stream response: Error {:?}", e),
        }
        let final_response = match stream_res {
            Ok(response) => {
                log::info!("Stream started successfully");
                response
                    .map(|stream_part| {
                        log::info!("Stream part received: {:?}", stream_part);
                        if let Some(on_event) = on_event.as_ref() {
                            match &stream_part {
                                LLMResponse::Success(s) => {
                                    let parsed = partial_parse_fn(&s.content);
                                    on_event(FunctionResult::new(
                                        node.scope.clone(),
                                        LLMResponse::Success(s.clone()),
                                        Some(parsed),
                                    ));
                                }
                                _ => {}
                            }
                        }
                        stream_part
                    })
                    .fold(None, |_, current| {
                        log::info!("Folding stream part: {:?}", current);
                        Some(current)
                    })
                    .await
                    .unwrap_or_else(|| {
                        log::warn!("Stream ended without response");
                        LLMResponse::LLMFailure(LLMErrorResponse {
                            client: node.provider.name().into(),
                            model: None,
                            prompt,
                            start_time: system_start,
                            latency: instant_start.elapsed(),
                            invocation_params: node.provider.invocation_params().clone(),
                            message: "Stream ended without response".to_string(),
                            code: crate::internal::llm_client::ErrorCode::from_u16(2),
                        })
                    })
            }
            Err(response) => {
                log::error!("Stream response error: {:?}", response);
                response
            }
        };

        let parsed_response = match &final_response {
            LLMResponse::Success(s) => Some(parse_fn(&s.content)),
            _ => None,
        };
        let sleep_duration = node.error_sleep_duration().cloned();
        results.push((node.scope, final_response, parsed_response));

        // Currently, we break out of the loop if an LLM responded, even if we couldn't parse the result.
        if results
            .last()
            .map_or(false, |(_, r, _)| matches!(r, LLMResponse::Success(_)))
        {
            break;
        } else {
            if let Some(duration) = sleep_duration {
                total_sleep_duration += duration;
                async_std::task::sleep(duration).await;
            }
        }
    }

    (results, total_sleep_duration)
}
