//! Tests for the `baml.ai` provider/capability spine and the OpenAI provider.
//!
//! - Offline: capability negotiation + error normalization via `baml_test!`.
//! - Mocked: the full OpenAI `call<string>` pipeline against a wiremock server.
//! - Live (gated on `OPENAI_API_KEY`): a real `gpt-5.4-mini` call.

use baml_tests::baml_test;
use bex_engine::BexExternalValue;
use wiremock::{
    Mock, MockServer, ResponseTemplate,
    matchers::{method, path},
};

/// `OpenAi` implements the `Provider` marker and the `HttpProvider` capability, so
/// a value upcast to the existential `Provider` matches `HttpProvider` at runtime.
#[tokio::test]
async fn openai_implements_capabilities() {
    let output = baml_test!(
        r#"
        function main() -> bool {
            let p: baml.ai.Provider = baml.ai.OpenAi { model: "m", api_key: "k", base_url: null };
            match (p) {
                let h: baml.ai.HttpProvider => true,
                _ => false,
            }
        }
        "#
    );
    assert_eq!(output.result.unwrap(), BexExternalValue::Bool(true));
}

/// A deliberately-thrown `CallError` keeps its concrete identity through a
/// normalizing `catch`, while a foreign error boxes into `UnknownError`.
#[tokio::test]
async fn capability_error_normalization() {
    let output = baml_test!(
        r#"
        class RateLimit {
            retry_after int
            implements baml.errors.CallError {
                function is_network_error(self) -> bool { false }
                function is_rate_limit(self) -> bool { true }
                function is_parse_error(self) -> bool { false }
            }
        }
        class Foreign { code int }

        function raw(rate_limited: bool) -> int throws RateLimit | Foreign {
            if (rate_limited) { throw RateLimit { retry_after: 5 } } else { throw Foreign { code: 7 } }
        }

        // normalize: known CallError passes through; foreign boxes into UnknownError.
        function guarded(rate_limited: bool) -> int throws baml.errors.CallError | baml.errors.UnknownError {
            raw(rate_limited) catch (e) {
                let c: baml.errors.CallError => throw c,
                _ => throw baml.errors.UnknownError { data: e, message: ["guarded failed"] },
            }
        }

        function describe(rate_limited: bool) -> string {
            let n: int = guarded(rate_limited) catch (e) {
                let r: RateLimit => return "rl:" + r.retry_after.to_string(),
                let u: baml.errors.UnknownError => return "boxed:" + u.message.join(","),
                let c: baml.errors.CallError => return "iface",
            };
            "ok:" + n.to_string()
        }

        function main() -> string {
            describe(true) + "|" + describe(false)
        }
        "#
    );
    assert_eq!(
        output.result.unwrap(),
        BexExternalValue::String("rl:5|boxed:guarded failed".into())
    );
    // describe(true) => concrete RateLimit passed through => "rl:5"
    // describe(false) => foreign Foreign boxed into UnknownError => "boxed:guarded failed"
}

/// The full OpenAI `call<string>` pipeline (build_request → send → parse) against a
/// mocked Chat Completions endpoint — deterministic, no network/key required.
#[tokio::test]
async fn openai_call_via_mock() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_string(
            r#"{"choices":[{"message":{"content":"pong"},"finish_reason":"stop"}]}"#,
        ))
        .mount(&server)
        .await;
    let uri = server.uri();

    let output = baml_test!(&format!(
        r#"
        function main() -> string {{
            let p = baml.ai.OpenAi {{ model: "gpt-5.4-mini", api_key: "test-key", base_url: "{uri}" }};
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

/// Live smoke test against the real OpenAI API. Skipped unless `OPENAI_API_KEY` is set.
#[tokio::test]
async fn openai_live_call() {
    if std::env::var("OPENAI_API_KEY").is_err() {
        eprintln!("skipping openai_live_call: OPENAI_API_KEY not set");
        return;
    }
    let output = baml_test!(
        r#"
        function main() -> string {
            let p = baml.ai.OpenAi {
                model: "gpt-5.4-mini",
                api_key: baml.env.get_or_panic("OPENAI_API_KEY"),
                base_url: null,
            };
            p.call<string>("Reply with exactly the lowercase word: pong") catch (e) {
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
        "live model reply did not contain 'pong': {s:?}"
    );
}

/// Structured output: `call<Resume>` against a mocked response whose content is a
/// markdown-fenced JSON object. SAP decodes it into the typed value.
#[tokio::test]
async fn openai_structured_via_mock() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_string(
            r#"{"choices":[{"message":{"content":"```json\n{\"name\": \"Ada Lovelace\", \"years\": 36}\n```"},"finish_reason":"stop"}]}"#,
        ))
        .mount(&server)
        .await;
    let uri = server.uri();

    let output = baml_test!(&format!(
        r#"
        class Resume {{ name string, years int }}
        function main() -> string {{
            let p = baml.ai.OpenAi {{ model: "gpt-5.4-mini", api_key: "test-key", base_url: "{uri}" }};
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
}

/// Live structured output against the real API. Skipped unless `OPENAI_API_KEY` is set.
#[tokio::test]
async fn openai_structured_live_call() {
    if std::env::var("OPENAI_API_KEY").is_err() {
        eprintln!("skipping openai_structured_live_call: OPENAI_API_KEY not set");
        return;
    }
    let output = baml_test!(
        r#"
        class Person { name string, age int }
        function main() -> string {
            let p = baml.ai.OpenAi {
                model: "gpt-5.4-mini",
                api_key: baml.env.get_or_panic("OPENAI_API_KEY"),
                base_url: null,
            };
            let r: Person = p.call<Person>("Ada Lovelace is 36 years old. Extract her as a Person.") catch (e) {
                let u: baml.errors.UnknownError => return "ERR:" + u.message.join(","),
                let c: baml.errors.CallError => return "CALLERR",
            };
            r.name + "|" + r.age.to_string()
        }
        "#
    );
    let got = output.result.unwrap();
    let BexExternalValue::String(s) = got else {
        panic!("expected string, got {got:?}");
    };
    assert!(
        s.to_lowercase().contains("ada") && s.contains("36"),
        "live structured reply unexpected: {s:?}"
    );
}

/// END-TO-END WIRING: a real user-declared `client<llm>` + LLM `function` executes
/// through the new `baml.ai.OpenAi` provider (orchestrator delegation), against a mock.
#[tokio::test]
async fn e2e_client_function_via_new_provider_mock() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_string(
            r#"{"choices":[{"message":{"content":"pong"},"finish_reason":"stop"}]}"#,
        ))
        .mount(&server)
        .await;
    let uri = server.uri();

    let output = baml_test!(&format!(
        r#"
        client<llm> MockClient {{
          provider openai
          options {{ model "gpt-5.4-mini" api_key "test-key" base_url "{uri}" }}
        }}

        function Ask(question: string) -> string {{
          client MockClient
          prompt `Answer this: ${{question}}`
        }}

        function main() -> string {{
          Ask("Reply with exactly the word: pong")
        }}
        "#
    ));
    assert_eq!(
        output.result.unwrap(),
        BexExternalValue::String("pong".into())
    );
}

/// END-TO-END WIRING, live: a real client + LLM function through the new provider,
/// hitting the real API. Skipped unless `OPENAI_API_KEY` is set.
#[tokio::test]
async fn e2e_client_function_via_new_provider_live() {
    if std::env::var("OPENAI_API_KEY").is_err() {
        eprintln!("skipping e2e_client_function_via_new_provider_live: OPENAI_API_KEY not set");
        return;
    }
    let output = baml_test!(
        r#"
        client<llm> LiveClient {
          provider openai
          options { model "gpt-5.4-mini" api_key env.OPENAI_API_KEY }
        }

        function Ask(question: string) -> string {
          client LiveClient
          prompt `${question}`
        }

        function main() -> string {
          Ask("Reply with exactly the lowercase word: pong")
        }
        "#
    );
    let got = output.result.unwrap();
    let BexExternalValue::String(s) = got else {
        panic!("expected string, got {got:?}");
    };
    assert!(
        s.to_lowercase().contains("pong"),
        "live e2e reply did not contain 'pong': {s:?}"
    );
}

/// `fallback_to` routes to the first HTTP-capable member that succeeds: a broken
/// provider (connection refused) falls through to a working mocked one.
#[tokio::test]
async fn fallback_routes_to_first_working_member() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/chat/completions"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string(r#"{"choices":[{"message":{"content":"pong"}}]}"#),
        )
        .mount(&server)
        .await;
    let uri = server.uri();

    let output = baml_test!(&format!(
        r#"
        function main() -> string {{
            let broken = baml.ai.OpenAi {{ model: "m", api_key: "k", base_url: "http://127.0.0.1:1/v1" }};
            let good   = baml.ai.OpenAi {{ model: "m", api_key: "k", base_url: "{uri}" }};
            let fb = broken.fallback_to(good);
            fb.call<string>("hi") catch (e) {{
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

/// `with_retry` re-drives the wrapped provider: two 500s (which fail parsing) then a
/// 200 succeeds within the retry budget.
#[tokio::test]
async fn retry_recovers_after_transient_failures() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/chat/completions"))
        .respond_with(ResponseTemplate::new(500))
        .up_to_n_times(2)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/chat/completions"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string(r#"{"choices":[{"message":{"content":"pong"}}]}"#),
        )
        .mount(&server)
        .await;
    let uri = server.uri();

    let output = baml_test!(&format!(
        r#"
        function main() -> string {{
            let p = baml.ai.OpenAi {{ model: "m", api_key: "k", base_url: "{uri}" }}.with_retry(3);
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

/// `with_retry` gives up and surfaces the error after exhausting its budget.
#[tokio::test]
async fn retry_exhausts_and_throws() {
    let output = baml_test!(
        r#"
        function main() -> string {
            let p = baml.ai.OpenAi { model: "m", api_key: "k", base_url: "http://127.0.0.1:1/v1" }.with_retry(2);
            p.call<string>("hi") catch (e) {
                let u: baml.errors.UnknownError => "ERR",
            }
        }
        "#
    );
    assert_eq!(
        output.result.unwrap(),
        BexExternalValue::String("ERR".into())
    );
}
