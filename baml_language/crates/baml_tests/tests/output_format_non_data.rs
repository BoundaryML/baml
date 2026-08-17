//! Regression coverage for non-data types at the LLM output-schema boundary.

use baml_tests::baml_test;
use bex_engine::BexExternalValue;

fn result_string(output: baml_tests::engine::TestOutput) -> String {
    let BexExternalValue::String(value) = output.result.expect("program should return a string")
    else {
        panic!("expected string result")
    };
    value.to_string()
}

const GENERIC_LIST: &str = r#"
client TestClient = openai.ResponsesClient.new(
    model = "gpt-4o-mini",
    api_key = "test-key",
    base_url = "http://localhost:1234",
);

function GenericList<T>(topic: string) -> T[] {
    client: TestClient
    prompt: `List ${topic}.\n${ctx.output_format}`
}
"#;

#[tokio::test]
async fn static_never_specialization_throws_compilation_error() {
    let source = format!(
        r#"
            {GENERIC_LIST}

            function main() -> string throws never {{
                let rendered = GenericList$render_prompt<never>("items") catch (e) {{
                    baml.reflect.errors.CompilationError => e.diagnostics[0].code + "|" + e.diagnostics[0].message,
                    _ => "wrong error",
                }}
                if rendered is string {{
                    return rendered
                }}
                "render did not throw"
            }}
            "#
    );
    let output = baml_test!(&source);

    assert_eq!(
        result_string(output),
        "E0164|non-data type `never` cannot be rendered as an LLM output schema"
    );
}

#[tokio::test]
async fn runtime_never_specialization_uses_the_same_diagnostic() {
    let source = format!(
        r#"
            {GENERIC_LIST}

            function main() -> string throws never {{
                let runtime_t = type.of<never>()
                let rendered = GenericList$render_prompt<unreflect(runtime_t)>("items") catch (e) {{
                    baml.reflect.errors.CompilationError => e.diagnostics[0].code + "|" + e.diagnostics[0].message,
                    _ => "wrong error",
                }}
                if rendered is string {{
                    return rendered
                }}
                "render did not throw"
            }}
            "#
    );
    let output = baml_test!(&source);

    assert_eq!(
        result_string(output),
        "E0164|non-data type `never` cannot be rendered as an LLM output schema"
    );
}

#[tokio::test]
async fn direct_llm_execution_fails_before_network_io() {
    let source = format!(
        r#"
            {GENERIC_LIST}

            function main() -> string throws never {{
                let result = GenericList<never>("items") catch (e) {{
                    baml.reflect.errors.CompilationError => e.diagnostics[0].code + "|" + e.diagnostics[0].message,
                    _ => "wrong error",
                }}
                if result is string {{
                    return result
                }}
                "call did not throw"
            }}
            "#
    );
    let output = baml_test!(&source);

    assert_eq!(
        result_string(output),
        "E0164|non-data type `never` cannot be rendered as an LLM output schema"
    );
}

#[tokio::test]
async fn ordinary_data_specialization_still_renders() {
    let source = format!(
        r#"
            {GENERIC_LIST}

            class Item {{
                name string?
                description string?
            }}

            function main() -> string throws unknown {{
                GenericList$render_prompt<Item>("items").text()
            }}
            "#
    );
    let output = baml_test!(&source);

    let rendered = result_string(output);
    assert!(
        rendered.contains("name"),
        "missing class schema: {rendered}"
    );
    assert!(
        rendered.contains("description"),
        "missing class schema: {rendered}"
    );
}
