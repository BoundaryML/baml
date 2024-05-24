mod parse_py_type;
mod python_types;

use baml_runtime::runtime_interface::ExperimentalTracingInterface;
use baml_runtime::{BamlRuntime, RuntimeContextManager};
use baml_types::BamlValue;
use parse_py_type::parse_py_type;
use pyo3::prelude::{pyclass, pyfunction, pymethods, pymodule, PyAnyMethods, PyModule, PyResult};
use pyo3::types::PyNone;
use pyo3::PyAny;
use pyo3::{create_exception, wrap_pyfunction, Bound, PyErr, PyObject, Python, ToPyObject};

use std::collections::HashMap;
use std::ops::Deref;
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

#[pyclass]
struct RuntimeContextManagerPy {
    inner: RuntimeContextManager,
}

#[pymethods]
impl RuntimeContextManagerPy {
    #[pyo3()]
    fn upsert_tags(&self, py: Python<'_>, tags: PyObject) -> PyResult<bool> {
        let tags = parse_py_type(tags.into_bound(py).to_object(py))?;
        let Some(tags) = tags.as_map_owned() else {
            return Err(BamlError::new_err("Failed to parse tags"));
        };
        self.inner.upsert_tags(tags.into_iter().collect());
        Ok(true)
    }

    #[pyo3()]
    fn deep_clone(&self) -> Self {
        RuntimeContextManagerPy {
            inner: self.inner.deep_clone(),
        }
    }
}

#[pymethods]
impl BamlRuntimeFfi {
    #[staticmethod]
    fn from_directory(directory: PathBuf, env_vars: HashMap<String, String>) -> PyResult<Self> {
        Ok(BamlRuntimeFfi {
            internal: Arc::new(
                BamlRuntime::from_directory(&directory, env_vars)
                    .map_err(BamlError::from_anyhow)?,
            ),
        })
    }

    #[pyo3()]
    fn create_context_manager(&self) -> RuntimeContextManagerPy {
        RuntimeContextManagerPy {
            inner: self.internal.create_ctx_manager(),
        }
    }

    /// TODO: ctx should be optional
    #[pyo3(signature = (function_name, args, *, ctx))]
    fn call_function(
        &self,
        py: Python<'_>,
        function_name: String,
        args: PyObject,
        ctx: &RuntimeContextManagerPy,
    ) -> PyResult<PyObject> {
        let args = parse_py_type(args.into_bound(py).to_object(py))?;
        let Some(args_map) = args.as_map_owned() else {
            return Err(BamlError::new_err("Failed to parse args"));
        };
        log::debug!("pyo3 call_function parsed args into: {:#?}", args_map);

        let baml_runtime = self.internal.clone();

        let ctx_mng = ctx.inner.clone();

        pyo3_asyncio::tokio::future_into_py(py, async move {
            let ctx_mng = ctx_mng;
            let result = baml_runtime
                .call_function(function_name, &args_map, &ctx_mng)
                .await;

            result
                .0
                .map(python_types::FunctionResult::new)
                .map_err(BamlError::from_anyhow)
        })
        .map(|f| f.into())
    }

    #[pyo3(signature = (function_name, args, on_event, ctx))]
    fn stream_function(
        &self,
        py: Python<'_>,
        function_name: String,
        args: PyObject,
        on_event: Option<PyObject>,
        ctx: &RuntimeContextManagerPy,
    ) -> PyResult<python_types::FunctionResultStream> {
        let args = parse_py_type(args.into_bound(py).to_object(py))?;
        let Some(args_map) = args.as_map() else {
            return Err(BamlError::new_err("Failed to parse args"));
        };
        log::debug!("pyo3 stream_function parsed args into: {:#?}", args_map);

        let ctx = ctx.inner.clone();
        let stream = self
            .internal
            .stream_function(function_name, args_map, &ctx)
            .map_err(BamlError::from_anyhow)?;

        Ok(python_types::FunctionResultStream::new(stream, on_event))
    }

    #[pyo3()]
    fn flush(&self) -> PyResult<()> {
        self.internal.flush().map_err(BamlError::from_anyhow)
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

    m.add_class::<BamlRuntimeFfi>()?;
    m.add_class::<python_types::FunctionResult>()?;
    m.add_class::<python_types::FunctionResultStream>()?;
    m.add_class::<python_types::BamlImagePy>()?;
    m.add_class::<RuntimeContextManagerPy>()?;
    m.add_class::<python_types::BamlSpanPy>()?;

    m.add_wrapped(wrap_pyfunction!(invoke_runtime_cli))?;

    Ok(())
}
