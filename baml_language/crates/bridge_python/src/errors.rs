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

/// Convert a `bex_engine::EngineError` into a Python exception.
pub fn engine_error_to_py(err: bex_engine::EngineError) -> PyErr {
    use bex_engine::EngineError;

    match &err {
        EngineError::FunctionNotFound { .. } => {
            PyErr::new::<BamlInvalidArgumentError, _>(err.to_string())
        }
        EngineError::Cancelled => PyErr::new::<BamlCancelledError, _>(err.to_string()),
        _ => PyErr::new::<BamlClientError, _>(err.to_string()),
    }
}
