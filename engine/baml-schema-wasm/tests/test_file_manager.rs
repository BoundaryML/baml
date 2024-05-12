#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use baml_schema_build::runtime_wasm::{WasmProject, WasmRuntime, WasmRuntimeContext};

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
        generator lang_python {
            language python
            project_root "../"
            test_command "poetry run pytest"
            install_command "poetry add baml@latest"
            package_version_command "poetry show baml"
        }
        
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

    /// Tests the `new` constructor for successful creation with BAML content.
    #[wasm_bindgen_test]
    fn test_new_project_with_baml_content() {
        wasm_logger::init(wasm_logger::Config::new(log::Level::Info));
        let mut files = HashMap::new();
        files.insert("main.baml".to_string(), sample_baml_content());
        let files_js = to_value(&files).unwrap();
        let project = WasmProject::new("baml_src", files_js);
        assert!(project.is_ok());
    }

    /// Tests retrieving BAML files correctly with `files` method.
    #[wasm_bindgen_test]
    fn test_files_method_with_baml() {
        let mut files = HashMap::new();
        files.insert("main.baml".to_string(), sample_baml_content());
        let files_js = to_value(&files).unwrap();
        let project = WasmProject::new("baml_src", files_js)
            .map_err(JsValue::from)
            .unwrap();
        assert_eq!(project.files().len(), 1);
    }

    /// Tests updating and removing BAML files.
    #[wasm_bindgen_test]
    fn test_update_and_remove_baml_file() {
        wasm_logger::init(wasm_logger::Config::new(log::Level::Info));

        let mut files = HashMap::new();
        files.insert("main.baml".to_string(), sample_baml_content());
        let files_js = to_value(&files).unwrap();
        let mut project = WasmProject::new("baml_src", files_js)
            .map_err(JsValue::from)
            .unwrap();

        // Update BAML file
        let updated_content = "// A COMMENT".to_string();
        project.update_file("main.baml", Some(updated_content.clone()));
        let project_files = project.files();
        assert!(project
            .files()
            .contains(&"main.bamlBAML_PATH_SPLTTER// A COMMENT".to_string()));

        // Remove BAML file
        project.update_file("main.baml", None);
        assert!(project.files().is_empty());
    }

    #[wasm_bindgen_test]
    fn test_diagnostics_no_errors() {
        wasm_logger::init(wasm_logger::Config::new(log::Level::Info));

        let mut files = HashMap::new();
        files.insert("error.baml".to_string(), sample_baml_content());
        let files_js = to_value(&files).unwrap();
        let project = WasmProject::new("baml_src", files_js)
            .map_err(JsValue::from)
            .unwrap();

        let wasm_runtime_ctx = WasmRuntimeContext::new();
        let current_runtime = project
            .runtime(&wasm_runtime_ctx)
            .map_err(JsValue::from)
            .unwrap();
        let diagnostics = project.diagnostics(&current_runtime);

        assert!(diagnostics.errors().is_empty());
    }
}
