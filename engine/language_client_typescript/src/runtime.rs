use std::{collections::HashMap, path::PathBuf};

use baml_runtime::{
    on_log_event::LogEvent, runtime_interface::ExperimentalTracingInterface,
    BamlRuntime as CoreRuntime,
};
use baml_types::BamlValue;
use internal_baml_core::feature_flags::FeatureFlags;
use napi::{
    bindgen_prelude::{
        FnArgs, Function, FunctionRef, Object, ObjectFinalize, Promise, PromiseRaw, Undefined,
    },
    threadsafe_function::{ThreadSafeCallContext, ThreadsafeFunctionCallMode},
    Env, Error,
};
use napi_derive::napi;
use serde::{Deserialize, Serialize};

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
    CoreRuntime,
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

#[napi]
impl BamlRuntime {
    #[napi(ts_return_type = "BamlRuntime")]
    pub fn from_directory(
        directory: String,
        env_vars: HashMap<String, String>,
    ) -> napi::Result<Self> {
        let directory = PathBuf::from(directory);
        Ok(
            CoreRuntime::from_directory(&directory, env_vars, FeatureFlags::new())
                .map_err(from_anyhow_error)?
                .into(),
        )
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
            CoreRuntime::from_file_content(&root_path, &files, env_vars, FeatureFlags::new())
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
        self.inner =
            CoreRuntime::from_file_content(&root_path, &files, env_vars, FeatureFlags::new())
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
        signal: Option<Object>, // NEW: AbortSignal parameter
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

        let baml_runtime = self.inner.clone();
        let ctx_mng = ctx.inner.clone();
        let tb = tb.map(|tb| tb.inner.clone());
        let cb = cb.map(|cb| cb.inner.clone());

        let collector_list = collectors
            .into_iter()
            .map(|c| c.inner.clone())
            .collect::<Vec<_>>();

        let fut = async move {
            let result = baml_runtime
                .call_function(
                    function_name,
                    &args_map,
                    &ctx_mng,
                    tb.as_ref(),
                    cb.as_ref(),
                    Some(collector_list),
                    Some(tags),
                    env_vars,
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
