use napi::{Env, JsUndefined};
use napi_derive::napi;

mod parse_ts_types;
mod runtime;
mod types;

pub(crate) use runtime::BamlRuntime;

#[napi(js_name = "invoke_runtime_cli")]
pub fn run_cli(env: Env, params: Vec<String>) -> napi::Result<JsUndefined> {
    baml_runtime::BamlRuntime::run_cli(params, baml_runtime::CallerType::Typescript)?;
    env.get_undefined()
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
