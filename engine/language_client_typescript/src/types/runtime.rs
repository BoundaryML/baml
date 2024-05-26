use super::function_result_stream::FunctionResultStreamPy;
use super::runtime_ctx_manager::RuntimeContextManagerPy;
use crate::types::function_results::FunctionResultPy;
use baml_runtime::runtime_interface::ExperimentalTracingInterface;
use baml_runtime::BamlRuntime;
use baml_types::BamlValue;
use napi::Env;
use napi::JsFunction;
use napi_derive::napi;
use std::collections::HashMap;
use std::path::PathBuf;

crate::lang_wrapper!(BamlRuntimePy, BamlRuntime, clone_safe);

#[napi]
impl BamlRuntimePy {
    #[napi(ts_return_type = "BamlRuntimePy")]
    pub fn from_directory(
        directory: String,
        env_vars: HashMap<String, String>,
    ) -> napi::Result<Self> {
        let directory = PathBuf::from(directory);
        Ok(BamlRuntime::from_directory(&directory, env_vars)
            .map_err(|e| napi::Error::new(napi::Status::GenericFailure, e.to_string()))?
            .into())
    }

    #[napi(ts_return_type = "BamlRuntimePy")]
    pub fn from_files(
        root_path: String,
        files: HashMap<String, String>,
        env_vars: HashMap<String, String>,
    ) -> napi::Result<Self> {
        Ok(BamlRuntime::from_file_content(&root_path, &files, env_vars)
            .map_err(|e| napi::Error::new(napi::Status::GenericFailure, e.to_string()))?
            .into())
    }

    #[napi]
    pub fn create_context_manager(&self) -> RuntimeContextManagerPy {
        self.inner.create_ctx_manager().into()
    }

    #[napi]
    pub async fn call_function(
        &self,
        function_name: String,
        args: serde_json::Value,
        ctx: &RuntimeContextManagerPy,
    ) -> napi::Result<FunctionResultPy> {
        let args: BamlValue = serde_json::from_value(args)
            .map_err(|e| napi::Error::new(napi::Status::GenericFailure, e.to_string()))?;
        let Some(args_map) = args.as_map() else {
            return Err(napi::Error::new(
                napi::Status::GenericFailure,
                "Invalid args",
            ));
        };

        let baml_runtime = self.inner.clone();
        let ctx_mng = ctx.inner.clone();

        let ctx_mng = ctx_mng;
        let result = baml_runtime
            .call_function(function_name, &args_map, &ctx_mng)
            .await;

        result
            .0
            .map(FunctionResultPy::from)
            .map_err(|e| napi::Error::new(napi::Status::GenericFailure, e.to_string()))
    }

    #[napi]
    pub fn stream_function(
        &self,
        env: Env,
        function_name: String,
        args: serde_json::Value,
        #[napi(ts_arg_type = "(err: any, param: FunctionResultPy) => void")] cb: Option<JsFunction>,
        ctx: &RuntimeContextManagerPy,
    ) -> napi::Result<FunctionResultStreamPy> {
        let args: BamlValue = serde_json::from_value(args)
            .map_err(|e| napi::Error::new(napi::Status::GenericFailure, e.to_string()))?;
        let Some(args_map) = args.as_map() else {
            return Err(napi::Error::new(
                napi::Status::GenericFailure,
                "Invalid args",
            ));
        };

        let ctx = ctx.inner.clone();
        let stream = self
            .inner
            .stream_function(function_name, args_map, &ctx)
            .map_err(|e| napi::Error::new(napi::Status::GenericFailure, e.to_string()))?;

        let cb = match cb {
            Some(cb) => Some(env.create_reference(cb)?),
            None => None,
        };

        Ok(FunctionResultStreamPy::new(stream, cb))
    }

    #[napi]
    pub fn flush(&self) -> napi::Result<()> {
        self.inner
            .flush()
            .map_err(|e| napi::Error::new(napi::Status::GenericFailure, e.to_string()))
    }
}
