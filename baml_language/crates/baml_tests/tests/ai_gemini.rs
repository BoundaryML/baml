//! Tests for the BAML-native Google Gemini (`generateContent`) provider
//! (`baml.ai.Gemini`).
//!
//! All mocked against a wiremock server — deterministic, no network / API key required.
//! Mirrors the Anthropic provider tests in `ai_anthropic.rs`.

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

/// The full Gemini `call<string>` pipeline (build_request → send → parse) against a
/// mocked `generateContent` endpoint — deterministic, no network/key required.
#[tokio::test]
async fn gemini_call_via_mock() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/models/gemini-2.0-flash:generateContent"))
        .respond_with(ResponseTemplate::new(200).set_body_string(
            r#"{"candidates":[{"content":{"parts":[{"text":"pong"}],"role":"model"},"finishReason":"STOP"}],"usageMetadata":{"promptTokenCount":8,"candidatesTokenCount":1}}"#,
        ))
        .mount(&server)
        .await;
    let uri = server.uri();

    let output = baml_test!(&format!(
        r#"
        function main() -> string {{
            let p = baml.ai.Gemini {{ model: "gemini-2.0-flash", api_key: "test-key", base_url: "{uri}" }};
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

/// Request-capture: assert the wire shape — the `x-goog-api-key` header, the
/// `/models/<model>:generateContent` path, the assistant→`"model"` role remap, the
/// system message hoisted to a top-level `systemInstruction` (NOT in `contents`), and
/// the user content as a `parts: [{text: ...}]` array.
#[tokio::test]
async fn gemini_request_shape_via_mock() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/models/gemini-2.0-flash:generateContent"))
        .respond_with(ResponseTemplate::new(200).set_body_string(
            r#"{"candidates":[{"content":{"parts":[{"text":"ok"}],"role":"model"},"finishReason":"STOP"}]}"#,
        ))
        .mount(&server)
        .await;
    let uri = server.uri();

    let output = baml_test!(&format!(
        r#"
        function main() -> string {{
            let p = baml.ai.Gemini {{ model: "gemini-2.0-flash", api_key: "test-key", base_url: "{uri}" }};
            let history = [
                baml.ai.ChatMessage.system("You are terse."),
                baml.ai.ChatMessage.user("Say hi."),
                baml.ai.ChatMessage.assistant("hi"),
                baml.ai.ChatMessage.user("Again."),
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

    // Header auth: x-goog-api-key.
    let api_key = req
        .headers
        .get("x-goog-api-key")
        .map(|v| v.to_str().unwrap_or("").to_string());
    assert_eq!(
        api_key.as_deref(),
        Some("test-key"),
        "x-goog-api-key header missing/wrong; headers: {:?}",
        req.headers
    );

    // Body shape.
    let body = String::from_utf8_lossy(&req.body).to_string();
    // System hoisted to top-level `systemInstruction`, NOT emitted as a message role.
    assert!(
        body.contains(r#""systemInstruction""#),
        "system not hoisted to top-level `systemInstruction`; body: {body}"
    );
    assert!(
        body.contains(r#""text":"You are terse.""#),
        "system text missing from systemInstruction; body: {body}"
    );
    assert!(
        !body.contains(r#""role":"system""#),
        "system role must NOT appear in contents; body: {body}"
    );
    // Assistant role remapped to Gemini's "model"; user passes through.
    assert!(
        body.contains(r#""role":"model""#),
        "assistant role not remapped to `model`; body: {body}"
    );
    assert!(
        body.contains(r#""role":"user""#),
        "user role missing; body: {body}"
    );
    // User content is a `parts: [{text: ...}]` array.
    assert!(
        body.contains(r#""parts""#) && body.contains(r#""text":"Say hi.""#),
        "user content not a parts/text array; body: {body}"
    );
}

/// Structured output: `call<Resume>` with the schema appended once; the mock returns a
/// markdown-fenced JSON text block in the candidate; SAP decodes it into the typed value.
#[tokio::test]
async fn gemini_structured_via_mock() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/models/gemini-2.0-flash:generateContent"))
        .respond_with(ResponseTemplate::new(200).set_body_string(
            r#"{"candidates":[{"content":{"parts":[{"text":"```json\n{\"name\": \"Ada Lovelace\", \"years\": 36}\n```"}],"role":"model"},"finishReason":"STOP"}]}"#,
        ))
        .mount(&server)
        .await;
    let uri = server.uri();

    let output = baml_test!(&format!(
        r#"
        class Resume {{ name string, years int }}
        function main() -> string {{
            let p = baml.ai.Gemini {{ model: "gemini-2.0-flash", api_key: "test-key", base_url: "{uri}" }};
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

/// D2/D8: a transient 429 is retryable — `with_retry` re-drives past two 429s then a
/// 200 succeeds. Mirrors `anthropic_error_retryable_via_mock`.
#[tokio::test]
async fn gemini_error_retryable_via_mock() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/models/gemini-2.0-flash:generateContent"))
        .respond_with(
            ResponseTemplate::new(429)
                .set_body_string(r#"{"error":{"code":429,"status":"RESOURCE_EXHAUSTED"}}"#),
        )
        .up_to_n_times(2)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/models/gemini-2.0-flash:generateContent"))
        .respond_with(ResponseTemplate::new(200).set_body_string(
            r#"{"candidates":[{"content":{"parts":[{"text":"pong"}],"role":"model"},"finishReason":"STOP"}]}"#,
        ))
        .mount(&server)
        .await;
    let uri = server.uri();

    let output = baml_test!(&format!(
        r#"
        function main() -> string {{
            let p = baml.ai.Gemini {{ model: "gemini-2.0-flash", api_key: "test-key", base_url: "{uri}" }}.with_retry(3);
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

/// The typed `GeminiHttpError` surfaces on a non-retryable status (400) and reports
/// `is_retryable() == false`; a 429 is classified retryable. Exercises the `Failure` axis.
#[tokio::test]
async fn gemini_http_error_is_typed() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/models/gemini-2.0-flash:generateContent"))
        .respond_with(
            ResponseTemplate::new(400)
                .set_body_string(r#"{"error":{"code":400,"status":"INVALID_ARGUMENT"}}"#),
        )
        .mount(&server)
        .await;
    let uri = server.uri();

    let output = baml_test!(&format!(
        r#"
        function main() -> string {{
            let p = baml.ai.Gemini {{ model: "gemini-2.0-flash", api_key: "test-key", base_url: "{uri}" }}.with_retry(3);
            p.call<string>("hi") catch (e) {{
                let he: baml.ai.GeminiHttpError => "http:" + he.status.to_string() + " retryable=" + he.is_retryable().to_string(),
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

/// Value + sidecar: `call_with` returns the answer AND a projected `Usage` from
/// `ResponseMeta`, mapping `usageMetadata.{promptTokenCount,candidatesTokenCount}`.
#[tokio::test]
async fn gemini_usage_meta_via_mock() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/models/gemini-2.0-flash:generateContent"))
        .respond_with(ResponseTemplate::new(200).set_body_string(
            r#"{"candidates":[{"content":{"parts":[{"text":"pong"}],"role":"model"}}],"usageMetadata":{"promptTokenCount":12,"candidatesTokenCount":5}}"#,
        ))
        .mount(&server)
        .await;
    let uri = server.uri();

    let output = baml_test!(&format!(
        r#"
        function main() -> string {{
            let p = baml.ai.Gemini {{ model: "gemini-2.0-flash", api_key: "test-key", base_url: "{uri}" }};
            let r = p.call_with<string, baml.ai.Usage, never>(
                "hi",
                (m: baml.ai.ResponseMeta) -> baml.ai.Usage {{ m.usage() }},
            ) catch (e) {{
                let u: baml.errors.UnknownError => return "ERR:" + u.message.join(","),
            }};
            r.value + "|" + r.meta.input_tokens.to_string() + "|" + r.meta.output_tokens.to_string()
        }}
        "#
    ));
    assert_eq!(
        output.result.unwrap(),
        BexExternalValue::String("pong|12|5".into())
    );
}

// ───────────────────────────── live tier (gated) ─────────────────────────────

/// Live smoke test against the real Gemini API. Skipped unless `GOOGLE_API_KEY` is
/// set — e.g. `infisical run --env=test -- cargo test -p baml_tests --test ai_gemini`.
#[tokio::test]
async fn gemini_live_call() {
    if std::env::var("GOOGLE_API_KEY").is_err() {
        eprintln!("skipping gemini_live_call: GOOGLE_API_KEY not set");
        return;
    }
    let output = baml_test!(
        r#"
        function main() -> string {
            // with_retry absorbs a first-connect transport flake seen on this host
            // (reqwest's initial connection to generativelanguage.googleapis.com can
            // fail where curl's happy-eyeballs succeeds; the retry lands).
            let p = baml.ai.Gemini {
                model: "gemini-2.5-flash",
                api_key: baml.env.get_or_panic("GOOGLE_API_KEY"),
                base_url: null,
            }.with_retry(2);
            p.call<string>("Reply with exactly the lowercase word: pong") catch (e) {
                let he: baml.ai.GeminiHttpError => "HTTP:" + he.status.to_string() + ":" + he.body,
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
        "live Gemini reply did not contain 'pong': {s:?}"
    );
}

/// Live structured extraction: `call<Person>` schema-injects, SAP decodes the reply.
/// Skipped without `GOOGLE_API_KEY`.
#[tokio::test]
async fn gemini_structured_live_call() {
    if std::env::var("GOOGLE_API_KEY").is_err() {
        eprintln!("skipping gemini_structured_live_call: GOOGLE_API_KEY not set");
        return;
    }
    let output = baml_test!(
        r#"
        class Person { name string, age int }
        function main() -> string {
            let p = baml.ai.Gemini {
                model: "gemini-2.5-flash",
                api_key: baml.env.get_or_panic("GOOGLE_API_KEY"),
                base_url: null,
            }.with_retry(2);
            let person: Person = p.call<Person>("Extract the person: Ada Lovelace, 36 years old.") catch (e) {
                let he: baml.ai.GeminiHttpError => return "HTTP:" + he.status.to_string() + ":" + he.body,
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

/// Streaming via `:streamGenerateContent?alt=sse` against a mocked SSE endpoint:
/// the host accumulator's google-ai arm concatenates `candidates[0].content.
/// parts[*].text` and finishes on `finishReason` (no `[DONE]` sentinel).
#[tokio::test]
async fn gemini_stream_via_mock() {
    let server = MockServer::start().await;
    let sse_body = "data: {\"candidates\":[{\"content\":{\"parts\":[{\"text\":\"po\"}],\"role\":\"model\"},\"index\":0}]}\n\n\
                    data: {\"candidates\":[{\"content\":{\"parts\":[{\"text\":\"ng\"}],\"role\":\"model\"},\"finishReason\":\"STOP\",\"index\":0}],\"usageMetadata\":{\"promptTokenCount\":5,\"candidatesTokenCount\":2}}\n\n";
    Mock::given(method("POST"))
        .and(path("/models/gemini-2.0-flash:streamGenerateContent"))
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
            let p = baml.ai.Gemini {{ model: "gemini-2.0-flash", api_key: "test-key", base_url: "{uri}" }};
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

/// Live streaming against the real Gemini API (skips without GOOGLE_API_KEY).
#[tokio::test]
async fn gemini_stream_live() {
    if std::env::var("GOOGLE_API_KEY").is_err() {
        eprintln!("skipping gemini_stream_live: GOOGLE_API_KEY not set");
        return;
    }
    let output = baml_test!(
        r#"
        function main() -> string {
            let p = baml.ai.Gemini {
                model: "gemini-2.5-flash",
                api_key: baml.env.get_or_panic("GOOGLE_API_KEY"),
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
            s.final() catch (e) { _ => "FINAL_ERR" }
        }
        "#
    );
    let got = output.result.unwrap();
    let BexExternalValue::String(s) = got else {
        panic!("expected string, got {got:?}");
    };
    assert!(
        s.contains('5') && !s.starts_with("ERR") && !s.starts_with("FINAL_ERR"),
        "live gemini stream final unexpected: {s:?}"
    );
}
