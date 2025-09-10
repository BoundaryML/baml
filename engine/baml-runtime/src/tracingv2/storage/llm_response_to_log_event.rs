use std::sync::Arc;

use baml_ids::{FunctionCallId, HttpRequestId};
use baml_types::tracing::events::{LLMUsage, LoggedLLMResponse, TraceData, TraceEvent};
use web_time::SystemTime;

use super::interface::TraceEventWithMeta;
use crate::internal::llm_client::LLMResponse;

/// Takes an `LLMResponse` plus some IDs and context info,
/// returns the appropriate TraceEvent populated with
/// LoggedLLMResponse fields and verbosity.
pub fn make_trace_event_for_response(
    llm_response: &LLMResponse,
    call_stack: Vec<FunctionCallId>,
    request_id: &HttpRequestId,
    client_stack: Vec<String>,
) -> TraceEventWithMeta {
    let response = match llm_response {
        LLMResponse::Success(llmcomplete_response) => LoggedLLMResponse::new_success(
            request_id.clone(),
            llmcomplete_response.model.clone(),
            llmcomplete_response.metadata.finish_reason.clone(),
            LLMUsage {
                input_tokens: llmcomplete_response.metadata.prompt_tokens,
                output_tokens: llmcomplete_response.metadata.output_tokens,
                total_tokens: llmcomplete_response.metadata.total_tokens,
                cached_input_tokens: llmcomplete_response.metadata.cached_input_tokens,
            },
            llmcomplete_response.content.clone(),
            client_stack,
        ),
        LLMResponse::LLMFailure(llmerror_response) => LoggedLLMResponse::new_failure(
            request_id.clone(),
            llmerror_response.message.clone(),
            llmerror_response.model.clone(),
            None,
            client_stack,
        ),
        LLMResponse::UserFailure(e)
        | LLMResponse::InternalFailure(e)
        | LLMResponse::Cancelled(e) => LoggedLLMResponse::new_failure(
            request_id.clone(),
            e.to_string(),
            None,
            None,
            client_stack,
        ),
    };
    TraceEventWithMeta::new_llm_response(call_stack, Arc::new(response))
}
