mod ts_types;

use baml_runtime::RuntimeInterface;
use futures::prelude::*;
use napi::bindgen_prelude::*;
use napi_derive::napi;
use std::collections::HashMap;
use std::fmt;
use std::path::PathBuf;
use std::sync::Arc;
use ts_types::FunctionResult;

#[napi]
pub struct BamlRuntimeFfi {
    internal: Arc<baml_runtime::BamlRuntime>,
}

#[napi]
impl BamlRuntimeFfi {
    #[napi]
    pub fn from_directory(directory: String) -> Result<BamlRuntimeFfi> {
        Ok(BamlRuntimeFfi {
            internal: Arc::new(baml_runtime::BamlRuntime::from_directory(&PathBuf::from(
                directory,
            ))?),
        })
    }

    #[napi(
        ts_args_type = "function_name: string, args: Record<string, unknown>, ctx?: RuntimeContext"
    )]
    pub async fn call_function(
        &self,
        function_name: String,
        args: HashMap<String, serde_json::Value>,
        ctx: Option<ts_types::RuntimeContext>,
    ) -> Result<ts_types::FunctionResult> {
        let result = Arc::clone(&self.internal)
            .call_function(
                function_name,
                args,
                &baml_runtime::RuntimeContext::from_env().merge(ctx),
            )
            .await?;

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
