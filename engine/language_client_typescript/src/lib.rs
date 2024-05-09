use tokio::time::{sleep, Duration};

//#[macro_use]
//extern crate napi_derive;
//
//#[napi]
//pub async fn sum(a: i32, b: i32) -> Result<i32> {
//    sleep(Duration::from_secs(1)).await;
//    a + b
//}

mod ts_types;

use baml_runtime::RuntimeInterface;
use futures::prelude::*;
use napi::bindgen_prelude::*;
use napi_derive::napi;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use ts_types::FunctionResult;

//#[napi]
//async fn read_file_async(path: String) -> Result<Buffer> {
//    log::info!("reading file async");
//    let buf = fs::read(path)
//        .map(|r| match r {
//            Ok(content) => Ok(content.into()),
//            Err(e) => Err(Error::new(
//                Status::GenericFailure,
//                format!("failed to read file, {}", e),
//            )),
//        })
//        .await;
//    log::info!("finished reading file async");
//    buf
//}

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

    /// TODO: ctx should be optional
    #[napi(
        ts_args_type = "call_function: (function_name: string, args: unknown, ctx: unknown) => FunctionResult"
    )]
    pub async fn call_function(
        &self,
        function_name: String,
        args: HashMap<String, serde_json::Value>,
        ctx: ts_types::RuntimeContext,
    ) -> Result<ts_types::FunctionResult> {
        //let mut js_ctx: RuntimeContext = ctx.env.from_js_value(js_ctx)?;

        //js_ctx.env = std::env::vars_os()
        //    .map(|(k, v)| {
        //        (
        //            k.to_string_lossy().to_string(),
        //            v.to_string_lossy().to_string(),
        //        )
        //    })
        //    .chain(js_ctx.env.into_iter())
        //    .collect();

        //  let js_ctx = {

        //  }

        let result = Arc::clone(&self.internal)
            .call_function(
                function_name,
                args,
                &baml_runtime::RuntimeContext::from_env().merge(ctx.into()),
            )
            .await?;

        Ok(ts_types::FunctionResult::new(result))
    }

    //fn generate_client(&self, args: python_types::GenerateArgs) -> Result<()> {
    //    self.internal
    //        .generate_client(&args.client_type, &(&args).into())?;

    //    Ok(())
    //}
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
