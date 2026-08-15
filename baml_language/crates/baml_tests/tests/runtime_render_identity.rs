//! BEP-066 R-3 executable oracles: one LLM render context cannot assign two
//! non-equivalent runtime definitions to the same displayed qualified name.

use baml_tests::baml_test;
use bex_engine::BexExternalValue;

#[tokio::test]
async fn non_equivalent_same_name_runtime_classes_fail_before_render() {
    let output = baml_test!(
        r##"
        client TestClient = openai.OpenAiClient.new(
    model = "gpt-4o-mini",
    api_key = "test-key",
    base_url = "http://localhost:1234",
);

        function Render<T>() -> T {
            client: TestClient
            prompt: `${ctx.output_format}`
        }

        function main() -> string throws baml.reflect.errors.CompilationError {
            let left = reflect.class.new("Collision", {
                "left": type.of<int>(),
            })
            let right = reflect.class.new("Collision", {
                "right": type.of<string>(),
            })
            let combined = reflect.union.new([left.as_type(), right.as_type()])
            let rendered = Render$render_prompt<unreflect(combined.as_type())>() catch (e) {
                baml.reflect.errors.CompilationError => {
                    e.diagnostics[0].code + "|" + e.diagnostics[0].message
                },
                _ => "wrong render error",
            }
            if rendered is string {
                return rendered
            }
            return "render did not throw"
        }
        "##
    );

    assert_eq!(
        output.result,
        Ok(BexExternalValue::String(
            "E0162|type `Collision` has non-equivalent definitions in the same LLM render context"
                .into()
        ))
    );
}

#[tokio::test]
async fn recursive_and_equivalent_same_name_runtime_classes_still_render() {
    let output = baml_test!(
        r##"
        client TestClient = openai.OpenAiClient.new(
    model = "gpt-4o-mini",
    api_key = "test-key",
    base_url = "http://localhost:1234",
);

        function Render<T>() -> T {
            client: TestClient
            prompt: `${ctx.output_format}`
        }

        function main() -> string {
            let node = reflect.class.builder("Node")
            node.field("value", type.of<int>())
            node.field("next", node.type().optional())
            let node_t = node.build()
            let recursive_prompt = Render$render_prompt<unreflect(node_t.as_type())>().text()

            let left = reflect.class.new("Collision", {
                "value": type.of<int>(),
            })
            let right = reflect.class.new("Collision", {
                "value": type.of<int>(),
            })
            let equivalent = reflect.union.new([left.as_type(), right.as_type()])
            let equivalent_prompt = Render$render_prompt<unreflect(equivalent.as_type())>().text()

            return recursive_prompt + "\n<EQUIVALENT>\n" + equivalent_prompt
        }
        "##
    );

    let BexExternalValue::String(result) = output
        .result
        .expect("recursive and equivalent same-name definitions should render")
    else {
        panic!("expected a string result")
    };
    let (recursive, equivalent) = result
        .split_once("\n<EQUIVALENT>\n")
        .expect("missing prompt separator");
    assert!(
        recursive.contains("Node"),
        "recursive schema missing: {recursive}"
    );
    assert!(
        equivalent.contains("value"),
        "equivalent field missing: {equivalent}"
    );
}
