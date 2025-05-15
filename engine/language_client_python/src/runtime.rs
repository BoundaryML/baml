use crate::errors::{BamlError, BamlInvalidArgumentError};
use crate::parse_py_type::parse_py_type;
use crate::types::function_result_stream::{FunctionResultStream, SyncFunctionResultStream};
use crate::types::function_results::{pythonize_strict, FunctionResult};
use crate::types::runtime_ctx_manager::RuntimeContextManager;
use crate::types::trace_stats::TraceStats;
use crate::types::type_builder::TypeBuilder;
use crate::types::{ClientRegistry, Collector, HTTPRequest};
use baml_runtime::runtime_interface::ExperimentalTracingInterface;
use baml_runtime::BamlRuntime as CoreBamlRuntime;
use pyo3::prelude::{pymethods, PyResult};
use pyo3::types::{PyAnyMethods, PyList};
use pyo3::{pyclass, Bound, IntoPyObjectExt, PyObject, PyRef, Python};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::Mutex;

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

#[pyclass]
pub struct BamlRuntime {
    runtime: Arc<Mutex<baml_runtime::BamlRuntime>>,
    env_vars: HashMap<String, String>,
}

#[pymethods]
impl BamlRuntime {
    #[new]
    pub fn new() -> PyResult<Self> {
        Ok(CoreBamlRuntime::new().into())
    }

    pub fn call_function_sync(
        &self,
        function_name: String,
        args: HashMap<String, serde_json::Value>,
    ) -> PyResult<serde_json::Value> {
        let runtime = self.runtime.lock().unwrap();
        Ok(runtime
            .call_function_sync(&function_name, args, self.env_vars.clone())
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(e.to_string()))?)
    }

    pub fn stream_function_sync(
        &self,
        function_name: String,
        args: HashMap<String, serde_json::Value>,
    ) -> PyResult<Vec<serde_json::Value>> {
        let runtime = self.runtime.lock().unwrap();
        Ok(runtime
            .stream_function_sync(&function_name, args, self.env_vars.clone())
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(e.to_string()))?)
    }

    pub fn build_request_sync(
        &self,
        function_name: String,
        args: HashMap<String, serde_json::Value>,
    ) -> PyResult<serde_json::Value> {
        let runtime = self.runtime.lock().unwrap();
        Ok(runtime
            .build_request_sync(&function_name, args, self.env_vars.clone())
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(e.to_string()))?)
    }

    pub fn parse_llm_response(
        &self,
        types: Vec<String>,
        partial_types: Vec<String>,
        llm_response: String,
    ) -> PyResult<serde_json::Value> {
        let runtime = self.runtime.lock().unwrap();
        Ok(runtime
            .parse_llm_response(types, partial_types, llm_response, self.env_vars.clone())
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(e.to_string()))?)
    }

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
    fn reset(
        &mut self,
        root_path: String,
        files: HashMap<String, String>,
        env_vars: HashMap<String, String>,
    ) -> PyResult<()> {
        self.inner = CoreBamlRuntime::from_file_content(&root_path, &files, env_vars)
            .map_err(BamlError::from_anyhow)?
            .into();
        Ok(())
    }

    #[pyo3()]
    fn create_context_manager(&self) -> RuntimeContextManager {
        self.inner
            .create_ctx_manager(baml_types::BamlValue::String("python".to_string()), None)
            .into()
    }

    #[pyo3(signature = (function_name, args, ctx, tb, cb, collectors, env_vars))]
    fn call_function(
        &self,
        py: Python<'_>,
        function_name: String,
        args: PyObject,
        ctx: &RuntimeContextManager,
        tb: Option<&TypeBuilder>,
        cb: Option<&ClientRegistry>,
        collectors: &Bound<'_, PyList>,
        env_vars: HashMap<String, String>,
    ) -> PyResult<PyObject> {
        let Some(args) = parse_py_type(args.into_bound(py).into_py_any(py)?, false)? else {
            return Err(BamlInvalidArgumentError::new_err(
                "Failed to parse args, perhaps you used a non-serializable type?",
            ));
        };
        let Some(args_map) = args.as_map_owned() else {
            return Err(BamlInvalidArgumentError::new_err(
                "Failed to parse args. Expect kwargs",
            ));
        };
        log::debug!("pyo3 call_function parsed args into: {:#?}", args_map);

        let baml_runtime = self.inner.clone();
        let ctx_mng = ctx.inner.clone();
        let tb = tb.map(|tb| tb.inner.clone());
        let cb = cb.map(|cb| cb.inner.clone());

        let collector_list = collectors
            .into_iter()
            .map(|c| {
                let collector: PyRef<Collector> = c.extract().expect("Failed to extract collector");
                collector.inner.clone()
            })
            .collect::<Vec<_>>();

        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            let (result, _) = baml_runtime
                .call_function(
                    function_name,
                    &args_map,
                    &ctx_mng,
                    tb.as_ref(),
                    cb.as_ref(),
                    Some(collector_list),
                    env_vars,
                )
                .await;

            result
                .map(FunctionResult::from)
                .map_err(BamlError::from_anyhow)
        })
        .map(pyo3::Bound::into)
    }

    #[pyo3(signature = (function_name, args, ctx, tb, cb, collectors, env_vars))]
    fn call_function_sync(
        &self,
        function_name: String,
        args: PyObject,
        ctx: &RuntimeContextManager,
        tb: Option<&TypeBuilder>,
        cb: Option<&ClientRegistry>,
        collectors: &Bound<'_, PyList>,
        env_vars: HashMap<String, String>,
    ) -> PyResult<FunctionResult> {
        let Some(args) = parse_py_type(args, false)? else {
            return Err(BamlInvalidArgumentError::new_err(
                "Failed to parse args, perhaps you used a non-serializable type?",
            ));
        };
        let Some(args_map) = args.as_map_owned() else {
            return Err(BamlInvalidArgumentError::new_err(
                "Failed to parse args. Expect kwargs",
            ));
        };

        let ctx_mng = ctx.inner.clone();
        let tb = tb.map(|tb| tb.inner.clone());
        let cb = cb.map(|cb| cb.inner.clone());

        let collector_list = collectors
            .into_iter()
            .map(|c| {
                let collector: PyRef<Collector> = c.extract().expect("Failed to extract collector");
                collector.inner.clone()
            })
            .collect::<Vec<_>>();

        let (result, _) = self.inner.call_function_sync(
            function_name,
            &args_map,
            &ctx_mng,
            tb.as_ref(),
            cb.as_ref(),
            Some(collector_list),
            env_vars,
        );

        result
            .map(FunctionResult::from)
            .map_err(BamlError::from_anyhow)
    }

    #[pyo3(signature = (function_name, args, on_event, ctx, tb, cb, collectors, env_vars))]
    fn stream_function(
        &self,
        py: Python<'_>,
        function_name: String,
        args: PyObject,
        on_event: Option<PyObject>,
        ctx: &RuntimeContextManager,
        tb: Option<&TypeBuilder>,
        cb: Option<&ClientRegistry>,
        collectors: &Bound<'_, PyList>,
        env_vars: HashMap<String, String>,
    ) -> PyResult<FunctionResultStream> {
        let Some(args) = parse_py_type(args.into_bound(py).into_py_any(py)?, false)? else {
            return Err(BamlInvalidArgumentError::new_err(
                "Failed to parse args, perhaps you used a non-serializable type?",
            ));
        };
        let Some(args_map) = args.as_map_owned() else {
            return Err(BamlInvalidArgumentError::new_err(
                "Failed to parse args. Expect kwargs",
            ));
        };

        let baml_runtime = self.inner.clone();
        let ctx_mng = ctx.inner.clone();
        let tb = tb.map(|tb| tb.inner.clone());
        let cb = cb.map(|cb| cb.inner.clone());

        let collector_list = collectors
            .into_iter()
            .map(|c| {
                let collector: PyRef<Collector> = c.extract().expect("Failed to extract collector");
                collector.inner.clone()
            })
            .collect::<Vec<_>>();

        let stream = baml_runtime.stream_function(
            function_name,
            &args_map,
            &ctx_mng,
            tb.as_ref(),
            cb.as_ref(),
            Some(collector_list),
            env_vars,
        );

        Ok(FunctionResultStream::new(stream, on_event, tb, cb))
    }

    #[pyo3(signature = (function_name, args, on_event, ctx, tb, cb, collectors, env_vars))]
    fn stream_function_sync(
        &self,
        py: Python<'_>,
        function_name: String,
        args: PyObject,
        on_event: Option<PyObject>,
        ctx: &RuntimeContextManager,
        tb: Option<&TypeBuilder>,
        cb: Option<&ClientRegistry>,
        collectors: &Bound<'_, PyList>,
        env_vars: HashMap<String, String>,
    ) -> PyResult<SyncFunctionResultStream> {
        let Some(args) = parse_py_type(args.into_bound(py).into_py_any(py)?, false)? else {
            return Err(BamlInvalidArgumentError::new_err(
                "Failed to parse args, perhaps you used a non-serializable type?",
            ));
        };
        let Some(args_map) = args.as_map_owned() else {
            return Err(BamlInvalidArgumentError::new_err(
                "Failed to parse args. Expect kwargs",
            ));
        };

        let ctx_mng = ctx.inner.clone();
        let tb = tb.map(|tb| tb.inner.clone());
        let cb = cb.map(|cb| cb.inner.clone());

        let collector_list = collectors
            .into_iter()
            .map(|c| {
                let collector: PyRef<Collector> = c.extract().expect("Failed to extract collector");
                collector.inner.clone()
            })
            .collect::<Vec<_>>();

        let stream = self.inner.stream_function_sync(
            function_name,
            &args_map,
            &ctx_mng,
            tb.as_ref(),
            cb.as_ref(),
            Some(collector_list),
            env_vars,
        );

        Ok(SyncFunctionResultStream::new(stream, on_event, tb, cb))
    }

    #[pyo3(signature = (function_name, args, ctx, tb, cb, env_vars, stream))]
    fn build_request(
        &self,
        py: Python<'_>,
        function_name: String,
        args: PyObject,
        ctx: &RuntimeContextManager,
        tb: Option<&TypeBuilder>,
        cb: Option<&ClientRegistry>,
        env_vars: HashMap<String, String>,
        stream: bool,
    ) -> PyResult<PyObject> {
        let Some(args) = parse_py_type(args.into_bound(py).into_py_any(py)?, false)? else {
            return Err(BamlInvalidArgumentError::new_err(
                "Failed to parse args, perhaps you used a non-serializable type?",
            ));
        };
        let Some(args_map) = args.as_map_owned() else {
            return Err(BamlInvalidArgumentError::new_err(
                "Failed to parse args. Expect kwargs",
            ));
        };

        let ctx_mng = ctx.inner.clone();
        let tb = tb.map(|tb| tb.inner.clone());
        let cb = cb.map(|cb| cb.inner.clone());

        let request = self.inner.build_request(
            function_name,
            &args_map,
            &ctx_mng,
            tb.as_ref(),
            cb.as_ref(),
            env_vars,
            stream,
        );

        request
            .map(|r| r.into_py(py))
            .map_err(BamlError::from_anyhow)
    }

    #[pyo3(signature = (function_name, args, ctx, tb, cb, env_vars, stream))]
    fn build_request_sync(
        &self,
        py: Python<'_>,
        function_name: String,
        args: PyObject,
        ctx: &RuntimeContextManager,
        tb: Option<&TypeBuilder>,
        cb: Option<&ClientRegistry>,
        env_vars: HashMap<String, String>,
        stream: bool,
    ) -> PyResult<HTTPRequest> {
        let Some(args) = parse_py_type(args.into_bound(py).into_py_any(py)?, false)? else {
            return Err(BamlInvalidArgumentError::new_err(
                "Failed to parse args, perhaps you used a non-serializable type?",
            ));
        };
        let Some(args_map) = args.as_map_owned() else {
            return Err(BamlInvalidArgumentError::new_err(
                "Failed to parse args. Expect kwargs",
            ));
        };

        let ctx_mng = ctx.inner.clone();
        let tb = tb.map(|tb| tb.inner.clone());
        let cb = cb.map(|cb| cb.inner.clone());

        let request = self.inner.build_request_sync(
            function_name,
            &args_map,
            &ctx_mng,
            tb.as_ref(),
            cb.as_ref(),
            env_vars,
            stream,
        );

        request
            .map(HTTPRequest::from)
            .map_err(BamlError::from_anyhow)
    }

    #[pyo3(signature = (function_name, llm_response, enum_module, cls_module, partial_cls_module, allow_partials, ctx, tb, cb, env_vars))]
    fn parse_llm_response(
        &self,
        py: Python<'_>,
        function_name: String,
        llm_response: String,
        enum_module: pyo3::Bound<'_, pyo3::types::PyModule>,
        cls_module: pyo3::Bound<'_, pyo3::types::PyModule>,
        partial_cls_module: pyo3::Bound<'_, pyo3::types::PyModule>,
        allow_partials: bool,
        ctx: &RuntimeContextManager,
        tb: Option<&TypeBuilder>,
        cb: Option<&ClientRegistry>,
        env_vars: HashMap<String, String>,
    ) -> PyResult<PyObject> {
        let ctx_mng = ctx.inner.clone();
        let tb = tb.map(|tb| tb.inner.clone());
        let cb = cb.map(|cb| cb.inner.clone());

        let result = self.inner.parse_llm_response(
            function_name,
            llm_response,
            enum_module,
            cls_module,
            partial_cls_module,
            allow_partials,
            &ctx_mng,
            tb.as_ref(),
            cb.as_ref(),
            env_vars,
        );

        result
            .map(|r| r.into_py(py))
            .map_err(BamlError::from_anyhow)
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
    fn set_log_event_callback(&self, callback: Option<PyObject>, py: Python<'_>) -> PyResult<()> {
        self.inner
            .set_log_event_callback(callback.map(|cb| cb.into_bound(py)))
            .map_err(BamlError::from_anyhow)
    }
}
