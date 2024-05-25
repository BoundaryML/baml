mod parse_py_type;
mod types;

use pyo3::prelude::{pyfunction, pymodule, PyAnyMethods, PyModule, PyResult};
use pyo3::{create_exception, wrap_pyfunction, Bound, PyErr, Python};

create_exception!(baml_py, BamlError, pyo3::exceptions::PyException);

impl BamlError {
    fn from_anyhow(err: anyhow::Error) -> PyErr {
        PyErr::new::<BamlError, _>(format!("{:?}", err))
    }
}

#[pyfunction]
fn invoke_runtime_cli(py: Python) -> PyResult<()> {
    Ok(baml_runtime::BamlRuntime::run_cli(
        py.import_bound("sys")?
            .getattr("argv")?
            .extract::<Vec<String>>()?,
        baml_runtime::CallerType::Python,
    )
    .map_err(BamlError::from_anyhow)?)
}

#[pymodule]
fn baml_py(_: Python<'_>, m: Bound<'_, PyModule>) -> PyResult<()> {
    if let Err(e) = env_logger::try_init_from_env(
        env_logger::Env::new()
            .filter("BAML_LOG")
            .write_style("BAML_LOG_STYLE"),
    ) {
        eprintln!("Failed to initialize BAML logger: {:#}", e);
    };

    m.add_class::<types::BamlRuntimePy>()?;
    m.add_class::<types::FunctionResultPy>()?;
    m.add_class::<types::FunctionResultStreamPy>()?;
    m.add_class::<types::BamlImagePy>()?;
    m.add_class::<types::RuntimeContextManagerPy>()?;
    m.add_class::<types::BamlSpanPy>()?;

    m.add_wrapped(wrap_pyfunction!(invoke_runtime_cli))?;

    Ok(())
}
