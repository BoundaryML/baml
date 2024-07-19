use crate::parse_py_type::parse_py_type;
use crate::types::function_results::FunctionResult;
use crate::types::trace_stats::TraceStats;
use crate::BamlError;

use crate::types::function_result_stream::{FunctionResultStream, SyncFunctionResultStream};
use crate::types::runtime_ctx_manager::RuntimeContextManager;
use crate::types::type_builder::TypeBuilder;
use crate::types::ClientRegistry;
use baml_runtime::runtime_interface::ExperimentalTracingInterface;
use baml_runtime::BamlRuntime as CoreBamlRuntime;
use pyo3::prelude::{pymethods, PyResult};
use pyo3::{pyclass, PyObject, Python, ToPyObject};
use std::collections::HashMap;
use std::path::PathBuf;

crate::lang_wrapper!(BamlRuntime, CoreBamlRuntime, clone_safe);

#[derive(Debug, Clone)]
#[pyclass]
pub struct BamlLogEvent {
    pub metadata: LogEventMetadata,
    pub prompt: Option<String>,
    pub raw_output: Option<String>,
    // json structure or a string
    pub parsed_output: Option<String>,
    pub start_time: String,
}

#[derive(Debug, Clone)]
#[pyclass]
pub struct LogEventMetadata {
    pub event_id: String,
    pub parent_id: Option<String>,
    pub root_event_id: String,
}

#[pymethods]
impl BamlLogEvent {
    fn __repr__(&self) -> String {
        format!(
            "BamlLogEvent {{\n    metadata: {:?},\n    prompt: {:?},\n    raw_output: {:?},\n    parsed_output: {:?},\n    start_time: {:?}\n}}",
            self.metadata, self.prompt, self.raw_output, self.parsed_output, self.start_time
        )
    }

    fn __str__(&self) -> String {
        let prompt = self
            .prompt
            .as_ref()
            .map_or("None".to_string(), |p| format!("\"{p}\""));
        let raw_output = self
            .raw_output
            .as_ref()
            .map_or("None".to_string(), |r| format!("\"{r}\""));
        let parsed_output = self
            .parsed_output
            .as_ref()
            .map_or("None".to_string(), |p| format!("\"{p}\""));

        format!(
            "BamlLogEvent {{\n    metadata: {{\n        event_id: \"{}\",\n        parent_id: {},\n        root_event_id: \"{}\"\n    }},\n    prompt: {},\n    raw_output: {},\n    parsed_output: {},\n    start_time: \"{}\"\n}}",
            self.metadata.event_id,
            self.metadata.parent_id.as_ref().map_or("None".to_string(), |id| format!("\"{}\"", id)),
            self.metadata.root_event_id,
            prompt,
            raw_output,
            parsed_output,
            self.start_time
        )
    }
}

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

    #[pyo3(signature = (function_name, args, ctx, tb, cb))]
    fn call_function(
        &self,
        py: Python<'_>,
        function_name: String,
        args: PyObject,
        ctx: &RuntimeContextManager,
        tb: Option<&TypeBuilder>,
        cb: Option<&ClientRegistry>,
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
        let cb = cb.map(|cb| cb.inner.clone());

        pyo3_asyncio::tokio::future_into_py(py, async move {
            let ctx_mng = ctx_mng;
            let result = baml_runtime
                .call_function(function_name, &args_map, &ctx_mng, tb.as_ref(), cb.as_ref())
                .await;

            result
                .0
                .map(FunctionResult::from)
                .map_err(BamlError::from_anyhow)
        })
        .map(|f| f.into())
    }

    #[pyo3(signature = (function_name, args, ctx, tb, cb))]
    fn call_function_sync(
        &self,
        function_name: String,
        args: PyObject,
        ctx: &RuntimeContextManager,
        tb: Option<&TypeBuilder>,
        cb: Option<&ClientRegistry>,
    ) -> PyResult<FunctionResult> {
        let Some(args) = parse_py_type(args, false)? else {
            return Err(BamlError::new_err(
                "Failed to parse args, perhaps you used a non-serializable type?",
            ));
        };
        let Some(args_map) = args.as_map_owned() else {
            return Err(BamlError::new_err("Failed to parse args"));
        };
        log::debug!("pyo3 call_function_sync parsed args into: {:#?}", args_map);

        let ctx_mng = ctx.inner.clone();
        let tb = tb.map(|tb| tb.inner.clone());
        let cb = cb.map(|cb| cb.inner.clone());

        let (result, _event_id) = self.inner.call_function_sync(
            function_name,
            &args_map,
            &ctx_mng,
            tb.as_ref(),
            cb.as_ref(),
        );

        result
            .map(FunctionResult::from)
            .map_err(BamlError::from_anyhow)
    }

    #[pyo3(signature = (function_name, args, on_event, ctx, tb, cb))]
    fn stream_function(
        &self,
        py: Python<'_>,
        function_name: String,
        args: PyObject,
        on_event: Option<PyObject>,
        ctx: &RuntimeContextManager,
        tb: Option<&TypeBuilder>,
        cb: Option<&ClientRegistry>,
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
                cb.map(|cb| cb.inner.clone()).as_ref(),
            )
            .map_err(BamlError::from_anyhow)?;

        Ok(FunctionResultStream::new(
            stream,
            on_event,
            tb.map(|tb| tb.inner.clone()),
            cb.map(|cb| cb.inner.clone()),
        ))
    }

    #[pyo3(signature = (function_name, args, on_event, ctx, tb, cb))]
    fn stream_function_sync(
        &self,
        py: Python<'_>,
        function_name: String,
        args: PyObject,
        on_event: Option<PyObject>,
        ctx: &RuntimeContextManager,
        tb: Option<&TypeBuilder>,
        cb: Option<&ClientRegistry>,
    ) -> PyResult<SyncFunctionResultStream> {
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
                cb.map(|cb| cb.inner.clone()).as_ref(),
            )
            .map_err(BamlError::from_anyhow)?;

        Ok(SyncFunctionResultStream::new(
            stream,
            on_event,
            tb.map(|tb| tb.inner.clone()),
            cb.map(|cb| cb.inner.clone()),
        ))
    }

    #[pyo3()]
    fn flush(&self) -> PyResult<()> {
        self.inner.flush().map_err(BamlError::from_anyhow)
    }

    #[pyo3()]
    fn drain_stats(&self) -> TraceStats {
        self.inner.drain_stats().into()
    }

    #[pyo3()]
    fn set_log_event_callback(&self, callback: PyObject) -> PyResult<()> {
        let callback = callback.clone();
        let baml_runtime = self.inner.clone();

        baml_runtime
            .as_ref()
            .set_log_event_callback(Box::new(move |log_event| {
                Python::with_gil(|py| {
                    match callback.call1(
                        py,
                        (BamlLogEvent {
                            metadata: LogEventMetadata {
                                event_id: log_event.metadata.event_id.clone(),
                                parent_id: log_event.metadata.parent_id.clone(),
                                root_event_id: log_event.metadata.root_event_id.clone(),
                            },
                            prompt: log_event.prompt.clone(),
                            raw_output: log_event.raw_output.clone(),
                            parsed_output: log_event.parsed_output.clone(),
                            start_time: log_event.start_time.clone(),
                        },),
                    ) {
                        Ok(_) => Ok(()),
                        Err(e) => {
                            log::error!("Error calling log_event_callback: {:?}", e);
                            Err(anyhow::Error::new(e).into()) // Proper error handling
                        }
                    }
                })
            }))
            .map_err(BamlError::from_anyhow)
    }
}
