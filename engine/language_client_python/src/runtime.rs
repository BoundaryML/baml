use std::{collections::HashMap, path::PathBuf, sync::Arc};

use baml_runtime::{runtime_interface::ExperimentalTracingInterface, TripWire};
use pyo3::{
    prelude::{pymethods, PyResult},
    pyclass,
    types::{PyAnyMethods, PyList},
    Bound, IntoPyObjectExt, PyObject, PyRef, Python,
};

#[cfg(feature = "interpreter")]
use pyo3::types::PyDict;
#[cfg(feature = "interpreter")]
use std::time::SystemTime;

// Type alias for pickle reduce return type
type PickleReduceResult = PyResult<(
    PyObject,
    (
        String,
        std::collections::HashMap<String, String>,
        std::collections::HashMap<String, String>,
    ),
)>;

// Conditional runtime selection based on the "interpreter" feature flag
#[cfg(feature = "interpreter")]
pub use baml_runtime::async_interpreter_runtime::BamlAsyncInterpreterRuntime as CoreBamlRuntime;
#[cfg(not(feature = "interpreter"))]
pub use baml_runtime::async_vm_runtime::BamlAsyncVmRuntime as CoreBamlRuntime;

use crate::{
    errors::{BamlError, BamlInvalidArgumentError},
    parse_py_type::parse_py_type,
    types::{
        function_result_stream::{FunctionResultStream, SyncFunctionResultStream},
        function_results::{pythonize_strict, FunctionResult},
        runtime_ctx_manager::RuntimeContextManager,
        trace_stats::TraceStats,
        type_builder::TypeBuilder,
        ClientRegistry, Collector, HTTPRequest,
    },
};

crate::lang_wrapper!(
    BamlRuntime,
    CoreBamlRuntime,
    clone_safe,
    root_path: String = String::new(),
    env_vars: HashMap<String, String> = HashMap::new(),
    files: HashMap<String, String> = HashMap::new()
);

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
            self.metadata.parent_id.as_ref().map_or("None".to_string(), |id| format!("\"{id}\"")),
            self.metadata.root_event_id,
            prompt,
            raw_output,
            parsed_output,
            self.start_time
        )
    }
}

// Helper struct to store event callbacks
#[cfg(feature = "interpreter")]
struct EmitCallbacks {
    var_handlers: HashMap<String, Vec<Arc<PyObject>>>,
    stream_handlers: HashMap<String, Vec<Arc<PyObject>>>,
    block_handlers: Vec<Arc<PyObject>>,
}

// Extract event handlers from the EventCollector.__handlers__() result
#[cfg(feature = "interpreter")]
fn extract_emit_callbacks(py: Python, events_obj: PyObject) -> PyResult<Option<EmitCallbacks>> {
    // Call __handlers() method to get InternalEventBindings
    let handlers_result = events_obj.call_method0(py, "__handlers__")?;
    let bindings = handlers_result.downcast_bound::<PyDict>(py)?;

    // Extract block handlers
    let block_handlers = if let Ok(block_list) = bindings.get_item("block") {
        if let Some(block_bound) = block_list {
            if let Ok(block_list) = block_bound.downcast::<PyList>() {
                block_list
                    .iter()
                    .map(|handler| Arc::new(handler.into_py_any(py).unwrap()))
                    .collect::<Vec<_>>()
            } else {
                Vec::new()
            }
        } else {
            Vec::new()
        }
    } else {
        Vec::new()
    };

    // Extract var handlers
    let mut var_handlers = HashMap::new();
    if let Ok(Some(vars_dict)) = bindings.get_item("vars") {
        if let Ok(vars_dict) = vars_dict.downcast::<PyDict>() {
            for (key, value) in vars_dict.iter() {
                if let Ok(key_str) = key.extract::<String>() {
                    if let Ok(handler_list) = value.downcast::<PyList>() {
                        let handlers: Vec<Arc<PyObject>> = handler_list
                            .iter()
                            .filter_map(|h| h.into_py_any(py).ok().map(Arc::new))
                            .collect();
                        if !handlers.is_empty() {
                            var_handlers.insert(key_str, handlers);
                        }
                    }
                }
            }
        }
    }

    // Extract stream handlers
    let mut stream_handlers = HashMap::new();
    if let Ok(Some(streams_dict)) = bindings.get_item("streams") {
        if let Ok(streams_dict) = streams_dict.downcast::<PyDict>() {
            for (key, value) in streams_dict.iter() {
                if let Ok(key_str) = key.extract::<String>() {
                    if let Ok(handler_list) = value.downcast::<PyList>() {
                        let handlers: Vec<Arc<PyObject>> = handler_list
                            .iter()
                            .filter_map(|h| h.into_py_any(py).ok().map(Arc::new))
                            .collect();
                        if !handlers.is_empty() {
                            stream_handlers.insert(key_str, handlers);
                        }
                    }
                }
            }
        }
    }

    Ok(Some(EmitCallbacks {
        var_handlers,
        stream_handlers,
        block_handlers,
    }))
}

#[pymethods]
impl BamlRuntime {
    // Called by pickle to serialize the object using __reduce__ protocol
    fn __reduce__(&self, py: Python) -> PickleReduceResult {
        let cls = py.get_type::<Self>();
        let args = (
            self.root_path.clone(),
            self.env_vars.clone(),
            self.files.clone(),
        );
        Ok((cls.getattr("_create_from_state")?.into(), args))
    }

    fn disassemble(&self, function_name: String) {
        self.inner.disassemble(&function_name);
    }

    /// Static method to recreate BamlRuntime from pickle state
    #[staticmethod]
    fn _create_from_state(
        root_path: String,
        env_vars: std::collections::HashMap<String, String>,
        files: std::collections::HashMap<String, String>,
    ) -> PyResult<Self> {
        let core = CoreBamlRuntime::from_file_content(&root_path, &files, env_vars.clone())
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(format!("{e}")))?;
        Ok(BamlRuntime {
            inner: std::sync::Arc::new(core),
            root_path,
            env_vars,
            files,
        })
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

    #[pyo3(signature = (function_name, args, ctx, tb, cb, collectors, env_vars, tags, abort_controller=None, events=None))]
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
        tags: Option<HashMap<String, String>>,
        abort_controller: Option<&crate::abort_controller::AbortController>,
        events: Option<PyObject>,
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
        log::debug!("pyo3 call_function parsed args into: {args_map:#?}");

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

        let tripwire = abort_controller
            .map(|ac| ac.create_tripwire())
            .unwrap_or_else(|| TripWire::new(None));

        // Extract emit callbacks from EventCollector (only for interpreter)
        #[cfg(feature = "interpreter")]
        let emit_callbacks = if let Some(events_obj) = events {
            extract_emit_callbacks(py, events_obj)?
        } else {
            None
        };

        #[cfg(feature = "interpreter")]
        let emit_handler = move |event: baml_compiler::emit::EmitEvent| {
            if let Some(ref callbacks) = emit_callbacks {
                Python::with_gil(|py| {
                    match event.value {
                        baml_compiler::emit::EmitBamlValue::Block(block_label) => {
                            // Fire block events to all registered block handlers
                            for handler in &callbacks.block_handlers {
                                let block_event_dict = PyDict::new(py);
                                let _ =
                                    block_event_dict.set_item("block_label", block_label.clone());
                                let _ = block_event_dict.set_item("event_type", "enter");
                                let _ = handler.call1(py, (block_event_dict,));
                            }
                        }
                        baml_compiler::emit::EmitBamlValue::Value(value) => {
                            if let Some(var_name) = &event.variable_name {
                                // Serialize BamlValue to JSON
                                let serialized = serde_json::to_value(value.value())
                                    .unwrap_or(serde_json::Value::Null);

                                let var_event_dict = PyDict::new(py);
                                let _ = var_event_dict.set_item("variable_name", var_name.clone());
                                let _ = var_event_dict.set_item("value", serialized.to_string());
                                let _ = var_event_dict.set_item(
                                    "timestamp",
                                    SystemTime::now()
                                        .duration_since(SystemTime::UNIX_EPOCH)
                                        .unwrap()
                                        .as_millis()
                                        .to_string(),
                                );
                                let _ = var_event_dict
                                    .set_item("function_name", event.function_name.clone());

                                // Fire to appropriate handler based on stream vs var
                                let handlers = if event.is_stream {
                                    &callbacks.stream_handlers
                                } else {
                                    &callbacks.var_handlers
                                };

                                if let Some(handler_list) = handlers.get(var_name) {
                                    for handler in handler_list {
                                        let _ = handler.call1(py, (var_event_dict.clone(),));
                                    }
                                }
                            }
                        }
                    }
                });
            }
        };

        #[cfg(feature = "interpreter")]
        let result_future = baml_runtime.call_function(
            function_name,
            &args_map,
            &ctx_mng,
            tb.as_ref(),
            cb.as_ref(),
            Some(collector_list),
            env_vars,
            tags,
            tripwire,
            Some(emit_handler),
        );

        #[cfg(not(feature = "interpreter"))]
        {
            let _ = events; // Suppress unused variable warning
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
                        tags,
                        tripwire,
                        None::<fn(baml_compiler::emit::EmitEvent)>,
                    )
                    .await;
                result
                    .map(FunctionResult::from)
                    .map_err(BamlError::from_anyhow)
            })
            .map(pyo3::Bound::into)
        }

        #[cfg(feature = "interpreter")]
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
                    tags,
                    tripwire,
                    Some(watch_handler),
                )
                .await;

            result
                .map(FunctionResult::from)
                .map_err(BamlError::from_anyhow)
        })
        .map(pyo3::Bound::into)
    }

    #[pyo3(signature = (function_name, args, ctx, tb, cb, collectors, env_vars, tags, abort_controller=None, events=None))]
    fn call_function_sync(
        &self,
        py: Python<'_>,
        function_name: String,
        args: PyObject,
        ctx: &RuntimeContextManager,
        tb: Option<&TypeBuilder>,
        cb: Option<&ClientRegistry>,
        collectors: &Bound<'_, PyList>,
        env_vars: HashMap<String, String>,
        tags: Option<HashMap<String, String>>,
        abort_controller: Option<&crate::abort_controller::AbortController>,
        #[allow(unused_variables)] events: Option<PyObject>,
    ) -> PyResult<FunctionResult> {
        let Some(args) = parse_py_type(args, false)? else {
            return Err(BamlInvalidArgumentError::new_err(
                "Failed to parse args, perhaps you used a non-serializable type?",
            ));
        };
        let Some(args_map) = args.as_map_owned() else {
            return Err(BamlInvalidArgumentError::new_err(
                "Failed to parse args as a map",
            ));
        };
        log::debug!("pyo3 call_function_sync parsed args into: {args_map:#?}");

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

        // Check if already aborted
        let tripwire = abort_controller
            .map(|ac| ac.create_tripwire())
            .unwrap_or_else(|| TripWire::new(None));

        // Extract emit callbacks from EventCollector (only for interpreter)
        #[cfg(feature = "interpreter")]
        let emit_callbacks = if let Some(events_obj) = events {
            extract_emit_callbacks(py, events_obj)?
        } else {
            None
        };

        #[cfg(feature = "interpreter")]
        let emit_handler = move |event: baml_compiler::emit::EmitEvent| {
            if let Some(ref callbacks) = emit_callbacks {
                Python::with_gil(|py| {
                    match event.value {
                        baml_compiler::emit::EmitBamlValue::Block(block_label) => {
                            // Fire block events to all registered block handlers
                            for handler in &callbacks.block_handlers {
                                let block_event_dict = PyDict::new(py);
                                let _ =
                                    block_event_dict.set_item("block_label", block_label.clone());
                                let _ = block_event_dict.set_item("event_type", "enter");
                                let _ = handler.call1(py, (block_event_dict,));
                            }
                        }
                        baml_compiler::emit::EmitBamlValue::Value(value) => {
                            if let Some(var_name) = &event.variable_name {
                                // Serialize BamlValue to JSON
                                let serialized = serde_json::to_value(value.value())
                                    .unwrap_or(serde_json::Value::Null);

                                let var_event_dict = PyDict::new(py);
                                let _ = var_event_dict.set_item("variable_name", var_name.clone());
                                let _ = var_event_dict.set_item("value", serialized.to_string());
                                let _ = var_event_dict.set_item(
                                    "timestamp",
                                    SystemTime::now()
                                        .duration_since(SystemTime::UNIX_EPOCH)
                                        .unwrap()
                                        .as_millis()
                                        .to_string(),
                                );
                                let _ = var_event_dict
                                    .set_item("function_name", event.function_name.clone());

                                // Fire to appropriate handler based on stream vs var
                                let handlers = if event.is_stream {
                                    &callbacks.stream_handlers
                                } else {
                                    &callbacks.var_handlers
                                };

                                if let Some(handler_list) = handlers.get(var_name) {
                                    for handler in handler_list {
                                        let _ = handler.call1(py, (var_event_dict.clone(),));
                                    }
                                }
                            }
                        }
                    }
                });
            }
        };

        #[cfg(feature = "interpreter")]
        let (result, _event_id) = py.allow_threads(|| {
            self.inner.call_function_sync(
                function_name,
                &args_map,
                &ctx_mng,
                tb.as_ref(),
                cb.as_ref(),
                Some(collector_list),
                env_vars,
                tags,
                tripwire,
                Some(emit_handler),
                Some(watch_handler), // TODO: Notification handler.
            )
        });

        #[cfg(not(feature = "interpreter"))]
        let (result, _event_id) = py.allow_threads(|| {
            self.inner.call_function_sync(
                function_name,
                &args_map,
                &ctx_mng,
                tb.as_ref(),
                cb.as_ref(),
                Some(collector_list),
                env_vars,
                tags,
                tripwire,
                None::<fn(baml_compiler::emit::EmitEvent)>,
            )
        });

        result
            .map(FunctionResult::from)
            .map_err(BamlError::from_anyhow)
    }

    #[pyo3(signature = (function_name, args, on_event, ctx, tb, cb, collectors, env_vars, tags=None, on_tick=None, abort_controller=None))]
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
        tags: Option<HashMap<String, String>>,
        on_tick: Option<PyObject>,
        abort_controller: Option<&crate::abort_controller::AbortController>,
    ) -> PyResult<FunctionResultStream> {
        let Some(args) = parse_py_type(args.into_bound(py).into_py_any(py)?, false)? else {
            return Err(BamlInvalidArgumentError::new_err(
                "Failed to parse args, perhaps you used a non-serializable type?",
            ));
        };
        let Some(args_map) = args.as_map() else {
            return Err(BamlInvalidArgumentError::new_err("Failed to parse args"));
        };
        log::debug!("pyo3 stream_function parsed args into: {args_map:#?}");

        let ctx = ctx.inner.clone();
        let collector_list = collectors
            .into_iter()
            .map(|c| {
                let collector: PyRef<Collector> = c.extract().expect("Failed to extract collector");
                collector.inner.clone()
            })
            .collect::<Vec<_>>();
        let tripwire = abort_controller
            .map(|ac| ac.create_tripwire())
            .unwrap_or_else(|| TripWire::new(None));
        let stream = self
            .inner
            .stream_function(
                function_name,
                args_map,
                &ctx,
                tb.map(|tb| tb.inner.clone()).as_ref(),
                cb.map(|cb| cb.inner.clone()).as_ref(),
                Some(collector_list),
                env_vars.clone(),
                tags,
                tripwire,
            )
            .map_err(BamlError::from_anyhow)?;

        Ok(FunctionResultStream::new(
            stream,
            on_event,
            tb.map(|tb| tb.inner.clone()),
            cb.map(|cb| cb.inner.clone()),
            env_vars,
            on_tick,
        ))
    }

    #[pyo3(signature = (function_name, args, on_event, ctx, tb, cb, collectors, env_vars, tags=None, on_tick=None, abort_controller=None))]
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
        tags: Option<HashMap<String, String>>,
        on_tick: Option<PyObject>,
        abort_controller: Option<&crate::abort_controller::AbortController>,
    ) -> PyResult<SyncFunctionResultStream> {
        let Some(args) = parse_py_type(args.into_bound(py).into_py_any(py)?, false)? else {
            return Err(BamlInvalidArgumentError::new_err(
                "Failed to parse args, perhaps you used a non-serializable type?",
            ));
        };
        let Some(args_map) = args.as_map() else {
            return Err(BamlInvalidArgumentError::new_err("Failed to parse args"));
        };
        log::debug!("pyo3 stream_function parsed args into: {args_map:#?}");

        let ctx = ctx.inner.clone();
        let collector_list = collectors
            .into_iter()
            .map(|c| {
                let collector: PyRef<Collector> = c.extract().expect("Failed to extract collector");
                collector.inner.clone()
            })
            .collect::<Vec<_>>();
        let tripwire = abort_controller
            .map(|ac| ac.create_tripwire())
            .unwrap_or_else(|| TripWire::new(None));
        let stream = self
            .inner
            .stream_function(
                function_name,
                args_map,
                &ctx,
                tb.map(|tb| tb.inner.clone()).as_ref(),
                cb.map(|cb| cb.inner.clone()).as_ref(),
                Some(collector_list),
                env_vars.clone(),
                tags,
                tripwire,
            )
            .map_err(BamlError::from_anyhow)?;

        Ok(SyncFunctionResultStream::new(
            stream,
            on_event,
            tb.map(|tb| tb.inner.clone()),
            cb.map(|cb| cb.inner.clone()),
            env_vars,
            on_tick,
        ))
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

        let baml_runtime = self.inner.clone();
        let ctx_manager = ctx.inner.clone();
        let type_builder = tb.map(|tb| tb.inner.clone());
        let client_registry = cb.map(|cb| cb.inner.clone());

        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            baml_runtime
                .build_request(
                    function_name,
                    &args_map,
                    &ctx_manager,
                    type_builder.as_ref(),
                    client_registry.as_ref(),
                    env_vars,
                    stream,
                )
                .await
                .map(HTTPRequest::from)
                .map_err(BamlError::from_anyhow)
        })
        .map(pyo3::Bound::into)
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
        let Some(args) = parse_py_type(args, false)? else {
            return Err(BamlInvalidArgumentError::new_err(
                "Failed to parse args, perhaps you used a non-serializable type?",
            ));
        };
        let Some(args_map) = args.as_map_owned() else {
            return Err(BamlInvalidArgumentError::new_err(
                "Failed to parse args as a map",
            ));
        };

        let context_manager = ctx.inner.clone();
        let type_builder = tb.map(|tb| tb.inner.clone());
        let client_registry = cb.map(|cb| cb.inner.clone());

        // TODO: Figure out if this will be async or not (images, media, etc).
        // If it's not async then skip gil and threads.
        let result = py.allow_threads(|| {
            self.inner.build_request_sync(
                function_name,
                &args_map,
                &context_manager,
                type_builder.as_ref(),
                client_registry.as_ref(),
                stream,
                env_vars,
            )
        });

        result
            .map(HTTPRequest::from)
            .map_err(BamlError::from_anyhow)
    }

    #[pyo3(signature = (function_name, llm_response, enum_module, cls_module, partial_cls_module, allow_partials, ctx, tb, cb, env_vars ))]
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

        // Having no intermediary object wrappers allows us to avoid clonning
        // the parsed value (unlike FunctionResult::cast_to). We pass that
        // straight into pythonize_strict and return the final python object.
        // Downside is we require a lot of parameters for this function, but
        // this is only called in codegen, not part of the public API.
        let parsed = self
            .inner
            .parse_llm_response(
                function_name,
                llm_response,
                allow_partials,
                &ctx_mng,
                tb.as_ref(),
                cb.as_ref(),
                env_vars,
            )
            .map_err(BamlError::from_anyhow)?;

        pythonize_strict(
            py,
            parsed,
            &enum_module,
            &cls_module,
            &partial_cls_module,
            allow_partials,
            self,
        )
    }

    #[pyo3()]
    fn flush(&self) -> PyResult<()> {
        self.inner.flush().map_err(BamlError::from_anyhow)
    }

    #[pyo3()]
    fn drain_stats(&self) -> TraceStats {
        self.inner.drain_stats().into()
    }

    #[pyo3(signature = (callback = None))]
    fn set_log_event_callback(&self, callback: Option<PyObject>, py: Python<'_>) -> PyResult<()> {
        let baml_runtime = self.inner.clone();

        if let Some(callback) = callback {
            let arc_callback = Arc::new(callback.into_py_any(py)?);
            baml_runtime
                .as_ref()
                .set_log_event_callback(Some(Box::new(move |log_event| {
                    Python::with_gil(|py| {
                        match arc_callback.call1(
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
                                log::error!("Error calling log_event_callback: {e:?}");
                                Err(anyhow::Error::new(e)) // Proper error handling
                            }
                        }
                    })
                })))
                .map_err(BamlError::from_anyhow)
        } else {
            baml_runtime
                .as_ref()
                .set_log_event_callback(None)
                .map_err(BamlError::from_anyhow)
        }
    }
}
