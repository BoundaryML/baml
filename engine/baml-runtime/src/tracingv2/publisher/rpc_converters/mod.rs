use anyhow::Result;
use std::sync::Arc;

use baml_ids::FunctionCallId;
use baml_rpc::ast::tops::BamlFunctionId;
use baml_rpc::BamlTypeId;
use baml_types::HasFieldType;

use crate::tracingv2::storage::interface::TraceEventWithMeta;

mod errors;
mod trace_data;
pub mod types;

pub trait TypeLookup {
    fn type_lookup(&self, name: &str) -> Option<Arc<BamlTypeId>>;
    fn function_lookup(&self, name: &str) -> Option<Arc<BamlFunctionId>>;
}

pub(crate) trait IntoRpcEvent<'a, RpcOutputType> {
    fn into_rpc_event(&'a self, lookup: &(impl TypeLookup + ?Sized)) -> RpcOutputType;
}

pub(super) fn to_rpc_event<'a>(
    event: &'a TraceEventWithMeta,
    lookup: &(impl TypeLookup + ?Sized),
) -> baml_rpc::runtime_api::TraceEvent<'a> {
    let timestamp = baml_rpc::EpochMsTimestamp::try_from(event.timestamp)
        .expect("Failed to convert timestamp to EpochMsTimestamp");
    baml_rpc::runtime_api::TraceEvent {
        call_id: event.call_id.clone(),
        function_event_id: event.function_event_id.clone(),
        call_stack: event.call_stack.clone(),
        timestamp,
        content: event.content.into_rpc_event(lookup),
    }
}

impl<'a, T: HasFieldType> IntoRpcEvent<'a, baml_rpc::runtime_api::TraceData<'a>>
    for baml_types::tracing::events::TraceData<'a, T>
{
    fn into_rpc_event(
        &'a self,
        lookup: &(impl TypeLookup + ?Sized),
    ) -> baml_rpc::runtime_api::TraceData<'a> {
        use baml_types::tracing::events::TraceData;

        match self {
            TraceData::FunctionStart(function_start) => function_start.into_rpc_event(lookup),
            TraceData::FunctionEnd(function_end) => function_end.into_rpc_event(lookup),
            TraceData::LLMRequest(logged_llmrequest) => {
                baml_rpc::runtime_api::TraceData::Intermediate(
                    logged_llmrequest.into_rpc_event(lookup),
                )
            }
            TraceData::RawLLMRequest(httprequest) => {
                baml_rpc::runtime_api::TraceData::Intermediate(httprequest.into_rpc_event(lookup))
            }
            TraceData::RawLLMResponse(httpresponse) => {
                baml_rpc::runtime_api::TraceData::Intermediate(httpresponse.into_rpc_event(lookup))
            }
            TraceData::LLMResponse(logged_llmresponse) => {
                baml_rpc::runtime_api::TraceData::Intermediate(
                    logged_llmresponse.into_rpc_event(lookup),
                )
            }
            TraceData::SetTags(tags) => baml_rpc::runtime_api::TraceData::Intermediate(
                baml_rpc::runtime_api::IntermediateData::SetTags(
                    tags.clone().into_iter().collect(),
                ),
            ),
        }
    }
}
