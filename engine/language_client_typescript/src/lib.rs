mod parse_ts_types;
mod ts_types;

use baml_runtime::{RuntimeContext, RuntimeInterface};
use futures::prelude::*;
use indexmap::IndexMap;
use napi::bindgen_prelude::*;
use napi::{CallContext, JsUnknown};
use napi_derive::napi;
use std::collections::HashMap;
use std::fmt;
use std::path::PathBuf;
use std::sync::Arc;
use ts_types::FunctionResult;

#[napi]
pub fn rust_is_instance(env: Env, val: Unknown) -> Result<bool> {
    BamlRuntimeFfi::instance_of(env, val)
}

#[napi]
pub struct BamlRuntimeFfi {
    internal: Arc<baml_runtime::BamlRuntime>,
}

#[napi]
impl BamlRuntimeFfi {
    #[napi]
    pub fn from_directory(directory: String) -> Result<BamlRuntimeFfi> {
        let ctx = baml_runtime::RuntimeContext::from_env();

        Ok(BamlRuntimeFfi {
            internal: Arc::new(baml_runtime::BamlRuntime::from_directory(
                &PathBuf::from(directory),
                &ctx,
            )?),
        })
    }

    #[napi(
        ts_args_type = "function_name: string, args: Record<string, unknown>, ctx?: RuntimeContext"
    )]
    pub async fn call_function(
        &self,
        function_name: String,
        args: HashMap<String, serde_json::Value>,
    ) -> Result<ts_types::FunctionResult> {
        let args = args.into_iter().collect::<IndexMap<_, _>>();

        let rt = self.internal.clone();
        let ctx = baml_runtime::RuntimeContext::from_env();

        let result = rt.call_function(function_name, &args, &ctx).await?;

        Ok(ts_types::FunctionResult::new(result))
    }

    #[napi]
    pub fn run_cli(args: Vec<String>) -> Result<()> {
        Ok(baml_runtime::BamlRuntime::run_cli(
            args.into(),
            baml_runtime::CallerType::Typescript,
        )?)
    }
}

#[napi::module_init]
fn module_init() {
    if let Err(e) = env_logger::try_init_from_env(
        env_logger::Env::new()
            .filter("BAML_LOG")
            .write_style("BAML_LOG_STYLE"),
    ) {
        eprintln!("Failed to initialize BAML logger: {:#}", e);
    };
}
