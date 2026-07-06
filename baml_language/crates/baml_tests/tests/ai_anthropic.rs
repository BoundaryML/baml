//! Tests for the BAML-native Anthropic (Messages API) provider (`baml.ai.Anthropic`).
//!
//! All mocked against a wiremock server — deterministic, no network / API key required.
//! Mirrors the OpenAI provider tests in `ai_provider.rs`.

use baml_tests::baml_test;
use bex_engine::BexExternalValue;
use wiremock::{
    Mock, MockServer, ResponseTemplate,
    matchers::{method, path},
};

/// Helper: pull the recorded POST bodies from a wiremock server.
async fn recorded_bodies(server: &MockServer) -> Vec<String> {
    server
        .received_requests()
        .await
        .unwrap_or_default()
        .iter()
        .map(|r| String::from_utf8_lossy(&r.body).to_string())
        .collect()
}

/// The full Anthropic `call<string>` pipeline (build_request → send → parse) against a
/// mocked Messages endpoint — deterministic, no network/key required.
#[tokio::test]
async fn anthropic_call_via_mock() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .respond_with(ResponseTemplate::new(200).set_body_string(
            r#"{"id":"msg_1","model":"claude-x","content":[{"type":"text","text":"pong"}],"stop_reason":"end_turn","usage":{"input_tokens":8,"output_tokens":1}}"#,
        ))
        .mount(&server)
        .await;
    let uri = server.uri();

    let output = baml_test!(&format!(
        r#"
        function main() -> string {{
            let p = baml.ai.Anthropic {{ model: "claude-x", api_key: "test-key", base_url: "{uri}" }};
            p.call<string>("Reply with exactly the word: pong") catch (e) {{
                let u: baml.errors.UnknownError => "ERR:" + u.message.join(","),
                let c: baml.errors.CallError => "CALLERR",
            }}
        }}
        "#
    ));
    assert_eq!(
        output.result.unwrap(),
        BexExternalValue::String("pong".into())
    );
}

/// Request-capture: assert the wire shape — `x-api-key` + `anthropic-version` headers,
/// `max_tokens` present, system message hoisted to a top-level `system` param (NOT in
/// `messages`), and the user content as a content-block array.
#[tokio::test]
async fn anthropic_request_shape_via_mock() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .respond_with(ResponseTemplate::new(200).set_body_string(
            r#"{"id":"msg_1","model":"claude-x","content":[{"type":"text","text":"ok"}],"stop_reason":"end_turn"}"#,
        ))
        .mount(&server)
        .await;
    let uri = server.uri();

    let output = baml_test!(&format!(
        r#"
        function main() -> string {{
            let p = baml.ai.Anthropic {{ model: "claude-x", api_key: "test-key", base_url: "{uri}" }};
            let history = [
                baml.ai.ChatMessage.system("You are terse."),
                baml.ai.ChatMessage.user("Say hi."),
            ];
            p.call_messages<string>(history) catch (e) {{
                let u: baml.errors.UnknownError => "ERR:" + u.message.join(","),
                let c: baml.errors.CallError => "CALLERR",
            }}
        }}
        "#
    ));
    assert_eq!(
        output.result.unwrap(),
        BexExternalValue::String("ok".into())
    );

    let reqs = server.received_requests().await.unwrap();
    assert_eq!(reqs.len(), 1, "expected exactly one request");
    let req = &reqs[0];

    // Headers.
    let api_key = req
        .headers
        .get("x-api-key")
        .map(|v| v.to_str().unwrap_or("").to_string());
    assert_eq!(
        api_key.as_deref(),
        Some("test-key"),
        "x-api-key header missing/wrong; headers: {:?}",
        req.headers
    );
    let version = req
        .headers
        .get("anthropic-version")
        .map(|v| v.to_str().unwrap_or("").to_string());
    assert_eq!(
        version.as_deref(),
        Some("2023-06-01"),
        "anthropic-version header missing/wrong; headers: {:?}",
        req.headers
    );

    // Body shape.
    let body = String::from_utf8_lossy(&req.body).to_string();
    assert!(
        body.contains("\"max_tokens\":4096") || body.contains("\"max_tokens\": 4096"),
        "max_tokens missing/defaulted wrong; body: {body}"
    );
    // System hoisted to the top level, NOT emitted as a message role.
    assert!(
        body.contains(r#""system":"You are terse.""#)
            || body.contains(r#""system": "You are terse.""#),
        "system not hoisted to top-level `system`; body: {body}"
    );
    assert!(
        !body.contains(r#""role":"system""#),
        "system role must NOT appear in messages; body: {body}"
    );
    // User content is a content-block array.
    assert!(
        body.contains(r#""role":"user""#)
            && body.contains(r#""type":"text""#)
            && body.contains(r#""text":"Say hi.""#),
        "user content not a content-block array; body: {body}"
    );
}

/// Structured output: `call<Resume>` with the schema appended once; the mock returns a
/// markdown-fenced JSON text block; SAP decodes it into the typed value.
#[tokio::test]
async fn anthropic_structured_via_mock() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .respond_with(ResponseTemplate::new(200).set_body_string(
            r#"{"id":"msg_1","model":"claude-x","content":[{"type":"text","text":"```json\n{\"name\": \"Ada Lovelace\", \"years\": 36}\n```"}],"stop_reason":"end_turn"}"#,
        ))
        .mount(&server)
        .await;
    let uri = server.uri();

    let output = baml_test!(&format!(
        r#"
        class Resume {{ name string, years int }}
        function main() -> string {{
            let p = baml.ai.Anthropic {{ model: "claude-x", api_key: "test-key", base_url: "{uri}" }};
            let r: Resume = p.call<Resume>("Extract the resume.") catch (e) {{
                let u: baml.errors.UnknownError => return "ERR:" + u.message.join(","),
                let c: baml.errors.CallError => return "CALLERR",
            }};
            r.name + "|" + r.years.to_string()
        }}
        "#
    ));
    assert_eq!(
        output.result.unwrap(),
        BexExternalValue::String("Ada Lovelace|36".into())
    );

    // The schema must be injected exactly once.
    let bodies = recorded_bodies(&server).await;
    let marker = "Answer in JSON using this schema";
    let count = bodies[0].matches(marker).count();
    assert_eq!(
        count, 1,
        "schema must appear exactly once, found {count}; body: {}",
        bodies[0]
    );
}

/// Streaming through a mocked SSE endpoint: Anthropic `content_block_delta` deltas
/// accumulate; a terminal `message_stop` ends the stream.
#[tokio::test]
async fn anthropic_stream_via_mock() {
    let server = MockServer::start().await;
    let sse_body = "event: message_start\n\
                    data: {\"type\":\"message_start\",\"message\":{\"model\":\"claude-x\",\"usage\":{\"input_tokens\":5}}}\n\n\
                    event: content_block_delta\n\
                    data: {\"type\":\"content_block_delta\",\"delta\":{\"type\":\"text_delta\",\"text\":\"po\"}}\n\n\
                    event: content_block_delta\n\
                    data: {\"type\":\"content_block_delta\",\"delta\":{\"type\":\"text_delta\",\"text\":\"ng\"}}\n\n\
                    event: message_delta\n\
                    data: {\"type\":\"message_delta\",\"delta\":{\"stop_reason\":\"end_turn\"},\"usage\":{\"output_tokens\":2}}\n\n\
                    event: message_stop\n\
                    data: {\"type\":\"message_stop\"}\n\n";
    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-type", "text/event-stream")
                .set_body_string(sse_body),
        )
        .mount(&server)
        .await;
    let uri = server.uri();

    let output = baml_test!(&format!(
        r#"
        function main() -> string {{
            let p = baml.ai.Anthropic {{ model: "claude-x", api_key: "test-key", base_url: "{uri}" }};
            let s = p.stream<string, string>("hi") catch (e) {{
                let u: baml.errors.UnknownError => return "ERR:" + u.message.join(","),
            }};
            let n = 0;
            while (n < 100) {{
                match (s.next()) {{
                    baml.stream.StreamFinished => {{ break; }},
                    let part: string => {{ n = n + 1; }},
                }}
            }}
            s.final() catch (e) {{ _ => "FINAL_ERR" }}
        }}
        "#
    ));
    assert_eq!(
        output.result.unwrap(),
        BexExternalValue::String("pong".into())
    );
}

/// D2/D8: a transient 529 ("overloaded") is retryable — `with_retry` re-drives past two
/// 529s then a 200 succeeds. Mirrors `retry_recovers_after_transient_failures`.
#[tokio::test]
async fn anthropic_error_retryable_via_mock() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .respond_with(
            ResponseTemplate::new(529)
                .set_body_string(r#"{"type":"error","error":{"type":"overloaded_error"}}"#),
        )
        .up_to_n_times(2)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .respond_with(ResponseTemplate::new(200).set_body_string(
            r#"{"id":"msg_1","model":"claude-x","content":[{"type":"text","text":"pong"}],"stop_reason":"end_turn"}"#,
        ))
        .mount(&server)
        .await;
    let uri = server.uri();

    let output = baml_test!(&format!(
        r#"
        function main() -> string {{
            let p = baml.ai.Anthropic {{ model: "claude-x", api_key: "test-key", base_url: "{uri}" }}.with_retry(3);
            p.call<string>("hi") catch (e) {{
                let u: baml.errors.UnknownError => "ERR:" + u.message.join(","),
            }}
        }}
        "#
    ));
    assert_eq!(
        output.result.unwrap(),
        BexExternalValue::String("pong".into())
    );
}

/// The typed `AnthropicHttpError` surfaces on a non-retryable status (400) and reports
/// `is_retryable() == false`; a 529 is classified retryable. Exercises the `Failure` axis.
#[tokio::test]
async fn anthropic_http_error_is_typed() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/messages"))
        .respond_with(
            ResponseTemplate::new(400)
                .set_body_string(r#"{"type":"error","error":{"type":"invalid_request_error"}}"#),
        )
        .mount(&server)
        .await;
    let uri = server.uri();

    let output = baml_test!(&format!(
        r#"
        function main() -> string {{
            let p = baml.ai.Anthropic {{ model: "claude-x", api_key: "test-key", base_url: "{uri}" }}.with_retry(3);
            p.call<string>("hi") catch (e) {{
                let he: baml.ai.AnthropicHttpError => "http:" + he.status.to_string() + " retryable=" + he.is_retryable().to_string(),
                let u: baml.errors.UnknownError => "unknown",
                let c: baml.errors.CallError => "callerr",
            }}
        }}
        "#
    ));
    assert_eq!(
        output.result.unwrap(),
        BexExternalValue::String("http:400 retryable=false".into())
    );
    // A non-retryable 400 must not be re-driven.
    let n = server.received_requests().await.unwrap().len();
    assert_eq!(
        n, 1,
        "a non-retryable 400 must not be re-driven, saw {n} requests"
    );
}

// ───────────────────────────── live tier (gated) ─────────────────────────────

/// Live smoke test against the real Anthropic API. Skipped unless `ANTHROPIC_API_KEY`
/// is set — e.g. `infisical run --env=test -- cargo test -p baml_tests --test ai_anthropic`.
#[tokio::test]
async fn anthropic_live_call() {
    if std::env::var("ANTHROPIC_API_KEY").is_err() {
        eprintln!("skipping anthropic_live_call: ANTHROPIC_API_KEY not set");
        return;
    }
    let output = baml_test!(
        r#"
        function main() -> string {
            let p = baml.ai.Anthropic {
                model: "claude-haiku-4-5-20251001",
                api_key: baml.env.get_or_panic("ANTHROPIC_API_KEY"),
                base_url: null,
            };
            p.call<string>("Reply with exactly the lowercase word: pong") catch (e) {
                let he: baml.ai.AnthropicHttpError => "HTTP:" + he.status.to_string() + ":" + he.body,
                let u: baml.errors.UnknownError => "ERR:" + u.message.join(","),
                let c: baml.errors.CallError => "CALLERR",
            }
        }
        "#
    );
    let got = output.result.unwrap();
    let BexExternalValue::String(s) = got else {
        panic!("expected string, got {got:?}");
    };
    assert!(
        s.to_lowercase().contains("pong"),
        "live Anthropic reply did not contain 'pong': {s:?}"
    );
}

/// Live structured extraction: `call<Person>` schema-injects, the model replies JSON,
/// SAP decodes into the typed value. Skipped without `ANTHROPIC_API_KEY`.
#[tokio::test]
async fn anthropic_structured_live_call() {
    if std::env::var("ANTHROPIC_API_KEY").is_err() {
        eprintln!("skipping anthropic_structured_live_call: ANTHROPIC_API_KEY not set");
        return;
    }
    let output = baml_test!(
        r#"
        class Person { name string, age int }
        function main() -> string {
            let p = baml.ai.Anthropic {
                model: "claude-haiku-4-5-20251001",
                api_key: baml.env.get_or_panic("ANTHROPIC_API_KEY"),
                base_url: null,
            };
            let person: Person = p.call<Person>("Extract the person: Ada Lovelace, 36 years old.") catch (e) {
                let he: baml.ai.AnthropicHttpError => return "HTTP:" + he.status.to_string() + ":" + he.body,
                let u: baml.errors.UnknownError => return "ERR:" + u.message.join(","),
                let c: baml.errors.CallError => return "CALLERR",
            };
            person.name + "|" + person.age.to_string()
        }
        "#
    );
    let got = output.result.unwrap();
    let BexExternalValue::String(s) = got else {
        panic!("expected string, got {got:?}");
    };
    assert!(
        s.contains("Ada") && s.contains("36"),
        "live structured extraction mismatch: {s:?}"
    );
}

/// Live SSE streaming: partial deltas accumulate, `final()` returns the completed text.
/// Skipped without `ANTHROPIC_API_KEY`.
#[tokio::test]
async fn anthropic_stream_live() {
    if std::env::var("ANTHROPIC_API_KEY").is_err() {
        eprintln!("skipping anthropic_stream_live: ANTHROPIC_API_KEY not set");
        return;
    }
    let output = baml_test!(
        r#"
        function main() -> string {
            let p = baml.ai.Anthropic {
                model: "claude-haiku-4-5-20251001",
                api_key: baml.env.get_or_panic("ANTHROPIC_API_KEY"),
                base_url: null,
            };
            let s = p.stream<string, string>("Count from 1 to 5 as digits separated by spaces.") catch (e) {
                let u: baml.errors.UnknownError => return "ERR:" + u.message.join(","),
            };
            let partials = 0;
            while (partials < 10000) {
                match (s.next()) {
                    baml.stream.StreamFinished => { break; },
                    let part: string => { partials = partials + 1; },
                }
            }
            let final_text = s.final() catch (e) { _ => return "FINAL_ERR" };
            if (partials > 0) { final_text } else { "NO_PARTIALS:" + final_text }
        }
        "#
    );
    let got = output.result.unwrap();
    let BexExternalValue::String(s) = got else {
        panic!("expected string, got {got:?}");
    };
    assert!(
        s.contains('5') && !s.starts_with("NO_PARTIALS") && !s.starts_with("ERR"),
        "live stream failed or produced no partials: {s:?}"
    );
}
