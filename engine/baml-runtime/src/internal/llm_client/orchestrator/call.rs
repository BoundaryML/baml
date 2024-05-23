use anyhow::Result;
use baml_types::BamlValue;
use instant::Duration;
use jsonish::BamlValueWithFlags;

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

use super::{OrchestrationScope, OrchestratorNodeIterator};

pub async fn orchestrate(
    iter: OrchestratorNodeIterator,
    ctx: &RuntimeContext,
    prompt: &PromptRenderer,
    params: &BamlValue,
    parse_fn: impl Fn(&str, &RuntimeContext) -> Result<BamlValueWithFlags>,
) -> (
    Vec<(
        OrchestrationScope,
        LLMResponse,
        Option<Result<BamlValueWithFlags>>,
    )>,
    Duration,
) {
    let mut results = Vec::new();
    let mut total_sleep_duration = std::time::Duration::from_secs(0);

    for node in iter {
        let prompt = match node.render_prompt(prompt, ctx, params) {
            Ok(p) => p,
            Err(e) => {
                results.push((node.scope, LLMResponse::OtherFailure(e.to_string()), None));
                continue;
            }
        };
        let response = node.single_call(&ctx, &prompt).await;
        let parsed_response = match &response {
            LLMResponse::Success(s) => Some(parse_fn(&s.content, ctx)),
            _ => None,
        };

        let sleep_duration = node.error_sleep_duration().cloned();
        results.push((node.scope, response, parsed_response));

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
