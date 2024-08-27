use baml_runtime::{
    errors::ExposedError, internal::llm_client::LLMResponse,
    internal_core::ir::scope_diagnostics::ScopeStack,
};
use pyo3::{create_exception, PyErr};

create_exception!(baml_py, BamlError, pyo3::exceptions::PyException);
create_exception!(baml_py, BamlInvalidArgumentError, BamlError);
create_exception!(baml_py, BamlClientError, BamlError);
create_exception!(baml_py, BamlClientHttpError, BamlClientError);
create_exception!(baml_py, BamlValidationError, BamlError);

impl BamlError {
    pub fn from_anyhow(err: anyhow::Error) -> PyErr {
        if let Some(er) = err.downcast_ref::<ExposedError>() {
            match er {
                ExposedError::ValidationError(_) => {
                    PyErr::new::<BamlValidationError, _>(format!("{}", err))
                }
            }
        } else if let Some(er) = err.downcast_ref::<ScopeStack>() {
            PyErr::new::<BamlInvalidArgumentError, _>(format!("Invalid argument: {}", er))
        } else if let Some(er) = err.downcast_ref::<LLMResponse>() {
            match er {
                LLMResponse::Success(_) => {
                    PyErr::new::<BamlError, _>(format!("Unexpected error from BAML: {}", err))
                }
                LLMResponse::LLMFailure(failed) => match &failed.code {
                    baml_runtime::internal::llm_client::ErrorCode::Other(2) => {
                        PyErr::new::<BamlClientError, _>(format!(
                            "Something went wrong with the LLM client: {}",
                            err
                        ))
                    }
                    baml_runtime::internal::llm_client::ErrorCode::Other(_)
                    | baml_runtime::internal::llm_client::ErrorCode::InvalidAuthentication
                    | baml_runtime::internal::llm_client::ErrorCode::NotSupported
                    | baml_runtime::internal::llm_client::ErrorCode::RateLimited
                    | baml_runtime::internal::llm_client::ErrorCode::ServerError
                    | baml_runtime::internal::llm_client::ErrorCode::ServiceUnavailable
                    | baml_runtime::internal::llm_client::ErrorCode::UnsupportedResponse(_) => {
                        PyErr::new::<BamlClientHttpError, _>(format!("{}", err))
                    }
                },
                LLMResponse::OtherFailure(_) => PyErr::new::<BamlClientError, _>(format!(
                    "Something went wrong with the LLM client: {}",
                    err
                )),
            }
        } else {
            PyErr::new::<BamlError, _>(format!("{:?}", err))
        }
    }
}
