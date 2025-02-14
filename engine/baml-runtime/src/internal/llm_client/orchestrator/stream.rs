use anyhow::Result;
use async_std::stream::StreamExt;
use baml_types::{BamlValue, BamlValueWithMeta};
use internal_baml_core::ir::repr::IntermediateRepr;
use jsonish::BamlValueWithFlags;
use web_time::Duration;

use crate::{
    internal::{
        llm_client::{
            parsed_value_to_response,
            traits::{WithClientProperties, WithPrompt, WithStreamable},
            LLMErrorResponse, LLMResponse, ResponseBamlValue,
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
    partial_parse_fn: impl Fn(&str) -> Result<ResponseBamlValue>,
    parse_fn: impl Fn(&str) -> Result<ResponseBamlValue>,
    on_event: Option<F>,
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
{
    let mut results = Vec::new();
    let mut total_sleep_duration = std::time::Duration::from_secs(0);

    //advanced curl viewing, use render_raw_curl on each node. TODO
    for node in iter {
        let prompt = match node.render_prompt(ir, prompt, ctx, params).await {
            Ok(p) => p,
            Err(e) => {
                results.push((
                    node.scope,
                    LLMResponse::InternalFailure(e.to_string()),
                    None,
                ));
                continue;
            }
        };

        let (system_start, instant_start) = (web_time::SystemTime::now(), web_time::Instant::now());
        let stream_res = node.stream(ctx, &prompt).await;
        let final_response = match stream_res {
            Ok(response) => response
                .map(|stream_part| {
                    if let Some(on_event) = on_event.as_ref() {
                        if let LLMResponse::Success(s) = &stream_part {
                            let response_value = partial_parse_fn(&s.content);
                            let response_value_without_flags = match response_value {
                                Ok(baml_value) => Ok(ResponseBamlValue(
                                    baml_value.0.map_meta_owned(|m| (vec![], m.1, m.2)),
                                )),
                                Err(e) => Err(e),
                            };
                            on_event(FunctionResult::new(
                                node.scope.clone(),
                                LLMResponse::Success(s.clone()),
                                Some(response_value_without_flags),
                            ));
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
            LLMResponse::LLMFailure(LLMErrorResponse { code, client, message, .. }) => {
                match code {
                    // This is some internal BAML error, so handle it like any other error
                    crate::internal::llm_client::ErrorCode::Other(2) => None,
                    _ => Some(Err(anyhow::anyhow!(crate::errors::ExposedError::ClientHttpError {
                        client_name: client.clone(),
                        message: message.clone(),
                        status_code: code.clone(),
                    }))),
                }
            }
            _ => None,
        };
        let sleep_duration = node.error_sleep_duration().cloned();
        results.push((node.scope, final_response, response_value));

        // Currently, we break out of the loop if an LLM responded, even if we couldn't parse the result.
        if results
            .last()
            .map_or(false, |(_, r, _)| matches!(r, LLMResponse::Success(_)))
        {
            break;
        } else if let Some(duration) = sleep_duration {
            total_sleep_duration += duration;
            async_std::task::sleep(duration).await;
        }
    }

    (results, total_sleep_duration)
}
