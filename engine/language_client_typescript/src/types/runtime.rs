use super::function_result_stream::FunctionResultStream;
use super::runtime_ctx_manager::RuntimeContextManager;
use super::type_builder::TypeBuilder;
use crate::parse_ts_types;
use crate::types::function_results::FunctionResult;
use baml_runtime::on_log_event::{LogEvent, LogEventCallbackSync};
use baml_runtime::runtime_interface::ExperimentalTracingInterface;
use baml_runtime::BamlRuntime as CoreRuntime;
use baml_types::BamlValue;
use napi::bindgen_prelude::ObjectFinalize;
use napi::threadsafe_function::{ThreadSafeCallContext, ThreadsafeFunctionCallMode};
use napi::JsFunction;
use napi::JsObject;
use napi::{Env, JsUndefined};
use napi_derive::napi;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

crate::lang_wrapper!(BamlRuntime,
    CoreRuntime,
    clone_safe,
    custom_finalize,
    callback: Option<napi::Ref<()>> = None
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
        Ok(CoreRuntime::from_directory(&directory, env_vars)
            .map_err(|e| napi::Error::new(napi::Status::GenericFailure, e.to_string()))?
            .into())
    }

    #[napi(ts_return_type = "BamlRuntime")]
    pub fn from_files(
        root_path: String,
        files: HashMap<String, String>,
        env_vars: HashMap<String, String>,
    ) -> napi::Result<Self> {
        Ok(CoreRuntime::from_file_content(&root_path, &files, env_vars)
            .map_err(|e| napi::Error::new(napi::Status::GenericFailure, e.to_string()))?
            .into())
    }

    #[napi]
    pub fn create_context_manager(&self) -> RuntimeContextManager {
        self.inner
            .create_ctx_manager(BamlValue::String("typescript".to_string()))
            .into()
    }

    #[napi(ts_return_type = "Promise<FunctionResult>")]
    pub fn call_function(
        &self,
        env: Env,
        function_name: String,
        #[napi(ts_arg_type = "{ [string]: any }")] args: JsObject,
        ctx: &RuntimeContextManager,
        tb: Option<&TypeBuilder>,
    ) -> napi::Result<JsObject> {
        let args = parse_ts_types::js_object_to_baml_value(env, args)?;

        if !args.is_map() {
            return Err(napi::Error::new(
                napi::Status::GenericFailure,
                format!(
                    "Invalid args: Expected a map of arguments, got: {}",
                    args.r#type()
                ),
            ));
        }
        let args_map = args.as_map_owned().unwrap();

        let baml_runtime = self.inner.clone();
        let ctx_mng = ctx.inner.clone();
        let tb = tb.map(|tb| tb.inner.clone());

        let fut = async move {
            let result = baml_runtime
                .call_function(function_name, &args_map, &ctx_mng, tb.as_ref())
                .await;

            result
                .0
                .map(FunctionResult::from)
                .map_err(|e| napi::Error::new(napi::Status::GenericFailure, e.to_string()))
        };

        env.execute_tokio_future(fut, |&mut _, data| Ok(data))
    }

    #[napi]
    pub fn stream_function(
        &self,
        env: Env,
        function_name: String,
        #[napi(ts_arg_type = "{ [string]: any }")] args: JsObject,
        #[napi(ts_arg_type = "(err: any, param: FunctionResult) => void")] cb: Option<JsFunction>,
        ctx: &RuntimeContextManager,
        tb: Option<&TypeBuilder>,
    ) -> napi::Result<FunctionResultStream> {
        let args: BamlValue = parse_ts_types::js_object_to_baml_value(env, args)?;
        if !args.is_map() {
            return Err(napi::Error::new(
                napi::Status::GenericFailure,
                format!(
                    "Invalid args: Expected a map of arguments, got: {}",
                    args.r#type()
                ),
            ));
        }
        let args_map = args.as_map_owned().unwrap();

        let ctx = ctx.inner.clone();
        let tb = tb.map(|tb| tb.inner.clone());
        let stream = self
            .inner
            .stream_function(function_name, &args_map, &ctx, tb.as_ref())
            .map_err(|e| napi::Error::new(napi::Status::GenericFailure, e.to_string()))?;

        let cb = match cb {
            Some(cb) => Some(env.create_reference(cb)?),
            None => None,
        };

        Ok(FunctionResultStream::new(stream, cb, tb))
    }

    #[napi]
    pub fn set_log_event_callback(
        &mut self,
        env: Env,
        #[napi(ts_arg_type = "(err: any, param: BamlLogEvent) => void")] func: JsFunction,
    ) -> napi::Result<JsUndefined> {
        let cb = env.create_reference(func)?;
        // let prev = self.callback.take();
        // if let Some(mut old_cb) = prev {
        //     old_cb.unref(env)?;
        // }
        self.callback = Some(cb);

        let res = match &self.callback {
            Some(callback_ref) => {
                let cb = env.get_reference_value::<JsFunction>(callback_ref)?;
                let mut tsfn = env.create_threadsafe_function(
                    &cb,
                    0,
                    |ctx: ThreadSafeCallContext<BamlLogEvent>| {
                        Ok(vec![BamlLogEvent::from(ctx.value)])
                    },
                )?;
                let tsfn_clone = tsfn.clone();

                let res = self
                    .inner
                    .set_log_event_callback(Box::new(move |event: LogEvent| {
                        // let env = callback.env;
                        let event = BamlLogEvent {
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

                        let res = tsfn_clone.call(Ok(event), ThreadsafeFunctionCallMode::Blocking);
                        if res != napi::Status::Ok {
                            log::error!("Error calling on_log_event callback: {:?}", res);
                        }

                        Ok(())
                    }))
                    .map_err(|e| napi::Error::new(napi::Status::GenericFailure, e.to_string()));
                let _ = tsfn.unref(&env);

                match res {
                    Ok(_) => Ok(()),
                    Err(e) => {
                        log::error!("Error setting log_event_callback: {:?}", e);
                        Err(e)
                    }
                }
            }
            None => Ok(()),
        };

        let _ = match res {
            Ok(_) => Ok(env.get_undefined()?),
            Err(e) => {
                log::error!("Error setting log_event_callback: {:?}", e);
                Err(e)
            }
        };

        env.get_undefined()
    }

    #[napi]
    pub fn flush(&mut self, env: Env) -> napi::Result<()> {
        let res = self
            .inner
            .flush()
            .map_err(|e| napi::Error::new(napi::Status::GenericFailure, e.to_string()));

        res
    }
}

impl ObjectFinalize for BamlRuntime {
    fn finalize(mut self, env: Env) -> napi::Result<()> {
        if let Some(mut cb) = self.callback.take() {
            match cb.unref(env) {
                Ok(_) => (),
                Err(e) => log::error!("Error unrefing callback: {:?}", e),
            }
        }
        Ok(())
    }
}
