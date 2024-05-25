use crate::parse_py_type::parse_py_type;
use crate::types::function_results::FunctionResultPy;
use crate::BamlError;

use super::function_result_stream::FunctionResultStreamPy;
use super::runtime_ctx_manager::RuntimeContextManagerPy;
use baml_runtime::runtime_interface::ExperimentalTracingInterface;
use baml_runtime::BamlRuntime;
use pyo3::prelude::{pymethods, PyResult};
use pyo3::{PyObject, Python, ToPyObject};
use std::collections::HashMap;
use std::path::PathBuf;

crate::lang_wrapper!(BamlRuntimePy, BamlRuntime, clone_safe);

#[pymethods]
impl BamlRuntimePy {
    #[staticmethod]
    fn from_directory(directory: PathBuf, env_vars: HashMap<String, String>) -> PyResult<Self> {
        Ok(BamlRuntime::from_directory(&directory, env_vars)
            .map_err(BamlError::from_anyhow)?
            .into())
    }

    #[pyo3()]
    fn create_context_manager(&self) -> RuntimeContextManagerPy {
        self.inner.create_ctx_manager().into()
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

        let baml_runtime = self.inner.clone();
        let ctx_mng = ctx.inner.clone();

        pyo3_asyncio::tokio::future_into_py(py, async move {
            let ctx_mng = ctx_mng;
            let result = baml_runtime
                .call_function(function_name, &args_map, &ctx_mng)
                .await;

            result
                .0
                .map(FunctionResultPy::from)
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
    ) -> PyResult<FunctionResultStreamPy> {
        let args = parse_py_type(args.into_bound(py).to_object(py))?;
        let Some(args_map) = args.as_map() else {
            return Err(BamlError::new_err("Failed to parse args"));
        };
        log::debug!("pyo3 stream_function parsed args into: {:#?}", args_map);

        let ctx = ctx.inner.clone();
        let stream = self
            .inner
            .stream_function(function_name, args_map, &ctx)
            .map_err(BamlError::from_anyhow)?;

        Ok(FunctionResultStreamPy::new(stream, on_event))
    }

    #[pyo3()]
    fn flush(&self) -> PyResult<()> {
        self.inner.flush().map_err(BamlError::from_anyhow)
    }
}
