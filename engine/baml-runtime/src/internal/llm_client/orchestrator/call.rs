use anyhow::Result;
use baml_ids::HttpRequestId;
use baml_types::BamlValue;
use internal_baml_core::ir::repr::IntermediateRepr;
use jsonish::{BamlValueWithFlags, ResponseBamlValue};
use web_time::Duration;

use super::{OrchestrationScope, OrchestratorNodeIterator};
use crate::{
    internal::{
        llm_client::{
            parsed_value_to_response,
            traits::{HttpContext, WithClientProperties, WithPrompt, WithSingleCallable},
            LLMErrorResponse, LLMResponse,
        },
        prompt_renderer::PromptRenderer,
    },
    RuntimeContext,
};

pub(super) struct CtxWithHttpRequestId<'a> {
    runtime_context: &'a RuntimeContext,
    http_request_id: HttpRequestId,
}

impl<'a> HttpContext for CtxWithHttpRequestId<'a> {
    fn http_request_id(&self) -> &baml_ids::HttpRequestId {
        &self.http_request_id
    }

    fn runtime_context(&self) -> &RuntimeContext {
        self.runtime_context
    }
}

impl<'a> From<&'a RuntimeContext> for CtxWithHttpRequestId<'a> {
    fn from(runtime_context: &'a RuntimeContext) -> Self {
        Self {
            runtime_context,
            http_request_id: HttpRequestId::new(),
        }
    }
}

pub async fn orchestrate(
    iter: OrchestratorNodeIterator,
    ir: &IntermediateRepr,
    ctx: &RuntimeContext,
    prompt: &PromptRenderer,
    params: &BamlValue,
    parse_fn: impl Fn(&str) -> Result<ResponseBamlValue>,
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
                results.push((
                    node.scope,
                    LLMResponse::InternalFailure(e.to_string()),
                    Some(Err(anyhow::anyhow!(e.to_string()))),
                ));
                continue;
            }
        };

        let ctx = CtxWithHttpRequestId::from(ctx);
        let response = node.single_call(&ctx, &prompt).await;
        let parsed_response = match &response {
            LLMResponse::Success(s) => {
                if !node
                    .finish_reason_filter()
                    .is_allowed(s.metadata.finish_reason.as_ref())
                {
                    Some(Err(anyhow::anyhow!(
                        crate::errors::ExposedError::FinishReasonError {
                            prompt: prompt.to_string(),
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

        let sleep_duration = node.error_sleep_duration().cloned();
        results.push((node.scope, response, parsed_response));

        // Currently, we break out of the loop if an LLM responded, even if we couldn't parse the result.
        if results
            .last()
            .is_some_and(|(_, r, _)| matches!(r, LLMResponse::Success(_)))
        {
            break;
        } else if let Some(duration) = sleep_duration {
            total_sleep_duration += duration;
            async_std::task::sleep(duration).await;
        }
    }

    (results, total_sleep_duration)
}
