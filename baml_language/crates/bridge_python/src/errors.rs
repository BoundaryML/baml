//! Python exception types for BAML errors.

use pyo3::{
    Bound, PyErr, create_exception,
    prelude::{PyModule, PyResult},
    types::PyModuleMethods,
};

create_exception!(baml_py, BamlError, pyo3::exceptions::PyException);
create_exception!(baml_py, BamlInvalidArgumentError, BamlError);
create_exception!(baml_py, BamlClientError, BamlError);
create_exception!(baml_py, BamlCancelledError, BamlError);

/// Register error types on the module.
pub fn register_errors(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add("BamlError", m.py().get_type::<BamlError>())?;
    m.add(
        "BamlInvalidArgumentError",
        m.py().get_type::<BamlInvalidArgumentError>(),
    )?;
    m.add("BamlClientError", m.py().get_type::<BamlClientError>())?;
    m.add(
        "BamlCancelledError",
        m.py().get_type::<BamlCancelledError>(),
    )?;
    Ok(())
}

pub fn bridge_error_to_py(err: bridge_cffi::error::BridgeError) -> PyErr {
    match err {
        bridge_cffi::BridgeError::Ctypes(ctypes_error) => {
            PyErr::new::<BamlInvalidArgumentError, _>(format!("Ctypes error: {ctypes_error}"))
        }
        bridge_cffi::BridgeError::NotInitialized => PyErr::new::<BamlInvalidArgumentError, _>(
            "Engine not initialized. Call create_baml_runtime first.",
        ),
        bridge_cffi::BridgeError::ProjectNotInitialized => {
            PyErr::new::<BamlClientError, _>("Project not initialized")
        }
        bridge_cffi::BridgeError::LockPoisoned => {
            PyErr::new::<BamlClientError, _>("Internal error: lock poisoned")
        }
        bridge_cffi::BridgeError::Runtime(runtime_error) => runtime_error_to_py(runtime_error),
        bridge_cffi::BridgeError::NullFunctionName => {
            PyErr::new::<BamlInvalidArgumentError, _>("Null function name pointer")
        }
        bridge_cffi::BridgeError::InvalidFunctionName(utf8_error) => {
            PyErr::new::<BamlInvalidArgumentError, _>(format!(
                "Invalid UTF-8 in function name: {utf8_error}",
            ))
        }
        bridge_cffi::BridgeError::FunctionNotFound { name } => {
            PyErr::new::<BamlInvalidArgumentError, _>(format!("Function not found: {name}"))
        }
        bridge_cffi::BridgeError::MissingArgument {
            function,
            parameter,
        } => PyErr::new::<BamlInvalidArgumentError, _>(format!(
            "Missing argument '{parameter}' for function '{function}'",
        )),
        bridge_cffi::BridgeError::NotImplemented(msg) => {
            PyErr::new::<BamlInvalidArgumentError, _>(format!("Not implemented: {msg}"))
        }
        bridge_cffi::BridgeError::DuplicateCallId(id) => PyErr::new::<BamlInvalidArgumentError, _>(
            format!("call_id {id} is already in use by an active call",),
        ),
        bridge_cffi::BridgeError::Internal(msg) => {
            PyErr::new::<BamlError, _>(format!("Internal error: {msg}"))
        }
    }
}

/// Convert a `bex_project::RuntimeError` into a Python exception.
pub fn runtime_error_to_py(err: bex_project::RuntimeError) -> PyErr {
    use bex_project::RuntimeError;

    match &err {
        RuntimeError::InvalidArgument { .. } => {
            PyErr::new::<BamlInvalidArgumentError, _>(err.to_string())
        }
        RuntimeError::Engine(engine_err) => {
            use bex_project::EngineError;
            match engine_err {
                EngineError::FunctionNotFound { .. } => {
                    PyErr::new::<BamlInvalidArgumentError, _>(err.to_string())
                }
                EngineError::Cancelled => PyErr::new::<BamlCancelledError, _>(err.to_string()),
                _ => PyErr::new::<BamlClientError, _>(err.to_string()),
            }
        }
        _ => PyErr::new::<BamlClientError, _>(err.to_string()),
    }
}
