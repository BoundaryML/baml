// Run from the baml-schema-wasm folder with:
// RUST_LOG=info wasm-pack test --node --test test_log_collector
// and make sure to set rust-analyzer target in vscode settings to:   "rust-analyzer.cargo.target": "wasm32-unknown-unknown",

// Browser test command is:
// RUST_BACKTRACE=1 RUST_LOG=info wasm-pack test --chrome --headless --test test_log_collector -- --nocapture
#[cfg(target_arch = "wasm32")]
#[cfg(test)]
mod tests {

    use std::collections::HashMap;

    use baml_runtime::tracingv2::publisher::publisher::flush;
    // pub static GLOBAL_TRACE_STORAGE: Lazy<Mutex<u32>> = Lazy::new(|| Mutex::new(0));
    use baml_runtime::{tracingv2::storage::storage::BAML_TRACER, InternalRuntimeInterface};
    use baml_schema_build::runtime_wasm::WasmProject;
    use serde_wasm_bindgen::to_value;
    use wasm_bindgen::JsValue;
    use wasm_bindgen_test::*;

    // instantiate logger

    wasm_bindgen_test_configure!(run_in_browser);

    fn sample_baml_content() -> String {
        r##"


        class Email {
            subject string
            body string
            from_address string
        }

        enum OrderStatus {
            ORDERED
            SHIPPED
            DELIVERED
            CANCELLED
        }

        class OrderInfo {
            order_status OrderStatus
            tracking_number string?
            estimated_arrival_date string?
        }

        client<llm> GPT4Turbo {
            provider baml-openai-chat
            options {
                model gpt-4-1106-preview
                api_key env.OPENAI_API_KEY
            }
        }

        function GetOrderInfo(input: string) -> OrderInfo {
            client GPT4Turbo
            prompt #"
            Extract this info from the email in JSON format:
            Before you output the JSON, please explain your
            reasoning step-by-step. Here is an example on how to do this:
            'If we think step by step we can see that ...
             therefore the output JSON is:
            {
              ... the json schema ...
            }'
          "#
        }
        "##
        .to_string()
    }

    // TODO: add a flag to run_test that's debug, to attach a collector on every test. The collector can be created inside run_test. Make the collector call track_function(..)
    // Then we can check the baml_tracer events since they'll be kept.
    #[wasm_bindgen_test]
    async fn test_run_tests() {
        wasm_logger::init(wasm_logger::Config::new(log::Level::Info));
        let sample_baml_content = r##"
    function Func(name: string ) -> string {
            client "openai/gpt-4o"
            prompt #"
            Return the name of {{name}}
            "#
    }

    test One {
        functions [Func]
        args {
            name "john"
        }
    }


            "##;
        let mut files = HashMap::new();
        files.insert("error.baml".to_string(), sample_baml_content.to_string());
        let files_js = to_value(&files).unwrap();
        let project = WasmProject::new("baml_src", files_js)
            .map_err(JsValue::from)
            .unwrap();

        let env_vars = [("OPENAI_API_KEY", "12345")]
            .iter()
            .cloned()
            .collect::<HashMap<_, _>>();
        let env_vars_js = to_value(&env_vars).unwrap();
        let undefined_js = JsValue::UNDEFINED;

        let mut current_runtime = project.runtime(env_vars_js.clone(), undefined_js).unwrap();

        let diagnostics = project.diagnostics(&current_runtime);
        assert!(diagnostics.errors().is_empty());

        let functions = current_runtime.list_functions();

        for f in functions.iter() {
            f.run_test(
                &mut current_runtime,
                "One".to_string(),
                js_sys::Function::new_no_args(""),
                js_sys::Function::new_no_args(""),
                env_vars_js.clone().into(),
                None,
            )
            .await;
        }

        let events = BAML_TRACER.lock().unwrap().events();
        log::info!("Events {events:#?}");
        // TODO: this makes the test hang on Node, but not in browser.
        flush().await;

        log::info!("done!!")
    }
}
