//! BEP-066 R-3 executable oracles now live in
//! `baml_src/ns_runtime_render_identity/` as native BAML tests. This file
//! keeps only the host-boundary case: `ctx.output_format_with()` declares
//! `throws never`, so its deferred RenderPrompt error is uncatchable in BAML
//! and must be observed as an escaping host error.

use baml_tests::baml_test;
use bex_engine::BexExternalValue;

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
