//! Tests for the OpenAI-compatible generic provider (`baml.ai.OpenAiCompatible`):
//! the Chat Completions wire shape against an arbitrary `base_url` with a TYPED
//! auth field (Bearer / custom header / none) instead of a hardwired key.
//!
//! All mocked against a wiremock server — deterministic, no network / API key
//! required. The delegate pipeline is `OpenAi`'s, so only the auth/URL seams and
//! the streaming path need dedicated coverage here.

use baml_tests::baml_test;
use bex_engine::BexExternalValue;
use wiremock::{
    Mock, MockServer, ResponseTemplate,
    matchers::{method, path},
};

const CHAT_OK: &str = r#"{"choices":[{"message":{"content":"pong"},"finish_reason":"stop"}],"usage":{"prompt_tokens":3,"completion_tokens":1}}"#;

/// Bearer auth: `Authorization: Bearer <token>` rides the request; the OpenAi
/// parse pipeline handles the response unchanged.
#[tokio::test]
async fn compat_bearer_auth_via_mock() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_string(CHAT_OK))
        .mount(&server)
        .await;
    let uri = server.uri();

    let output = baml_test!(&format!(
        r#"
        function main() -> string {{
            let p = baml.ai.OpenAiCompatible {{
                model: "llama-3.1-8b",
                base_url: "{uri}",
                auth: baml.ai.BearerAuth {{ token: "sk-proxy" }},
            }};
            p.call<string>("ping") catch (e) {{
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

    let reqs = server.received_requests().await.unwrap_or_default();
    assert_eq!(reqs.len(), 1);
    assert_eq!(
        reqs[0]
            .headers
            .get("authorization")
            .and_then(|v| v.to_str().ok()),
        Some("Bearer sk-proxy"),
        "Authorization header missing/wrong; headers: {:?}",
        reqs[0].headers
    );
    let body = String::from_utf8_lossy(&reqs[0].body).to_string();
    assert!(
        body.contains(r#""model":"llama-3.1-8b""#),
        "model missing from body: {body}"
    );
}

/// Custom header auth (`api-key: …` proxies): the named header rides the
/// request and NO Authorization header is sent.
#[tokio::test]
async fn compat_header_auth_via_mock() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_string(CHAT_OK))
        .mount(&server)
        .await;
    let uri = server.uri();

    let output = baml_test!(&format!(
        r#"
        function main() -> string {{
            let p = baml.ai.OpenAiCompatible {{
                model: "m",
                base_url: "{uri}",
                auth: baml.ai.HeaderAuth {{ name: "api-key", value: "abc123" }},
            }};
            p.call<string>("ping") catch (e) {{
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

    let reqs = server.received_requests().await.unwrap_or_default();
    assert_eq!(reqs.len(), 1);
    assert_eq!(
        reqs[0].headers.get("api-key").and_then(|v| v.to_str().ok()),
        Some("abc123"),
        "api-key header missing/wrong; headers: {:?}",
        reqs[0].headers
    );
    assert!(
        reqs[0].headers.get("authorization").is_none(),
        "no Authorization header expected with HeaderAuth; headers: {:?}",
        reqs[0].headers
    );
}

/// NoAuth (local runtimes): the request carries neither Authorization nor any
/// custom auth header — just content-type.
#[tokio::test]
async fn compat_no_auth_via_mock() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_string(CHAT_OK))
        .mount(&server)
        .await;
    let uri = server.uri();

    let output = baml_test!(&format!(
        r#"
        function main() -> string {{
            let p = baml.ai.OpenAiCompatible {{
                model: "local-model",
                base_url: "{uri}",
                auth: baml.ai.NoAuth {{}},
            }};
            p.call<string>("ping") catch (e) {{
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

    let reqs = server.received_requests().await.unwrap_or_default();
    assert_eq!(reqs.len(), 1);
    assert!(
        reqs[0].headers.get("authorization").is_none(),
        "NoAuth must not send Authorization; headers: {:?}",
        reqs[0].headers
    );
}

/// Streaming over the compat endpoint: chat-completions deltas accumulate via
/// the openai-generic host accumulator.
#[tokio::test]
async fn compat_stream_via_mock() {
    let server = MockServer::start().await;
    let sse_body = "data: {\"choices\":[{\"index\":0,\"delta\":{\"content\":\"po\"}}]}\n\n\
                    data: {\"choices\":[{\"index\":0,\"delta\":{\"content\":\"ng\"}}]}\n\n\
                    data: {\"choices\":[{\"index\":0,\"delta\":{},\"finish_reason\":\"stop\"}]}\n\n\
                    data: [DONE]\n\n";
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

    let output = baml_test!(&format!(
        r#"
        function main() -> string {{
            let p = baml.ai.OpenAiCompatible {{
                model: "local-model",
                base_url: "{uri}",
                auth: baml.ai.NoAuth {{}},
            }};
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

    let reqs = server.received_requests().await.unwrap_or_default();
    let body = String::from_utf8_lossy(&reqs[0].body).to_string();
    assert!(
        body.contains(r#""stream":true"#),
        "stream flag missing from body: {body}"
    );
}

/// A non-2xx from the compat endpoint surfaces as the TYPED OpenAiHttpError
/// (the delegate's error normalization) on the CallError channel.
#[tokio::test]
async fn compat_http_error_is_typed() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/chat/completions"))
        .respond_with(ResponseTemplate::new(429).set_body_string(r#"{"error":"slow down"}"#))
        .mount(&server)
        .await;
    let uri = server.uri();

    let output = baml_test!(&format!(
        r#"
        function main() -> string {{
            let p = baml.ai.OpenAiCompatible {{
                model: "m",
                base_url: "{uri}",
                auth: baml.ai.NoAuth {{}},
            }};
            p.call<string>("ping") catch (e) {{
                let he: baml.ai.OpenAiHttpError => "TYPED:" + he.status.to_string(),
                let c: baml.errors.CallError => "CALLERR",
                let u: baml.errors.UnknownError => "ERR",
            }}
        }}
        "#
    ));
    assert_eq!(
        output.result.unwrap(),
        BexExternalValue::String("TYPED:429".into())
    );
}

/// A DECLARED `client<llm> { provider openai-generic }` config now routes
/// through the native OpenAiCompatible class via `native_provider_for`
/// (api_key -> Bearer auth); without a base_url or model it stays legacy.
#[tokio::test]
async fn compat_declared_client_routes_natively() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_string(CHAT_OK))
        .mount(&server)
        .await;
    let uri = server.uri();

    let output = baml_test!(&format!(
        r##"
        client<llm> LocalCompat {{
            provider openai-generic
            options {{
                model "local-m"
                base_url "{uri}"
                api_key "k123"
            }}
        }}

        function Ask(q: string) -> string {{
            client LocalCompat
            prompt #"{{{{ q }}}}"#
        }}

        function main() -> string {{
            Ask("ping") catch (e) {{ _ => "ERR" }}
        }}
        "##
    ));
    assert_eq!(
        output.result.unwrap(),
        BexExternalValue::String("pong".into())
    );

    let reqs = server.received_requests().await.unwrap_or_default();
    assert_eq!(reqs.len(), 1);
    assert_eq!(
        reqs[0]
            .headers
            .get("authorization")
            .and_then(|v| v.to_str().ok()),
        Some("Bearer k123"),
        "declared openai-generic client should Bearer-auth via the native class; headers: {:?}",
        reqs[0].headers
    );
    let body = String::from_utf8_lossy(&reqs[0].body).to_string();
    assert!(
        body.contains(r#""model":"local-m""#),
        "native chat-completions body expected: {body}"
    );
}
