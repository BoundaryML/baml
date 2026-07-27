//! Layer 3: Full streaming parse tests.
//!
//! Tests the complete pipeline: SSE → StreamAccumulator → SAP partial/final parse → typed values.
//! Uses WireMock to serve OpenAI-format SSE responses, then exercises the
//! compiler-generated `$parse_stream` companion and `Stream<T>` consumption.
//!
//! These tests perform HTTP fetches inside the stdlib against the client's
//! compile-time `base_url`. The wiremock URI must be interpolated into the
//! BAML source at compile time — not expressible from a corpus test block.

use baml_tests::baml_test;
use bex_engine::BexExternalValue;
use wiremock::{
    Mock, MockServer, ResponseTemplate,
    matchers::{method, path},
};

/// Build an OpenAI-format SSE body from content chunks and a finish reason.
fn openai_sse_body(chunks: &[&str], finish_reason: &str) -> String {
    let mut body = String::new();
    for chunk in chunks {
        body.push_str(&format!(
            "data: {{\"choices\":[{{\"delta\":{{\"content\":\"{chunk}\"}}}}]}}\n\n"
        ));
    }
    body.push_str(&format!(
        "data: {{\"choices\":[{{\"delta\":{{}},\"finish_reason\":\"{finish_reason}\"}}]}}\n\n"
    ));
    body.push_str("data: [DONE]\n\n");
    body
}

/// BAML source for a streaming LLM test with a mock server URL.
fn streaming_llm_source(base_url: &str) -> String {
    format!(
        r##"
        client<llm> TestClient {{
            provider openai
            options {{
                model "gpt-4o"
                api_key "test-key"
                base_url "{base_url}"
            }}
        }}

        function TestFunc(input: string) -> string {{
            client TestClient
            prompt `Say hello to ${{input}}`
        }}
    "##
    )
}

/// Inferred-type-args pin (paired with `projects/compiles/stream_llm_inferred_typeargs/`):
/// `baml.llm.stream_llm_function(...)` is called with **no** explicit
/// `<TStream, TFinal>` — they are inferred from the let-binding annotation
/// (TIR phase-0 reverse inference, persisted in `CallPlan.type_args`,
/// materialized into the callee frame by MIR) and reified inside
/// `__make_stream` via `reflect.type_of<TStream/TFinal>()`. There is no
/// name-keyed registry fallback anymore; a propagation gap surfaces as a
/// "Non-parsable type: ..." crash from `StreamCache.new`.
#[tokio::test]
async fn stream_string_final_value() {
    let server = MockServer::start().await;
    let sse_body = openai_sse_body(&["Hello", ", ", "world", "!"], "stop");
    // OpenAI client appends `/chat/completions` to base_url (no `/v1` prefix
    // when base_url is already a full host URI).
    Mock::given(method("POST"))
        .and(path("/chat/completions"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-type", "text/event-stream")
                .set_body_string(sse_body),
        )
        .mount(&server)
        .await;
    let uri = server.uri();

    let source = format!(
        r#"
        {llm_source}

        function main() -> string {{
            let stream: baml.llm.Stream<null | string, string> = baml.llm.stream_llm_function(TestClient, "TestFunc", {{"input": "world"}}, prompt_closure = prompt`Say hello to world`);
            stream.final()
        }}
    "#,
        llm_source = streaming_llm_source(&uri)
    );

    let output = baml_test!(&source);
    assert_eq!(
        output.result,
        Ok(BexExternalValue::String("Hello, world!".to_string().into()))
    );
}

/// Regression pin: class-typed streaming for a function declared inside a
/// namespace. Historically broken twice by the same genus of bug — a
/// stream-expanded type expr re-resolved in the wrong context, lowering
/// `Unknown → Void` and crashing at runtime in `StreamCache.new` with
/// "Non-parsable type: Void":
///   1. emit's `compute_stream_return_type` (since deleted) passed the
///      namespace path into `lower_type_expr`'s *generic-params* slot;
///   2. MIR's `type_expr_to_template` resolved explicit call-site type args
///      against HIR's package items, where PPIR-synthetic `Doc$stream`
///      doesn't exist.
/// The stream type now travels only through the PPIR-synthesized companion
/// (signature + explicit type args) and is reified via
/// `reflect.type_of<TStream/TFinal>()` in `__make_stream` — there is no
/// baked `stream_return_type` metadata left to diverge. See
/// thoughts/sam-projects/bridge-generics/streaming/00 + 01 and the
/// live-OpenAI coverage in `sdk_tests/.../test_streaming_class_e2e.py`.
#[tokio::test]
async fn stream_class_in_namespace_final_value() {
    let server = MockServer::start().await;
    // Two chunks splitting mid-string so the partial parser sees an
    // incomplete object before the final one.
    let sse_body = openai_sse_body(
        &[
            r#"{\"title\": \"Hello\", \"body\": \"Wor"#,
            r#"ld\", \"word_count\": 1}"#,
        ],
        "stop",
    );
    Mock::given(method("POST"))
        .and(path("/chat/completions"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-type", "text/event-stream")
                .set_body_string(sse_body),
        )
        .mount(&server)
        .await;
    let uri = server.uri();

    let ns_source = format!(
        r##"
        client<llm> TestClient {{
            provider openai
            options {{
                model "gpt-4o"
                api_key "test-key"
                base_url "{uri}"
            }}
        }}

        class Doc {{
            title string
            body string?
            word_count int
        }}

        function TestFunc(input: string) -> Doc {{
            client TestClient
            prompt `Say hello to ${{input}}`
        }}

        function main() -> string {{
            let s = TestFunc$stream("world");
            let d = s.final();
            d.title
        }}
    "##
    );

    let program =
        baml_tests::engine::compile_multi_file(&[("ns_extract/main.baml", ns_source.as_str())]);
    let output = baml_tests::engine::run_compiled(
        program,
        "extract.main",
        baml_tests::engine::IndexMap::new(),
        false,
    )
    .await;
    assert_eq!(
        output.result,
        Ok(BexExternalValue::String("Hello".to_string().into()))
    );
}

#[tokio::test]
async fn stream_server_error_propagates() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(ResponseTemplate::new(500).set_body_string("Internal Server Error"))
        .mount(&server)
        .await;
    let uri = server.uri();

    let source = format!(
        r#"
        {llm_source}

        function main() -> string {{
            let stream: baml.llm.Stream<null | string, string> = baml.llm.stream_llm_function(TestClient, "TestFunc", {{"input": "world"}}, prompt_closure = prompt`Say hello to world`);
            stream.final()
        }}
    "#,
        llm_source = streaming_llm_source(&uri)
    );

    let output = baml_test!(&source);
    assert!(
        output.result.is_err(),
        "Expected error for streaming with 500 response"
    );
}
