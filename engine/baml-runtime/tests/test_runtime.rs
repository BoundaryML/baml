#[cfg(feature = "internal")]
mod internal_tests {
    use std::collections::HashMap;

    use baml_runtime::BamlRuntime;
    use baml_runtime::{PublicInterface, RuntimeContext};
    use indexmap::IndexMap;

    use wasm_bindgen_test::*;
    use wasm_logger;

    // #[tokio::test]
    #[wasm_bindgen_test]
    async fn test_call_function_wasm() -> Result<(), Box<dyn std::error::Error>> {
        wasm_logger::init(wasm_logger::Config::new(log::Level::Info));

        log::info!("Running test_call_function_wasm");
        // let directory = PathBuf::from("/Users/aaronvillalpando/Projects/baml/integ-tests/baml_src");
        // let files = vec![
        //     PathBuf::from(
        //         "/Users/aaronvillalpando/Projects/baml/integ-tests/baml_src/ExtractNames.baml",
        //     ),
        //     PathBuf::from(
        //         "/Users/aaronvillalpando/Projects/baml/integ-tests/baml_src/ExtractNames.baml",
        //     ),
        // ];
        let mut files = HashMap::new();
        files.insert(
            "main.baml",
            r##"
            generator lang_python {
              language python
              // This is where your baml_client will be generated
              // Usually the root of your source code relative to this file
              project_root "../"
              // This command is used by "baml test" to run tests
              // defined in the playground
              test_command "poetry run pytest"
              // This command is used by "baml update-client" to install
              // dependencies to your language environment
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
            "##,
        );
        log::info!("Files: {:?}", files);
        let ctx = RuntimeContext::new().add_env("OPENAI_API_KEY".into(), "API_KEY".to_string());
        log::info!("Context: {:?}", ctx);

        let runtime = BamlRuntime::from_file_content("baml_src", &files, &ctx);
        log::info!("Runtime:");

        // Replace the OPENAI_API_KEY value with the actual key
        let ctx =
            RuntimeContext::new().add_env("OPENAI_API_KEY".into(), "OPENAI_API_KEY".to_string());

        let mut params = IndexMap::new();

        params.insert(
            "input".to_string(),
            baml_types::BamlValue::String("Attention Is All You Need. Mark. Hello.".into()),
        );

        let (res, _) = runtime?
            .call_function("GetOrderInfo".to_string(), params, ctx)
            .await;

        assert!(res.is_ok(), "Result: {:#?}", res.err());
        log::info!("Result: {}", res.ok().unwrap());

        Ok(())
    }

    // #[wasm_bindgen_test]
    // async fn test_run_test() {
    //     let client = OpenAIClient::new();
    //     let response = client.call_llm("test".to_string()).await;
    //     // Add further assertions
    // }
}
