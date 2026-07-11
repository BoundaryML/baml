//! Unified tests for environment variable operations.

#![allow(unsafe_code)]

use baml_tests::baml_test;
use bex_engine::BexExternalValue;

#[tokio::test]
async fn env_get_or_panic_existing_var() {
    unsafe { std::env::set_var("BAML_TEST_ENV_PANIC", "panic_value") };
    let output = baml_test!(
        r#"
            function main() -> string {
                baml.env.get_or_panic("BAML_TEST_ENV_PANIC")
            }
        "#
    );

    insta::assert_snapshot!(output.bytecode, @r#"
    function main() -> string {
        load_const "BAML_TEST_ENV_PANIC"
        call baml.env.get_or_panic
        return
    }
    "#);
    assert_eq!(
        output.result,
        Ok(BexExternalValue::String("panic_value".to_string().into()))
    );
}

#[tokio::test]
async fn env_get_existing_var() {
    unsafe { std::env::set_var("BAML_TEST_ENV_GET", "hello_env") };
    let output = baml_test!(
        r#"
            function main() -> string? {
                baml.env.get("BAML_TEST_ENV_GET")
            }
        "#
    );

    insta::assert_snapshot!(output.bytecode, @r#"
    function main() -> string | null {
        load_const "BAML_TEST_ENV_GET"
        sys_op baml.env.get
        return
    }
    "#);
    assert_eq!(
        output.result,
        Ok(BexExternalValue::String("hello_env".to_string().into()))
    );
}

#[tokio::test]
async fn env_sugar_existing_var() {
    unsafe { std::env::set_var("BAML_TEST_SUGAR_VAR", "sugar_value") };
    let output = baml_test!(
        r#"
            function main() -> string {
                env.BAML_TEST_SUGAR_VAR
            }
        "#
    );

    insta::assert_snapshot!(output.bytecode, @r#"
    function main() -> string {
        load_const "BAML_TEST_SUGAR_VAR"
        call baml.env.get_or_panic
        return
    }
    "#);
    assert_eq!(
        output.result,
        Ok(BexExternalValue::String("sugar_value".to_string().into()))
    );
}

// ─── Client `api_key` env defaulting ──────────────────────────────────────────
//
// The `OPENAI_API_KEY` default is applied at exactly two construction sites —
// compile-time lowering of `client<llm>` declarations and the runtime
// `"openai/model"` shorthand — NOT by the openai provider itself. A
// `PrimitiveClient` constructed as a plain object at runtime therefore builds
// an *unauthenticated* request even with `OPENAI_API_KEY` set. The two tests
// below pin both sides of that contrast.

/// Extract the headers map from a `main() -> map<string, string>` run.
fn result_headers(
    result: Result<BexExternalValue, impl std::fmt::Debug>,
) -> indexmap::IndexMap<String, BexExternalValue> {
    match result {
        Ok(BexExternalValue::Map { entries, .. }) => entries,
        other => panic!("expected Ok(Map), got: {other:?}"),
    }
}

#[tokio::test]
async fn declared_openai_client_defaults_api_key_from_env() {
    unsafe { std::env::set_var("OPENAI_API_KEY", "sk-from-env") };
    let output = baml_test!(
        r#"
            client<llm> EnvClient {
                provider openai
                options {
                    model "gpt-4o"
                }
            }

            function Greet(name: string) -> string {
                client EnvClient
                prompt `Hello ${name}!`
            }

            function main() -> map<string, string> {
                Greet$build_request("World").headers
            }
        "#
    );
    let headers = result_headers(output.result);
    assert_eq!(
        headers.get("authorization"),
        Some(&BexExternalValue::String("Bearer sk-from-env".into())),
        "a declared client<llm> with no api_key defaults it from OPENAI_API_KEY"
    );
}

#[tokio::test]
async fn runtime_constructed_primitive_client_does_not_read_openai_api_key_env() {
    unsafe { std::env::set_var("OPENAI_API_KEY", "sk-from-env") };
    let output = baml_test!(
        r#"
            function main() -> map<string, string> {
                let pc = baml.llm.PrimitiveClient {
                    name: "runtime-openai",
                    provider: "openai",
                    options: baml.llm.PrimitiveClientOptions {
                        model: "gpt-4o",
                        headers: {},
                        query_params: {},
                        request_body: {},
                    },
                };
                let prompt = baml.llm.assemble_prompt_ast(["Say hi"], []);
                pc.build_request(prompt, reflect.type_of<string>()).headers
            }
        "#
    );
    let headers = result_headers(output.result);
    assert!(
        !headers.contains_key("authorization"),
        "surprise: a runtime-constructed PrimitiveClient gets NO api_key env \
         default, so the request goes out without authorization; got: {headers:?}"
    );
}
