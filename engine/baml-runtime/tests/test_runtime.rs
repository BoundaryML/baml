// run these tests with:
// RUST_LOG=info cargo test test_call_function_unions1 --no-default-features --features "internal" -- --nocapture
// need to fix the tokio runtime getting closed but at least you can log things.
// #[cfg(feature = "internal")]
mod internal_tests {
    use std::any;
    use std::collections::HashMap;

    use baml_runtime::BamlRuntime;
    use std::sync::Once;

    use baml_runtime::internal::llm_client::orchestrator::OrchestrationScope;
    use baml_runtime::InternalRuntimeInterface;
    use baml_types::BamlValue;

    use baml_runtime::{
        internal::llm_client::LLMResponse, DiagnosticsError, IRHelper, RenderedPrompt,
    };

    use wasm_bindgen_test::*;
    use wasm_logger;

    static INIT: Once = Once::new();

    // #[tokio::test]
    // // #[wasm_bindgen_test]
    // async fn test_call_function() -> Result<(), Box<dyn std::error::Error>> {
    //     // wasm_logger::init(wasm_logger::Config::new(log::Level::Info));

    //     log::info!("Running test_call_function");
    //     // let directory = PathBuf::from("/Users/aaronvillalpando/Projects/baml/integ-tests/baml_src");
    //     // let files = vec![
    //     //     PathBuf::from(
    //     //         "/Users/aaronvillalpando/Projects/baml/integ-tests/baml_src/ExtractNames.baml",
    //     //     ),
    //     //     PathBuf::from(
    //     //         "/Users/aaronvillalpando/Projects/baml/integ-tests/baml_src/ExtractNames.baml",
    //     //     ),
    //     // ];
    //     let mut files = HashMap::new();
    //     files.insert(
    //         "main.baml",
    //         r##"
    //         generator lang_python {

    //         }

    //         class Email {
    //             subject string
    //             body string
    //             from_address string
    //         }

    //         enum OrderStatus {
    //             ORDERED
    //             SHIPPED
    //             DELIVERED
    //             CANCELLED
    //         }

    //         class OrderInfo {
    //             order_status OrderStatus
    //             tracking_number string?
    //             estimated_arrival_date string?
    //         }

    //         client<llm> GPT4Turbo {
    //           provider baml-openai-chat
    //           options {
    //             model gpt-4-1106-preview
    //             api_key env.OPENAI_API_KEY
    //           }
    //         }

    //         function GetOrderInfo(input: string) -> OrderInfo {
    //           client GPT4Turbo
    //           prompt #"

    //             Extract this info from the email in JSON format:

    //             Before you output the JSON, please explain your
    //             reasoning step-by-step. Here is an example on how to do this:
    //             'If we think step by step we can see that ...
    //              therefore the output JSON is:
    //             {
    //               ... the json schema ...
    //             }'
    //           "#
    //         }
    //         "##,
    //     );
    //     log::info!("Files: {:?}", files);

    //     let runtime = BamlRuntime::from_file_content(
    //         "baml_src",
    //         &files,
    //         [("OPENAI_API_KEY", "OPENAI_API_KEY")].into(),
    //     )?;
    //     log::info!("Runtime:");

    //     let params = [(
    //         "input".into(),
    //         baml_types::BamlValue::String("Attention Is All You Need. Mark. Hello.".into()),
    //     )]
    //     .into_iter()
    //     .collect();

    //     let ctx = runtime.create_ctx_manager(BamlValue::String("test".to_string()), None);
    //     let (res, _) = runtime
    //         .call_function("GetOrderInfo".to_string(), &params, &ctx, None, None)
    //         .await;

    //     // runtime.get_test_params(function_name, test_name, ctx);

    //     // runtime.internal().render_prompt(function_name, ctx, params, node_index)

    //     assert!(res.is_ok(), "Result: {:#?}", res.err());

    //     Ok(())
    // }

    #[test]
    fn test_call_function2() -> Result<(), Box<dyn std::error::Error>> {
        INIT.call_once(|| {
            env_logger::init();
        });
        log::info!("Running test_call_function");

        let mut files = HashMap::new();
        files.insert(
            "main.baml",
            r##"
          
          class Education {
            school string | null @description(#"
              111
            "#)
            degree string @description(#"
              2222222
            "#)
          }

          client<llm> GPT4Turbo {
            provider baml-openai-chat
            options {
              model gpt-4-1106-preview
              api_key env.OPENAI_API_KEY
            }
          }
          
          
          function Extract(input: string) -> Education {
            client GPT4Turbo
            prompt #"
          
              {{ ctx.output_format }}
            "#
          }  

          test Test {
            functions [Extract]
            args {
              input "hi"
            }
          }
          "##,
        );

        let function_name = "Extract";
        let test_name = "Test";

        let runtime = BamlRuntime::from_file_content(
            "baml_src",
            &files,
            [("OPENAI_API_KEY", "OPENAI_API_KEY")].into(),
        )?;
        log::info!("Runtime:");

        let missing_env_vars = runtime.internal().ir().required_env_vars();

        let ctx = runtime
            .create_ctx_manager(BamlValue::String("test".to_string()), None)
            .create_ctx_with_default(missing_env_vars.iter());

        let params = runtime.get_test_params(function_name, test_name, &ctx)?;

        let render_prompt_future =
            runtime
                .internal()
                .render_prompt(&function_name, &ctx, &params, Some(0));

        let (prompt, scope, _) = runtime.async_runtime.block_on(render_prompt_future)?;

        log::info!("Prompt: {:#?}", prompt);

        Ok(())
    }

    #[test]
    fn test_call_function_unions1() -> Result<(), Box<dyn std::error::Error>> {
        INIT.call_once(|| {
            env_logger::init();
        });
        log::info!("Running test_call_function");

        let mut files = HashMap::new();
        files.insert(
            "main.baml",
            r##"
          
          class Education {
            // school string | (null | int) @description(#"
            //   111
            // "#)
            // degree string @description(#"
            //   2222222
            // "#)
            something (string | int) @description(#"
              333333
            "#)
          }

          client<llm> GPT4Turbo {
            provider baml-openai-chat
            options {
              model gpt-4-1106-preview
              api_key env.OPENAI_API_KEY
            }
          }
          
          
          function Extract(input: string) -> Education {
            client GPT4Turbo
            prompt #"
          
              {{ ctx.output_format }}
            "#
          }  

          test Test {
            functions [Extract]
            args {
              input "hi"
            }
          }
          "##,
        );

        let function_name = "Extract";
        let test_name = "Test";

        let runtime = BamlRuntime::from_file_content(
            "baml_src",
            &files,
            [("OPENAI_API_KEY", "OPENAI_API_KEY")].into(),
        )?;
        log::info!("Runtime:");

        let missing_env_vars = runtime.internal().ir().required_env_vars();

        let ctx = runtime
            .create_ctx_manager(BamlValue::String("test".to_string()), None)
            .create_ctx_with_default(missing_env_vars.iter());

        let params = runtime.get_test_params(function_name, test_name, &ctx)?;

        let render_prompt_future =
            runtime
                .internal()
                .render_prompt(&function_name, &ctx, &params, Some(0));

        let (prompt, scope, _) = runtime.async_runtime.block_on(render_prompt_future)?;

        // let prompt = render_prompt_future
        //     .await
        //     .as_ref()
        //     .map(|(p, scope)| p)
        //     .map_err(|e| anyhow::anyhow!("Error rendering prompt: {:#?}", e))?;

        log::info!("Prompt: {:#?}", prompt);

        Ok(())
    }

    fn make_test_runtime(file_content: &str) -> anyhow::Result<BamlRuntime> {
        let mut files = HashMap::new();
        files.insert("main.baml", file_content);
        BamlRuntime::from_file_content(
            "baml_src",
            &files,
            [("OPENAI_API_KEY", "OPENAI_API_KEY")].into(),
        )
    }

    #[test]
    fn test_with_image_union() -> anyhow::Result<()> {
        let runtime = make_test_runtime(
            r##"
class Receipt {
  establishment_name string
  date string @description("ISO8601 formatted date")
  total int @description("The total amount of the receipt")
  currency string
  items Item[] @description("The items on the receipt")
}

class Item {
  name string
  price float
  quantity int @description("If not specified, assume 1")
}
 
// This is our LLM function we can call in Python or Typescript
// the receipt can be an image OR text here!
function ExtractReceipt(receipt: image | string) -> Receipt {
  // see clients.baml
  client "openai/gpt-4o"
  prompt #"
    {# start a user message #}
    {{ _.role("user") }}

    Extract info from this receipt:
    {{ receipt }}

    {# special macro to print the output schema instructions. #}
    {{ ctx.output_format }}
  "#
}

// Test when the input is an image
test ImageReceiptTest {
  functions [ExtractReceipt]
  args {
    receipt { url "https://i.redd.it/adzt4bz4llfc1.jpeg"}
  }
}
        "##,
        )?;

        let missing_env_vars = runtime.internal().ir().required_env_vars();

        let ctx = runtime
            .create_ctx_manager(BamlValue::String("test".to_string()), None)
            .create_ctx_with_default(missing_env_vars.iter());

        let function_name = "ExtractReceipt";
        let test_name = "ImageReceiptTest";
        let params = runtime.get_test_params(function_name, test_name, &ctx)?;
        let render_prompt_future =
            runtime
                .internal()
                .render_prompt(function_name, &ctx, &params, None);
        let (prompt, scope, _) = runtime.async_runtime.block_on(render_prompt_future)?;

        Ok(())
    }
}
