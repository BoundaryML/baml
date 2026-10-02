//! End-to-end reliability tests through the real OpenAI Responses client and a
//! replay server. The BAML unit corpus pins the policy itself; these tests prove
//! failures from a strict SSE transport, and member deadlines from the real HTTP
//! layer, reach that policy at the correct boundary. A deadline needs a server
//! that holds its response past wall-clock time, which BAML source cannot fake.

use std::{
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};

use baml_tests::baml_test;
use bex_engine::BexExternalValue;
use wiremock::{
    Mock, MockServer, ResponseTemplate,
    matchers::{method, path},
};

fn completed_sse(text: &str) -> String {
    let delta = serde_json::to_string(text).expect("stream delta is JSON-serializable");
    format!(
        "data: {{\"type\":\"response.output_text.delta\",\"delta\":{delta}}}\n\n\
         data: {{\"type\":\"response.completed\",\"response\":{{\"status\":\"completed\",\"output\":[],\"usage\":{{\"input_tokens\":1,\"output_tokens\":1}}}}}}\n\n"
    )
}

fn truncated_sse(text: &str) -> String {
    let delta = serde_json::to_string(text).expect("stream delta is JSON-serializable");
    format!("data: {{\"type\":\"response.output_text.delta\",\"delta\":{delta}}}\n\n")
}

/// How long a stalled member holds its response: far past `MEMBER_TIMEOUT_MS`.
const STALLED_RESPONSE: Duration = Duration::from_secs(3);
/// The member's own request deadline under test.
const MEMBER_TIMEOUT_MS: u64 = 200;

fn completed_json(text: &str) -> String {
    let text = serde_json::to_string(text).expect("response text is JSON-serializable");
    format!(
        "{{\"status\":\"completed\",\"output\":[{{\"type\":\"message\",\"role\":\"assistant\",\
         \"content\":[{{\"type\":\"output_text\",\"text\":{text}}}]}}],\
         \"usage\":{{\"input_tokens\":1,\"output_tokens\":1}}}}"
    )
}

fn response_client(name: &str, model: &str, base_url: &str) -> String {
    response_client_with_options(name, model, base_url, "")
}

/// A Responses client whose own request deadline is `MEMBER_TIMEOUT_MS`.
fn timed_response_client(name: &str, model: &str, base_url: &str) -> String {
    response_client_with_options(
        name,
        model,
        base_url,
        &format!("timeout = baml.time.Duration.from_milliseconds({MEMBER_TIMEOUT_MS}),"),
    )
}

fn response_client_with_options(name: &str, model: &str, base_url: &str, options: &str) -> String {
    format!(
        r#"
        client {name} = openai.ResponsesClient.new(
            model = "{model}",
            api_key = "test-key",
            base_url = "{base_url}",
            {options}
        );
        "#
    )
}

async fn responses_server(template: ResponseTemplate) -> MockServer {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/responses"))
        .respond_with(template)
        .mount(&server)
        .await;
    server
}

async fn request_count(server: &MockServer) -> usize {
    server
        .received_requests()
        .await
        .expect("requests should be recorded")
        .len()
}

#[tokio::test]
async fn retry_reopens_after_pre_delta_disconnect() {
    let server = MockServer::start().await;
    let calls = Arc::new(AtomicUsize::new(0));
    let responder_calls = Arc::clone(&calls);
    Mock::given(method("POST"))
        .and(path("/responses"))
        .respond_with(move |_: &wiremock::Request| {
            let attempt = responder_calls.fetch_add(1, Ordering::SeqCst);
            let body = if attempt == 0 {
                String::new()
            } else {
                completed_sse("ok")
            };
            ResponseTemplate::new(200)
                .insert_header("content-type", "text/event-stream")
                .set_body_string(body)
        })
        .mount(&server)
        .await;

    let source = format!(
        r#"
        {leaf}
        client Retried = ai.clients.Retry.new(
            inner = Leaf,
            max_attempts = 2,
            backoff = ai.clients.Backoff.new(initial_ms = 0, max_ms = 0),
        );

        function Echo() -> string {{
            client: Retried
            prompt: `Say ok.`
        }}

        function main() -> string {{
            Echo@stream().final()
        }}
        "#,
        leaf = response_client("Leaf", "retry-leaf", &server.uri()),
    );

    let output = baml_test!(&source);
    assert_eq!(
        output.result,
        Ok(BexExternalValue::String("ok".to_string().into()))
    );
    assert_eq!(
        calls.load(Ordering::SeqCst),
        2,
        "one retry must open one new stream"
    );
}

#[tokio::test]
async fn retry_does_not_reopen_after_visible_prefix() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/responses"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-type", "text/event-stream")
                .set_body_string(truncated_sse("prefix")),
        )
        .mount(&server)
        .await;

    let source = format!(
        r#"
        {leaf}

        function Echo() -> string {{
            client: Leaf
            prompt: `Say something.`
        }}

        function input() -> ai.ModelTurnInput {{
            let spec = Echo@spec();
            ai.ModelTurnInput {{
                prompt: spec.prompt_template,
                journal: ai.Journal {{ log: [] }},
                toolbox: spec.tools(),
                output_type: spec.output_type(),
            }}
        }}

        function main() -> string {{
            let retried = ai.clients.Retry.new(
                inner = Leaf,
                max_attempts = 3,
                backoff = ai.clients.Backoff.new(initial_ms = 0, max_ms = 0),
            );
            let stream = retried.invoke_stream(input());
            let first = match (stream.next()) {{
                let batch: string[] => batch.join(""),
                let done: ai.stream.Done => "done",
            }};
            let second = stream.next() catch (e) {{
                let network: ai.errors.NetworkFailure => `NetworkFailure:${{network.raw_body ?? ""}}`,
            }};
            `${{first}}|${{second.to_string()}}`
        }}
        "#,
        leaf = response_client("Leaf", "retry-leaf", &server.uri()),
    );

    let output = baml_test!(&source);
    assert_eq!(
        output.result,
        Ok(BexExternalValue::String(
            "prefix|NetworkFailure:prefix".to_string().into()
        ))
    );
    let requests = server
        .received_requests()
        .await
        .expect("stream request should be recorded");
    assert_eq!(
        requests.len(),
        1,
        "a visible prefix forbids replaying the request"
    );
}

#[tokio::test]
async fn fallback_advances_only_after_pre_delta_disconnect() {
    let first = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/responses"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-type", "text/event-stream")
                .set_body_string(""),
        )
        .mount(&first)
        .await;

    let second = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/responses"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-type", "text/event-stream")
                .set_body_string(completed_sse("winner")),
        )
        .mount(&second)
        .await;

    let source = format!(
        r#"
        {first_client}
        {second_client}
        client Reliable = ai.clients.Fallback {{ members: [First, Second] }};

        function Echo() -> string {{
            client: Reliable
            prompt: `Say winner.`
        }}

        function main() -> string {{
            Echo@stream().final()
        }}
        "#,
        first_client = response_client("First", "first", &first.uri()),
        second_client = response_client("Second", "second", &second.uri()),
    );

    let output = baml_test!(&source);
    assert_eq!(
        output.result,
        Ok(BexExternalValue::String("winner".to_string().into()))
    );
    assert_eq!(
        first
            .received_requests()
            .await
            .expect("first request should be recorded")
            .len(),
        1
    );
    assert_eq!(
        second
            .received_requests()
            .await
            .expect("fallback request should be recorded")
            .len(),
        1
    );
}

#[tokio::test]
async fn round_robin_selects_once_and_rotates_between_streams() {
    let first = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/responses"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-type", "text/event-stream")
                .set_body_string(completed_sse("a")),
        )
        .mount(&first)
        .await;

    let second = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/responses"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-type", "text/event-stream")
                .set_body_string(completed_sse("b")),
        )
        .mount(&second)
        .await;

    let source = format!(
        r#"
        {first_client}
        {second_client}
        client Rotating = ai.clients.RoundRobin.new([First, Second]);

        function Echo() -> string {{
            client: Rotating
            prompt: `Return one letter.`
        }}

        function main() -> string {{
            [Echo@stream().final(), Echo@stream().final(), Echo@stream().final()].join(",")
        }}
        "#,
        first_client = response_client("First", "first", &first.uri()),
        second_client = response_client("Second", "second", &second.uri()),
    );

    let output = baml_test!(&source);
    assert_eq!(
        output.result,
        Ok(BexExternalValue::String("a,b,a".to_string().into()))
    );
    assert_eq!(
        first
            .received_requests()
            .await
            .expect("first member requests should be recorded")
            .len(),
        2
    );
    assert_eq!(
        second
            .received_requests()
            .await
            .expect("second member request should be recorded")
            .len(),
        1
    );
}

#[tokio::test]
async fn fallback_moves_past_member_request_timeout() {
    let stalled = responses_server(
        ResponseTemplate::new(200)
            .insert_header("content-type", "application/json")
            .set_body_string(completed_json("stalled"))
            .set_delay(STALLED_RESPONSE),
    )
    .await;
    let backup = responses_server(
        ResponseTemplate::new(200)
            .insert_header("content-type", "application/json")
            .set_body_string(completed_json("backup")),
    )
    .await;

    let source = format!(
        r#"
        {stalled_client}
        {backup_client}
        client Reliable = ai.clients.Fallback {{ members: [Stalled, Backup] }};

        function Echo() -> string {{
            client: Reliable
            prompt: `Say backup.`
        }}

        function main() -> string {{
            Echo()
        }}
        "#,
        stalled_client = timed_response_client("Stalled", "stalled", &stalled.uri()),
        backup_client = response_client("Backup", "backup", &backup.uri()),
    );

    let output = baml_test!(&source);
    assert_eq!(
        output.result,
        Ok(BexExternalValue::String("backup".to_string().into())),
        "the stalled member's timeout must hand the call to the next member"
    );
    assert_eq!(request_count(&stalled).await, 1);
    assert_eq!(request_count(&backup).await, 1);
}

#[tokio::test]
async fn fallback_stream_moves_past_member_request_timeout() {
    let stalled = responses_server(
        ResponseTemplate::new(200)
            .insert_header("content-type", "text/event-stream")
            .set_body_string(completed_sse("stalled"))
            .set_delay(STALLED_RESPONSE),
    )
    .await;
    let backup = responses_server(
        ResponseTemplate::new(200)
            .insert_header("content-type", "text/event-stream")
            .set_body_string(completed_sse("backup")),
    )
    .await;

    let source = format!(
        r#"
        {stalled_client}
        {backup_client}
        client Reliable = ai.clients.Fallback {{ members: [Stalled, Backup] }};

        function Echo() -> string {{
            client: Reliable
            prompt: `Say backup.`
        }}

        function main() -> string {{
            Echo@stream().final()
        }}
        "#,
        stalled_client = timed_response_client("Stalled", "stalled", &stalled.uri()),
        backup_client = response_client("Backup", "backup", &backup.uri()),
    );

    let output = baml_test!(&source);
    assert_eq!(
        output.result,
        Ok(BexExternalValue::String("backup".to_string().into())),
        "a member deadline before the first delta must hand the stream to the next member"
    );
    assert_eq!(request_count(&stalled).await, 1);
    assert_eq!(request_count(&backup).await, 1);
}

#[tokio::test]
async fn retry_retries_member_request_timeout() {
    let server = MockServer::start().await;
    let calls = Arc::new(AtomicUsize::new(0));
    let responder_calls = Arc::clone(&calls);
    Mock::given(method("POST"))
        .and(path("/responses"))
        .respond_with(move |_: &wiremock::Request| {
            let template = ResponseTemplate::new(200)
                .insert_header("content-type", "application/json")
                .set_body_string(completed_json("ok"));
            if responder_calls.fetch_add(1, Ordering::SeqCst) == 0 {
                template.set_delay(STALLED_RESPONSE)
            } else {
                template
            }
        })
        .mount(&server)
        .await;

    let source = format!(
        r#"
        {leaf}
        client Retried = ai.clients.Retry.new(
            inner = Leaf,
            max_attempts = 2,
            backoff = ai.clients.Backoff.new(initial_ms = 0, max_ms = 0),
        );

        function Echo() -> string {{
            client: Retried
            prompt: `Say ok.`
        }}

        function main() -> string {{
            Echo()
        }}
        "#,
        leaf = timed_response_client("Leaf", "retry-leaf", &server.uri()),
    );

    let output = baml_test!(&source);
    assert_eq!(
        output.result,
        Ok(BexExternalValue::String("ok".to_string().into())),
        "a timed-out attempt must be retried"
    );
    assert_eq!(
        calls.load(Ordering::SeqCst),
        2,
        "one retry must send one new request"
    );
}
