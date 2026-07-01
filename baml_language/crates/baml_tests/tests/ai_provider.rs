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
