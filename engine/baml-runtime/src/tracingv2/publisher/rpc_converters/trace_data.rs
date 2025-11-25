use std::borrow::Cow;

use anyhow::Result;
use baml_rpc::RpcClientDetails;
use baml_types::{
    tracing::events::{redact_headers, FunctionType},
    type_meta, HasType,
};

use super::{types::to_rpc_event_without_types, IRRpcState, IntoRpcEvent};

impl<'a, T: std::fmt::Debug + HasType<type_meta::NonStreaming>>
    IntoRpcEvent<'a, baml_rpc::runtime_api::TraceData<'a>>
    for baml_types::tracing::events::FunctionStart<T>
{
    fn to_rpc_event(
        &'a self,
        lookup: &(impl IRRpcState + ?Sized),
    ) -> baml_rpc::runtime_api::TraceData<'a> {
        // For Native functions, skip generating expensive type references
        // For LLM functions, include full type information
        let args: Vec<(String, baml_rpc::runtime_api::BamlValue)> =
            if self.function_type == FunctionType::Native {
                self.args
                    .iter()
                    .map(|(k, v)| (k.clone(), to_rpc_event_without_types(v, lookup)))
                    .collect()
            } else {
                self.args
                    .iter()
                    .map(|(k, v)| (k.clone(), v.to_rpc_event(lookup)))
                    .collect()
            };

        baml_rpc::runtime_api::TraceData::FunctionStart {
            function_display_name: self.name.clone(),
            function_type: function_type_to_rpc(&self.function_type),
            args,
            is_stream: self.is_stream,
            tags: self
                .options
                .tags
                .iter()
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect(),
            baml_function_content: self.to_rpc_event(lookup),
        }
    }
}

fn function_type_to_rpc(
    value: &baml_types::tracing::events::FunctionType,
) -> baml_rpc::runtime_api::FunctionType {
    match value {
        baml_types::tracing::events::FunctionType::BamlLlm => {
            baml_rpc::runtime_api::FunctionType::BamlLlm
        }
        baml_types::tracing::events::FunctionType::Native => {
            baml_rpc::runtime_api::FunctionType::Native
        }
    }
}

impl<'a, T: HasType<type_meta::NonStreaming>>
    IntoRpcEvent<'a, Option<baml_rpc::runtime_api::BamlFunctionStart>>
    for baml_types::tracing::events::FunctionStart<T>
{
    fn to_rpc_event(
        &'a self,
        lookup: &(impl IRRpcState + ?Sized),
    ) -> Option<baml_rpc::runtime_api::BamlFunctionStart> {
        if self.function_type == FunctionType::BamlLlm {
            lookup
                .function_lookup(&self.name)
                .map(|id| baml_rpc::runtime_api::BamlFunctionStart {
                    function_id: id,
                    baml_src_hash: lookup
                        .baml_src_hash()
                        .unwrap_or_else(|| "unknown_hash".to_string()),
                    eval_context: self.options.to_rpc_event(lookup),
                })
        } else {
            None
        }
    }
}

impl<'a> IntoRpcEvent<'a, baml_rpc::runtime_api::EvaluationContext>
    for baml_types::tracing::events::EvaluationContext
{
    fn to_rpc_event(
        &'a self,
        lookup: &(impl IRRpcState + ?Sized),
    ) -> baml_rpc::runtime_api::EvaluationContext {
        baml_rpc::runtime_api::EvaluationContext {
            tags: self
                .tags
                .iter()
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect(),
            type_builder: None,
        }
    }
}
impl<'a, T: std::fmt::Debug + HasType<type_meta::NonStreaming>>
    IntoRpcEvent<'a, baml_rpc::runtime_api::TraceData<'a>>
    for baml_types::tracing::events::FunctionEnd<'a, T>
{
    fn to_rpc_event(
        &'a self,
        lookup: &(impl IRRpcState + ?Sized),
    ) -> baml_rpc::runtime_api::TraceData<'a> {
        let end = match self {
            baml_types::tracing::events::FunctionEnd::Success {
                value,
                function_type,
            } => {
                // For Native functions, skip generating expensive type references
                let result = if *function_type == FunctionType::Native {
                    to_rpc_event_without_types(value, lookup)
                } else {
                    value.to_rpc_event(lookup)
                };
                baml_rpc::runtime_api::FunctionEnd::Success { result }
            }
            baml_types::tracing::events::FunctionEnd::Error {
                error,
                function_type: _,
            } => baml_rpc::runtime_api::FunctionEnd::Error {
                error: error.to_rpc_event(lookup),
            },
        };

        baml_rpc::runtime_api::TraceData::FunctionEnd(end)
    }
}

impl<'a> IntoRpcEvent<'a, baml_rpc::runtime_api::IntermediateData<'a>>
    for baml_types::tracing::events::LoggedLLMRequest
{
    fn to_rpc_event(
        &'a self,
        lookup: &(impl IRRpcState + ?Sized),
    ) -> baml_rpc::runtime_api::IntermediateData<'a> {
        baml_rpc::runtime_api::IntermediateData::LLMRequest {
            client_name: self.client_name.clone(),
            client_provider: self.client_provider.clone(),
            params: self
                .params
                .iter()
                .map(|(k, v)| (k.clone(), Cow::Borrowed(v)))
                .collect(),
            prompt: self.prompt.iter().map(|p| p.to_rpc_event(lookup)).collect(),
        }
    }
}

impl<'a> IntoRpcEvent<'a, baml_rpc::runtime_api::LLMChatMessage<'a>>
    for baml_types::tracing::events::LLMChatMessage
{
    fn to_rpc_event(
        &'a self,
        lookup: &(impl IRRpcState + ?Sized),
    ) -> baml_rpc::runtime_api::LLMChatMessage<'a> {
        baml_rpc::runtime_api::LLMChatMessage {
            role: self.role.clone(),
            content: self
                .content
                .iter()
                .map(|p| p.to_rpc_event(lookup))
                .collect(),
        }
    }
}

impl<'a> IntoRpcEvent<'a, baml_rpc::runtime_api::LLMChatMessagePart<'a>>
    for baml_types::tracing::events::LLMChatMessagePart
{
    fn to_rpc_event(
        &'a self,
        lookup: &(impl IRRpcState + ?Sized),
    ) -> baml_rpc::runtime_api::LLMChatMessagePart<'a> {
        match self {
            baml_types::tracing::events::LLMChatMessagePart::Text(t) => {
                baml_rpc::runtime_api::LLMChatMessagePart::Text(Cow::Borrowed(t))
            }
            baml_types::tracing::events::LLMChatMessagePart::Media(baml_media) => {
                baml_rpc::runtime_api::LLMChatMessagePart::Media(baml_media.to_rpc_event(lookup))
            }
            baml_types::tracing::events::LLMChatMessagePart::WithMeta(
                llmchat_message_part,
                hash_map,
            ) => baml_rpc::runtime_api::LLMChatMessagePart::WithMeta(
                Box::new(llmchat_message_part.to_rpc_event(lookup)),
                hash_map.clone(),
            ),
        }
    }
}

impl<'a> IntoRpcEvent<'a, baml_rpc::runtime_api::LLMUsage>
    for baml_types::tracing::events::LLMUsage
{
    fn to_rpc_event(
        &'a self,
        lookup: &(impl IRRpcState + ?Sized),
    ) -> baml_rpc::runtime_api::LLMUsage {
        baml_rpc::runtime_api::LLMUsage {
            input_tokens: self.input_tokens,
            output_tokens: self.output_tokens,
            total_tokens: self.total_tokens,
            cached_input_tokens: self.cached_input_tokens,
        }
    }
}

impl<'a> IntoRpcEvent<'a, baml_rpc::runtime_api::HTTPBody<'a>>
    for baml_types::tracing::events::HTTPBody
{
    fn to_rpc_event(
        &'a self,
        lookup: &(impl IRRpcState + ?Sized),
    ) -> baml_rpc::runtime_api::HTTPBody<'a> {
        baml_rpc::runtime_api::HTTPBody {
            raw: Cow::Borrowed(self.raw()),
        }
    }
}

impl<'a> IntoRpcEvent<'a, baml_rpc::runtime_api::IntermediateData<'a>>
    for baml_types::tracing::events::HTTPRequest
{
    fn to_rpc_event(
        &'a self,
        lookup: &(impl IRRpcState + ?Sized),
    ) -> baml_rpc::runtime_api::IntermediateData<'a> {
        baml_rpc::runtime_api::IntermediateData::RawLLMRequest {
            http_request_id: self.id.to_string(),
            url: self.url().to_string(),
            method: self.method().to_string(),
            headers: redact_headers(self.headers().clone()),
            client_details: RpcClientDetails {
                name: self.client_details.name.clone(),
                provider: self.client_details.provider.clone(),
                options: self.client_details.options.clone(),
            },
            body: self.body().to_rpc_event(lookup),
        }
    }
}

impl<'a> IntoRpcEvent<'a, baml_rpc::runtime_api::IntermediateData<'a>>
    for baml_types::tracing::events::HTTPResponse
{
    fn to_rpc_event(
        &'a self,
        lookup: &(impl IRRpcState + ?Sized),
    ) -> baml_rpc::runtime_api::IntermediateData<'a> {
        baml_rpc::runtime_api::IntermediateData::RawLLMResponse {
            http_request_id: self.request_id.to_string(),
            status: self.status,
            headers: self.headers().cloned().map(redact_headers),
            body: self.body.to_rpc_event(lookup),
            client_details: RpcClientDetails {
                name: self.client_details.name.clone(),
                provider: self.client_details.provider.clone(),
                options: self.client_details.options.clone(),
            },
        }
    }
}

impl<'a> IntoRpcEvent<'a, baml_rpc::runtime_api::IntermediateData<'a>>
    for baml_types::tracing::events::HTTPResponseStream
{
    fn to_rpc_event(
        &'a self,
        lookup: &(impl IRRpcState + ?Sized),
    ) -> baml_rpc::runtime_api::IntermediateData<'a> {
        baml_rpc::runtime_api::IntermediateData::RawLLMResponseStream {
            http_request_id: self.request_id.to_string(),
            event: baml_rpc::runtime_api::Event {
                raw: Cow::Borrowed(&self.event.data),
            },
        }
    }
}

impl<'a> IntoRpcEvent<'a, baml_rpc::runtime_api::IntermediateData<'a>>
    for baml_types::tracing::events::LoggedLLMResponse
{
    fn to_rpc_event(
        &'a self,
        lookup: &(impl IRRpcState + ?Sized),
    ) -> baml_rpc::runtime_api::IntermediateData<'a> {
        baml_rpc::runtime_api::IntermediateData::LLMResponse {
            client_stack: self.client_stack.clone(),
            model: self.model.clone(),
            finish_reason: self.finish_reason.clone(),
            usage: self.usage.as_ref().map(|u| u.to_rpc_event(lookup)),
            raw_text_output: self
                .raw_text_output
                .as_ref()
                .map(|s| Cow::Borrowed(s.as_str())),
        }
    }
}
