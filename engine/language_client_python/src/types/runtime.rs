use crate::parse_py_type::parse_py_type;
use crate::types::function_results::FunctionResult;
use crate::BamlError;

use super::function_result_stream::FunctionResultStream;
use super::runtime_ctx_manager::RuntimeContextManager;
use baml_runtime::runtime_interface::ExperimentalTracingInterface;
use baml_runtime::BamlRuntime as CoreBamlRuntime;
use pyo3::prelude::{pymethods, PyResult};
use pyo3::{PyObject, Python, ToPyObject};
use std::collections::HashMap;
use std::path::PathBuf;

crate::lang_wrapper!(BamlRuntime, CoreBamlRuntime, clone_safe);

#[pymethods]
impl BamlRuntime {
    #[staticmethod]
    fn from_directory(directory: PathBuf, env_vars: HashMap<String, String>) -> PyResult<Self> {
        Ok(CoreBamlRuntime::from_directory(&directory, env_vars)
            .map_err(BamlError::from_anyhow)?
            .into())
    }

    #[staticmethod]
    fn from_files(
        root_path: String,
        files: HashMap<String, String>,
        env_vars: HashMap<String, String>,
    ) -> PyResult<Self> {
        Ok(
            CoreBamlRuntime::from_file_content(&root_path, &files, env_vars)
                .map_err(BamlError::from_anyhow)?
                .into(),
        )
    }

    #[pyo3()]
    fn create_context_manager(&self) -> RuntimeContextManager {
        self.inner.create_ctx_manager().into()
    }

    #[pyo3(signature = (function_name, args, ctx))]
    fn call_function(
        &self,
        py: Python<'_>,
        function_name: String,
        args: PyObject,
        ctx: &RuntimeContextManager,
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
                .map(FunctionResult::from)
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
        ctx: &RuntimeContextManager,
    ) -> PyResult<FunctionResultStream> {
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

        Ok(FunctionResultStream::new(stream, on_event))
    }

    #[pyo3()]
    fn flush(&self) -> PyResult<()> {
        self.inner.flush().map_err(BamlError::from_anyhow)
    }
}
