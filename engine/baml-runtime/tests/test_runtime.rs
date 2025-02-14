// run these tests with:
// RUST_LOG=info cargo test test_call_function_unions1 --no-default-features --features "internal" -- --nocapture
// need to fix the tokio runtime getting closed but at least you can log things.
// #[cfg(feature = "internal")]
mod internal_tests {
    use std::any;
    use std::collections::HashMap;

    use baml_runtime::BamlRuntime;
    use std::sync::Once;

    // use baml_runtime::internal::llm_client::orchestrator::OrchestrationScope;
    use baml_runtime::InternalRuntimeInterface;
    use baml_types::BamlValue;

    use baml_runtime::{
        internal::llm_client::LLMResponse, DiagnosticsError, IRHelper, RenderedPrompt,
    };

    use wasm_bindgen_test::*;

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
            .create_ctx_with_default();

        let params = runtime.get_test_params(function_name, test_name, &ctx, true)?;

        let render_prompt_future =
            runtime
                .internal()
                .render_prompt(function_name, &ctx, &params, Some(0));

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

        let ctx = runtime
            .create_ctx_manager(BamlValue::String("test".to_string()), None)
            .create_ctx_with_default();

        let params = runtime.get_test_params(function_name, test_name, &ctx, true)?;

        let render_prompt_future =
            runtime
                .internal()
                .render_prompt(function_name, &ctx, &params, Some(0));

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
            [(
                "OPENAI_API_KEY",
                // Use this to test with a real API key.
                // option_env!("OPENAI_API_KEY").unwrap_or("NO_API_KEY"),
                "OPENAI_API_KEY",
            )]
            .into(),
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

        let ctx = runtime
            .create_ctx_manager(BamlValue::String("test".to_string()), None)
            .create_ctx_with_default();

        let function_name = "ExtractReceipt";
        let test_name = "ImageReceiptTest";
        let params = runtime.get_test_params(function_name, test_name, &ctx, true)?;
        let render_prompt_future =
            runtime
                .internal()
                .render_prompt(function_name, &ctx, &params, None);
        let (prompt, scope, _) = runtime.async_runtime.block_on(render_prompt_future)?;

        Ok(())
    }

    #[test]
    fn test_literals() -> anyhow::Result<()> {
        let runtime = make_test_runtime(
            r##"
// My first tool
class GetWeather {
  name "weather"
  // any other params
}

class CheckCalendar {
  name "check_calendar"
  // any other params
}

class GetDelivery {
  name "get_delivery_date" @description(#"Get the delivery date for a customer's order. Call this whenever you need to know the delivery date, for example when a customer asks 'Where is my package'"#)
  order_id string
}

class Response {
  name "reply_to_user"
  response string
}

class Message {
  role "user" | "assistant"
  message string
}

function Bot(convo: Message[]) -> GetWeather | CheckCalendar | GetDelivery | Response {
  client "openai/gpt-4o"
  prompt #"
    You are a helpful assistant.
    {{ ctx.output_format }}

    {% for m in convo %}
    {{ _.role(m.role) }}
    {{ m.message }}
    {% endfor %}
  "#
}

test TestName {
  functions [Bot]
  args {
    convo [
      {
        role "user"
        message "Hi, can you tell me the delivery date for my order?"
      }
    {
      role "assistant"
      message "Hi there! I can help with that. Can you please provide your order ID?"
    }
    {
      role "user"
      message "i think it is order_12345"
    }
    ]
  }
}
        "##,
        )?;

        let ctx = runtime
            .create_ctx_manager(BamlValue::String("test".to_string()), None)
            .create_ctx_with_default();

        let function_name = "Bot";
        let test_name = "TestName";
        let params = runtime.get_test_params(function_name, test_name, &ctx, true)?;
        let render_prompt_future =
            runtime
                .internal()
                .render_prompt(function_name, &ctx, &params, None);
        let (prompt, scope, _) = runtime.async_runtime.block_on(render_prompt_future)?;

        Ok(())
    }

    #[test]
    fn test_recursive_types() -> anyhow::Result<()> {
        let runtime = make_test_runtime(
            r##"
class Tree {
  data int
  children Forest
}

class Forest {
  trees Tree[]
}

class BinaryNode {
  data int
  left BinaryNode?
  right BinaryNode?
}

function BuildTree(input: BinaryNode) -> Tree {
  client "openai/gpt-4o"
  prompt #"
    Given the input binary tree, transform it into a generic tree using the given schema.

    INPUT:
    {{ input }}

    {{ ctx.output_format }}
  "#
}

test TestTree {
  functions [BuildTree]
  args {
    input {
      data 2
      left {
        data 1
        left null
        right null
      }
      right {
        data 3
        left null
        right null
      }
    }
  }
}
        "##,
        )?;

        let ctx = runtime
            .create_ctx_manager(BamlValue::String("test".to_string()), None)
            .create_ctx_with_default();

        let function_name = "BuildTree";
        let test_name = "TestTree";
        let params = runtime.get_test_params(function_name, test_name, &ctx, true)?;
        let render_prompt_future =
            runtime
                .internal()
                .render_prompt(function_name, &ctx, &params, None);
        let (prompt, scope, _) = runtime.async_runtime.block_on(render_prompt_future)?;

        Ok(())
    }

    #[test]
    fn test_constrained_type_alias() -> anyhow::Result<()> {
        let runtime = make_test_runtime(
            r##"
class Foo2 {
    bar int
    baz string
    sub Subthing @assert( {{ this.bar == 10}} ) | null
}

class Foo3 {
    bar int
    baz string
    sub Foo3 | null
}

type Subthing = Foo2 @assert( {{ this.bar == 10 }})

function RunFoo2(input: Foo3) -> Foo2 {
    client "openai/gpt-4o"
    prompt #"Generate a Foo2 wrapping 30. Use {{ input }}.
       {{ ctx.output_format }}
    "#
}

test RunFoo2Test {
  functions [RunFoo2]
  args {
    input {
      bar 30
      baz "hello"
      sub null
    }
  }
}
        "##,
        )?;

        let ctx = runtime
            .create_ctx_manager(BamlValue::String("test".to_string()), None)
            .create_ctx_with_default();

        let function_name = "RunFoo2";
        let test_name = "RunFoo2Test";
        let params = runtime.get_test_params(function_name, test_name, &ctx, true)?;
        let render_prompt_future =
            runtime
                .internal()
                .render_prompt(function_name, &ctx, &params, None);
        let (prompt, scope, _) = runtime.async_runtime.block_on(render_prompt_future)?;

        Ok(())
    }

    #[test]
    fn test_recursive_alias_cycle() -> anyhow::Result<()> {
        let runtime = make_test_runtime(
            r##"
type RecAliasOne = RecAliasTwo
type RecAliasTwo = RecAliasThree
type RecAliasThree = RecAliasOne[]

function RecursiveAliasCycle(input: RecAliasOne) -> RecAliasOne {
    client "openai/gpt-4o"
    prompt r#"
      Return the given value:

      {{ input }}

      {{ ctx.output_format }}
    "#
}

test RecursiveAliasCycle {
  functions [RecursiveAliasCycle]
  args {
    input [
      []
      []
      [[], []]
    ]
  }
}
        "##,
        )?;

        let ctx = runtime
            .create_ctx_manager(BamlValue::String("test".to_string()), None)
            .create_ctx_with_default();

        let function_name = "RecursiveAliasCycle";
        let test_name = "RecursiveAliasCycle";
        let params = runtime.get_test_params(function_name, test_name, &ctx, true)?;
        let render_prompt_future =
            runtime
                .internal()
                .render_prompt(function_name, &ctx, &params, None);
        let (prompt, scope, _) = runtime.async_runtime.block_on(render_prompt_future)?;

        Ok(())
    }

    struct TypeBuilderBlockTest {
        function_name: &'static str,
        test_name: &'static str,
        baml: &'static str,
    }

    fn run_type_builder_block_test(
        TypeBuilderBlockTest {
            function_name,
            test_name,
            baml,
        }: TypeBuilderBlockTest,
    ) -> anyhow::Result<()> {
        // Use this and RUST_LOG=debug to see the rendered prompt in the
        // terminal.
        env_logger::init();

        let runtime = make_test_runtime(baml)?;

        let ctx = runtime.create_ctx_manager(BamlValue::String("test".to_string()), None);

        let run_test_future = runtime.run_test(function_name, test_name, &ctx, Some(|r| {}));
        let (res, span) = runtime.async_runtime.block_on(run_test_future);

        Ok(())
    }

    #[test]
    fn test_type_builder_block_with_dynamic_class() -> anyhow::Result<()> {
        run_type_builder_block_test(TypeBuilderBlockTest {
            function_name: "ExtractResume",
            test_name: "ReturnDynamicClassTest",
            baml: r##"
                class Resume {
                  name string
                  education Education[]
                  skills string[]
                  @@dynamic
                }

                class Education {
                  school string
                  degree string
                  year int
                }

                function ExtractResume(from_text: string) -> Resume {
                  client "openai/gpt-4o"
                  prompt #"
                    Extract the resume information from the given text.

                    {{ from_text }}

                    {{ ctx.output_format }}
                  "#
                }

                test ReturnDynamicClassTest {
                  functions [ExtractResume]
                  type_builder {
                    class Experience {
                      title string
                      company string
                      start_date string
                      end_date string
                    }

                    dynamic Resume {
                      experience Experience[]
                    }
                  }
                  args {
                    from_text #"
                      John Doe

                      Education
                      - University of California, Berkeley, B.S. in Computer Science, 2020

                      Experience
                      - Software Engineer, Boundary, Sep 2022 - Sep 2023

                      Skills
                      - Python
                      - Java
                    "#
                  }
                }
            "##,
        })
    }

    #[test]
    fn test_type_builder_block_with_dynamic_enum() -> anyhow::Result<()> {
        run_type_builder_block_test(TypeBuilderBlockTest {
            function_name: "ClassifyMessage",
            test_name: "ReturnDynamicEnumTest",
            baml: r##"
                enum Category {
                  Refund
                  CancelOrder
                  AccountIssue
                  @@dynamic
                }

                // Function that returns the dynamic enum.
                function ClassifyMessage(message: string) -> Category {
                  client "openai/gpt-4o"
                  prompt #"
                    Classify this message:

                    {{ message }}

                    {{ ctx.output_format }}
                  "#
                }

                test ReturnDynamicEnumTest {
                  functions [ClassifyMessage]
                  type_builder {
                    dynamic Category {
                      Question
                      Feedback
                      TechnicalSupport
                    }
                  }
                  args {
                    message "I think the product is great!"
                  }
                }
            "##,
        })
    }

    #[test]
    fn test_type_builder_block_mixed_enums_and_classes() -> anyhow::Result<()> {
        run_type_builder_block_test(TypeBuilderBlockTest {
            function_name: "ExtractResume",
            test_name: "ReturnDynamicClassTest",
            baml: r##"
              class Resume {
                name string
                education Education[]
                skills string[]
                @@dynamic
              }

              class Education {
                school string
                degree string
                year int
              }

              enum Role {
                SoftwareEngineer
                DataScientist
                @@dynamic
              }

              function ExtractResume(from_text: string) -> Resume {
                client "openai/gpt-4o"
                prompt #"
                  Extract the resume information from the given text.

                  {{ from_text }}

                  {{ ctx.output_format }}
                "#
              }

              test ReturnDynamicClassTest {
                functions [ExtractResume]
                type_builder {
                  class Experience {
                    title string
                    company string
                    start_date string
                    end_date string
                  }

                  enum Industry {
                    Tech
                    Finance
                    Healthcare
                  }

                  dynamic Role {
                    ProductManager
                    Sales
                  }

                  dynamic Resume {
                    experience Experience[]
                    role Role
                    industry Industry
                  }
                }
                args {
                  from_text #"
                    John Doe

                    Education
                    - University of California, Berkeley, B.S. in Computer Science, 2020

                    Experience
                    - Software Engineer, Boundary, Sep 2022 - Sep 2023

                    Skills
                    - Python
                    - Java
                  "#
                }
              }
          "##,
        })
    }

    #[test]
    fn test_type_builder_block_type_aliases() -> anyhow::Result<()> {
        run_type_builder_block_test(TypeBuilderBlockTest {
            function_name: "ExtractResume",
            test_name: "ReturnDynamicClassTest",
            baml: r##"
                class Resume {
                  name string
                  education Education[]
                  skills string[]
                  @@dynamic
                }

                class Education {
                  school string
                  degree string
                  year int
                }

                function ExtractResume(from_text: string) -> Resume {
                  client "openai/gpt-4o"
                  prompt #"
                    Extract the resume information from the given text.

                    {{ from_text }}

                    {{ ctx.output_format }}
                  "#
                }

                test ReturnDynamicClassTest {
                  functions [ExtractResume]
                  type_builder {
                    class Experience {
                      title string
                      company string
                      start_date string
                      end_date string
                    }

                    type ExpAlias = Experience

                    dynamic Resume {
                      experience ExpAlias
                    }
                  }
                  args {
                    from_text #"
                      John Doe

                      Education
                      - University of California, Berkeley, B.S. in Computer Science, 2020

                      Experience
                      - Software Engineer, Boundary, Sep 2022 - Sep 2023

                      Skills
                      - Python
                      - Java
                    "#
                  }
                }
            "##,
        })
    }

    #[test]
    fn test_type_builder_block_recursive_type_aliases() -> anyhow::Result<()> {
        run_type_builder_block_test(TypeBuilderBlockTest {
            function_name: "ExtractResume",
            test_name: "ReturnDynamicClassTest",
            baml: r##"
                class Resume {
                  name string
                  education Education[]
                  skills string[]
                  @@dynamic
                }

                class Education {
                  school string
                  degree string
                  year int
                }

                class WhatTheFuck {
                  j JsonValue
                }

                type JsonValue = int | float | bool | string | JsonValue[] | map<string, JsonValue>

                function ExtractResume(from_text: string) -> Resume {
                  client "openai/gpt-4o"
                  prompt #"
                    Extract the resume information from the given text.

                    {{ from_text }}

                    {{ ctx.output_format }}
                  "#
                }

                test ReturnDynamicClassTest {
                  functions [ExtractResume]
                  type_builder {
                    type JSON = int | float | bool | string | JSON[] | map<string, JSON>

                    dynamic Resume {
                      experience JSON
                    }
                  }
                  args {
                    from_text #"
                      John Doe

                      Education
                      - University of California, Berkeley, B.S. in Computer Science, 2020

                      Experience
                      - Software Engineer, Boundary, Sep 2022 - Sep 2023

                      Skills
                      - Python
                      - Java
                    "#
                  }
                }
            "##,
        })
    }
}
