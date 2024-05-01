//mod parse_py_type;
mod python_types;

use anyhow::{bail, Result};
use baml_runtime::{BamlRuntime, RuntimeContext, RuntimeInterface};
//use parse_py_type::parse_py_type;
use pyo3::exceptions::{PyRuntimeError, PyTypeError};
//use pyo3::prelude::{Bound, PyAnyMethods};
use pyo3::prelude::{
    pyclass, pyfunction, pymethods, pymodule, wrap_pyfunction, PyModule, PyResult,
};
use pyo3::{create_exception, Py, PyAny, PyErr, PyObject, Python, ToPyObject};
use pythonize::depythonize;
use std::collections::HashMap;
use std::path::PathBuf;
use tokio::time::Duration;

create_exception!(baml_py, BamlError, pyo3::exceptions::PyException);

impl BamlError {
    fn from_anyhow(err: anyhow::Error) -> PyErr {
        PyErr::new::<BamlError, _>(format!("{:?}", err))
    }
}

#[pyclass]
struct BamlRuntimeFfi {
    internal: BamlRuntime,
    t: tokio::runtime::Runtime,
}

impl BamlRuntimeFfi {
    async fn call_async(&mut self) -> Result<python_types::FunctionResult> {
        let result = self
            .internal
            .call_function(
                "placeholder function".to_string(),
                HashMap::new(),
                &RuntimeContext::default(),
            )
            .await;
    }
}

#[pymethods]
impl BamlRuntimeFfi {
    #[staticmethod]
    fn from_directory(directory: PathBuf) -> PyResult<Self> {
        Ok(BamlRuntimeFfi {
            internal: BamlRuntime::from_directory(&directory).map_err(BamlError::from_anyhow)?,
            t: tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()?,
        })
    }

    /// TODO: ctx should be optional
    #[pyo3(signature = (function_name, args, *, ctx))]
    fn call_function(
        &mut self,
        function_name: String,
        args: PyObject,
        ctx: PyObject,
    ) -> PyResult<python_types::FunctionResult> {
        Python::with_gil(|py| {
            let args: HashMap<String, serde_json::Value> = depythonize(args.as_ref(py))?;
            let mut ctx: RuntimeContext = depythonize(ctx.as_ref(py))?;

            ctx.env = std::env::vars_os()
                .map(|(k, v)| {
                    (
                        k.to_string_lossy().to_string(),
                        v.to_string_lossy().to_string(),
                    )
                })
                .chain(ctx.env.into_iter())
                .collect();

            // TODO: support async
            let retval = self.t.block_on(self.internal.call_function(
                function_name.clone(),
                args,
                &ctx,
            ))?;

            Ok(python_types::FunctionResult::new(retval))
        })
        .map_err(BamlError::from_anyhow)
    }
}

#[pyfunction]
fn rust_sleep(py: Python<'_>) -> PyResult<&PyAny> {
    pyo3_asyncio::tokio::future_into_py(py, async {
        log::info!("Sleeping for 3 seconds");
        tokio::time::sleep(std::time::Duration::from_secs(3)).await;
        log::info!("Slept for 3 seconds");
        Ok(Python::with_gil(|py| py.None()))
    })
}

#[pymodule]
fn baml_py(py: Python<'_>, m: &PyModule) -> PyResult<()> {
    if let Err(e) = env_logger::try_init_from_env(
        env_logger::Env::new()
            .filter("BAML_LOG")
            .write_style("BAML_LOG_STYLE"),
    ) {
        eprintln!("Failed to initialize BAML logger: {:#}", e);
    };

    m.add_class::<BamlRuntimeFfi>()?;
    m.add_function(wrap_pyfunction!(rust_sleep, m)?)?;

    Ok(())
}
