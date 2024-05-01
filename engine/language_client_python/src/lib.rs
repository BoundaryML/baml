mod parse_py_type;
mod python_types;

use anyhow::{bail, Result};
use baml_runtime::{BamlRuntime, RuntimeContext, RuntimeInterface};
use parse_py_type::parse_py_type;
use pyo3::exceptions::{PyRuntimeError, PyTypeError};
use pyo3::prelude::{
    pyclass, pyfunction, pymethods, pymodule, wrap_pyfunction, Bound, PyAnyMethods, PyModule,
    PyResult,
};
use pyo3::{create_exception, Py, PyAny, PyErr, PyObject, Python, ToPyObject};
use pythonize::depythonize_bound;
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
            let args: HashMap<String, serde_json::Value> = depythonize_bound(args.into_bound(py))?;
            let mut ctx: RuntimeContext = depythonize_bound(ctx.into_bound(py))?;

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

    fn sleep_3s(&self) -> PyResult<()> {
        Python::with_gil(|py| {
            //pyo3_asyncio::tokio::future_into_py(py, async move {
            //    tokio::time::sleep(Duration::from_secs(secs)).await;
            //    Ok(())
            //})
        });
        todo!()
    }
}

#[pymodule]
fn baml_py(m: &Bound<'_, PyModule>) -> PyResult<()> {
    if let Err(e) = env_logger::try_init_from_env(
        env_logger::Env::new()
            .filter("BAML_LOG")
            .write_style("BAML_LOG_STYLE"),
    ) {
        eprintln!("Failed to initialize BAML logger: {:#}", e);
    };

    m.add_class::<BamlRuntimeFfi>()?;
    Ok(())
}
