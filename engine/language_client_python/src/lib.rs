//mod parse_py_type;
mod python_types;

use baml_runtime::{BamlRuntime, RuntimeContext, RuntimeInterface};
use pyo3::prelude::{pyclass, pymethods, pymodule, PyModule, PyResult};
use pyo3::{create_exception, PyErr, PyObject, Python};
use pythonize::depythonize;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

create_exception!(baml_py, BamlError, pyo3::exceptions::PyException);

impl BamlError {
    fn from_anyhow(err: anyhow::Error) -> PyErr {
        PyErr::new::<BamlError, _>(format!("{:?}", err))
    }
}

#[pyclass]
struct BamlRuntimeFfi {
    internal: Arc<BamlRuntime>,
}

#[pymethods]
impl BamlRuntimeFfi {
    #[staticmethod]
    fn from_directory(directory: PathBuf) -> PyResult<Self> {
        Ok(BamlRuntimeFfi {
            internal: Arc::new(
                BamlRuntime::from_directory(&directory).map_err(BamlError::from_anyhow)?,
            ),
        })
    }

    /// TODO: ctx should be optional
    #[pyo3(signature = (function_name, args, *, ctx))]
    fn call_function(
        &self,
        py: Python<'_>,
        function_name: String,
        args: PyObject,
        ctx: PyObject,
    ) -> PyResult<PyObject> {
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

        let baml_runtime = Arc::clone(&self.internal);

        pyo3_asyncio::tokio::future_into_py(py, async move {
            let result = baml_runtime
                .call_function(function_name, args, &ctx)
                .await
                .map(python_types::FunctionResult::new)
                .map_err(BamlError::from_anyhow);

            result
        })
        .map(|f| f.into())
    }
}

#[pymodule]
fn baml_py(_: Python<'_>, m: &PyModule) -> PyResult<()> {
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
