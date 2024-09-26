use std::collections::HashMap;

use anyhow::Result;
use baml_types::BamlValue;
use internal_baml_core::ir::{repr::IntermediateRepr, repr::Class};
use jsonish::BamlValueWithFlags;
use web_time::Duration;

use crate::{
    internal::{
        llm_client::{
            traits::{WithPrompt, WithSingleCallable},
            LLMResponse,
        },
        prompt_renderer::PromptRenderer,
    },
    RuntimeContext,
};

use baml_types::{ConstraintFailure, ConstraintsResult};
use super::{user_checks::{run_user_checks}, OrchestrationScope, OrchestratorNodeIterator};

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
        Option<Result<BamlValueWithFlags>>,
        Vec<ConstraintFailure>,
    )>,
    Duration,
) {
    let mut results = Vec::new();
    let mut total_sleep_duration = std::time::Duration::from_secs(0);

    for node in iter {
        let prompt = match node.render_prompt(ir, prompt, ctx, params).await {
            Ok(p) => p,
            Err(e) => {
                results.push((node.scope, LLMResponse::OtherFailure(e.to_string()), None, vec![]));
                continue;
            }
        };
        let response = node.single_call(&ctx, &prompt).await;
        let parsed_response = match &response {
            LLMResponse::Success(s) => Some(parse_fn(&s.content)),
            _ => None,
        };

        let user_checks_result = match parsed_response.as_ref() {
            Some(Ok(ref val_with_tags)) => {
                let val: BamlValue = val_with_tags.clone().into();
                let typing_context = ir
                    .walk_classes()
                    .map(|class_node| (class_node.name(), class_node.elem()))
                    .collect();
                run_user_checks(&val, &typing_context)
            },
            _ => Ok(ConstraintsResult::Success),
        };

        let (checked_response, check_failures) = match user_checks_result {
            Ok(ConstraintsResult::Success) => (parsed_response, vec![]),
            Ok(ConstraintsResult::AssertFailure(f)) => (Some(Err(anyhow::anyhow!(format!("TODO: got assert failure: {:?}", f)))), vec![]),
            Ok(ConstraintsResult::CheckFailures(fs)) => (parsed_response, fs),
            Err(e) => (Some(Err(anyhow::anyhow!("Failed to run user_checks: {}", e))), vec![]),
        };

        let sleep_duration = node.error_sleep_duration().cloned();
        results.push((node.scope, response, checked_response, check_failures));

        // Currently, we break out of the loop if an LLM responded, even if we couldn't parse the result.
        if results
            .last()
            .map_or(false, |(_, r, _, _)| matches!(r, LLMResponse::Success(_))) // TODO: (Greg) Handle asserts/checks?
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
