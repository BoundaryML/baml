use napi::{Env, JsUndefined};
use napi_derive::napi;

mod errors;
mod parse_ts_types;
mod runtime;
mod types;

pub(crate) use runtime::BamlRuntime;
use tracing_subscriber::{self, EnvFilter};

#[napi(js_name = "invoke_runtime_cli")]
pub fn run_cli(env: Env, params: Vec<String>) -> napi::Result<JsUndefined> {
    baml_cli::run_cli(
        params,
        baml_runtime::RuntimeCliDefaults {
            output_type: baml_types::GeneratorOutputType::Typescript,
        },
    )?;
    env.get_undefined()
}

#[napi(js_name = "get_version")]
pub fn get_version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

#[napi::module_init]
fn module_init() {
    match baml_log::init() {
        Ok(_) => (),
        Err(e) => {
            eprintln!("Failed to initialize BAML logger: {:#}", e);
        }
    }
}
