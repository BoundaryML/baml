//! Layer 3: Full streaming parse tests.
//!
//! Tests the complete pipeline: SSE → StreamAccumulator → SAP partial/final parse → typed values.
//! Uses WireMock to serve OpenAI-format SSE responses, then exercises the
//! compiler-generated `$parse_stream` companion and `Stream<T>` consumption.

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
            prompt #"Say hello to {{{{ input }}}}"#
        }}
    "##
    )
}

/// Phase 6 regression pin (paired with `projects/compiles/stream_llm_inferred_typeargs/`):
/// `baml.llm.stream_llm_function(...)` is called with **no** explicit `<T, S>`.
/// `T` and `S` are inferred via the let-binding annotation, but inferred
/// type-args don't reach MIR lowering today, so `__make_stream<T, S>` falls
/// back to a registry lookup (`get_return_type(function_name)`) instead of
/// `reflect.type_of<T>()`. If anyone removes that workaround without first
/// landing inferred-arg lowering, this test will crash with a
/// `Non-parsable type: BuiltinUnknown` error from the sys-op layer.
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
            let stream: baml.llm.Stream<null | string, string> = baml.llm.stream_llm_function(TestClient, "TestFunc", {{"input": "world"}});
            stream.final()
        }}
    "#,
        llm_source = streaming_llm_source(&uri)
    );

    let output = baml_test!(&source);
    assert_eq!(
        output.result,
        Ok(BexExternalValue::String("Hello, world!".to_string()))
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
            let stream: baml.llm.Stream<null | string, string> = baml.llm.stream_llm_function(TestClient, "TestFunc", {{"input": "world"}});
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

#[tokio::test]
async fn stream_done_signal_required() {
    let server = MockServer::start().await;
    // SSE body with content but no finish_reason and no [DONE] — stream just ends
    let sse_body = "data: {\"choices\":[{\"delta\":{\"content\":\"partial\"}}]}\n\n";
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
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
            let stream: baml.llm.Stream<null | string, string> = baml.llm.stream_llm_function(TestClient, "TestFunc", {{"input": "world"}});
            stream.final()
        }}
    "#,
        llm_source = streaming_llm_source(&uri)
    );

    let output = baml_test!(&source);
    // Stream ended without [DONE] or finish_reason — should error
    assert!(
        output.result.is_err(),
        "Expected error when stream ends without completion signal"
    );
}
