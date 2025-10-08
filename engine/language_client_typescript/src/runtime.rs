use std::{collections::HashMap, path::PathBuf, time::SystemTime};

// Conditional runtime selection based on the "interpreter" feature flag
#[cfg(feature = "interpreter")]
pub use baml_runtime::async_interpreter_runtime::BamlAsyncInterpreterRuntime as CoreBamlRuntime;
#[cfg(not(feature = "interpreter"))]
pub use baml_runtime::async_vm_runtime::BamlAsyncVmRuntime as CoreBamlRuntime;
use baml_runtime::{on_log_event::LogEvent, runtime_interface::ExperimentalTracingInterface};
use baml_types::BamlValue;
use napi::{
    bindgen_prelude::{
        FnArgs, Function, FunctionRef, JsObjectValue, Object, ObjectFinalize, Promise, PromiseRaw,
        Undefined,
    },
    threadsafe_function::{ThreadSafeCallContext, ThreadsafeFunctionCallMode},
    Env, Error,
};
use napi_derive::napi;
use serde::{Deserialize, Serialize};

#[cfg(feature = "interpreter")]
use baml_compiler;

use crate::{
    abort_controller::js_abort_signal_to_rust_tripwire,
    errors::{from_anyhow_error, invalid_argument_error},
    parse_ts_types,
    types::{
        client_registry::ClientRegistry, function_result_stream::FunctionResultStream,
        function_results::FunctionResult, log_collector::Collector, request::HTTPRequest,
        runtime_ctx_manager::RuntimeContextManager, trace_stats::TraceStats,
        type_builder::TypeBuilder,
    },
};

type LogEventCallbackArgs = FnArgs<(Option<Error>, BamlLogEvent)>;

crate::lang_wrapper!(BamlRuntime,
    CoreBamlRuntime,
    clone_safe,
    custom_finalize,
    callback: Option<FunctionRef<LogEventCallbackArgs, ()>> = None
);

#[napi(object)]
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct LogEventMetadata {
    pub event_id: String,
    pub parent_id: Option<String>,
    pub root_event_id: String,
}

#[napi(object)]
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct BamlLogEvent {
    pub metadata: LogEventMetadata,
    pub prompt: Option<String>,
    pub raw_output: Option<String>,
    // json structure or a string
    pub parsed_output: Option<String>,
    pub start_time: String,
}

// Emit event types matching the generated events.ts
#[napi(object)]
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct BlockEvent {
    pub block_label: String,
    pub event_type: String, // "enter" | "exit"
}

#[napi(object)]
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct VarEvent {
    pub variable_name: String,
    pub value: serde_json::Value, // Serialized BamlValue
    pub timestamp: String,
    pub function_name: String,
}

// Storage for event handlers extracted from EventCollector
#[cfg(feature = "interpreter")]
struct EmitCallbacks {
    var_handlers: HashMap<String, napi::threadsafe_function::ThreadsafeFunction<VarEvent, ()>>,
    stream_handlers: HashMap<String, napi::threadsafe_function::ThreadsafeFunction<VarEvent, ()>>,
    block_handlers: Vec<napi::threadsafe_function::ThreadsafeFunction<BlockEvent, ()>>,
}

// Extract event handlers from the EventCollector.__handlers() result
#[cfg(feature = "interpreter")]
fn extract_emit_callbacks(env: &Env, events_obj: &Object) -> napi::Result<Option<EmitCallbacks>> {
    // Call __handlers() method to get InternalEventBindings
    let handlers_fn: Function = events_obj.get_named_property("__handlers")?;
    let bindings: Object = handlers_fn.call(None, &[])?;

    // Extract block handlers
    let block_array: Vec<Function> = bindings.get_named_property("block").unwrap_or_default();
    let block_handlers = block_array
        .into_iter()
        .filter_map(|handler| {
            handler
                .build_threadsafe_function()
                .build_callback(|ctx: ThreadSafeCallContext<BlockEvent>| Ok(vec![ctx.value]))
                .ok()
        })
        .collect::<Vec<_>>();

    // Extract var handlers
    let vars_obj: Object = bindings
        .get_named_property("vars")
        .unwrap_or_else(|_| env.create_object().unwrap());
    let var_names: Vec<String> = vars_obj.get_property_names()?.into_iter().collect();
    let mut var_handlers = HashMap::new();

    for var_name in var_names {
        let handler_array: Vec<Function> = vars_obj.get_named_property(&var_name)?;
        if let Some(handler) = handler_array.first() {
            if let Ok(tsfn) = handler
                .build_threadsafe_function()
                .build_callback(|ctx: ThreadSafeCallContext<VarEvent>| Ok(vec![ctx.value]))
            {
                var_handlers.insert(var_name, tsfn);
            }
        }
    }

    // Extract stream handlers
    let streams_obj: Object = bindings
        .get_named_property("streams")
        .unwrap_or_else(|_| env.create_object().unwrap());
    let stream_names: Vec<String> = streams_obj.get_property_names()?.into_iter().collect();
    let mut stream_handlers = HashMap::new();

    for stream_name in stream_names {
        let handler_array: Vec<Function> = streams_obj.get_named_property(&stream_name)?;
        if let Some(handler) = handler_array.first() {
            if let Ok(tsfn) = handler
                .build_threadsafe_function()
                .build_callback(|ctx: ThreadSafeCallContext<VarEvent>| Ok(vec![ctx.value]))
            {
                stream_handlers.insert(stream_name, tsfn);
            }
        }
    }

    Ok(Some(EmitCallbacks {
        var_handlers,
        stream_handlers,
        block_handlers,
    }))
}

#[napi]
impl BamlRuntime {
    #[napi(ts_return_type = "BamlRuntime")]
    pub fn from_directory(
        directory: String,
        env_vars: HashMap<String, String>,
    ) -> napi::Result<Self> {
        let directory = PathBuf::from(directory);
        Ok(CoreBamlRuntime::from_directory(&directory, env_vars)
            .map_err(from_anyhow_error)?
            .into())
    }

    #[napi(ts_return_type = "BamlRuntime")]
    pub fn from_files(
        root_path: String,
        files: HashMap<String, String>,
        env_vars: HashMap<String, Option<String>>,
    ) -> napi::Result<Self> {
        let env_vars = env_vars
            .into_iter()
            .filter_map(|(key, value)| value.map(|value| (key, value)))
            .collect();
        Ok(
            CoreBamlRuntime::from_file_content(&root_path, &files, env_vars)
                .map_err(from_anyhow_error)?
                .into(),
        )
    }

    #[napi]
    pub fn reset(
        &mut self,
        root_path: String,
        files: HashMap<String, String>,
        env_vars: HashMap<String, String>,
    ) -> napi::Result<()> {
        self.inner = CoreBamlRuntime::from_file_content(&root_path, &files, env_vars)
            .map_err(from_anyhow_error)?
            .into();
        Ok(())
    }

    #[napi]
    pub fn create_context_manager(&self) -> RuntimeContextManager {
        self.inner
            .create_ctx_manager(BamlValue::String("typescript".to_string()), None)
            .into()
    }

    #[napi(ts_return_type = "Promise<FunctionResult>")]
    pub fn call_function<'e>(
        &self,
        env: &'e Env,
        function_name: String,
        #[napi(ts_arg_type = "{ [name: string]: any }")] args: Object,
        ctx: &RuntimeContextManager,
        tb: Option<&TypeBuilder>,
        cb: Option<&ClientRegistry>,
        collectors: Vec<&Collector>,
        tags: HashMap<String, String>,
        env_vars: HashMap<String, String>,
        signal: Option<Object>, // AbortSignal parameter
        events: Option<Object>, // NEW: EventCollector parameter
    ) -> napi::Result<PromiseRaw<'e, FunctionResult>> {
        let args = parse_ts_types::js_object_to_baml_value(env, args)?;

        if !args.is_map() {
            return Err(invalid_argument_error(&format!(
                "Expected a map of arguments, got: {}",
                args.r#type()
            )));
        }
        let args_map = args.as_map_owned().unwrap();

        // Convert AbortSignal to Tripwire
        let tripwire = js_abort_signal_to_rust_tripwire(env, signal)?;

        // Extract emit callbacks from EventCollector (only for interpreter)
        #[cfg(feature = "interpreter")]
        let emit_callbacks = if let Some(ref events_obj) = events {
            extract_emit_callbacks(env, events_obj)?
        } else {
            None
        };

        let baml_runtime = self.inner.clone();
        let ctx_mng = ctx.inner.clone();
        let tb = tb.map(|tb| tb.inner.clone());
        let cb = cb.map(|cb| cb.inner.clone());

        let collector_list = collectors
            .into_iter()
            .map(|c| c.inner.clone())
            .collect::<Vec<_>>();

        let function_name_clone = function_name.clone();

        let fut = async move {
            // Create emit_handler closure (only for interpreter)
            #[cfg(feature = "interpreter")]
            let emit_handler = move |event: baml_compiler::emit::EmitEvent| {
                if let Some(ref callbacks) = emit_callbacks {
                    match event.value {
                        baml_compiler::emit::EmitBamlValue::Block(block_label) => {
                            // Fire block events to all registered block handlers
                            for handler in &callbacks.block_handlers {
                                let block_event = BlockEvent {
                                    block_label: block_label.clone(),
                                    event_type: "enter".to_string(), // TODO: track enter/exit
                                };
                                let _ = handler
                                    .call(Ok(block_event), ThreadsafeFunctionCallMode::NonBlocking);
                            }
                        }
                        baml_compiler::emit::EmitBamlValue::Value(value) => {
                            if let Some(var_name) = &event.variable_name {
                                // Serialize BamlValue to JSON
                                let serialized = serde_json::to_value(&value.value())
                                    .unwrap_or(serde_json::Value::Null);

                                let var_event = VarEvent {
                                    variable_name: var_name.clone(),
                                    value: serialized,
                                    timestamp: SystemTime::now()
                                        .duration_since(SystemTime::UNIX_EPOCH)
                                        .unwrap()
                                        .as_millis()
                                        .to_string(),
                                    function_name: event.function_name.clone(),
                                };

                                // Fire to appropriate handler based on stream vs var
                                let handlers = if event.is_stream {
                                    &callbacks.stream_handlers
                                } else {
                                    &callbacks.var_handlers
                                };

                                if let Some(handler) = handlers.get(var_name) {
                                    let _ = handler.call(
                                        Ok(var_event),
                                        ThreadsafeFunctionCallMode::NonBlocking,
                                    );
                                }
                            }
                        }
                    }
                }
            };

            #[cfg(feature = "interpreter")]
            let result = baml_runtime
                .call_function(
                    function_name,
                    &args_map,
                    &ctx_mng,
                    tb.as_ref(),
                    cb.as_ref(),
                    Some(collector_list),
                    env_vars,
                    Some(tags),
                    tripwire,
                    Some(emit_handler), // pass emit handler for interpreter runtime
                )
                .await;

            #[cfg(not(feature = "interpreter"))]
            let result = baml_runtime
                .call_function(
                    function_name,
                    &args_map,
                    &ctx_mng,
                    tb.as_ref(),
                    cb.as_ref(),
                    Some(collector_list),
                    env_vars,
                    Some(tags),
                    tripwire,
                )
                .await;

            result
                .0
                .map(FunctionResult::from)
                .map_err(from_anyhow_error)
        };

        env.spawn_future(fut)
    }

    #[napi]
    pub fn call_function_sync(
        &self,
        env: Env,
        function_name: String,
        #[napi(ts_arg_type = "{ [name: string]: any }")] args: Object,
        ctx: &RuntimeContextManager,
        tb: Option<&TypeBuilder>,
        cb: Option<&ClientRegistry>,
        collectors: Vec<&Collector>,
        tags: HashMap<String, String>,
        env_vars: HashMap<String, String>,
        signal: Option<Object>, // NEW: AbortSignal parameter (sync doesn't actually use it)
    ) -> napi::Result<FunctionResult> {
        let args = parse_ts_types::js_object_to_baml_value(&env, args)?;
        let tripwire = js_abort_signal_to_rust_tripwire(&env, signal)?;

        if !args.is_map() {
            return Err(invalid_argument_error(&format!(
                "Expected a map of arguments, got: {}",
                args.r#type()
            )));
        }
        let args_map = args.as_map_owned().unwrap();

        let ctx_mng = ctx.inner.clone();
        let tb = tb.map(|tb| tb.inner.clone());
        let cb = cb.map(|cb| cb.inner.clone());
        let collector_list = collectors
            .into_iter()
            .map(|c| c.inner.clone())
            .collect::<Vec<_>>();
        let (result, _event_id) = self.inner.call_function_sync(
            function_name,
            &args_map,
            &ctx_mng,
            tb.as_ref(),
            cb.as_ref(),
            Some(collector_list),
            env_vars,
            Some(tags),
            tripwire,
        );

        result.map(FunctionResult::from).map_err(from_anyhow_error)
    }

    #[napi]
    pub fn stream_function(
        &self,
        env: Env,
        function_name: String,
        #[napi(ts_arg_type = "{ [name: string]: any }")] args: Object,
        #[napi(ts_arg_type = "((err: any, param: FunctionResult) => void) | undefined")] cb: Option<
            Function<FnArgs<(Error, FunctionResult)>, ()>,
        >,
        ctx: &RuntimeContextManager,
        tb: Option<&TypeBuilder>,
        client_registry: Option<&ClientRegistry>,
        collectors: Vec<&Collector>,
        tags: HashMap<String, String>,
        env_vars: HashMap<String, String>,
        signal: Option<Object>, // NEW: AbortSignal parameter
        #[napi(ts_arg_type = "(() => void) | undefined")] on_tick: Option<Function<(), ()>>,
    ) -> napi::Result<FunctionResultStream> {
        let args: BamlValue = parse_ts_types::js_object_to_baml_value(&env, args)?;
        if !args.is_map() {
            return Err(invalid_argument_error(&format!(
                "Expected a map of arguments, got: {}",
                args.r#type()
            )));
        }
        let args_map = args.as_map_owned().unwrap();

        let tripwire = js_abort_signal_to_rust_tripwire(&env, signal)?;

        let ctx = ctx.inner.clone();
        let tb = tb.map(|tb| tb.inner.clone());
        let client_registry = client_registry.map(|cb| cb.inner.clone());
        let collector_list = collectors
            .into_iter()
            .map(|c| c.inner.clone())
            .collect::<Vec<_>>();
        let stream = self
            .inner
            .stream_function(
                function_name,
                &args_map,
                &ctx,
                tb.as_ref(),
                client_registry.as_ref(),
                Some(collector_list),
                env_vars,
                Some(tags),
                tripwire,
            )
            .map_err(from_anyhow_error)?;

        let cb = match cb {
            Some(func) => Some(func.create_ref()?),
            None => None,
        };

        let on_tick = match on_tick {
            Some(tick_cb) => Some(tick_cb.create_ref()?),
            None => None,
        };

        Ok(FunctionResultStream::new(
            stream,
            cb,
            on_tick,
            tb,
            client_registry,
        ))
    }

    #[napi]
    pub fn stream_function_sync(
        &self,
        env: Env,
        function_name: String,
        #[napi(ts_arg_type = "{ [name: string]: any }")] args: Object,
        #[napi(ts_arg_type = "((err: any, param: FunctionResult) => void) | undefined")] cb: Option<
            Function<FnArgs<(Error, FunctionResult)>, ()>,
        >,
        ctx: &RuntimeContextManager,
        tb: Option<&TypeBuilder>,
        client_registry: Option<&ClientRegistry>,
        collectors: Vec<&Collector>,
        tags: HashMap<String, String>,
        env_vars: HashMap<String, String>,
        signal: Option<Object>, // NEW: AbortSignal parameter
        #[napi(ts_arg_type = "(() => void) | undefined")] on_tick: Option<Function<(), ()>>,
    ) -> napi::Result<FunctionResultStream> {
        let args: BamlValue = parse_ts_types::js_object_to_baml_value(&env, args)?;
        if !args.is_map() {
            return Err(invalid_argument_error(&format!(
                "Expected a map of arguments, got: {}",
                args.r#type()
            )));
        }
        let args_map = args.as_map_owned().unwrap();

        let ctx = ctx.inner.clone();
        let tb = tb.map(|tb| tb.inner.clone());
        let client_registry = client_registry.map(|cb| cb.inner.clone());
        let collector_list = collectors
            .into_iter()
            .map(|c| c.inner.clone())
            .collect::<Vec<_>>();
        let tripwire = js_abort_signal_to_rust_tripwire(&env, signal)?;
        let stream = self
            .inner
            .stream_function(
                function_name,
                &args_map,
                &ctx,
                tb.as_ref(),
                client_registry.as_ref(),
                Some(collector_list),
                env_vars,
                Some(tags),
                tripwire,
            )
            .map_err(from_anyhow_error)?;

        let cb = match cb {
            Some(func) => Some(func.create_ref()?),
            None => None,
        };

        let on_tick = match on_tick {
            Some(tick_cb) => Some(tick_cb.create_ref()?),
            None => None,
        };

        Ok(FunctionResultStream::new(
            stream,
            cb,
            on_tick,
            tb,
            client_registry,
        ))
    }

    #[napi(ts_return_type = "Promise<HTTPRequest>")]
    pub fn build_request<'e>(
        &self,
        env: &'e Env,
        function_name: String,
        #[napi(ts_arg_type = "{ [name: string]: any }")] args: Object,
        ctx: &RuntimeContextManager,
        tb: Option<&TypeBuilder>,
        cb: Option<&ClientRegistry>,
        stream: bool,
        env_vars: HashMap<String, String>,
    ) -> napi::Result<PromiseRaw<'e, HTTPRequest>> {
        let args = parse_ts_types::js_object_to_baml_value(env, args)?;

        if !args.is_map() {
            return Err(invalid_argument_error(&format!(
                "Expected a map of arguments, got: {}",
                args.r#type()
            )));
        }
        let args_map = args.as_map_owned().unwrap();

        let baml_runtime = self.inner.clone();
        let ctx_mng = ctx.inner.clone();
        let tb = tb.map(|tb| tb.inner.clone());
        let cb = cb.map(|cb| cb.inner.clone());

        let fut = async move {
            baml_runtime
                .build_request(
                    function_name,
                    &args_map,
                    &ctx_mng,
                    tb.as_ref(),
                    cb.as_ref(),
                    env_vars,
                    stream,
                )
                .await
                .map(HTTPRequest::from)
                .map_err(from_anyhow_error)
        };

        env.spawn_future(fut)
    }

    #[napi]
    pub fn build_request_sync(
        &self,
        env: Env,
        function_name: String,
        #[napi(ts_arg_type = "{ [name: string]: any }")] args: Object,
        ctx: &RuntimeContextManager,
        tb: Option<&TypeBuilder>,
        cb: Option<&ClientRegistry>,
        stream: bool,
        env_vars: HashMap<String, String>,
    ) -> napi::Result<HTTPRequest> {
        let args = parse_ts_types::js_object_to_baml_value(&env, args)?;

        if !args.is_map() {
            return Err(invalid_argument_error(&format!(
                "Expected a map of arguments, got: {}",
                args.r#type()
            )));
        }
        let args_map = args.as_map_owned().unwrap();

        let ctx_mng = ctx.inner.clone();
        let tb = tb.map(|tb| tb.inner.clone());
        let cb = cb.map(|cb| cb.inner.clone());

        self.inner
            .build_request_sync(
                function_name,
                &args_map,
                &ctx_mng,
                tb.as_ref(),
                cb.as_ref(),
                stream,
                env_vars,
            )
            .map(HTTPRequest::from)
            .map_err(from_anyhow_error)
    }

    #[napi]
    pub fn parse_llm_response(
        &self,
        env: Env,
        function_name: String,
        llm_response: String,
        allow_partials: bool,
        ctx: &RuntimeContextManager,
        tb: Option<&TypeBuilder>,
        cb: Option<&ClientRegistry>,
        env_vars: HashMap<String, String>,
    ) -> napi::Result<serde_json::Value> {
        let ctx_mng = ctx.inner.clone();
        let tb = tb.map(|tb| tb.inner.clone());
        let cb = cb.map(|cb| cb.inner.clone());

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
            .map_err(from_anyhow_error)?;

        let value = serde_json::to_value(if allow_partials {
            parsed.serialize_partial()
        } else {
            parsed.serialize_final()
        });

        value.map_err(|e| napi::Error::from_reason(format!("Could not parse LLM response: {e}")))
    }

    #[napi]
    pub fn set_log_event_callback(
        &mut self,
        env: Env,
        #[napi(ts_arg_type = "undefined | ((err: any, param: BamlLogEvent) => void)")] func: Option<
            Function<LogEventCallbackArgs, ()>,
        >,
    ) -> napi::Result<Undefined> {
        // drop any previous callback automatically
        self.callback = match func {
            Some(f) => Some(f.create_ref()?),
            None => None,
        };

        let Some(cb_ref) = &self.callback else {
            return self
                .inner
                .set_log_event_callback(None)
                .map_err(from_anyhow_error);
        };

        // configure runtime callback
        let cb = cb_ref.borrow_back(&env)?;
        let thread_safe_fn = cb.build_threadsafe_function().build_callback(
            |ctx: ThreadSafeCallContext<(Option<Error>, BamlLogEvent)>| {
                Ok(FnArgs::from((Option::<Error>::None, ctx.value)))
            },
        )?;

        let rust_cb = Box::new(move |event: LogEvent| {
            let js_evt = BamlLogEvent {
                metadata: LogEventMetadata {
                    event_id: event.metadata.event_id,
                    parent_id: event.metadata.parent_id,
                    root_event_id: event.metadata.root_event_id,
                },
                prompt: event.prompt,
                raw_output: event.raw_output,
                parsed_output: event.parsed_output,
                start_time: event.start_time,
            };

            let status = thread_safe_fn.call((None, js_evt), ThreadsafeFunctionCallMode::Blocking);
            if status != napi::Status::Ok {
                log::error!("Error calling log_event callback: {status:?}");
            }
            Ok(())
        });

        self.inner
            .set_log_event_callback(Some(rust_cb))
            .map_err(from_anyhow_error)
    }

    #[napi]
    pub fn flush(&mut self, _env: Env) -> napi::Result<()> {
        self.inner.flush().map_err(from_anyhow_error)
    }

    #[napi]
    pub fn drain_stats(&self) -> TraceStats {
        self.inner.drain_stats().into()
    }
}

// TODO: This is probably no longer necessary since dropping FunctionRef
// automatically unrefs the Node callback. Fix the macro that creates the
// wrapper to remove custom_finalize.
impl ObjectFinalize for BamlRuntime {
    fn finalize(self, _env: Env) -> napi::Result<()> {
        // dropping self also drops any FunctionRef callbacks
        Ok(())
    }
}
