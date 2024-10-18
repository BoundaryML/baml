use anyhow::Result;
use baml_types::BamlValue;
use internal_baml_core::ir::repr::IntermediateRepr;
use jsonish::BamlValueWithFlags;
use web_time::Duration;

use crate::{
    internal::{
        llm_client::{
            parsed_value_to_response, traits::{WithPrompt, WithSingleCallable}, LLMResponse, ResponseBamlValue
        },
        prompt_renderer::PromptRenderer,
    },
    RuntimeContext,
};

use super::{OrchestrationScope, OrchestratorNodeIterator};

pub async fn orchestrate(
    iter: OrchestratorNodeIterator,
    ir: &IntermediateRepr,
    ctx: &RuntimeContext,
    prompt: &PromptRenderer,
    params: &BamlValue,
    parse_fn: impl Fn(&str) -> Result<BamlValueWithFlags>,
) -> (
    Vec<(
        OrchestrationScope,
        LLMResponse,
        Option<Result<ResponseBamlValue>>,
    )>,
    Duration,
) {
    let mut results = Vec::new();
    let mut total_sleep_duration = std::time::Duration::from_secs(0);

    for node in iter {
        let prompt = match node.render_prompt(ir, prompt, ctx, params).await {
            Ok(p) => p,
            Err(e) => {
                results.push((node.scope, LLMResponse::InternalFailure(e.to_string()), None));
                continue;
            }
        };
        let response = node.single_call(&ctx, &prompt).await;
        let parsed_response = match &response {
            LLMResponse::Success(s) => Some(parse_fn(&s.content)),
            _ => None,
        };

        let sleep_duration = node.error_sleep_duration().cloned();
        let response_with_constraints: Option<Result<ResponseBamlValue, _>> =
            parsed_response.map(
                |r| r.and_then(
                    |v| parsed_value_to_response(v)
                )
            );
        results.push((node.scope, response, response_with_constraints));

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
