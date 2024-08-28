use baml_runtime::{
    errors::ExposedError, internal::llm_client::LLMResponse,
    internal_core::ir::scope_diagnostics::ScopeStack,
};

// napi::Error::new(napi::Status::GenericFailure, e.to_string()))

pub fn invalid_argument_error(message: &str) -> napi::Error {
    napi::Error::new(
        napi::Status::InvalidArg,
        format!("BamlError: BamlInvalidArgumentError: {}", message),
    )
}

pub fn from_anyhow_error(err: anyhow::Error) -> napi::Error {
    if let Some(er) = err.downcast_ref::<ExposedError>() {
        match er {
            ExposedError::ValidationError(_) => napi::Error::new(
                napi::Status::GenericFailure,
                format!("BamlError: BamlValidationError: {}", err),
            ),
        }
    } else if let Some(er) = err.downcast_ref::<ScopeStack>() {
        invalid_argument_error(&format!("{}", er))
    } else if let Some(er) = err.downcast_ref::<LLMResponse>() {
        match er {
            LLMResponse::Success(_) => napi::Error::new(
                napi::Status::GenericFailure,
                format!("BamlError: Unexpected error from BAML: {}", err),
            ),
            LLMResponse::LLMFailure(failed) => match &failed.code {
                baml_runtime::internal::llm_client::ErrorCode::Other(2) => napi::Error::new(
                    napi::Status::GenericFailure,
                    format!(
                        "BamlError: BamlClientError: Something went wrong with the LLM client: {}",
                        err
                    ),
                ),
                baml_runtime::internal::llm_client::ErrorCode::Other(_)
                | baml_runtime::internal::llm_client::ErrorCode::InvalidAuthentication
                | baml_runtime::internal::llm_client::ErrorCode::NotSupported
                | baml_runtime::internal::llm_client::ErrorCode::RateLimited
                | baml_runtime::internal::llm_client::ErrorCode::ServerError
                | baml_runtime::internal::llm_client::ErrorCode::ServiceUnavailable
                | baml_runtime::internal::llm_client::ErrorCode::UnsupportedResponse(_) => {
                    napi::Error::new(
                        napi::Status::GenericFailure,
                        format!("BamlError: BamlClientError: BamlClientHttpError: {}", err),
                    )
                }
            },
            LLMResponse::OtherFailure(_) => napi::Error::new(
                napi::Status::GenericFailure,
                format!(
                    "BamlError: BamlClientError: Something went wrong with the LLM client: {}",
                    err
                ),
            ),
        }
    } else {
        napi::Error::new(
            napi::Status::GenericFailure,
            format!("BamlError: {:?}", err),
        )
    }
}
