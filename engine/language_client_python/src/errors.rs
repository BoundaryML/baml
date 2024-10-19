use baml_runtime::{
    errors::ExposedError, internal::llm_client::LLMResponse, scope_diagnostics::ScopeStack,
};
use pyo3::types::PyModule;
use pyo3::{create_exception, pymodule, Bound, PyErr, PyResult, Python};

create_exception!(baml_py, BamlError, pyo3::exceptions::PyException);
// Existing exception definitions
// A note on custom exceptions https://github.com/PyO3/pyo3/issues/295
create_exception!(baml_py, BamlInvalidArgumentError, BamlError);
create_exception!(baml_py, BamlClientError, BamlError);
create_exception!(baml_py, BamlClientHttpError, BamlClientError);

// Define the BamlValidationError exception with additional fields
// can't use extends=PyException yet https://github.com/PyO3/pyo3/discussions/3838

#[allow(non_snake_case)]
fn raise_baml_validation_error(prompt: String, message: String, raw_output: String) -> PyErr {
    Python::with_gil(|py| {
        let internal_monkeypatch = py.import("baml_py.internal_monkeypatch").unwrap();
        let exception = internal_monkeypatch.getattr("BamlValidationError").unwrap();
        let args = (prompt, message, raw_output);
        let inst = exception.call1(args).unwrap();
        PyErr::from_value(inst)
    })
}

/// Defines the errors module with the BamlValidationError exception.
/// IIRC the name of this function is the name of the module that pyo3 generates (errors.py)
#[pymodule]
pub fn errors(parent_module: &Bound<'_, PyModule>) -> PyResult<()> {
    parent_module.add(
        "BamlError",
        parent_module.py().get_type_bound::<BamlError>(),
    )?;
    parent_module.add(
        "BamlInvalidArgumentError",
        parent_module
            .py()
            .get_type_bound::<BamlInvalidArgumentError>(),
    )?;
    parent_module.add(
        "BamlClientError",
        parent_module.py().get_type_bound::<BamlClientError>(),
    )?;
    parent_module.add(
        "BamlClientHttpError",
        parent_module.py().get_type_bound::<BamlClientHttpError>(),
    )?;

    Ok(())
}

impl BamlError {
    pub fn from_anyhow(err: anyhow::Error) -> PyErr {
        if let Some(er) = err.downcast_ref::<ExposedError>() {
            match er {
                ExposedError::ValidationError {
                    prompt,
                    raw_output,
                    message,
                } => {
                    // Assuming ValidationError has fields that correspond to prompt, message, and raw_output
                    // If not, you may need to adjust this part based on the actual structure of ValidationError
                    raise_baml_validation_error(prompt.clone(), message.clone(), raw_output.clone())
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
                LLMResponse::UserFailure(msg) => {
                    PyErr::new::<BamlInvalidArgumentError, _>(format!("Invalid argument: {}", msg))
                }
                LLMResponse::InternalFailure(_) => PyErr::new::<BamlClientError, _>(format!(
                    "Something went wrong with the LLM client: {}",
                    err
                )),
            }
        } else {
            PyErr::new::<BamlError, _>(format!("{:?}", err))
        }
    }
}
