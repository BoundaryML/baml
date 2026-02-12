//! Python exception types for BAML errors.

use pyo3::{
    create_exception,
    prelude::{PyModule, PyResult},
    types::PyModuleMethods,
    Bound, PyErr,
};

create_exception!(baml_py, BamlError, pyo3::exceptions::PyException);
create_exception!(baml_py, BamlInvalidArgumentError, BamlError);
create_exception!(baml_py, BamlClientError, BamlError);

/// Register error types on the module.
pub fn register_errors(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add("BamlError", m.py().get_type::<BamlError>())?;
    m.add(
        "BamlInvalidArgumentError",
        m.py().get_type::<BamlInvalidArgumentError>(),
    )?;
    m.add(
        "BamlClientError",
        m.py().get_type::<BamlClientError>(),
    )?;
    Ok(())
}

/// Convert a `baml_cffi::BridgeError` into a Python exception.
pub fn bridge_error_to_py(err: baml_cffi::error::BridgeError) -> PyErr {
    use baml_cffi::error::BridgeError;

    match &err {
        BridgeError::NotInitialized
        | BridgeError::ProjectNotInitialized
        | BridgeError::LockPoisoned => PyErr::new::<BamlError, _>(err.to_string()),

        BridgeError::Compilation { .. } => {
            PyErr::new::<BamlInvalidArgumentError, _>(err.to_string())
        }

        BridgeError::Engine(_) => PyErr::new::<BamlClientError, _>(err.to_string()),

        BridgeError::FunctionNotFound { .. }
        | BridgeError::MissingArgument { .. }
        | BridgeError::HandleNotSupported
        | BridgeError::MapEntryMissingKey => {
            PyErr::new::<BamlInvalidArgumentError, _>(err.to_string())
        }

        BridgeError::ProtobufDecode(_)
        | BridgeError::NullBuffer
        | BridgeError::NullFunctionName
        | BridgeError::InvalidFunctionName(_) => {
            PyErr::new::<BamlError, _>(err.to_string())
        }

        BridgeError::NotImplemented(_) => PyErr::new::<BamlError, _>(err.to_string()),
    }
}

/// Convert a `bex_engine::EngineError` into a Python exception.
pub fn engine_error_to_py(err: bex_engine::EngineError) -> PyErr {
    use bex_engine::EngineError;

    match &err {
        EngineError::FunctionNotFound { .. } => {
            PyErr::new::<BamlInvalidArgumentError, _>(err.to_string())
        }
        _ => PyErr::new::<BamlClientError, _>(err.to_string()),
    }
}
