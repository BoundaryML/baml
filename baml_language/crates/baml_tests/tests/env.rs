//! Unified tests for environment variable operations.
//!
//! Every test in this file requires `std::env::set_var` on the host before
//! execution. BAML's stdlib is read-only over the environment
//! (`baml.env.get` / `baml.env.get_or_panic` / `baml.env.ref` only), so tests
//! that establish sentinel values must run in Rust. Three of them additionally
//! pin bytecode with insta snapshots, which requires a compiled artifact.

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

/// `env.X` is a LATE-BOUND reference: it desugars to `baml.env.ref("X")`,
/// which builds a `baml.env.Ref` carrying the variable NAME only. Nothing is
/// read until the reference is used, so the value never lands in a constructed
/// value and a host may load secrets after the runtime initializes.
#[tokio::test]
async fn env_sugar_existing_var() {
    unsafe { std::env::set_var("BAML_TEST_SUGAR_VAR", "sugar_value") };
    let output = baml_test!(
        r#"
            function main() -> string {
                env.BAML_TEST_SUGAR_VAR.get_or_panic()
            }
        "#
    );

    assert_eq!(
        output.result,
        Ok(BexExternalValue::String("sugar_value".to_string().into()))
    );
}

/// The bare sugar yields the reference itself — the name, not the secret.
#[tokio::test]
async fn env_sugar_is_a_late_bound_ref() {
    unsafe { std::env::set_var("BAML_TEST_SUGAR_REF_VAR", "never-captured") };
    let output = baml_test!(
        r#"
            function main() -> string {
                env.BAML_TEST_SUGAR_REF_VAR.name
            }
        "#
    );

    // The desugar itself: `baml.env.ref("NAME")`, not an eager read.
    insta::assert_snapshot!(output.bytecode, @r#"
    function main() -> string {
        load_const "BAML_TEST_SUGAR_REF_VAR"
        call baml.env.ref
        load_field .name
        return
    }
    "#);
    assert_eq!(
        output.result,
        Ok(BexExternalValue::String(
            "BAML_TEST_SUGAR_REF_VAR".to_string().into()
        ))
    );
}

// ─── Provider `api_key` env defaulting ────────────────────────────────────────

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
async fn runtime_constructed_openai_client_defaults_api_key_from_env() {
    unsafe { std::env::set_var("OPENAI_API_KEY", "sk-from-env") };
    let output = baml_test!(
        r#"
            function EnvPrompt() -> string {
                client: "openai/gpt-4o-mini"
                tools: []
                prompt: `Say hi`
            }

            function main() -> map<string, string> {
                let spec = EnvPrompt$spec();
                let input = ai.ModelTurnInput {
                    prompt: spec.prompt_template,
                    journal: ai.Journal { log: [] },
                    toolbox: ai.tools.Toolbox.new([]),
                    output_type: reflect.Type.of<string>(),
                };
                openai.internal.openai_render(
                    openai.ResponsesClient.new(model = "gpt-4o"),
                    input,
                ).headers
            }
        "#
    );
    let headers = result_headers(output.result);
    assert_eq!(
        headers.get("authorization"),
        Some(&BexExternalValue::String("Bearer sk-from-env".into())),
        "a runtime-constructed OpenAI client should default api_key from OPENAI_API_KEY"
    );
}

#[tokio::test]
async fn anthropic_clients_default_api_key_from_env_at_runtime() {
    unsafe { std::env::set_var("ANTHROPIC_API_KEY", "sk-ant-from-env") };
    let output = baml_test!(
        r#"
            function EnvPrompt() -> string {
                client: "anthropic/claude-haiku-4-5"
                tools: []
                prompt: `Say hi`
            }

            function main() -> map<string, string> {
                let spec = EnvPrompt$spec();
                let input = ai.ModelTurnInput {
                    prompt: spec.prompt_template,
                    journal: ai.Journal { log: [] },
                    toolbox: ai.tools.Toolbox.new([]),
                    output_type: reflect.Type.of<string>(),
                };
                let runtime = anthropic.internal._anthropic_request(
                    anthropic.AnthropicClient.new(model = "claude-sonnet-4-20250514"),
                    input,
                    false,
                ).headers.get("x-api-key") ?? "missing";
                { "runtime": runtime }
            }
        "#
    );
    let headers = result_headers(output.result);
    let expected = BexExternalValue::String("sk-ant-from-env".into());
    assert_eq!(headers.get("runtime"), Some(&expected));
}
