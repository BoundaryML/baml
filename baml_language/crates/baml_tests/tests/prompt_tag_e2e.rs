//! BEP-049 M5e/M5f end-to-end: a backtick `prompt` is compiled into a closure
//! that the orchestrator invokes per attempt, producing a `PromptAst` that
//! flows into the real provider request — byte-for-byte the same path the
//! legacy Jinja `#"..."#` prompts took before B-1024 removed them.
//!
//! These tests drive the **companion** function (`Greet("World")`), not the raw
//! `baml.llm.call_llm_function` builtin, so the closure synthesized in
//! `lower_cst` is threaded all the way through `execute_once_oneshot`. A
//! WireMock server captures the outgoing request so we can assert the rendered
//! prompt actually reaches the wire.
//!
//! These tests require Rust-side mocking (WireMock) with dynamic URI injection
//! into client declarations and wire-level request capture. BAML client options
//! are static and `baml.env` is read-only, so in-BAML HTTP servers bound to
//! OS-assigned ports cannot be reached from client declarations.

use baml_tests::baml_test;
use bex_engine::BexExternalValue;
use wiremock::{
    Mock, MockServer, ResponseTemplate,
    matchers::{method, path},
};

/// A single-choice OpenAI chat-completion response carrying `content`.
fn openai_chat_response(content: &str) -> String {
    format!(
        "{{\"id\":\"chatcmpl-test\",\"object\":\"chat.completion\",\"created\":0,\
         \"model\":\"gpt-4o\",\
         \"choices\":[{{\"index\":0,\"message\":{{\"role\":\"assistant\",\"content\":\"{content}\"}},\
         \"finish_reason\":\"stop\"}}],\
         \"usage\":{{\"prompt_tokens\":1,\"completion_tokens\":1,\"total_tokens\":2}}}}"
    )
}

/// An OpenAI client pointed at the mock server.
fn client_decl(base_url: &str) -> String {
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
    "##
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

#[tokio::test]
async fn backtick_prompt_renders_into_provider_request() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/chat/completions"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-type", "application/json")
                .set_body_string(openai_chat_response("Hi there")),
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
            Greet("World")
        }}
    "#,
        client = client_decl(&uri)
    );

    let output = baml_test!(&source);
    assert_eq!(
        output.result,
        Ok(BexExternalValue::String("Hi there".to_string().into())),
        "the parsed LLM response should flow back out: {:?}",
        output.result
    );

    let requests = server
        .received_requests()
        .await
        .expect("WireMock recorded the request");
    let messages = request_messages(&requests);
    assert!(
        messages.contains("Hello World!"),
        "the closure-rendered prompt must reach the provider request: {messages:?}"
    );
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
async fn backtick_prompt_streams_through_orchestrator() {
    // BEP-049 M5e: the streaming path (`stream_llm_function` → `execute_once_stream`)
    // must thread the same closure as the oneshot path. The `$stream` companion
    // is generated in PPIR from the body `lower_cst` pre-built off the backtick,
    // so the closure renders the prompt.
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
            let stream: baml.llm.Stream<null | string, string> = Greet$stream("World");
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

/// Golden wire pin: a backtick prompt must put exactly its rendered text on the
/// wire. Until B-1024 removed the Jinja engine this was a parity test against
/// the legacy `#"..."#` twin; the pinned bytes are the ones both paths produced.
#[tokio::test]
async fn backtick_prompt_puts_rendered_messages_on_the_wire() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/chat/completions"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-type", "application/json")
                .set_body_string(openai_chat_response("ok")),
        )
        .mount(&server)
        .await;
    let uri = server.uri();

    // Backtick (new-mode) function.
    let backtick_src = format!(
        r#"
        {client}

        function Greet(name: string) -> string {{
            client TestClient
            prompt `Hello ${{name}}!`
        }}

        function main() -> string {{
            Greet("World")
        }}
    "#,
        client = client_decl(&uri)
    );
    let _ = baml_test!(&backtick_src);
    let backtick_messages = request_messages(
        &server
            .received_requests()
            .await
            .expect("backtick request recorded"),
    );

    assert_eq!(
        backtick_messages, "Hello World!",
        "the backtick prompt must render byte-identical to the legacy path it replaced"
    );
}

/// Streaming `ctx.output_format` over a CLASS return. The streaming
/// orchestrator resolves the real return type from the registry (inferred
/// type-args don't reach MIR, so `reflect.type_of<TFinal>()` is
/// `BuiltinUnknown` there); this pins that a streaming backtick prompt renders
/// the class schema end to end.
#[tokio::test]
async fn backtick_streaming_renders_output_format() {
    async fn run_stream(server: &MockServer, fn_decl: &str) -> String {
        Mock::given(method("POST"))
            .and(path("/chat/completions"))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("content-type", "text/event-stream")
                    .set_body_string(openai_sse_body(&["{\"name\": \"Ada\"}"])),
            )
            .mount(server)
            .await;
        let src = format!(
            r#"
            {client}

            class Person {{ name string }}

            {fn_decl}

            function main() -> Person {{
                let s = GetPerson$stream();
                s.final()
            }}
        "#,
            client = client_decl(&server.uri()),
        );
        let _ = baml_test!(&src);
        request_messages(
            &server
                .received_requests()
                .await
                .expect("stream request recorded"),
        )
    }

    let server = MockServer::start().await;
    let backtick = run_stream(
        &server,
        "function GetPerson() -> Person {\n    client TestClient\n    prompt `Make a person.${ctx.output_format}`\n}",
    )
    .await;

    assert!(
        backtick.contains("name"),
        "backtick streaming should render the Person schema (its `name` field): {backtick:?}"
    );
}
