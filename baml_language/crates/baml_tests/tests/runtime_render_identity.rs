//! BEP-066 R-3 executable oracles now live in
//! `baml_src/ns_runtime_render_identity/` as native BAML tests. This file
//! keeps only the host-boundary case: `ai.OutputFormat._render` declares
//! `throws never` yet throws `RenderPrompt` for a program the compiler
//! admitted but cannot render, so the error is uncatchable in BAML and the
//! runtime reports the undeclared throw as an internal error that still
//! carries the render diagnostic.

use baml_tests::baml_test;

#[tokio::test]
async fn output_format_surfaces_deferred_recursive_generic_error() {
    let output = baml_test!(
        r##"
        class Chain<T> {
            next Chain<Chain<T>>
        }

        function main() -> string {
            let rt = Chain<int>.type()
            let render_ctx = ai.Context {
                client: ai.ContextClient {
                    name: "test",
                    provider: "openai",
                    default_role: "user",
                    allowed_roles: ["user"],
                },
                tags: {},
                _output_format: ai.internal.build_output_format(rt),
            }
            render_ctx.output_format()
        }
        "##
    );

    let Err(bex_engine::EngineError::VmInternalError(
        bex_vm::errors::VmInternalError::UndeclaredThrow {
            operation,
            declared,
            detail,
        },
    )) = output.result
    else {
        panic!(
            "expected the undeclared RenderPrompt as an internal error, got: {:?}",
            output.result
        );
    };
    assert_eq!(declared, "never");
    assert!(
        operation.contains("render"),
        "the render operation is named, got {operation:?}"
    );
    assert!(
        detail.contains("baml.errors.RenderPrompt"),
        "the thrown class is named, got {detail:?}"
    );
    assert!(
        detail.contains(
            "Non-regular recursive generic class 'Chain' expands from 'Chain<int>' to 'Chain<Chain<int>>'"
        ),
        "the render diagnostic survives, got {detail:?}"
    );
}
