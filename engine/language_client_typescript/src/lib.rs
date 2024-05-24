mod parse_ts_types;
mod ts_types;

use baml_runtime::RuntimeContextManager;
use baml_types::{BamlMap, BamlValue};
use indexmap::IndexMap;
use napi::bindgen_prelude::*;

use napi_derive::napi;
use std::collections::HashMap;

use std::path::PathBuf;
use std::sync::Arc;

#[napi]
pub fn rust_is_instance(env: Env, val: Unknown) -> Result<bool> {
    BamlRuntimeFfi::instance_of(env, val)
}

#[napi]
pub struct BamlRuntimeFfi {
    internal: Arc<baml_runtime::BamlRuntime>,
}

#[napi]
pub struct RuntimeContextManagerTs {
    inner: RuntimeContextManager,
}

#[napi]
impl BamlRuntimeFfi {
    #[napi]
    pub fn from_directory(
        directory: String,
        env_vars: HashMap<String, String>,
    ) -> Result<BamlRuntimeFfi> {
        Ok(BamlRuntimeFfi {
            internal: Arc::new(baml_runtime::BamlRuntime::from_directory(
                &PathBuf::from(directory),
                env_vars,
            )?),
        })
    }

    #[napi]
    pub fn create_context_manager(&self) -> RuntimeContextManagerTs {
        RuntimeContextManagerTs {
            inner: self.internal.create_ctx_manager(),
        }
    }

    #[napi(
        ts_args_type = "function_name: string, args: Record<string, unknown>, ctx?: RuntimeContext"
    )]
    pub async fn call_function(
        &self,
        function_name: String,
        args: HashMap<String, serde_json::Value>,
        ctx: &RuntimeContextManagerTs,
    ) -> Result<ts_types::FunctionResult> {
        // Convert each arg to a BamlValue
        let raw_args = args
            .into_iter()
            .map(|(k, v)| (k, serde_json::from_value::<BamlValue>(v)))
            .collect::<IndexMap<String, _>>();

        let (ok, err) = raw_args
            .into_iter()
            .partition::<Vec<_>, _>(|(_, v)| v.is_ok());
        if !err.is_empty() {
            return Err(Error::new(
                Status::InvalidArg,
                format!(
                    "Failed to parse args: {:#?}",
                    err.into_iter().map(|(k, v)| (k, v)).collect::<Vec<_>>()
                ),
            ));
        }

        let rt = self.internal.clone();

        let args = ok
            .into_iter()
            .map(|(k, v)| (k.clone(), v.unwrap().clone()))
            .collect::<BamlMap<_, _>>();
        let (result, _) = rt.call_function(function_name, &args, &ctx.inner).await;

        result.map(ts_types::FunctionResult::new).map_err(|e| {
            Error::new(
                Status::InvalidArg,
                format!("Failed to call function: {:#}", e),
            )
        })
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
