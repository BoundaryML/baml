// Run from the baml-schema-wasm folder with:
// RUST_LOG=info wasm-pack test --node --test test_log_collector
// and make sure to set rust-analyzer target in vscode settings to:   "rust-analyzer.cargo.target": "wasm32-unknown-unknown",

// Browser test command is:
// RUST_BACKTRACE=1 RUST_LOG=info wasm-pack test --chrome --headless --test test_log_collector -- --nocapture
// #[cfg(target_arch = "wasm32")]
// #[cfg(test)]
// mod tests {

//     use core::time;
//     use core::time::Duration;
//     use once_cell::sync::Lazy;
//     use std::sync::Mutex;
//     use wasmtimer::tokio::*;

//     // pub static GLOBAL_TRACE_STORAGE: Lazy<Mutex<u32>> = Lazy::new(|| Mutex::new(0));
//     use baml_runtime::{
//         tracingv2::storage::storage::{Collector, BAML_TRACER},
//         InternalRuntimeInterface,
//     };
//     use std::collections::HashMap;

//     use baml_schema_build::runtime_wasm::{WasmProject, WasmRuntime, WasmFunctionResponse};

//     use baml_runtime::{tracingv2::publisher::publisher::flush, BamlRuntime, RuntimeContext};
//     use serde_wasm_bindgen::to_value;
//     use wasm_bindgen::JsValue;
//     use wasm_bindgen_test::*;
//     use wasm_logger;

//     // instantiate logger

//     wasm_bindgen_test_configure!(run_in_browser);

//     use futures_timer::Delay;
//     use wasm_bindgen::prelude::*;
//     use wasmtimer::tokio::{interval, sleep, timeout};
//     use web_sys::console::log_1;

//     use futures::future::join_all;
//     use reqwest::Client;
//     use std::time::Instant;
//     use wasm_bindgen_futures::spawn_local;
//     use web_sys::console;


// #[wasm_bindgen]
// #[derive(Debug)]
// pub struct WasmTestResponse {
//     test_response: anyhow::Result<baml_runtime::TestResponse>,
//     span: Option<uuid::Uuid>,
//     tracing_project_id: Option<String>,
// }

//     async fn sample_runner(
//         rt: &mut WasmRuntime,
//         test_name: String,
//     ) -> Result<WasmTestResponse, JsValue> {
//         let function_name = "test_func".to_string();

//         // Create the closure to handle partial responses:
//         let cb = Box::new(move |r| {
//             let this = JsValue::NULL;
//             let res = WasmFunctionResponse {
//                 function_response: r,
//             }
//             .into();
//             on_partial_response.call1(&this, &res).unwrap();
//         });

//         // Create your evaluation context, etc.
//         // let ctx = rt.create_ctx_manager(
//         //     BamlValue::String("wasm".to_string()),
//         //     js_fn_to_baml_src_reader(get_baml_src_cb),
//         // );

//         // // Now pass collector_arc to your runtime's run_test
//         // let (test_response, span) = rt
//         //     .run_test(&function_name, &test_name, &ctx, Some(cb), None)
//         //     .await;

//         // log::info!("test_response: {:#?}", test_response);

//         Ok(WasmTestResponse {
//             test_response,
//             span,
//             tracing_project_id: rt.env_vars().get("BOUNDARY_PROJECT_ID").cloned(),
//         })
//     }

//     #[wasm_bindgen_test(async)]
//     async fn test_sample_runner() {

//         wasm_logger::init(wasm_logger::Config::new(log::Level::Info));
//         let sample_baml_content = r##"
//     function Func(name: string ) -> string {
//             client "openai/gpt-4o"
//             prompt #"
//             Return the name of {{name}}
//             "#
//     }

//     test One {
//         functions [Func]
//         args {
//             name "john"
//         }
//     }


//             "##;
//         let mut files = HashMap::new();
//         files.insert("error.baml".to_string(), sample_baml_content.to_string());
//         let files_js = to_value(&files).unwrap();
//         let project = WasmProject::new("baml_src", files_js)
//             .map_err(JsValue::from)
//             .unwrap();

//         let env_vars = [("OPENAI_API_KEY", "12345")]
//             .iter()
//             .cloned()
//             .collect::<HashMap<_, _>>();
//         let env_vars_js = to_value(&env_vars).unwrap();

//         let mut current_runtime = project.runtime(env_vars_js).map_err(JsValue::from).unwrap();

//         let diagnostics = project.diagnostics(&current_runtime);
//         assert!(diagnostics.errors().is_empty());

//         let functions = current_runtime.list_functions();

//         sample_runner(&mut current_runtime,
//             "test_case".to_string());
        
//     }
// }
