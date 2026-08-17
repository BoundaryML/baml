//! BEP-066 R-3 executable oracles: one LLM render context cannot assign two
//! non-equivalent runtime definitions to the same displayed qualified name.

use baml_tests::baml_test;
use bex_engine::BexExternalValue;

#[tokio::test]
async fn non_equivalent_same_name_runtime_classes_fail_before_render() {
    let output = baml_test!(
        r##"
        client TestClient = openai.ResponsesClient.new(
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
        client TestClient = openai.ResponsesClient.new(
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

#[tokio::test]
async fn non_regular_recursive_generic_fails_before_render() {
    let output = baml_test!(
        r##"
        client TestClient = openai.ResponsesClient.new(
    model = "gpt-4o-mini",
    api_key = "test-key",
    base_url = "http://localhost:1234",
);

        class Chain<T> {
            next Chain<Chain<T>>
        }

        function Render<T>() -> T {
            client: TestClient
            prompt: `${ctx.output_format}`
        }

        function main() -> string throws baml.reflect.errors.CompilationError {
            let rendered = Render$render_prompt<Chain<int>>().text() catch (e) {
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
            "E0162|non-regular recursive generic class `Chain` expands from `Chain<int>` to `Chain<Chain<int>>` and cannot be rendered as an LLM output schema"
                .into()
        ))
    );
}

#[tokio::test]
async fn output_format_with_surfaces_deferred_recursive_generic_error() {
    let output = baml_test!(
        r##"
        class Chain<T> {
            next Chain<Chain<T>>
        }

        function main() -> string {
            let rt = type.of<Chain<int>>()
            let render_ctx = ai.Context {
                client: ai.ContextClient {
                    name: "test",
                    provider: "openai",
                    default_role: "user",
                    allowed_roles: ["user"],
                },
                tags: {},
                output_format: "",
                _output_format: ai.internal.build_output_format(rt),
            }
            render_ctx.output_format_with()
        }
        "##
    );

    let Err(bex_engine::EngineError::UnhandledThrow { value, .. }) = output.result else {
        panic!("expected an uncaught RenderPrompt");
    };
    let BexExternalValue::Instance {
        class_name, fields, ..
    } = *value
    else {
        panic!("expected a RenderPrompt instance, got: {value:?}");
    };
    assert_eq!(class_name, "baml.errors.RenderPrompt");
    assert_eq!(
        fields.get("message"),
        Some(&BexExternalValue::String(
            "Non-regular recursive generic class 'Chain' expands from 'Chain<int>' to 'Chain<Chain<int>>'"
                .into()
        ))
    );
}

#[tokio::test]
async fn interface_wrapped_recursive_generic_fails_before_interface_walk() {
    let output = baml_test!(
        r##"
        interface Wrapped<T> {
            value T
        }

        class Chain<T> {
            next Chain<Wrapped<T>>
        }

        client TestClient = openai.ResponsesClient.new(
    model = "gpt-4o-mini",
    api_key = "test-key",
    base_url = "http://localhost:1234",
);

        function Render<T>() -> T {
            client: TestClient
            prompt: `${ctx.output_format}`
        }

        function main() -> string {
            let rendered = Render$render_prompt<Chain<int>>() catch (e) {
                baml.reflect.errors.CompilationError => {
                    e.diagnostics[0].code + "|" + e.diagnostics[0].message
                }
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
            "E0162|non-regular recursive generic class `Chain` expands from `Chain<int>` to `Chain<Wrapped<int>>` and cannot be rendered as an LLM output schema"
                .into()
        ))
    );
}

#[tokio::test]
async fn colliding_non_hoisted_generic_aliases_render_inline() {
    let output = baml_test!(
        r##"
        client TestClient = openai.ResponsesClient.new(
    model = "gpt-4o-mini",
    api_key = "test-key",
    base_url = "http://localhost:1234",
);

        class Box<T> {
            value T
            @@alias("Container")
        }

        class Crate<T> {
            value T
            @@alias("Container")
        }

        class Both {
            boxed Box<int>
            crated Crate<int>
        }

        function Render<T>() -> T {
            client: TestClient
            prompt: `${ctx.output_format}`
        }

        function main() -> string {
            return Render$render_prompt<Both>().text()
        }
        "##
    );

    let BexExternalValue::String(rendered) = output
        .result
        .expect("non-hoisted aliases do not create rendered definitions")
    else {
        panic!("expected rendered prompt")
    };
    assert!(rendered.contains("boxed:"), "{rendered}");
    assert!(rendered.contains("crated:"), "{rendered}");
    assert!(!rendered.contains("Container<int>"), "{rendered}");
}

#[tokio::test]
async fn finite_transformed_recursion_reaches_an_exact_cycle() {
    let output = baml_test!(
        r##"
        client TestClient = openai.ResponsesClient.new(
    model = "gpt-4o-mini",
    api_key = "test-key",
    base_url = "http://localhost:1234",
);

        class Step<A, B> {
            next Step<B[], int>
        }

        function Render<T>() -> T {
            client: TestClient
            prompt: `${ctx.output_format}`
        }

        function main() -> string {
            return Render$render_prompt<Step<string, bool>>().text()
        }
        "##
    );

    let BexExternalValue::String(rendered) = output
        .result
        .expect("finite specialization sequence should render")
    else {
        panic!("expected rendered prompt")
    };
    assert!(rendered.contains("Step<int[], int>"), "{rendered}");
}

#[tokio::test]
async fn open_interface_in_second_generic_specialization_is_rejected() {
    let output = baml_test!(
        r##"
        interface OpenValue {
            value string
        }

        class Box<T> {
            value T
        }

        class Envelope {
            concrete Box<int>
            open Box<OpenValue>
        }

        client TestClient = openai.ResponsesClient.new(
    model = "gpt-4o-mini",
    api_key = "test-key",
    base_url = "http://localhost:1234",
);

        function Render<T>() -> T {
            client: TestClient
            prompt: `${ctx.output_format}`
        }

        function main() -> string {
            let rendered = Render$render_prompt<Envelope>() catch (e) {
                baml.reflect.errors.CompilationError => {
                    e.diagnostics[0].code + "|" + e.diagnostics[0].message
                }
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
            "E0161|field `Envelope.open.value` has open interface type `OpenValue`, which cannot be rendered as an LLM output schema"
                .into()
        ))
    );
}

#[tokio::test]
async fn runtime_type_arguments_with_the_same_display_name_remain_distinct() {
    let output = baml_test!(
        r##"
        class Box<T> {
            value T
            next Box<T>?
            @@alias("Container")
        }

        client TestClient = openai.ResponsesClient.new(
    model = "gpt-4o-mini",
    api_key = "test-key",
    base_url = "http://localhost:1234",
);

        function RenderPair<T, U>() -> Box<T> | Box<U> {
            client: TestClient
            prompt: `${ctx.output_format}`
        }

        function main() -> string {
            let left = reflect.class.new("Arg", {
                "value": type.of<int>(),
            })
            let right = reflect.class.new("Arg", {
                "value": type.of<int>(),
            })
            let rendered = RenderPair$render_prompt<
                unreflect(left.as_type()),
                unreflect(right.as_type()),
            >() catch (e) {
                baml.reflect.errors.CompilationError => {
                    e.diagnostics[0].code + "|" + e.diagnostics[0].message
                }
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
            "E0162|classes `Box<Arg>` and `Box<Arg>` both render as `Container<Arg>` in the same LLM render context"
                .into()
        ))
    );
}
