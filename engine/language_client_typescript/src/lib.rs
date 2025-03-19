use std::str::FromStr;

use napi::{Env, JsUndefined};
use napi_derive::napi;

mod errors;
mod parse_ts_types;
mod runtime;
mod types;

pub(crate) use runtime::BamlRuntime;
use tracing_subscriber::{self, EnvFilter};

#[napi(js_name = "invoke_runtime_cli")]
pub fn run_cli(env: Env, params: Vec<String>) -> napi::Result<i32> {
    let exit_code = baml_cli::run_cli(
        params,
        baml_runtime::RuntimeCliDefaults {
            output_type: baml_types::GeneratorOutputType::Typescript,
        },
    )?;

    Ok(exit_code.into())
}

#[napi(js_name = "get_version")]
pub fn get_version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

#[napi(js_name = "getLogLevel")]
pub fn get_log_level() -> String {
    baml_log::get_log_level().as_str().into()
}

#[napi(js_name = "setLogLevel")]
pub fn set_log_level(level: String) {
    let _ = baml_log::Level::from_str(&level).map(baml_log::set_log_level);
}

#[napi(js_name = "setLogJsonMode")]
pub fn set_log_json_mode(use_json: bool) {
    let _ = baml_log::set_json_mode(use_json);
}

#[napi(js_name = "setLogMaxChunkLength")]
pub fn set_log_max_chunk_length(length: u32) {
    let _ = baml_log::set_max_message_length(length as usize);
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
