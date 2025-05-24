use anyhow::Result;
use std::borrow::Cow;

use super::{IntoRpcEvent, TypeLookup};
use baml_types::HasFieldType;

impl<'a, T: HasFieldType> IntoRpcEvent<'a, baml_rpc::runtime_api::TraceData<'a>>
    for baml_types::tracing::events::FunctionStart<T>
{
    fn into_rpc_event(
        &'a self,
        lookup: &(impl TypeLookup + ?Sized),
    ) -> baml_rpc::runtime_api::TraceData<'a> {
        baml_rpc::runtime_api::TraceData::FunctionStart {
            function_display_name: self.name.clone(),
            args: self
                .args
                .iter()
                .map(|(k, v)| (k.clone(), v.into_rpc_event(lookup)))
                .collect(),
            tags: self
                .options
                .tags
                .iter()
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect(),
            baml_function_content: self.into_rpc_event(lookup),
        }
    }
}

impl<'a, T: HasFieldType> IntoRpcEvent<'a, Option<baml_rpc::runtime_api::BamlFunctionStart>>
    for baml_types::tracing::events::FunctionStart<T>
{
    fn into_rpc_event(
        &'a self,
        lookup: &(impl TypeLookup + ?Sized),
    ) -> Option<baml_rpc::runtime_api::BamlFunctionStart> {
        if self.is_baml_function {
            match lookup.function_lookup(&self.name).map(|id| {
                baml_rpc::runtime_api::BamlFunctionStart {
                    function_id: id,
                    eval_context: self.options.into_rpc_event(lookup),
                }
            }) {
                Some(baml_function_start) => Some(baml_function_start),
                None => {
                    // It's just a normal function call, not a baml function. No need to log.
                    // baml_log::error!("observability: Failed to find baml function: {}", self.name);
                    None
                }
            }
        } else {
            None
        }
    }
}

impl<'a, 'b> IntoRpcEvent<'a, baml_rpc::runtime_api::EvaluationContext>
    for baml_types::tracing::events::EvaluationContext
{
    fn into_rpc_event(
        &'a self,
        lookup: &(impl TypeLookup + ?Sized),
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
impl<'a, T: HasFieldType> IntoRpcEvent<'a, baml_rpc::runtime_api::TraceData<'a>>
    for baml_types::tracing::events::FunctionEnd<'a, T>
{
    fn into_rpc_event(
        &'a self,
        lookup: &(impl TypeLookup + ?Sized),
    ) -> baml_rpc::runtime_api::TraceData<'a> {
        let end = match self {
            baml_types::tracing::events::FunctionEnd::Success(baml_value_with_meta) => {
                baml_rpc::runtime_api::FunctionEnd::Success {
                    result: baml_value_with_meta.into_rpc_event(lookup),
                }
            }
            baml_types::tracing::events::FunctionEnd::Error(baml_error) => {
                baml_rpc::runtime_api::FunctionEnd::Error {
                    error: baml_error.into_rpc_event(lookup),
                }
            }
        };

        baml_rpc::runtime_api::TraceData::FunctionEnd(end)
    }
}

impl<'a, 'b> IntoRpcEvent<'a, baml_rpc::runtime_api::IntermediateData<'a>>
    for baml_types::tracing::events::LoggedLLMRequest
{
    fn into_rpc_event(
        &'a self,
        lookup: &(impl TypeLookup + ?Sized),
    ) -> baml_rpc::runtime_api::IntermediateData<'a> {
        baml_rpc::runtime_api::IntermediateData::LLMRequest {
            client_name: self.client_name.clone(),
            client_provider: self.client_provider.clone(),
            params: self
                .params
                .iter()
                .map(|(k, v)| (k.clone(), Cow::Borrowed(v)))
                .collect(),
            prompt: self
                .prompt
                .iter()
                .map(|p| p.into_rpc_event(lookup))
                .collect(),
        }
    }
}

impl<'a, 'b> IntoRpcEvent<'a, baml_rpc::runtime_api::LLMChatMessage<'a>>
    for baml_types::tracing::events::LLMChatMessage
{
    fn into_rpc_event(
        &'a self,
        lookup: &(impl TypeLookup + ?Sized),
    ) -> baml_rpc::runtime_api::LLMChatMessage<'a> {
        baml_rpc::runtime_api::LLMChatMessage {
            role: self.role.clone(),
            content: self
                .content
                .iter()
                .map(|p| p.into_rpc_event(lookup))
                .collect(),
        }
    }
}

impl<'a, 'b> IntoRpcEvent<'a, baml_rpc::runtime_api::LLMChatMessagePart<'a>>
    for baml_types::tracing::events::LLMChatMessagePart
{
    fn into_rpc_event(
        &'a self,
        lookup: &(impl TypeLookup + ?Sized),
    ) -> baml_rpc::runtime_api::LLMChatMessagePart<'a> {
        match self {
            baml_types::tracing::events::LLMChatMessagePart::Text(t) => {
                baml_rpc::runtime_api::LLMChatMessagePart::Text(Cow::Borrowed(t))
            }
            baml_types::tracing::events::LLMChatMessagePart::Media(baml_media) => {
                baml_rpc::runtime_api::LLMChatMessagePart::Media(baml_media.into_rpc_event(lookup))
            }
            baml_types::tracing::events::LLMChatMessagePart::WithMeta(
                llmchat_message_part,
                hash_map,
            ) => baml_rpc::runtime_api::LLMChatMessagePart::WithMeta(
                Box::new(llmchat_message_part.into_rpc_event(lookup)),
                hash_map.clone(),
            ),
        }
    }
}

impl<'a, 'b> IntoRpcEvent<'a, baml_rpc::runtime_api::LLMUsage>
    for baml_types::tracing::events::LLMUsage
{
    fn into_rpc_event(
        &'a self,
        lookup: &(impl TypeLookup + ?Sized),
    ) -> baml_rpc::runtime_api::LLMUsage {
        baml_rpc::runtime_api::LLMUsage {
            input_tokens: self.input_tokens,
            output_tokens: self.output_tokens,
            total_tokens: self.total_tokens,
        }
    }
}

impl<'a, 'b> IntoRpcEvent<'a, baml_rpc::runtime_api::HTTPBody<'a>>
    for baml_types::tracing::events::HTTPBody
{
    fn into_rpc_event(
        &'a self,
        lookup: &(impl TypeLookup + ?Sized),
    ) -> baml_rpc::runtime_api::HTTPBody<'a> {
        baml_rpc::runtime_api::HTTPBody {
            raw: Cow::Borrowed(self.raw()),
        }
    }
}

impl<'a, 'b> IntoRpcEvent<'a, baml_rpc::runtime_api::IntermediateData<'a>>
    for baml_types::tracing::events::HTTPRequest
{
    fn into_rpc_event(
        &'a self,
        lookup: &(impl TypeLookup + ?Sized),
    ) -> baml_rpc::runtime_api::IntermediateData<'a> {
        baml_rpc::runtime_api::IntermediateData::RawLLMRequest {
            url: self.url.clone(),
            method: self.method.clone(),
            headers: self.headers.clone(),
            body: self.body.into_rpc_event(lookup),
        }
    }
}

impl<'a, 'b> IntoRpcEvent<'a, baml_rpc::runtime_api::IntermediateData<'a>>
    for baml_types::tracing::events::HTTPResponse
{
    fn into_rpc_event(
        &'a self,
        lookup: &(impl TypeLookup + ?Sized),
    ) -> baml_rpc::runtime_api::IntermediateData<'a> {
        baml_rpc::runtime_api::IntermediateData::RawLLMResponse {
            status: self.status,
            headers: self.headers.clone(),
            body: self.body.into_rpc_event(lookup),
        }
    }
}

impl<'a, 'b> IntoRpcEvent<'a, baml_rpc::runtime_api::IntermediateData<'a>>
    for baml_types::tracing::events::LoggedLLMResponse
{
    fn into_rpc_event(
        &'a self,
        lookup: &(impl TypeLookup + ?Sized),
    ) -> baml_rpc::runtime_api::IntermediateData<'a> {
        baml_rpc::runtime_api::IntermediateData::LLMResponse {
            model: self.model.clone(),
            finish_reason: self.finish_reason.clone(),
            usage: self.usage.as_ref().map(|u| u.into_rpc_event(lookup)),
            raw_text_output: self
                .raw_text_output
                .as_ref()
                .map(|s| Cow::Borrowed(s.as_str())),
        }
    }
}
