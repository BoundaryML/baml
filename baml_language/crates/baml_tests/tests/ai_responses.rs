//! Tests for the `baml.ai.OpenAiResponses` provider (OpenAI `/v1/responses`).
//!
//! - Mocked: the `call<string>` pipeline and the server-stored `Chain.extend` path
//!   against a wiremock server.
//! - Live (gated on `OPENAI_API_KEY`): a real server-stored chain — `start_chain`
//!   then `extend`, proving scenario 20 (server-stored chains) end to end.

use baml_tests::baml_test;
use bex_engine::BexExternalValue;
use wiremock::{
    Mock, MockServer, ResponseTemplate,
    matchers::{method, path},
};

/// The full `OpenAiResponses.call<string>` pipeline (build_request → send → parse)
/// against a mocked `/responses` endpoint — deterministic, no network/key required.
#[tokio::test]
async fn responses_call_via_mock() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/responses"))
        .respond_with(ResponseTemplate::new(200).set_body_string(
            r#"{"id":"resp_1","output":[{"type":"message","content":[{"type":"output_text","text":"pong"}]}],"usage":{"input_tokens":3,"output_tokens":1}}"#,
        ))
        .mount(&server)
        .await;
    let uri = server.uri();

    let output = baml_test!(&format!(
        r#"
        function main() -> string {{
            let p = baml.ai.OpenAiResponses {{ model: "gpt-5.4-mini", api_key: "test-key", base_url: "{uri}" }};
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

/// Server-stored chain over two mocked turns: `start_chain` then `extend`. The SECOND
/// request body must carry `"previous_response_id":"resp_1"` (scenario 20 on the wire).
#[tokio::test]
async fn responses_chain_via_mock() {
    let server = MockServer::start().await;
    // Turn 1: start_chain — returns a stored response with id resp_1.
    Mock::given(method("POST"))
        .and(path("/responses"))
        .respond_with(ResponseTemplate::new(200).set_body_string(
            r#"{"id":"resp_1","output":[{"type":"message","content":[{"type":"output_text","text":"OK"}]}]}"#,
        ))
        .up_to_n_times(1)
        .mount(&server)
        .await;
    // Turn 2: extend — the continuation.
    Mock::given(method("POST"))
        .and(path("/responses"))
        .respond_with(ResponseTemplate::new(200).set_body_string(
            r#"{"id":"resp_2","output":[{"type":"message","content":[{"type":"output_text","text":"41"}]}]}"#,
        ))
        .mount(&server)
        .await;
    let uri = server.uri();

    let output = baml_test!(&format!(
        r#"
        function main() -> string {{
            let p = baml.ai.OpenAiResponses {{ model: "gpt-5.4-mini", api_key: "test-key", base_url: "{uri}" }};
            let handle = p.start_chain("My favorite number is 41. Remember it. Reply OK.") catch (e) {{
                let u: baml.errors.UnknownError => return "ERR:" + u.message.join(","),
            }};
            let ans: string = p.extend<string>("What is my favorite number?", handle) catch (e) {{
                let u: baml.errors.UnknownError => return "ERR:" + u.message.join(","),
            }};
            handle.id + "|" + ans
        }}
        "#
    ));
    assert_eq!(
        output.result.unwrap(),
        BexExternalValue::String("resp_1|41".into())
    );

    let reqs = server.received_requests().await.unwrap();
    assert_eq!(reqs.len(), 2, "expected exactly two requests");
    let body2 = String::from_utf8_lossy(&reqs[1].body).to_string();
    assert!(
        body2.contains(r#""previous_response_id":"resp_1""#),
        "second request must chain off resp_1; body: {body2}"
    );
}

/// LIVE server-stored chain (scenario 20): teach the model a fact in `start_chain`, then
/// `extend` the stored response and confirm it recalls the fact. Skipped without a key.
#[tokio::test]
async fn responses_live_chain() {
    if std::env::var("OPENAI_API_KEY").is_err() {
        eprintln!("skipping responses_live_chain: OPENAI_API_KEY not set");
        return;
    }
    let output = baml_test!(
        r#"
        function main() -> string {
            let p = baml.ai.OpenAiResponses {
                model: "gpt-5.4-mini",
                api_key: baml.env.get_or_panic("OPENAI_API_KEY"),
                base_url: null,
            };
            let handle = p.start_chain("My favorite number is 41. Remember it. Reply OK.") catch (e) {
                let u: baml.errors.UnknownError => return "ERR:" + u.message.join(","),
            };
            let ans: string = p.extend<string>(
                "What is my favorite number? Reply with just the number.",
                handle,
            ) catch (e) {
                let u: baml.errors.UnknownError => return "ERR:" + u.message.join(","),
            };
            ans
        }
        "#
    );
    let got = output.result.unwrap();
    let BexExternalValue::String(s) = got else {
        panic!("expected string, got {got:?}");
    };
    assert!(
        s.contains("41"),
        "live server-stored chain did not recall the number: {s:?}"
    );
}
