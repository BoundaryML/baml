//! BEP-066 slice 2: runtime-minted enums through offline LLM companions.

use baml_tests::baml_test;
use bex_engine::BexExternalValue;

#[tokio::test]
async fn unreflect_reifies_the_runtime_type_argument() {
    let output = baml_test!(
        r#"
        function inspect<T>() -> string throws never {
            return type.of<T>().to_string()
        }

        function main() -> string throws baml.reflect.errors.CompilationError {
            let t = reflect.enum.new("Category", ["RED", "BLUE"])
            return inspect<unreflect(t)>()
        }
        "#
    );
    assert_eq!(
        output.result,
        Ok(BexExternalValue::String("Category".into()))
    );
}

#[tokio::test]
async fn runtime_enum_renders_and_alias_round_trips_through_sap() {
    let output = baml_test!(
        r##"
        client<llm> TestClient {
            provider openai
            options {
                model "gpt-4o-mini"
                api_key "test-key"
                base_url "http://localhost:1234"
            }
        }

        function Classify<T>(input: string) -> T {
            client TestClient
            prompt #"Choose a category for {{ input }}.\n{{ ctx.output_format }}"#
        }

        function main() -> string {
            let t = reflect.enum.new("Category", [
                reflect.enum.value("RED", alias = "k7", description = "warm"),
                reflect.enum.value("BLUE", description = "cool"),
            ])
            let prompt = Classify$render_prompt<unreflect(t)>("sample").text()
            let parsed = Classify$parse<unreflect(t)>(`"k7"`)
            return prompt + "\n<PARSED>" + reflect.enum.get_value(parsed)
        }
        "##
    );

    let BexExternalValue::String(result) = output
        .result
        .expect("runtime enum render and parse should succeed")
    else {
        panic!("expected string result")
    };
    assert!(
        result.contains("Category"),
        "schema omitted enum name: {result}"
    );
    assert!(
        result.contains("k7"),
        "schema omitted serialized alias: {result}"
    );
    assert!(
        result.contains("BLUE"),
        "schema omitted ordinary value: {result}"
    );
    assert!(
        result.ends_with("<PARSED>RED"),
        "alias must parse back to the source value name: {result}"
    );
}

#[tokio::test]
async fn runtime_enum_identity_and_metadata_are_preserved() {
    let output = baml_test!(
        r#"
        function main() -> string throws baml.reflect.errors.CompilationError {
            let left = reflect.enum.new("Category", ["RED", "BLUE"])
            let right = reflect.enum.new("Category", ["RED", "BLUE"])
            let tagged = left.meta(
                alias = "category_code",
                description = "A generated category",
                docstring = "runtime docs",
                other = { "owner": "tests" },
            )
            let owner = tagged.other.get("owner")
            return (left != right).to_string()
                + "|" + (tagged.ty == left).to_string()
                + "|" + (tagged.alias ?? "null")
                + "|" + (tagged.description ?? "null")
                + "|" + (tagged.docstring ?? "null")
                + "|" + (owner ?? "null")
        }
        "#
    );

    assert_eq!(
        output.result,
        Ok(BexExternalValue::String(
            "true|true|category_code|A generated category|runtime docs|tests".into()
        ))
    );
}

#[tokio::test]
async fn duplicate_runtime_enum_value_uses_compiler_diagnostic() {
    let output = baml_test!(
        r#"
        function main() -> string throws never {
            let result = reflect.enum.new("Category", ["RED", "RED"]) catch (e) {
                baml.reflect.errors.CompilationError => e.diagnostics[0].code + "|" + e.diagnostics[0].message
            }
            if result is string {
                return result
            }
            return "constructor did not throw"
        }
        "#
    );

    let BexExternalValue::String(result) = output
        .result
        .expect("duplicate definition should be catchable")
    else {
        panic!("expected string result")
    };
    assert!(
        result.starts_with("E0012|"),
        "wrong diagnostic code: {result}"
    );
    assert!(
        result.contains("duplicate variant `Category.RED`"),
        "wrong diagnostic message: {result}"
    );
}
