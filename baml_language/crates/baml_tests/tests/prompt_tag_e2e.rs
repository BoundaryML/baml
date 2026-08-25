//! BEP-049 M5e/M5f end-to-end prompt and streaming coverage.
//!
//! Removed with the legacy LLM path (see git history):
//!   - `backtick_prompt_renders_into_provider_request` — asserted on the wire
//!     request built by the legacy `call_llm_function` orchestrator; prompt
//!     rendering is now covered by the `$render_prompt` companion tests in
//!     `baml_src/ns_prompt_tag_runtime/`.
//!
//! The remaining tests exercise the ai-world `$stream` companion against a
//! local OpenAI Responses API endpoint.

#![allow(dead_code)]

use baml_tests::baml_test;
use bex_engine::BexExternalValue;
use wiremock::{
    Mock, MockServer, ResponseTemplate,
    matchers::{method, path},
};

/// An OpenAI client pointed at the mock server.
fn client_decl(base_url: &str) -> String {
    format!(
        r#"
        client TestClient = openai.ResponsesClient.new(
            model = "gpt-4o",
            api_key = "test-key",
            base_url = "{base_url}",
        );
    "#
    )
}

/// Pull the concatenated `input[].content` out of a captured Responses request.
fn request_messages(server_requests: &[wiremock::Request]) -> String {
    let req = server_requests
        .iter()
        .find(|r| r.url.path() == "/responses")
        .expect("a /responses request was recorded");
    let body: serde_json::Value = serde_json::from_slice(&req.body).expect("request body is JSON");
    body["input"]
        .as_array()
        .expect("input array present")
        .iter()
        .map(|m| message_text(&m["content"]))
        .collect::<Vec<_>>()
        .join("\n")
}

/// OpenAI `content` is either a plain string or an array of `{type,text}` parts.
fn message_text(content: &serde_json::Value) -> String {
    if let Some(s) = content.as_str() {
        return s.to_string();
    }
    content
        .as_array()
        .map(|parts| {
            parts
                .iter()
                .filter_map(|p| p["text"].as_str())
                .collect::<Vec<_>>()
                .join("")
        })
        .unwrap_or_default()
}

/// An OpenAI Responses SSE body streaming `chunks` then completing.
fn openai_sse_body(chunks: &[&str]) -> String {
    let mut body = String::new();
    for chunk in chunks {
        let delta = serde_json::to_string(chunk).expect("stream delta is JSON-serializable");
        body.push_str(&format!(
            "data: {{\"type\":\"response.output_text.delta\",\"delta\":{delta}}}\n\n"
        ));
    }
    body.push_str(
        "data: {\"type\":\"response.completed\",\"response\":{\"status\":\"completed\",\"output\":[],\"usage\":{\"input_tokens\":1,\"output_tokens\":1}}}\n\n",
    );
    body
}

#[tokio::test]
async fn backtick_prompt_streams_through_orchestrator() {
    // BEP-049 M5e: the streaming path must thread the same prompt closure as
    // the oneshot path, so the rendered backtick prompt reaches the wire.
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/responses"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-type", "text/event-stream")
                .set_body_string(openai_sse_body(&["Hi", " there"])),
        )
        .mount(&server)
        .await;
    let uri = server.uri();

    let source = format!(
        r#"
        {client}

        function Greet(name: string) -> string {{
            client: TestClient
            prompt: `Hello ${{name}}!`
        }}

        function main() -> string {{
            let stream = Greet$stream("World");
            stream.final()
        }}
    "#,
        client = client_decl(&uri)
    );

    let output = baml_test!(&source);
    assert_eq!(
        output.result,
        Ok(BexExternalValue::String("Hi there".to_string().into())),
        "streamed response should assemble: {:?}",
        output.result
    );

    let messages = request_messages(
        &server
            .received_requests()
            .await
            .expect("stream request recorded"),
    );
    assert!(
        messages.contains("Hello World!"),
        "the closure-rendered prompt must reach the streaming request: {messages:?}"
    );
}

/// Streaming render of `ctx.output_format()` over a CLASS return: the class
/// schema must reach the wire on the streaming path.
#[tokio::test]
async fn backtick_streaming_renders_output_format() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/responses"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-type", "text/event-stream")
                .set_body_string(openai_sse_body(&["{\"name\": \"Ada\"}"])),
        )
        .mount(&server)
        .await;

    let src = format!(
        r#"
        {client}

        class Person {{ name string }}

        function GetPerson() -> Person {{
            client: TestClient
            prompt: `Make a person.${{ctx.output_format()}}`
        }}

        function main() -> Person {{
            let s = GetPerson$stream();
            s.final()
        }}
    "#,
        client = client_decl(&server.uri()),
    );
    let output = baml_test!(&src);
    assert!(
        output.result.is_ok(),
        "streamed class response should parse: {:?}",
        output.result
    );
    let backtick = request_messages(
        &server
            .received_requests()
            .await
            .expect("stream request recorded"),
    );

    assert!(
        backtick.contains("name"),
        "backtick streaming should render the Person schema (its `name` field): {backtick:?}"
    );
}
