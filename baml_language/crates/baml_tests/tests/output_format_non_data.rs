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
    prompt: `List ${topic}.\n${ctx.output_format()}`
}
"#;

const GENERIC_VALUE: &str = r#"
client TestClient = openai.ResponsesClient.new(
    model = "gpt-4o-mini",
    api_key = "test-key",
    base_url = "http://localhost:1234",
);

function GenericValue<T>(topic: string) -> T {
    client: TestClient
    prompt: `Describe ${topic}.\n${ctx.output_format()}`
}
"#;

#[tokio::test]
async fn static_never_specialization_throws_compilation_error() {
    let source = format!(
        r#"
            {GENERIC_LIST}

            function main() -> string throws never {{
                let rendered = GenericList@render_prompt<never>("items") catch (e) {{
                    reflect.errors.CompilationError => e.diagnostics[0].code + "|" + e.diagnostics[0].message,
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
                let runtime_t = reflect.Type.of<never>()
                let rendered = GenericList@render_prompt<unreflect(runtime_t)>("items") catch (e) {{
                    reflect.errors.CompilationError => e.diagnostics[0].code + "|" + e.diagnostics[0].message,
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
                    reflect.errors.CompilationError => e.diagnostics[0].code + "|" + e.diagnostics[0].message,
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

            function main() -> string {{
                GenericList@render_prompt<Item>("items").text()
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

#[tokio::test]
async fn unknown_class_field_reports_its_path() {
    let source = format!(
        r#"
            {GENERIC_VALUE}

            class Payload {{
                value unknown
            }}

            function main() -> string throws never {{
                let rendered = GenericValue@render_prompt<Payload>("payload") catch (e) {{
                    reflect.errors.CompilationError => e.diagnostics[0].code + "|" + e.diagnostics[0].message,
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
        "E0164|field `Payload.value` has non-data type `unknown`, which cannot be rendered as an LLM output schema"
    );
}

#[tokio::test]
async fn nested_non_data_class_field_reports_the_full_path() {
    let source = format!(
        r#"
            {GENERIC_VALUE}

            class Inner {{
                payload reflect.Type
            }}

            class Envelope {{
                inner Inner
            }}

            function main() -> string throws never {{
                let rendered = GenericValue@render_prompt<Envelope>("envelope") catch (e) {{
                    reflect.errors.CompilationError => e.diagnostics[0].code + "|" + e.diagnostics[0].message,
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
        "E0164|field `Envelope.inner.payload` has non-data type `reflect.Type`, which cannot be rendered as an LLM output schema"
    );
}

#[tokio::test]
async fn runtime_minted_nested_non_data_field_is_rejected() {
    let source = format!(
        r#"
            {GENERIC_VALUE}

            function main() -> string {{
                let inner = reflect.class.new("RuntimeInner", {{
                    "payload": reflect.Type.of<unknown>(),
                }})
                let outer = reflect.class.new("RuntimeOuter", {{
                    "inner": inner.as_type(),
                }})
                let rendered = GenericValue@render_prompt<unreflect(outer.as_type())>("runtime") catch (e) {{
                    reflect.errors.CompilationError => e.diagnostics[0].code + "|" + e.diagnostics[0].message,
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
        "E0164|field `RuntimeOuter.inner.payload` has non-data type `unknown`, which cannot be rendered as an LLM output schema"
    );
}

#[tokio::test]
async fn skipped_non_data_and_open_interface_fields_do_not_block_rendering() {
    let source = format!(
        r#"
            {GENERIC_VALUE}

            interface HiddenOpen {{}}

            class SkipControl {{
                visible string
                raw_blob uint8array? @skip
                dynamic unknown @skip
                open HiddenOpen? @skip
            }}

            function main() -> string {{
                GenericValue@render_prompt<SkipControl>("visible data").text()
            }}
            "#
    );
    let output = baml_test!(&source);
    let rendered = result_string(output);

    assert!(
        rendered.contains("visible"),
        "missing data field: {rendered}"
    );
    for skipped in ["raw_blob", "dynamic", "open"] {
        assert!(
            !rendered.contains(skipped),
            "skipped field `{skipped}` leaked into schema: {rendered}"
        );
    }
}

/// Generic DATA classes render: `Wrapper<Item>` substitutes `T` and produces
/// a real schema. (Canary temporarily rejected every generic instantiation
/// because its formatter could not substitute class generics; this branch's
/// field-template substitution is the fix that comment deferred to.)
#[tokio::test]
async fn generic_data_class_output_renders() {
    let source = format!(
        r#"
            {GENERIC_VALUE}

            class Item {{
                name string
            }}

            class Wrapper<T> {{
                value T
            }}

            function main() -> string {{
                GenericValue@render_prompt<Wrapper<Item>>("wrapped item").text()
            }}
            "#
    );
    let output = baml_test!(&source);
    let rendered = result_string(output);
    assert!(
        rendered.contains("name"),
        "schema missing Item field: {rendered}"
    );
    assert!(
        rendered.contains("value"),
        "schema missing Wrapper field: {rendered}"
    );
}

/// A generic instantiation whose argument is genuinely non-data still fails
/// before provider IO, through the substituted field.
#[tokio::test]
async fn generic_class_output_fails_before_provider_io() {
    let source = format!(
        r#"
            {GENERIC_VALUE}

            class Wrapper<T> {{
                value T
            }}

            type Callback = (int) -> int throws never;

            function main() -> string throws never {{
                let result = GenericValue<Wrapper<Callback>>("wrapped fn") catch (e) {{
                    reflect.errors.CompilationError => e.diagnostics[0].code + "|" + e.diagnostics[0].message,
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

    let got = result_string(output);
    assert!(
        got.starts_with("E0164|") && got.contains("non-data"),
        "expected an E0164 non-data rejection, got: {got}"
    );
}

/// B-1582 item 4, verification: `never[]` reached through a generic companion is
/// the ticket's exact shape. #4470 already rejects it with a catchable E0164
/// rather than panicking in `output_format`; this pins the nested-in-a-container
/// case, which is the one that could have escaped `first_non_data_type`'s walk.
#[tokio::test]
async fn never_nested_in_a_runtime_class_field_is_rejected_not_panicked() {
    let source = format!(
        r#"
            {GENERIC_VALUE}

            function main() -> string throws never {{
                let outer = reflect.class.new("RuntimeOuter", {{
                    "items": reflect.Type.of<never>().array().as_type(),
                }}) catch (e) {{
                    _ => return "class.new threw",
                }}
                let rendered = GenericValue@render_prompt<unreflect(outer.as_type())>("runtime")
                    catch (e) {{
                        reflect.errors.CompilationError => e.diagnostics[0].code + "|" + e.diagnostics[0].message,
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
        "E0164|field `RuntimeOuter.items` has non-data type `never`, which cannot be rendered as an LLM output schema"
    );
}
