// run these tests with:
// RUST_LOG=info cargo test test_call_function_unions1 --no-default-features --features "internal" -- --nocapture
// need to fix the tokio runtime getting closed but at least you can log things.
#[cfg(feature = "internal")]
#[cfg(not(feature = "skip-integ-tests"))]
mod internal_tests {
    use std::{
        any,
        collections::HashMap,
        sync::{Arc, Once},
    };

    use baml_ids::FunctionCallId;
    // use baml_runtime::internal::llm_client::orchestrator::OrchestrationScope;
    use baml_runtime::{
        internal::llm_client::LLMResponse, BamlRuntime, DiagnosticsError, IRHelper, RenderedPrompt,
    };
    use baml_runtime::{InternalRuntimeInterface, TripWire};
    use baml_types::BamlValue;
    use internal_baml_core::FeatureFlags;
    use wasm_bindgen_test::*;

    #[tokio::test]
    // #[wasm_bindgen_test]
    async fn test_call_function() -> Result<(), Box<dyn std::error::Error>> {
        // wasm_logger::init(wasm_logger::Config::new(log::Level::Info));

        log::info!("Running test_call_function");
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
        log::info!("Files: {files:?}");

        let runtime = BamlRuntime::from_file_content(
            "baml_src",
            &files,
            [("OPENAI_API_KEY", "OPENAI_API_KEY")].into(),
            FeatureFlags::new(),
        )?;
        log::info!("Runtime:");

        let params = [(
            "input".into(),
            baml_types::BamlValue::String("Attention Is All You Need. Mark. Hello.".into()),
        )]
        .into_iter()
        .collect();

        let ctx = runtime.create_ctx_manager(BamlValue::String("test".to_string()), None);
        let (res, _) = runtime
            .call_function(
                "GetOrderInfo".to_string(),
                &params,
                &ctx,
                None,
                None,
                None,
                HashMap::new(),
                None,
                TripWire::new(None),
            )
            .await;

        // runtime.get_test_params(function_name, test_name, ctx);

        // runtime.render_prompt(function_name, ctx, params, node_index)

        assert!(res.is_ok(), "Result: {:#?}", res.err());

        Ok(())
    }

    #[test_log::test]
    fn test_call_function2() -> Result<(), Box<dyn std::error::Error>> {
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
            FeatureFlags::new(),
        )?;
        log::info!("Runtime:");

        let missing_env_vars = runtime.ir.required_env_vars();

        let ctx = runtime
            .create_ctx_manager(BamlValue::String("test".to_string()), None)
            .create_ctx_with_default();

        let params = runtime.get_test_params(function_name, test_name, &ctx, true)?;

        let render_prompt_future = runtime.render_prompt(function_name, &ctx, &params, Some(0));

        let (prompt, scope, _) = runtime.async_runtime.block_on(render_prompt_future)?;

        log::info!("Prompt: {prompt:#?}");

        Ok(())
    }

    #[test_log::test]
    fn test_call_function_unions1() -> Result<(), Box<dyn std::error::Error>> {
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
            FeatureFlags::new(),
        )?;
        log::info!("Runtime:");

        let ctx = runtime
            .create_ctx_manager(BamlValue::String("test".to_string()), None)
            .create_ctx_with_default();

        let params = runtime.get_test_params(function_name, test_name, &ctx, true)?;

        let render_prompt_future = runtime.render_prompt(function_name, &ctx, &params, Some(0));

        let (prompt, scope, _) = runtime.async_runtime.block_on(render_prompt_future)?;

        // let prompt = render_prompt_future
        //     .await
        //     .as_ref()
        //     .map(|(p, scope)| p)
        //     .map_err(|e| anyhow::anyhow!("Error rendering prompt: {:#?}", e))?;

        log::info!("Prompt: {prompt:#?}");

        Ok(())
    }

    #[test_log::test]
    fn test_class_with_block_description() -> Result<(), Box<dyn std::error::Error>> {
        let runtime = make_test_runtime(
            r##"
        class User {
          name string @description("Full name")
          email string

          @@description("Represents a system user")
        }

        function GetUser(input: string) -> User {
          client "openai/gpt-4o"
          prompt #"
            Extract user info:
            {{ ctx.output_format }}
          "#
        }

        test TestUser {
          functions [GetUser]
          args {
            input "John Doe john@example.com"
          }
        }
        "##,
        )?;

        let ctx = runtime
            .create_ctx_manager(BamlValue::String("test".to_string()), None)
            .create_ctx_with_default();

        let params = runtime.get_test_params("GetUser", "TestUser", &ctx, true)?;
        let render_prompt_future = runtime.render_prompt("GetUser", &ctx, &params, Some(0));
        let (prompt, _scope, _) = runtime.async_runtime.block_on(render_prompt_future)?;

        // Verify the rendered prompt contains the class description
        let prompt_str = prompt.to_string();
        assert!(
            prompt_str.contains("// Represents a system user"),
            "Missing class description"
        );
        assert!(
            prompt_str.contains("// Full name"),
            "Missing field description"
        );

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
            FeatureFlags::new(),
        )
    }

    #[test_log::test]
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
        let render_prompt_future = runtime.render_prompt(function_name, &ctx, &params, None);
        let (prompt, scope, _) = runtime.async_runtime.block_on(render_prompt_future)?;

        Ok(())
    }

    #[test_log::test]
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
        let render_prompt_future = runtime.render_prompt(function_name, &ctx, &params, None);
        let (prompt, scope, _) = runtime.async_runtime.block_on(render_prompt_future)?;

        Ok(())
    }

    #[test_log::test]
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
        let render_prompt_future = runtime.render_prompt(function_name, &ctx, &params, None);
        let (prompt, scope, _) = runtime.async_runtime.block_on(render_prompt_future)?;

        Ok(())
    }

    #[test_log::test]
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
        let render_prompt_future = runtime.render_prompt(function_name, &ctx, &params, None);
        let (prompt, scope, _) = runtime.async_runtime.block_on(render_prompt_future)?;

        Ok(())
    }

    #[test_log::test]
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
        let render_prompt_future = runtime.render_prompt(function_name, &ctx, &params, None);
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

        let runtime = make_test_runtime(baml)?;

        let ctx = runtime.create_ctx_manager(BamlValue::String("test".to_string()), None);

        let on_event = if false { Some(|_| {}) } else { None };
        let on_tick = if false { Some(|| {}) } else { None };
        let run_test_future = runtime.run_test(
            function_name,
            test_name,
            &ctx,
            on_event,
            None,
            HashMap::new(),
            None,
            TripWire::new(None),
            on_tick,
            None,
        );
        let (res, call) = runtime.async_runtime.block_on(run_test_future);

        Ok(())
    }

    #[test_log::test]
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

                    dynamic class Resume {
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

    #[test_log::test]
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
                    dynamic enum Category {
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

    #[test_log::test]
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

                  dynamic enum Role {
                    ProductManager
                    Sales
                  }

                  dynamic class Resume {
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

    #[test_log::test]
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

                    dynamic class Resume {
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

    #[test_log::test]
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

                    dynamic class Resume {
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

    #[test_log::test]
    fn test_type_builder_recursive_dynamic_classes() -> anyhow::Result<()> {
        run_type_builder_block_test(TypeBuilderBlockTest {
            function_name: "MyFunc",
            test_name: "Foo",
            baml: r##"
              class DynamicOutput {
                @@dynamic
              }

              function MyFunc(input: string) -> DynamicOutput {
                client "openai/gpt-4o"
                prompt #"
                  Given a string, extract info using the schema:

                  {{ input}}

                  {{ ctx.output_format }}
                "#
              }

              class Other {
                bar int
                @@dynamic
              }


              test Foo {
                functions [MyFunc]
                args {
                  input "hi"
                }
                type_builder {
                  class NewClass {
                    ten int
                  }

                  dynamic class Other {
                    other_dyn string
                    dyn_out DynamicOutput?
                  }

                  dynamic class DynamicOutput {
                    foo int
                    nc NewClass
                    other Other?
                  }

                }
              }
            "##,
        })
    }

    #[test_log::test]
    fn test_class_property_alias() -> anyhow::Result<()> {
        run_type_builder_block_test(TypeBuilderBlockTest {
            function_name: "Fn",
            test_name: "Test",
            baml: r##"
              class PropertyAlias {
                  property string? | int @alias("hello")
              }

              function Fn() -> PropertyAlias {
                  client "openai/gpt-4o"
                  prompt #"
                      {{ctx.output_format}}
                  "#
              }

              test Test {
                functions [Fn]
                args {
                }
              }
            "##,
        })
    }

    #[test]
    fn test_client_reload_on_env_var_change() -> anyhow::Result<()> {
        let runtime = make_test_runtime(
            r##"
            client<llm> GPT4Turbo {
                provider baml-openai-chat
                options {
                    model gpt-4-1106-preview
                    api_key env.OPENAI_API_KEY
                }
            }

            function Test(input: string) -> string {
                client GPT4Turbo
                prompt #"{{ input }}"#
            }

            test TestEnvVars {
                functions [Test]
                args {
                    input "test"
                }
            }
            "##,
        )?;

        let ctx = runtime.create_ctx_manager(BamlValue::String("test".to_string()), None);

        // First call with initial env var
        let on_event = if false { Some(|_| {}) } else { None };
        let on_tick = if false { Some(|| {}) } else { None };
        let run_test_future = runtime.run_test(
            "Test",
            "TestEnvVars",
            &ctx,
            on_event,
            None,
            HashMap::new(),
            None,
            TripWire::new(None),
            on_tick,
            None,
        );
        let (res1, _) = runtime.async_runtime.block_on(run_test_future);
        // Get the first client instance
        let client1 = runtime.llm_provider_from_function("Test", &ctx.create_ctx_with_default())?;

        // Change non-required env var
        let mut env_vars2 = HashMap::new();
        env_vars2.insert("NON_REQUIRED_VAR".to_string(), "value".to_string());
        let run_test_future = runtime.run_test(
            "Test",
            "TestEnvVars",
            &ctx,
            on_event,
            None,
            env_vars2.clone(),
            None,
            TripWire::new(None),
            on_tick,
            None,
        );
        let (res2, _) = runtime.async_runtime.block_on(run_test_future);
        let client2 = runtime.llm_provider_from_function(
            "Test",
            &ctx.create_ctx(None, None, env_vars2.clone(), vec![FunctionCallId::new()])?,
        )?;
        // Get the second client instance - should be the same as first
        assert!(
            Arc::ptr_eq(&client1, &client2),
            "Client should NOT reload on non-required env var change"
        );

        Ok(())
    }
}
