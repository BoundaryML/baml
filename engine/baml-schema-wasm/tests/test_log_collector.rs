// Run from the baml-schema-wasm folder with:
// wasm-pack test --node
// and make sure to set rust-analyzer target in vscode settings to:   "rust-analyzer.cargo.target": "wasm32-unknown-unknown",
#[cfg(target_arch = "wasm32")]
#[cfg(test)]
mod tests {

    use tokio::sync::Mutex;

    pub static GLOBAL_TRACE_STORAGE: Lazy<Mutex<u32>> = Lazy::new(|| Mutex::new(0));
    use std::collections::HashMap;

    use baml_schema_build::runtime_wasm::{WasmProject, WasmRuntime};

    use baml_runtime::{BamlRuntime, RuntimeContext};
    use serde_wasm_bindgen::to_value;
    use wasm_bindgen::JsValue;
    use wasm_bindgen_test::*;
    use wasm_logger;

    // instantiate logger

    // wasm_bindgen_test_configure!(run_in_browser);

    /// Sample BAML content for testing.
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

    #[wasm_bindgen_test]
    fn test_run_tests() {
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

test Two {
    functions [Func]
    args {
        name "jane"
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

        let current_runtime = project.runtime(env_vars_js).map_err(JsValue::from).unwrap();

        let diagnostics = project.diagnostics(&current_runtime);
        let functions = current_runtime.list_functions();
        functions.iter().for_each(|f| {
            log::info!("function: {:#?}", f);
            f.test_cases.iter().for_each(|t| {
                log::info!("test case: {:#?}", t);
            });
            f.run_test(&mut current_runtime, "One".to_string(), None, None);
        });

        assert!(diagnostics.errors().is_empty());
    }
}
