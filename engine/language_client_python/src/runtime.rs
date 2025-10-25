use std::{collections::HashMap, path::PathBuf, sync::Arc, time::SystemTime};

use baml_compiler::watch::shared_handler;
use baml_runtime::{runtime_interface::ExperimentalTracingInterface, TripWire};
use pyo3::{
    prelude::{pymethods, PyResult},
    pyclass,
    types::{PyAnyMethods, PyDict, PyList},
    Bound, IntoPyObjectExt, PyObject, PyRef, Python,
};

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

struct NotificationCallbacks {
    var_handlers: HashMap<String, Vec<Arc<PyObject>>>,
    stream_handlers: HashMap<String, Vec<Arc<PyObject>>>,
    block_handlers: Vec<Arc<PyObject>>,
}

// Helper function to recursively extract handlers from a bindings object

fn extract_handlers_recursive(
    py: Python,
    bindings: &Bound<'_, pyo3::PyAny>,
    function_prefix: &str,
    var_handlers: &mut HashMap<String, Vec<Arc<PyObject>>>,
    stream_handlers: &mut HashMap<String, Vec<Arc<PyObject>>>,
    block_handlers: &mut Vec<Arc<PyObject>>,
) -> PyResult<()> {
    // Get the function name from this bindings object
    let current_function_name = if let Ok(fn_name) = bindings.getattr("function_name") {
        fn_name.extract::<String>()?
    } else {
        function_prefix.to_string()
    };

    // Extract block handlers from this level
    if let Ok(block_bound) = bindings.getattr("block") {
        if let Ok(block_list) = block_bound.downcast::<PyList>() {
            for handler in block_list {
                if let Ok(h) = handler.into_py_any(py) {
                    block_handlers.push(Arc::new(h));
                }
            }
        }
    }

    // Extract var handlers from this level
    if let Ok(vars_bound) = bindings.getattr("vars") {
        if let Ok(vars_dict) = vars_bound.downcast::<PyDict>() {
            for (key, value) in vars_dict {
                if let Ok(var_name) = key.extract::<String>() {
                    if let Ok(handler_list) = value.downcast::<PyList>() {
                        let handlers: Vec<Arc<PyObject>> = handler_list
                            .into_iter()
                            .filter_map(|h| h.into_py_any(py).ok().map(Arc::new))
                            .collect();
                        if !handlers.is_empty() {
                            // Key by "FunctionName.variable_name"
                            let key = format!("{current_function_name}.{var_name}");
                            var_handlers.insert(key, handlers);
                        }
                    }
                }
            }
        }
    }

    // Extract stream handlers from this level
    if let Ok(streams_bound) = bindings.getattr("streams") {
        if let Ok(streams_dict) = streams_bound.downcast::<PyDict>() {
            for (key, value) in streams_dict {
                if let Ok(var_name) = key.extract::<String>() {
                    if let Ok(handler_list) = value.downcast::<PyList>() {
                        let handlers: Vec<Arc<PyObject>> = handler_list
                            .into_iter()
                            .filter_map(|h| h.into_py_any(py).ok().map(Arc::new))
                            .collect();
                        if !handlers.is_empty() {
                            // Key by "FunctionName.variable_name"
                            let key = format!("{current_function_name}.{var_name}");
                            stream_handlers.insert(key, handlers);
                        }
                    }
                }
            }
        }
    }

    // Recursively extract from nested functions
    if let Ok(functions_bound) = bindings.getattr("functions") {
        if let Ok(functions_dict) = functions_bound.downcast::<PyDict>() {
            for (key, value) in functions_dict {
                if let Ok(_child_fn_name) = key.extract::<String>() {
                    // Recursively extract from child function's bindings
                    extract_handlers_recursive(
                        py,
                        &value,
                        &current_function_name,
                        var_handlers,
                        stream_handlers,
                        block_handlers,
                    )?;
                }
            }
        }
    }

    Ok(())
}

// Extract event handlers from the EventCollector.__handlers__() result

fn extract_notification_callbacks(
    py: Python,
    events_obj: PyObject,
) -> PyResult<Option<NotificationCallbacks>> {
    // Call __handlers() method to get InternalEventBindings
    let handlers_result = events_obj.call_method0(py, "__handlers__")?;
    let bindings = handlers_result.bind(py);

    let mut var_handlers = HashMap::new();
    let mut stream_handlers = HashMap::new();
    let mut block_handlers = Vec::new();

    // Recursively extract all handlers including nested functions
    extract_handlers_recursive(
        py,
        bindings,
        "",
        &mut var_handlers,
        &mut stream_handlers,
        &mut block_handlers,
    )?;

    Ok(Some(NotificationCallbacks {
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

    #[pyo3(signature = (function_name, args, ctx, tb, cb, collectors, env_vars, tags, abort_controller=None, watchers=None))]
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
        watchers: Option<PyObject>,
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

        // Extract notification callbacks from EventCollector (only for interpreter)

        let notification_callbacks = if let Some(watchers_obj) = watchers {
            extract_notification_callbacks(py, watchers_obj)?
        } else {
            None
        };

        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            let watch_handler = shared_handler(move |notification| {
                if let Some(ref callbacks) = notification_callbacks {
                    Python::with_gil(|py| {
                        match notification.value {
                            baml_compiler::watch::WatchBamlValue::Block(block_label) => {
                                // Fire block events to all registered block handlers
                                for handler in &callbacks.block_handlers {
                                    let block_event_dict = PyDict::new(py);
                                    let _ = block_event_dict
                                        .set_item("block_label", block_label.clone());
                                    let _ = block_event_dict.set_item("event_type", "enter");
                                    let _ = handler.call1(py, (block_event_dict,));
                                }
                            }
                            baml_compiler::watch::WatchBamlValue::Value(value) => {
                                if let Some(var_name) = &notification.variable_name {
                                    // Serialize BamlValue to JSON and convert to Python object
                                    let serialized = serde_json::to_value(value.value())
                                        .unwrap_or(serde_json::Value::Null);

                                    // Convert JSON value to Python object using pythonize
                                    let py_value = match pythonize::pythonize(py, &serialized) {
                                        Ok(v) => v,
                                        Err(_) => py.None().into_bound(py),
                                    };

                                    // Create a simple namespace object with attributes
                                    // We'll use types.SimpleNamespace which allows attribute access
                                    let types_module = py.import("types").unwrap();
                                    let simple_namespace =
                                        types_module.getattr("SimpleNamespace").unwrap();

                                    let kwargs = PyDict::new(py);
                                    let _ = kwargs.set_item("variable_name", var_name.clone());
                                    let _ = kwargs.set_item("value", py_value);
                                    let _ = kwargs.set_item(
                                        "timestamp",
                                        SystemTime::now()
                                            .duration_since(SystemTime::UNIX_EPOCH)
                                            .unwrap()
                                            .as_millis()
                                            .to_string(),
                                    );
                                    let _ = kwargs.set_item(
                                        "function_name",
                                        notification.function_name.clone(),
                                    );

                                    let var_event =
                                        simple_namespace.call((), Some(&kwargs)).unwrap();

                                    // Fire to var handlers using composite key "FunctionName.channel_name"
                                    // Use channel_name if available, otherwise fall back to variable_name
                                    let channel =
                                        notification.channel_name.as_ref().unwrap_or(var_name);
                                    let handler_key =
                                        format!("{}.{}", notification.function_name, channel);
                                    if let Some(handler_list) =
                                        callbacks.var_handlers.get(&handler_key)
                                    {
                                        for handler in handler_list {
                                            let _ = handler.call1(py, (var_event.clone(),));
                                        }
                                    }
                                }
                            }
                            baml_compiler::watch::WatchBamlValue::StreamStart(stream_id) => {
                                if let Some(var_name) = &notification.variable_name {
                                    let channel =
                                        notification.channel_name.as_ref().unwrap_or(var_name);
                                    let handler_key =
                                        format!("{}.{}", notification.function_name, channel);
                                    if let Some(handler_list) =
                                        callbacks.stream_handlers.get(&handler_key)
                                    {
                                        let stream_event_dict = PyDict::new(py);
                                        let _ = stream_event_dict
                                            .set_item("stream_id", stream_id.clone());
                                        let _ = stream_event_dict.set_item("event_type", "start");
                                        for handler in handler_list {
                                            let _ = handler.call1(py, (stream_event_dict.clone(),));
                                        }
                                    }
                                }
                            }
                            baml_compiler::watch::WatchBamlValue::StreamUpdate(
                                stream_id,
                                value,
                            ) => {
                                if let Some(var_name) = &notification.variable_name {
                                    let channel =
                                        notification.channel_name.as_ref().unwrap_or(var_name);
                                    let handler_key =
                                        format!("{}.{}", notification.function_name, channel);
                                    if let Some(handler_list) =
                                        callbacks.stream_handlers.get(&handler_key)
                                    {
                                        let serialized = serde_json::to_value(value.value())
                                            .unwrap_or(serde_json::Value::Null);

                                        let stream_event_dict = PyDict::new(py);
                                        let _ = stream_event_dict
                                            .set_item("stream_id", stream_id.clone());
                                        let _ = stream_event_dict.set_item("event_type", "update");
                                        let _ = stream_event_dict
                                            .set_item("value", serialized.to_string());
                                        for handler in handler_list {
                                            let _ = handler.call1(py, (stream_event_dict.clone(),));
                                        }
                                    }
                                }
                            }
                            baml_compiler::watch::WatchBamlValue::StreamEnd(stream_id) => {
                                if let Some(var_name) = &notification.variable_name {
                                    let channel =
                                        notification.channel_name.as_ref().unwrap_or(var_name);
                                    let handler_key =
                                        format!("{}.{}", notification.function_name, channel);
                                    if let Some(handler_list) =
                                        callbacks.stream_handlers.get(&handler_key)
                                    {
                                        let stream_event_dict = PyDict::new(py);
                                        let _ = stream_event_dict
                                            .set_item("stream_id", stream_id.clone());
                                        let _ = stream_event_dict.set_item("event_type", "end");
                                        for handler in handler_list {
                                            let _ = handler.call1(py, (stream_event_dict.clone(),));
                                        }
                                    }
                                }
                            }
                        }
                    });
                }
            });

            let (result, _) = baml_runtime
                .call_function(
                    function_name,
                    &args_map,
                    &ctx_mng,
                    tb.as_ref(),
                    cb.as_ref(),
                    Some(collector_list),
                    env_vars,
                    tags.as_ref(),
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

    #[pyo3(signature = (function_name, args, ctx, tb, cb, collectors, env_vars, tags, abort_controller=None, watchers=None))]
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
        #[allow(unused_variables)] watchers: Option<PyObject>,
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

        // Extract notification callbacks from EventCollector (only for interpreter)

        let notification_callbacks = if let Some(watchers_obj) = watchers {
            extract_notification_callbacks(py, watchers_obj)?
        } else {
            None
        };

        let (result, _event_id) = py.allow_threads(|| {
            let watch_handler = shared_handler(move |event| {
                if let Some(ref callbacks) = notification_callbacks {
                    Python::with_gil(|py| {
                        match event.value {
                            baml_compiler::watch::WatchBamlValue::Block(block_label) => {
                                // Fire block events to all registered block handlers
                                for handler in &callbacks.block_handlers {
                                    let block_event_dict = PyDict::new(py);
                                    let _ = block_event_dict
                                        .set_item("block_label", block_label.clone());
                                    let _ = block_event_dict.set_item("event_type", "enter");
                                    let _ = handler.call1(py, (block_event_dict,));
                                }
                            }
                            baml_compiler::watch::WatchBamlValue::Value(value) => {
                                if let Some(var_name) = &event.variable_name {
                                    // Serialize BamlValue to JSON and convert to Python object
                                    let serialized = serde_json::to_value(value.value())
                                        .unwrap_or(serde_json::Value::Null);

                                    // Convert JSON value to Python object using pythonize
                                    let py_value = match pythonize::pythonize(py, &serialized) {
                                        Ok(v) => v,
                                        Err(_) => py.None().into_bound(py),
                                    };

                                    // Create a simple namespace object with attributes
                                    // We'll use types.SimpleNamespace which allows attribute access
                                    let types_module = py.import("types").unwrap();
                                    let simple_namespace =
                                        types_module.getattr("SimpleNamespace").unwrap();

                                    let kwargs = PyDict::new(py);
                                    let _ = kwargs.set_item("variable_name", var_name.clone());
                                    let _ = kwargs.set_item("value", py_value);
                                    let _ = kwargs.set_item(
                                        "timestamp",
                                        SystemTime::now()
                                            .duration_since(SystemTime::UNIX_EPOCH)
                                            .unwrap()
                                            .as_millis()
                                            .to_string(),
                                    );
                                    let _ = kwargs
                                        .set_item("function_name", event.function_name.clone());

                                    let var_event =
                                        simple_namespace.call((), Some(&kwargs)).unwrap();

                                    // Fire to var handlers using composite key "FunctionName.channel_name"
                                    // Use channel_name if available, otherwise fall back to variable_name
                                    let channel = event.channel_name.as_ref().unwrap_or(var_name);
                                    let handler_key =
                                        format!("{}.{}", event.function_name, channel);
                                    if let Some(handler_list) =
                                        callbacks.var_handlers.get(&handler_key)
                                    {
                                        for handler in handler_list {
                                            let _ = handler.call1(py, (var_event.clone(),));
                                        }
                                    }
                                }
                            }
                            baml_compiler::watch::WatchBamlValue::StreamStart(stream_id) => {
                                if let Some(var_name) = &event.variable_name {
                                    let channel = event.channel_name.as_ref().unwrap_or(var_name);
                                    let handler_key =
                                        format!("{}.{}", event.function_name, channel);
                                    if let Some(handler_list) =
                                        callbacks.stream_handlers.get(&handler_key)
                                    {
                                        let stream_event_dict = PyDict::new(py);
                                        let _ = stream_event_dict
                                            .set_item("stream_id", stream_id.clone());
                                        let _ = stream_event_dict.set_item("event_type", "start");
                                        for handler in handler_list {
                                            let _ = handler.call1(py, (stream_event_dict.clone(),));
                                        }
                                    }
                                }
                            }
                            baml_compiler::watch::WatchBamlValue::StreamUpdate(
                                stream_id,
                                value,
                            ) => {
                                if let Some(var_name) = &event.variable_name {
                                    let channel = event.channel_name.as_ref().unwrap_or(var_name);
                                    let handler_key =
                                        format!("{}.{}", event.function_name, channel);
                                    if let Some(handler_list) =
                                        callbacks.stream_handlers.get(&handler_key)
                                    {
                                        let serialized = serde_json::to_value(value.value())
                                            .unwrap_or(serde_json::Value::Null);

                                        let stream_event_dict = PyDict::new(py);
                                        let _ = stream_event_dict
                                            .set_item("stream_id", stream_id.clone());
                                        let _ = stream_event_dict.set_item("event_type", "update");
                                        let _ = stream_event_dict
                                            .set_item("value", serialized.to_string());
                                        for handler in handler_list {
                                            let _ = handler.call1(py, (stream_event_dict.clone(),));
                                        }
                                    }
                                }
                            }
                            baml_compiler::watch::WatchBamlValue::StreamEnd(stream_id) => {
                                if let Some(var_name) = &event.variable_name {
                                    let channel = event.channel_name.as_ref().unwrap_or(var_name);
                                    let handler_key =
                                        format!("{}.{}", event.function_name, channel);
                                    if let Some(handler_list) =
                                        callbacks.stream_handlers.get(&handler_key)
                                    {
                                        let stream_event_dict = PyDict::new(py);
                                        let _ = stream_event_dict
                                            .set_item("stream_id", stream_id.clone());
                                        let _ = stream_event_dict.set_item("event_type", "end");
                                        for handler in handler_list {
                                            let _ = handler.call1(py, (stream_event_dict.clone(),));
                                        }
                                    }
                                }
                            }
                        }
                    });
                }
            });

            self.inner.call_function_sync(
                function_name,
                &args_map,
                &ctx_mng,
                tb.as_ref(),
                cb.as_ref(),
                Some(collector_list),
                env_vars,
                tags.as_ref(),
                tripwire,
                Some(watch_handler),
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
                tripwire,
                tags.as_ref(),
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
                tripwire,
                tags.as_ref(),
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
