use std::sync::Arc;

use baml_ids::{FunctionCallId, HttpRequestId};
use baml_types::tracing::events::{LLMUsage, LoggedLLMResponse, TraceData, TraceEvent};
use web_time::SystemTime;

use crate::internal::llm_client::LLMResponse;

use super::interface::TraceEventWithMeta;

/// Takes an `LLMResponse` plus some IDs and context info,
/// returns the appropriate TraceEvent populated with
/// LoggedLLMResponse fields and verbosity.
pub fn make_trace_event_for_response(
    llm_response: &LLMResponse,
    call_stack: Vec<FunctionCallId>,
    request_id: &HttpRequestId,
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
            },
            llmcomplete_response.content.clone(),
        ),
        LLMResponse::LLMFailure(llmerror_response) => LoggedLLMResponse::new_failure(
            request_id.clone(),
            llmerror_response.message.clone(),
            llmerror_response.model.clone(),
            None,
        ),
        LLMResponse::UserFailure(e) | LLMResponse::InternalFailure(e) => {
            LoggedLLMResponse::new_failure(request_id.clone(), e.to_string(), None, None)
        }
    };
    TraceEventWithMeta::new_llm_response(call_stack, Arc::new(response))

    // let (verbosity, logged_response) = match llm_response {
    //     LLMResponse::Success(success) => (
    //         TraceLevel::Info,
    //         LoggedLLMResponse {
    //             request_id: request_id.clone(),
    //             model: Some(success.model.clone()),
    //             finish_reason: success.metadata.finish_reason.clone(),
    //             usage: Some(LLMUsage {
    //                 input_tokens: success.metadata.prompt_tokens,
    //                 output_tokens: success.metadata.output_tokens,
    //                 total_tokens: success.metadata.total_tokens,
    //             }),
    //             raw_text_output: Some(success.content.clone()),
    //             error_message: None,
    //         },
    //     ),
    //     LLMResponse::LLMFailure(fail) => (
    //         TraceLevel::Error,
    //         LoggedLLMResponse {
    //             request_id: request_id.clone(),
    //             model: fail.model.clone().map(|m| m.to_string()),
    //             finish_reason: None,
    //             usage: None,
    //             raw_text_output: None,
    //             error_message: Some(format!("LLM call failed: {}", fail.message)),
    //         },
    //     ),
    //     LLMResponse::UserFailure(msg) => (
    //         TraceLevel::Error,
    //         LoggedLLMResponse {
    //             request_id: request_id.clone(),
    //             model: None,
    //             finish_reason: None,
    //             usage: None,
    //             raw_text_output: None,
    //             error_message: Some(format!("User failure before LLM call: {}", msg)),
    //         },
    //     ),
    //     LLMResponse::InternalFailure(msg) => (
    //         TraceLevel::Error,
    //         LoggedLLMResponse {
    //             request_id: request_id.clone(),
    //             model: None,
    //             finish_reason: None,
    //             usage: None,
    //             raw_text_output: None,
    //             error_message: Some(format!("Internal error before LLM call: {}", msg)),
    //         },
    //     ),
    // };

    // let event_id = ContentId(uuid::Uuid::new_v4().to_string());
    // TraceEvent {
    //     call_id: function_id.clone(),
    //     event_id: event_id.clone(),
    //     // Could also parameterize or omit entirely; in your snippet you set
    //     // vector with function_id or empty. Adjust as needed.
    //     call_stack: vec![function_id.clone()],
    //     timestamp: SystemTime::now(),
    //     callsite: callsite.to_string(),
    //     verbosity,
    //     content: TraceData::LLMResponse(Arc::new(logged_response)),
    //     tags: Default::default(),
    // }
}
