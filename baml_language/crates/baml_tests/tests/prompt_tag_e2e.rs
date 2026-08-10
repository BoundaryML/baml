//! BEP-049 M5e/M5f end-to-end: a backtick `prompt` streams through the
//! orchestrator and its rendered text reaches the provider request.
//!
//! Removed with the legacy LLM path (see git history):
//!   - `backtick_prompt_renders_into_provider_request` — asserted on the wire
//!     request built by the legacy `call_llm_function` orchestrator; prompt
//!     rendering is now covered by the `$render_prompt` companion tests in
//!     `baml_src/ns_prompt_tag_runtime/`.
//!   - `backtick_and_jinja_prompts_produce_identical_messages` — Jinja
//!     `#"..."#` prompts are a compile error now, so there is no legacy twin
//!     to compare against.
//!
//! TODO(stream-migration): the two remaining tests exercise the removed
//! `$stream` companion and are `#[ignore]`d until the ai-world `$stream`
//! machinery lands. Their inline BAML still uses the removed
//! `client<llm>`/Jinja forms and must be migrated when restored (the Jinja
//! half of the parity test must be dropped entirely).

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
        client TestClient = openai.OpenAiClient.new(
            model = "gpt-4o",
            api_key = "test-key",
            base_url = "{base_url}",
        );
    "#
    )
}

/// Pull the concatenated `messages[].content` out of a captured chat request.
fn request_messages(server_requests: &[wiremock::Request]) -> String {
    let req = server_requests
        .iter()
        .find(|r| r.url.path() == "/chat/completions")
        .expect("a /chat/completions request was recorded");
    let body: serde_json::Value = serde_json::from_slice(&req.body).expect("request body is JSON");
    body["messages"]
        .as_array()
        .expect("messages array present")
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

/// An OpenAI-format SSE body streaming `chunks` then a `stop` finish.
fn openai_sse_body(chunks: &[&str]) -> String {
    let mut body = String::new();
    for chunk in chunks {
        body.push_str(&format!(
            "data: {{\"choices\":[{{\"delta\":{{\"content\":\"{chunk}\"}}}}]}}\n\n"
        ));
    }
    body.push_str("data: {\"choices\":[{\"delta\":{},\"finish_reason\":\"stop\"}]}\n\n");
    body.push_str("data: [DONE]\n\n");
    body
}

#[tokio::test]
#[ignore = "awaiting ai $stream"]
async fn backtick_prompt_streams_through_orchestrator() {
    // BEP-049 M5e: the streaming path must thread the same prompt closure as
    // the oneshot path, so the rendered backtick prompt reaches the wire.
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/chat/completions"))
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
            client TestClient
            prompt `Hello ${{name}}!`
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

/// Streaming render of `ctx.output_format` over a CLASS return: the class
/// schema must reach the wire on the streaming path.
#[tokio::test]
#[ignore = "awaiting ai $stream"]
async fn backtick_streaming_renders_output_format() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/chat/completions"))
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
            client TestClient
            prompt `Make a person.${{ctx.output_format}}`
        }}

        function main() -> Person {{
            let s = GetPerson$stream();
            s.final()
        }}
    "#,
        client = client_decl(&server.uri()),
    );
    let _ = baml_test!(&src);
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
