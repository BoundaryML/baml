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
    // Check if JSON logging is enabled
    let use_json = match std::env::var("BAML_LOG_JSON") {
        Ok(val) => val.trim().eq_ignore_ascii_case("true") || val.trim() == "1",
        Err(_) => false,
    };

    if use_json {
        // JSON formatting
        tracing_subscriber::fmt()
            .with_target(false)
            .with_file(false)
            .with_line_number(false)
            .json()
            .with_env_filter(
                EnvFilter::try_from_env("BAML_LOG").unwrap_or_else(|_| EnvFilter::new("info")),
            )
            .flatten_event(true)
            .with_current_span(false)
            .with_span_list(false)
            .init();
    } else {
        // Regular formatting
        if let Err(e) = env_logger::try_init_from_env(
            env_logger::Env::new()
                .filter("BAML_LOG")
                .write_style("BAML_LOG_STYLE"),
        ) {
            eprintln!("Failed to initialize BAML logger: {:#}", e);
        }
    }
}
