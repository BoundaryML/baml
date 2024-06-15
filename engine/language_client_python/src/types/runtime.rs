use crate::parse_py_type::parse_py_type;
use crate::types::function_results::FunctionResult;
use crate::BamlError;

use super::function_result_stream::FunctionResultStream;
use super::runtime_ctx_manager::RuntimeContextManager;
use super::type_builder::TypeBuilder;
use baml_runtime::runtime_interface::ExperimentalTracingInterface;
use baml_runtime::BamlRuntime as CoreBamlRuntime;
use pyo3::prelude::{pymethods, PyResult};
use pyo3::types::{PyDict, PyTuple};
use pyo3::{PyObject, Python, ToPyObject};
use std::collections::HashMap;
use std::path::PathBuf;

crate::lang_wrapper!(BamlRuntime, CoreBamlRuntime, clone_safe);
crate::lang_wrapper!(LogEvent, baml_runtime::on_log_event::LogEvent, clone_safe);

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
        self.inner
            .create_ctx_manager(baml_types::BamlValue::String("python".to_string()))
            .into()
    }

    #[pyo3(signature = (function_name, args, ctx, tb))]
    fn call_function(
        &self,
        py: Python<'_>,
        function_name: String,
        args: PyObject,
        ctx: &RuntimeContextManager,
        tb: Option<&TypeBuilder>,
    ) -> PyResult<PyObject> {
        let Some(args) = parse_py_type(args.into_bound(py).to_object(py), false)? else {
            return Err(BamlError::new_err(
                "Failed to parse args, perhaps you used a non-serializable type?",
            ));
        };
        let Some(args_map) = args.as_map_owned() else {
            return Err(BamlError::new_err("Failed to parse args"));
        };
        log::debug!("pyo3 call_function parsed args into: {:#?}", args_map);

        let baml_runtime = self.inner.clone();
        let ctx_mng = ctx.inner.clone();
        let tb = tb.map(|tb| tb.inner.clone());

        pyo3_asyncio::tokio::future_into_py(py, async move {
            let ctx_mng = ctx_mng;
            let result = baml_runtime
                .call_function(function_name, &args_map, &ctx_mng, tb.as_ref())
                .await;

            result
                .0
                .map(FunctionResult::from)
                .map_err(BamlError::from_anyhow)
        })
        .map(|f| f.into())
    }

    #[pyo3(signature = (function_name, args, on_event, ctx, tb))]
    fn stream_function(
        &self,
        py: Python<'_>,
        function_name: String,
        args: PyObject,
        on_event: Option<PyObject>,
        ctx: &RuntimeContextManager,
        tb: Option<&TypeBuilder>,
    ) -> PyResult<FunctionResultStream> {
        let Some(args) = parse_py_type(args.into_bound(py).to_object(py), false)? else {
            return Err(BamlError::new_err(
                "Failed to parse args, perhaps you used a non-serializable type?",
            ));
        };
        let Some(args_map) = args.as_map() else {
            return Err(BamlError::new_err("Failed to parse args"));
        };
        log::debug!("pyo3 stream_function parsed args into: {:#?}", args_map);

        let ctx = ctx.inner.clone();
        let stream = self
            .inner
            .stream_function(
                function_name,
                args_map,
                &ctx,
                tb.map(|tb| tb.inner.clone()).as_ref(),
            )
            .map_err(BamlError::from_anyhow)?;

        Ok(FunctionResultStream::new(
            stream,
            on_event,
            tb.map(|tb| tb.inner.clone()),
        ))
    }

    #[pyo3()]
    fn flush(&self) -> PyResult<()> {
        self.inner.flush().map_err(BamlError::from_anyhow)
    }

    #[pyo3()]
    fn set_log_event_callback(&self, callback: PyObject) -> PyResult<()> {
        let callback = callback.clone();
        let baml_runtime = self.inner.clone();

        let res = baml_runtime
            .as_ref()
            .set_log_event_callback(Box::new(move |_| {
                Python::with_gil(|py| {
                    // Ensure GIL is acquired before calling Python code
                    // let callback = callback.as_ref(py);
                    let any: PyObject = PyDict::new_bound(py).into();
                    match callback.call1(py, (any,)) {
                        Ok(_) => Ok(()),
                        Err(e) => {
                            log::error!("Error calling log_event_callback: {:?}", e);
                            Err(anyhow::Error::new(e).into()) // Proper error handling
                        }
                    }
                })
            }));

        Ok(())
    }
}
