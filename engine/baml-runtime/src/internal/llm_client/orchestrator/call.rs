use anyhow::Result;
use baml_ids::HttpRequestId;
use baml_types::BamlValue;
use internal_baml_core::ir::repr::IntermediateRepr;
use jsonish::{BamlValueWithFlags, ResponseBamlValue};
use stream_cancel::Tripwire;
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
    cancel_tripwire: Option<Tripwire>,
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

    // Create a future that either waits for cancellation or never completes
    let cancel_future = match cancel_tripwire {
        Some(tripwire) => Box::pin(async move {
            tripwire.await;
        })
            as std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send>>,
        None => Box::pin(futures::future::pending()),
    };
    tokio::pin!(cancel_future);

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

                let ctx = CtxWithHttpRequestId::from(ctx);
                let response = node.single_call(&ctx, &prompt).await;
                let parsed_response = match &response {
                    LLMResponse::Success(s) => {
                        if !node
                            .finish_reason_filter()
                            .is_allowed(s.metadata.finish_reason.as_ref())
                        {
                            let message = "Finish reason not allowed".to_string();
                            Some(Err(anyhow::anyhow!(
                                crate::errors::ExposedError::FinishReasonError {
                                    prompt: prompt.to_string(),
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
                            // Timeout error
                            crate::internal::llm_client::ErrorCode::Timeout => {
                                Some(Err(anyhow::anyhow!(
                                    crate::errors::ExposedError::TimeoutError {
                                        client_name: client.clone(),
                                        message: message.clone(),
                                    }
                                )))
                            }
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
                    _ => None,
                };

                let sleep_duration = node.error_sleep_duration().cloned();
                let result = (node.scope, response, parsed_response);

                // Return None to signal success and break
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
