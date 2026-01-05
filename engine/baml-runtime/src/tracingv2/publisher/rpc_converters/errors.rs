use std::borrow::Cow;

use super::{IRRpcState, IntoRpcEvent};

impl<'a> IntoRpcEvent<'a, baml_rpc::runtime_api::BamlFunctionCallError<'a>>
    for baml_types::tracing::events::BamlError<'a>
{
    fn to_rpc_event(
        &'a self,
        lookup: &(impl IRRpcState + ?Sized),
    ) -> baml_rpc::runtime_api::BamlFunctionCallError<'a> {
        match self {
            baml_types::tracing::events::BamlError::External { message } => {
                baml_rpc::runtime_api::BamlFunctionCallError::ExternalException {
                    message: Cow::Borrowed(message),
                }
            }
            baml_types::tracing::events::BamlError::Internal { message } => {
                baml_rpc::runtime_api::BamlFunctionCallError::InternalException {
                    message: Cow::Borrowed(message),
                }
            }
            baml_types::tracing::events::BamlError::Base { message } => {
                baml_rpc::runtime_api::BamlFunctionCallError::Base {
                    message: Cow::Borrowed(message),
                }
            }
            baml_types::tracing::events::BamlError::InvalidArgument { message } => {
                baml_rpc::runtime_api::BamlFunctionCallError::InvalidArgument {
                    message: Cow::Borrowed(message),
                }
            }
            baml_types::tracing::events::BamlError::Client { message } => {
                baml_rpc::runtime_api::BamlFunctionCallError::Client {
                    message: Cow::Borrowed(message),
                }
            }
            baml_types::tracing::events::BamlError::ClientHttp {
                message,
                status_code,
            } => baml_rpc::runtime_api::BamlFunctionCallError::ClientHttp {
                message: Cow::Borrowed(message),
                status_code: *status_code,
            },
            baml_types::tracing::events::BamlError::ClientFinishReason {
                finish_reason,
                message,
                prompt,
                raw_output,
            } => baml_rpc::runtime_api::BamlFunctionCallError::ClientFinishReason {
                finish_reason: Cow::Borrowed(finish_reason),
                message: Cow::Borrowed(message),
                prompt: Cow::Borrowed(prompt),
                raw_output: Cow::Borrowed(raw_output),
            },
            // prompt and raw_output are already available in the llm_response field
            // in the database and the http request/response data
            baml_types::tracing::events::BamlError::Validation {
                raw_output: _,
                message,
                prompt: _,
            } => baml_rpc::runtime_api::BamlFunctionCallError::Validation {
                raw_output: None,
                message: Cow::Borrowed(message),
                prompt: None,
            },
        }
    }
}
