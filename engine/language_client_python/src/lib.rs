mod parse_py_type;
mod python_types;

use baml_runtime::{BamlRuntime, PublicInterface, RuntimeContext};
use baml_types::BamlValue;
use indexmap::IndexMap;
use parse_py_type::parse_py_type;
use pyo3::prelude::{pyclass, pyfunction, pymethods, pymodule, PyAnyMethods, PyModule, PyResult};
use pyo3::{create_exception, wrap_pyfunction, Bound, PyErr, PyObject, Python, ToPyObject};
use pythonize::depythonize_bound;

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

fn convert_to_hashmap(value: BamlValue) -> Option<IndexMap<String, BamlValue>> {
    match value {
        BamlValue::Map(map) => Some(map.into_iter().collect()),
        _ => None,
    }
}

#[pymethods]
impl BamlRuntimeFfi {
    #[staticmethod]
    fn from_directory(py: Python<'_>, directory: PathBuf, ctx: PyObject) -> PyResult<Self> {
        let ctx: RuntimeContext =
            RuntimeContext::from_env().merge(Some(depythonize_bound::<
                baml_runtime::RuntimeContext,
            >(ctx.into_bound(py))?));

        Ok(BamlRuntimeFfi {
            internal: Arc::new(
                BamlRuntime::from_directory(&directory, &ctx).map_err(BamlError::from_anyhow)?,
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
        let args = parse_py_type(args.into_bound(py).to_object(py))?;
        let Some(args_map) = convert_to_hashmap(args) else {
            return Err(BamlError::new_err("Failed to parse args"));
        };
        log::debug!("pyo3 call_function parsed args into: {:#?}", args_map);
        let ctx = RuntimeContext::from_env().merge(Some(depythonize_bound::<
            baml_runtime::RuntimeContext,
        >(ctx.into_bound(py))?));

        let baml_runtime = self.internal.clone();

        pyo3_asyncio::tokio::future_into_py(py, async move {
            let result = baml_runtime
                .call_function(function_name, args_map, ctx)
                .await;

            result
                .0
                .map(python_types::FunctionResult::new)
                .map_err(BamlError::from_anyhow)
        })
        .map(|f| f.into())
    }

    #[pyo3(signature = (function_name, args, *, ctx, on_event = None))]
    fn stream_function(
        &self,
        py: Python<'_>,
        function_name: String,
        args: PyObject,
        ctx: PyObject,
        on_event: Option<PyObject>,
    ) -> PyResult<python_types::FunctionResultStream> {
        let args = parse_py_type(args.into_bound(py).to_object(py))?;
        let Some(args_map) = convert_to_hashmap(args) else {
            return Err(BamlError::new_err("Failed to parse args"));
        };
        log::debug!("pyo3 stream_function parsed args into: {:#?}", args_map);
        let ctx = RuntimeContext::from_env().merge(Some(depythonize_bound::<
            baml_runtime::RuntimeContext,
        >(ctx.into_bound(py))?));

        let stream = self
            .internal
            .stream_function(function_name, args_map, ctx)
            .map_err(BamlError::from_anyhow)?;

        Ok(python_types::FunctionResultStream::new(stream, on_event))
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

    m.add_wrapped(wrap_pyfunction!(invoke_runtime_cli))?;

    Ok(())
}
