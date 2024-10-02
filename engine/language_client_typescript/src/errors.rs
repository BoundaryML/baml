use baml_runtime::{
    errors::ExposedError, internal::llm_client::LLMResponse, scope_diagnostics::ScopeStack,
};

// napi::Error::new(napi::Status::GenericFailure, e.to_string()))

pub fn invalid_argument_error(message: &str) -> napi::Error {
    napi::Error::new(
        napi::Status::InvalidArg,
        format!("BamlError: BamlInvalidArgumentError: {}", message),
    )
}

// Creating custom errors in JS is still not supported https://github.com/napi-rs/napi-rs/issues/1205
pub fn from_anyhow_error(err: anyhow::Error) -> napi::Error {
    if let Some(er) = err.downcast_ref::<ExposedError>() {
        match er {
            ExposedError::ValidationError {
                prompt,
                message,
                raw_response,
            } => throw_baml_validation_error(prompt, raw_response, message),
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
            LLMResponse::UserFailure(msg) => napi::Error::new(
                napi::Status::GenericFailure,
                format!("BamlError: BamlInvalidArgumentError: {}", msg),
            ),
            LLMResponse::InternalFailure(_) => napi::Error::new(
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

pub fn throw_baml_validation_error(prompt: &str, raw_output: &str, message: &str) -> napi::Error {
    let error_json = serde_json::json!({
        "type": "BamlValidationError",
        "prompt": prompt,
        "raw_output": raw_output,
        "message": format!("BamlValidationError: {}", message),
    });
    napi::Error::new(napi::Status::GenericFailure, error_json.to_string())
}
