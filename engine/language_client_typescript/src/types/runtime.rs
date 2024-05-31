use super::function_result_stream::FunctionResultStream;
use super::runtime_ctx_manager::RuntimeContextManager;
use super::type_builder::TypeBuilder;
use crate::parse_ts_types;
use crate::types::function_results::FunctionResult;
use baml_runtime::runtime_interface::ExperimentalTracingInterface;
use baml_runtime::BamlRuntime as CoreRuntime;
use baml_types::BamlValue;
use napi::Env;
use napi::JsFunction;
use napi::JsObject;
use napi_derive::napi;
use std::collections::HashMap;
use std::path::PathBuf;

crate::lang_wrapper!(BamlRuntime, CoreRuntime, clone_safe);

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
    pub fn flush(&self) -> napi::Result<()> {
        self.inner
            .flush()
            .map_err(|e| napi::Error::new(napi::Status::GenericFailure, e.to_string()))
    }
}
