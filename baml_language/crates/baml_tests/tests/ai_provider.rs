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
            implements baml.errors.Failure {
                function is_retryable(self) -> bool { true }
                function is_effectful(self) -> bool { false }
                function is_policy_refusal(self) -> bool { false }
                function is_resumable(self) -> bool { false }
                function is_unsupported(self) -> bool { false }
            }
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

/// Live streaming through `OpenAi.stream`: drain partials, assert the final value.
#[tokio::test]
async fn openai_stream_live() {
    if std::env::var("OPENAI_API_KEY").is_err() {
        eprintln!("skipping openai_stream_live: OPENAI_API_KEY not set");
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
            let s = p.stream<string, string>("Reply with exactly the lowercase word: streamed") catch (e) {
                let u: baml.errors.UnknownError => return "ERR:" + u.message.join(","),
            };
            let n = 0;
            while (n < 500) {
                match (s.next()) {
                    baml.stream.StreamFinished => { break; },
                    let part: string => { n = n + 1; },
                }
            }
            s.final() catch (e) { _ => "FINAL_ERR" }
        }
        "#
    );
    let got = output.result.unwrap();
    let BexExternalValue::String(s) = got else {
        panic!("expected string, got {got:?}")
    };
    assert!(
        s.to_lowercase().contains("streamed"),
        "stream final unexpected: {s:?}"
    );
}

/// Streaming through a mocked SSE endpoint: OpenAI-format deltas accumulate to the final.
#[tokio::test]
async fn openai_stream_via_mock() {
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
            let p = baml.ai.OpenAi {{ model: "gpt-5.4-mini", api_key: "test-key", base_url: "{uri}" }};
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

/// E2E streaming: a user LLM function's `.stream()` companion routes through the new
/// provider's `Streaming` capability (orchestrator delegation), against a mocked SSE endpoint.
#[tokio::test]
async fn e2e_function_stream_via_new_provider_mock() {
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
        client<llm> MockClient {{
          provider openai
          options {{ model "gpt-5.4-mini" api_key "test-key" base_url "{uri}" }}
        }}
        function Ask(question: string) -> string {{
          client MockClient
          prompt `${{question}}`
        }}
        function main() -> string {{
            let s = Ask$stream("hi");
            let n = 0;
            while (n < 100) {{
                match (s.next()) {{
                    baml.stream.StreamFinished => {{ break; }},
                    let part: string => {{ n = n + 1; }},
                    null => {{ n = n + 1; }},
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

/// Value + sidecar (scenarios 32/34): `call_with` returns the answer AND a projected
/// `Usage` from `ResponseMeta`. The mock returns a `usage` block.
#[tokio::test]
async fn call_with_projects_usage() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_string(
            r#"{"choices":[{"message":{"content":"pong"}}],"usage":{"prompt_tokens":12,"completion_tokens":5}}"#,
        ))
        .mount(&server)
        .await;
    let uri = server.uri();

    let output = baml_test!(&format!(
        r#"
        function main() -> string {{
            let p = baml.ai.OpenAi {{ model: "m", api_key: "k", base_url: "{uri}" }};
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

/// Provider diversity (scenario 28): "routing is an ordinary function returning a
/// Provider" and "a proxy is the same class with a different base_url". A router picks
/// between two OpenAI-compatible endpoints by tier.
#[tokio::test]
async fn provider_diversity_routing() {
    let premium = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/chat/completions"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string(r#"{"choices":[{"message":{"content":"premium"}}]}"#),
        )
        .mount(&premium)
        .await;
    let basic = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/chat/completions"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string(r#"{"choices":[{"message":{"content":"basic"}}]}"#),
        )
        .mount(&basic)
        .await;
    let (pu, bu) = (premium.uri(), basic.uri());

    let output = baml_test!(&format!(
        r#"
        // routing = a function returning a Provider (client-as-a-function).
        function route(tier: string, premium_url: string, basic_url: string) -> baml.ai.Provider {{
            if (tier == "premium") {{
                baml.ai.OpenAi {{ model: "gpt-5.4-mini", api_key: "k", base_url: premium_url }}
            }} else {{
                baml.ai.OpenAi {{ model: "gpt-4o-mini", api_key: "k", base_url: basic_url }}
            }}
        }}
        function ask(tier: string) -> string {{
            let p = route(tier, "{pu}", "{bu}");
            match (p) {{
                let h: baml.ai.HttpProvider => h.call<string>("hi"),
                _ => "NO_HTTP",
            }} catch (e) {{
                let u: baml.errors.UnknownError => "ERR",
            }}
        }}
        function main() -> string {{
            ask("premium") + "|" + ask("basic")
        }}
        "#
    ));
    assert_eq!(
        output.result.unwrap(),
        BexExternalValue::String("premium|basic".into())
    );
}

/// Tool loop (scenario 09) via a mocked 2-turn exchange: turn 1 returns a tool_call,
/// the dispatcher answers, turn 2 returns the final text.
#[tokio::test]
async fn tools_loop_via_mock() {
    let server = MockServer::start().await;
    // Turn 1: the model asks to call get_weather.
    Mock::given(method("POST")).and(path("/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_string(
            r#"{"choices":[{"message":{"content":null,"tool_calls":[{"id":"call_1","type":"function","function":{"name":"get_weather","arguments":"{\"location\":\"Paris\"}"}}]}}]}"#))
        .up_to_n_times(1)
        .mount(&server).await;
    // Turn 2: the final answer.
    Mock::given(method("POST"))
        .and(path("/chat/completions"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string(r#"{"choices":[{"message":{"content":"Paris is sunny, 22C."}}]}"#),
        )
        .mount(&server)
        .await;
    let uri = server.uri();

    let output = baml_test!(&format!(
        r#"
        function main() -> string {{
            let p = baml.ai.OpenAi {{ model: "m", api_key: "k", base_url: "{uri}" }};
            let tools = [
                baml.ai.Tool {{
                    name: "get_weather",
                    description: "Get the current weather for a location",
                    parameters: "{{\"type\":\"object\",\"properties\":{{\"location\":{{\"type\":\"string\"}}}},\"required\":[\"location\"]}}",
                }},
            ];
            p.run_tools<string>(
                "What is the weather in Paris?",
                tools,
                (calls: baml.ai.ToolCall[]) -> baml.ai.ToolResult[] {{
                    let results: baml.ai.ToolResult[] = [];
                    for (let c in calls) {{
                        let out = if (c.name == "get_weather") {{ "sunny, 22C" }} else {{ "unknown" }};
                        results.push(baml.ai.ToolResult {{ id: c.id, output: out }});
                    }}
                    results
                }},
            ) catch (e) {{
                let u: baml.errors.UnknownError => "ERR:" + u.message.join(","),
            }}
        }}
        "#
    ));
    assert_eq!(
        output.result.unwrap(),
        BexExternalValue::String("Paris is sunny, 22C.".into())
    );
}

/// Live agentic tool loop against the real API. Skipped unless `OPENAI_API_KEY` is set.
#[tokio::test]
async fn tools_loop_live() {
    if std::env::var("OPENAI_API_KEY").is_err() {
        eprintln!("skipping tools_loop_live: OPENAI_API_KEY not set");
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
            let tools = [
                baml.ai.Tool {
                    name: "get_weather",
                    description: "Get the current weather for a location",
                    parameters: "{\"type\":\"object\",\"properties\":{\"location\":{\"type\":\"string\"}},\"required\":[\"location\"]}",
                },
            ];
            p.run_tools<string>(
                "What is the weather in Paris? Use the get_weather tool, then answer in one short sentence.",
                tools,
                (calls: baml.ai.ToolCall[]) -> baml.ai.ToolResult[] {
                    let results: baml.ai.ToolResult[] = [];
                    for (let c in calls) {
                        let out = if (c.name == "get_weather") { "sunny and 22 degrees C" } else { "unknown" };
                        results.push(baml.ai.ToolResult { id: c.id, output: out });
                    }
                    results
                },
            ) catch (e) {
                let u: baml.errors.UnknownError => "ERR:" + u.message.join(","),
            }
        }
        "#
    );
    let got = output.result.unwrap();
    let BexExternalValue::String(s) = got else {
        panic!("expected string, got {got:?}")
    };
    assert!(
        s.contains("22") || s.to_lowercase().contains("sunny"),
        "live tool loop answer did not reflect the tool result: {s:?}"
    );
}

/// Cascade & routing (scenario 30): a cheap provider answers, but escalates to an
/// expensive one when it signals low confidence (here, the sentinel "ESCALATE"). Cascade
/// is a Fallback-shaped pattern; routing is an ordinary function returning a Provider.
#[tokio::test]
async fn cascade_escalates_on_low_confidence() {
    // cheap model: returns ESCALATE for the hard question, a direct answer otherwise.
    let cheap = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/chat/completions"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string(r#"{"choices":[{"message":{"content":"ESCALATE"}}]}"#),
        )
        .mount(&cheap)
        .await;
    let expensive = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/chat/completions"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string(r#"{"choices":[{"message":{"content":"expert answer"}}]}"#),
        )
        .mount(&expensive)
        .await;
    let (cu, eu) = (cheap.uri(), expensive.uri());

    let output = baml_test!(&format!(
        r#"
        // A cascade: try the cheap provider; if it signals ESCALATE, fall up to the expensive one.
        function cascade(prompt: string, cheap_url: string, expensive_url: string) -> string
            throws baml.errors.CallError | baml.errors.UnknownError {{
            let cheap = baml.ai.OpenAi {{ model: "gpt-4o-mini", api_key: "k", base_url: cheap_url }};
            let expensive = baml.ai.OpenAi {{ model: "gpt-5.4-mini", api_key: "k", base_url: expensive_url }};
            let first = cheap.call<string>(prompt);
            if (first == "ESCALATE") {{
                expensive.call<string>(prompt)
            }} else {{
                first
            }}
        }}
        function main() -> string {{
            cascade("a hard question", "{cu}", "{eu}") catch (e) {{
                let u: baml.errors.UnknownError => "ERR",
            }}
        }}
        "#
    ));
    assert_eq!(
        output.result.unwrap(),
        BexExternalValue::String("expert answer".into())
    );
}

/// RoundRobin combinator load-balances across members via a mutable cursor.
#[tokio::test]
async fn round_robin_alternates_members() {
    let a = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/chat/completions"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string(r#"{"choices":[{"message":{"content":"A"}}]}"#),
        )
        .mount(&a)
        .await;
    let b = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/chat/completions"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string(r#"{"choices":[{"message":{"content":"B"}}]}"#),
        )
        .mount(&b)
        .await;
    let (au, bu) = (a.uri(), b.uri());

    let output = baml_test!(&format!(
        r#"
        function main() -> string {{
            let rr = baml.ai.RoundRobin {{
                members: [
                    baml.ai.OpenAi {{ model: "m", api_key: "k", base_url: "{au}" }},
                    baml.ai.OpenAi {{ model: "m", api_key: "k", base_url: "{bu}" }},
                ],
                counter: 0,
            }};
            let r1 = rr.call<string>("hi") catch (e) {{ let u: baml.errors.UnknownError => "E" }};
            let r2 = rr.call<string>("hi") catch (e) {{ let u: baml.errors.UnknownError => "E" }};
            let r3 = rr.call<string>("hi") catch (e) {{ let u: baml.errors.UnknownError => "E" }};
            r1 + "," + r2 + "," + r3
        }}
        "#
    ));
    assert_eq!(
        output.result.unwrap(),
        BexExternalValue::String("A,B,A".into())
    );
}

/// Constrained decoding (scenario 03): `OpenAi` does NOT implement `Constrained`, so a
/// function demanding a by-construction guarantee falls to the `_` arm and throws
/// `Unsupported` — the capability is a runtime promise (design gap B1), no fake guarantee.
#[tokio::test]
async fn constrained_capability_absent_is_runtime_promise() {
    let output = baml_test!(
        r#"
        function classify(p: baml.ai.Provider, text: string, pattern: string) -> string
            throws baml.errors.CallError | baml.errors.UnknownError {
            match (p) {
                let c: baml.ai.Constrained => c.decode<string>(text, pattern),
                _ => throw baml.errors.Unsupported { message: "client cannot guarantee constrained decoding" },
            }
        }
        function main() -> string {
            let p: baml.ai.Provider = baml.ai.OpenAi { model: "m", api_key: "k", base_url: null };
            classify(p, "hi", "(yes|no)") catch (e) {
                let un: baml.errors.Unsupported => "unsupported",
                let u: baml.errors.UnknownError => "err",
                let c: baml.errors.CallError => "callerr",
            }
        }
        "#
    );
    assert_eq!(
        output.result.unwrap(),
        BexExternalValue::String("unsupported".into())
    );
}

/// Enriched outputs (scenarios 07/08): reasoning + logprobs as `ResponseMeta` dimensions
/// projected via `call_with`. Reasoning is `Unavailable` on chat; logprobs parse when present.
#[tokio::test]
async fn response_meta_reasoning_and_logprobs() {
    let server = MockServer::start().await;
    Mock::given(method("POST")).and(path("/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_string(
            r#"{"choices":[{"message":{"content":"pong"},"logprobs":{"content":[{"token":"po","logprob":-0.1},{"token":"ng","logprob":-0.2}]}}]}"#))
        .mount(&server).await;
    let uri = server.uri();

    let output = baml_test!(&format!(
        r#"
        function main() -> string {{
            let p = baml.ai.OpenAi {{ model: "m", api_key: "k", base_url: "{uri}" }};
            let r1 = p.call_with<string, string, never>("hi", (m: baml.ai.ResponseMeta) -> string {{
                match (m.logprobs()) {{
                    let lps: baml.ai.Logprob[] => "logprobs:" + lps.length().to_string(),
                    let un: baml.ai.Unavailable => "logprobs_unavailable",
                }}
            }}) catch (e) {{ let u: baml.errors.UnknownError => return "ERR1" }};
            let r2 = p.call_with<string, string, never>("hi", (m: baml.ai.ResponseMeta) -> string {{
                match (m.reasoning()) {{
                    let s: string => "reasoning:" + s,
                    let un: baml.ai.Unavailable => "reasoning_unavailable",
                }}
            }}) catch (e) {{ let u: baml.errors.UnknownError => return "ERR2" }};
            r1.meta + "|" + r2.meta
        }}
        "#
    ));
    assert_eq!(
        output.result.unwrap(),
        BexExternalValue::String("logprobs:2|reasoning_unavailable".into())
    );
}

/// Realtime capability negotiation (scenarios 22–26): a realtime provider matches
/// `Realtime`/`LiveControl`; an HTTP-only provider does not (no fake `call`, OQ1).
#[tokio::test]
async fn realtime_capability_negotiation() {
    let output = baml_test!(
        r#"
        function main() -> string {
            let rt: baml.ai.Provider = baml.ai.OpenAiRealtime { voice: "alloy", api_key: "k" };
            let chat: baml.ai.Provider = baml.ai.OpenAi { model: "m", api_key: "k", base_url: null };
            let a = match (rt) { let r: baml.ai.Realtime => "realtime", _ => "no" };
            let b = match (chat) { let r: baml.ai.Realtime => "realtime", _ => "no" };
            let c = match (rt) { let lc: baml.ai.LiveControl => "live", _ => "no" };
            a + "|" + b + "|" + c
        }
        "#
    );
    assert_eq!(
        output.result.unwrap(),
        BexExternalValue::String("realtime|no|live".into())
    );
}

/// Stateful capabilities (17/27/44 …): an HTTP-only provider does not implement the
/// stateful capabilities, so their negotiated examples fall to the runtime-promise path.
#[tokio::test]
async fn stateful_capabilities_negotiation() {
    let output = baml_test!(
        r#"
        function main() -> string {
            let p: baml.ai.Provider = baml.ai.OpenAi { model: "m", api_key: "k", base_url: null };
            let session = baml.ai.Session { _id: "s1" };
            let a = match (p) {
                let conv: baml.ai.Conversational => conv.chat<string>("hi", session) catch (e) { _ => "conv_err" },
                _ => "no_conv",
            };
            let b = match (p) {
                let bg: baml.ai.Background => "bg",
                _ => "no_bg",
            };
            let c = match (p) {
                let s: baml.ai.Suspendable => "suspendable",
                _ => "no_suspend",
            };
            a + "|" + b + "|" + c
        }
        "#
    );
    assert_eq!(
        output.result.unwrap(),
        BexExternalValue::String("no_conv|no_bg|no_suspend".into())
    );
}

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

/// REGRESSION: client `options` (request_body params like temperature, custom headers)
/// must be forwarded by the new-provider delegation, as the legacy path did.
#[tokio::test]
async fn e2e_options_and_headers_forwarded() {
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
        client<llm> OptClient {{
          provider openai
          options {{
            model "gpt-5.4-mini"
            api_key "test-key"
            base_url "{uri}"
            temperature 0.7
            headers {{ x-custom "abc123" }}
          }}
        }}
        function Ask(q: string) -> string {{
          client OptClient
          prompt `${{q}}`
        }}
        function main() -> string {{ Ask("hi") }}
        "#
    ));
    assert_eq!(
        output.result.unwrap(),
        BexExternalValue::String("pong".into())
    );

    let reqs = server.received_requests().await.unwrap();
    assert_eq!(reqs.len(), 1, "expected exactly one request");
    let body = String::from_utf8_lossy(&reqs[0].body).to_string();
    assert!(
        body.contains("\"temperature\":0.7") || body.contains("\"temperature\": 0.7"),
        "request_body option `temperature` was dropped; body: {body}"
    );
    let hdr = reqs[0]
        .headers
        .get("x-custom")
        .map(|v| v.to_str().unwrap_or("").to_string());
    assert_eq!(
        hdr.as_deref(),
        Some("abc123"),
        "custom header was dropped; received headers: {:?}",
        reqs[0].headers
    );
}

/// REGRESSION: when the prompt references `${ctx.output_format}`, the schema must appear
/// exactly ONCE in the outgoing request (the provider must not append it a second time).
#[tokio::test]
async fn e2e_structured_schema_injected_once() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_string(
            r#"{"choices":[{"message":{"content":"{\"name\": \"Ada\", \"age\": 36}"}}]}"#,
        ))
        .mount(&server)
        .await;
    let uri = server.uri();

    let output = baml_test!(&format!(
        r#"
        class Person {{ name string, age int }}
        client<llm> SchemaClient {{
          provider openai
          options {{ model "gpt-5.4-mini" api_key "test-key" base_url "{uri}" }}
        }}
        function Extract(text: string) -> Person {{
          client SchemaClient
          prompt `Extract from: ${{text}}
${{ctx.output_format}}`
        }}
        function main() -> string {{
          let p = Extract("Ada, 36");
          p.name + "|" + p.age.to_string()
        }}
        "#
    ));
    assert_eq!(
        output.result.unwrap(),
        BexExternalValue::String("Ada|36".into())
    );

    let bodies = recorded_bodies(&server).await;
    let marker = "Answer in JSON using this schema";
    let count = bodies[0].matches(marker).count();
    assert_eq!(
        count, 1,
        "schema must appear exactly once, found {count}; body: {}",
        bodies[0]
    );
}

/// REGRESSION: a prompt carrying media (an image arg) must not silently drop the image —
/// the delegation must preserve the media parts on the wire.
#[tokio::test]
async fn e2e_media_prompt_reaches_wire() {
    let server = MockServer::start().await;
    Mock::given(method("POST")).and(path("/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_string(
            r#"{"id":"cmpl-1","object":"chat.completion","created":1,"model":"gpt-5.4-mini","choices":[{"index":0,"message":{"role":"assistant","content":"a cat"},"finish_reason":"stop"}]}"#))
        .mount(&server).await;
    let uri = server.uri();

    let output = baml_test!(&format!(
        r##"
        client<llm> MediaClient {{
          provider openai
          options {{ model "gpt-5.4-mini" api_key "test-key" base_url "{uri}" }}
        }}
        function Describe(img: image) -> string {{
          client MediaClient
          prompt `What is in this image? ${{img}}`
        }}
        function main() -> string {{
          Describe(baml.media.Image.from_url("https://example.com/cat.png", "image/png"))
        }}
        "##
    ));
    assert_eq!(
        output.result.unwrap(),
        BexExternalValue::String("a cat".into())
    );

    let bodies = recorded_bodies(&server).await;
    assert!(
        bodies[0].contains("cat.png"),
        "image was dropped from the request; body: {}",
        bodies[0]
    );
}

/// Native messages: `${role(...)}` structure is preserved on the wire (system + user
/// messages arrive as separate entries, not flattened into one).
#[tokio::test]
async fn e2e_roles_preserved_on_wire() {
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
        client<llm> RoleClient {{
          provider openai
          options {{ model "gpt-5.4-mini" api_key "test-key" base_url "{uri}" }}
        }}
        function Ask(q: string) -> string {{
          client RoleClient
          prompt `${{role("system")}}You are terse.${{role("user")}}${{q}}`
        }}
        function main() -> string {{ Ask("hi") }}
        "#
    ));
    assert_eq!(
        output.result.unwrap(),
        BexExternalValue::String("pong".into())
    );

    let bodies = recorded_bodies(&server).await;
    assert!(
        bodies[0].contains(r#""role":"system""#) && bodies[0].contains(r#""role":"user""#),
        "roles were flattened; body: {}",
        bodies[0]
    );
}

/// LIVE multimodal (scenario 05 for real): a user function with an image arg, described
/// by the real model through the native message pipeline. Gated on `OPENAI_API_KEY`.
#[tokio::test]
async fn e2e_multimodal_live() {
    if std::env::var("OPENAI_API_KEY").is_err() {
        eprintln!("skipping e2e_multimodal_live: OPENAI_API_KEY not set");
        return;
    }
    let output = baml_test!(
        r#"
        client<llm> VisionClient {
          provider openai
          options { model "gpt-5.4-mini" api_key env.OPENAI_API_KEY }
        }
        function Describe(img: image) -> string {
          client VisionClient
          prompt `In one word, what is the dominant color of this image? ${img}`
        }
        function main() -> string {
          // A 64x64 solid-red PNG, inline — no external fetch for OpenAI to reject.
          Describe(baml.media.Image.from_base64("iVBORw0KGgoAAAANSUhEUgAAAEAAAABACAIAAAAlC+aJAAAAb0lEQVR4nO3PAQkAAAyEwO9feoshgnABdLep8QUNyPEFDcjxBQ3I8QUNyPEFDcjxBQ3I8QUNyPEFDcjxBQ3I8QUNyPEFDcjxBQ3I8QUNyPEFDcjxBQ3I8QUNyPEFDcjxBQ3I8QUNyPEFDcjxBQ3IPanc8OLDQitxAAAAAElFTkSuQmCC", "image/png"))
        }
        "#
    );
    let got = output.result.unwrap();
    let BexExternalValue::String(s) = got else {
        panic!("expected string, got {got:?}")
    };
    assert!(
        s.to_lowercase().contains("red"),
        "vision answer did not say red: {s:?}"
    );
}

/// Within-run history (scenario 17) via the native message API: the app holds the
/// transcript as `ChatMessage[]` and threads it through `call_messages`. Mocked:
/// asserts the second request carries the full 4-message history.
#[tokio::test]
async fn conversation_history_via_mock() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/chat/completions"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string(r#"{"choices":[{"message":{"content":"Hello Ada!"}}]}"#),
        )
        .up_to_n_times(1)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/chat/completions"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string(r#"{"choices":[{"message":{"content":"Ada"}}]}"#),
        )
        .mount(&server)
        .await;
    let uri = server.uri();

    let output = baml_test!(&format!(
        r#"
        function main() -> string {{
            let p = baml.ai.OpenAi {{ model: "m", api_key: "k", base_url: "{uri}" }};
            let history = [
                baml.ai.ChatMessage.system("You are terse."),
                baml.ai.ChatMessage.user("My name is Ada. Say hello."),
            ];
            let r1: string = p.call_messages<string>(history) catch (e) {{
                let u: baml.errors.UnknownError => return "ERR1",
            }};
            history.push(baml.ai.ChatMessage.assistant(r1));
            history.push(baml.ai.ChatMessage.user("What is my name?"));
            p.call_messages<string>(history) catch (e) {{
                let u: baml.errors.UnknownError => "ERR2",
            }}
        }}
        "#
    ));
    assert_eq!(
        output.result.unwrap(),
        BexExternalValue::String("Ada".into())
    );

    let bodies = recorded_bodies(&server).await;
    assert_eq!(bodies.len(), 2);
    // Second request must carry the whole conversation: system + user + assistant + user.
    assert!(
        bodies[1].contains(r#""role":"system""#)
            && bodies[1].contains(r#""role":"assistant""#)
            && bodies[1].contains("Hello Ada!"),
        "history not threaded; body: {}",
        bodies[1]
    );
}

/// LIVE within-run history (scenario 17): the model remembers a fact stated earlier in
/// the threaded conversation. Gated on `OPENAI_API_KEY`.
#[tokio::test]
async fn conversation_history_live() {
    if std::env::var("OPENAI_API_KEY").is_err() {
        eprintln!("skipping conversation_history_live: OPENAI_API_KEY not set");
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
            let history = [
                baml.ai.ChatMessage.system("You are terse."),
                baml.ai.ChatMessage.user("My name is Ada. Greet me briefly."),
            ];
            let r1: string = p.call_messages<string>(history) catch (e) {
                let u: baml.errors.UnknownError => return "ERR1:" + u.message.join(","),
            };
            history.push(baml.ai.ChatMessage.assistant(r1));
            history.push(baml.ai.ChatMessage.user("What is my name? Reply with just the name."));
            p.call_messages<string>(history) catch (e) {
                let u: baml.errors.UnknownError => "ERR2:" + u.message.join(","),
            }
        }
        "#
    );
    let got = output.result.unwrap();
    let BexExternalValue::String(s) = got else {
        panic!("expected string, got {got:?}")
    };
    assert!(s.contains("Ada"), "model did not remember the name: {s:?}");
}

/// LIVE usage metering (scenario 34): `call_with(prompt, m => m.usage())` returns real
/// token counts from the API. Gated on `OPENAI_API_KEY`.
#[tokio::test]
async fn usage_metering_live() {
    if std::env::var("OPENAI_API_KEY").is_err() {
        eprintln!("skipping usage_metering_live: OPENAI_API_KEY not set");
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
            let r = p.call_with<string, baml.ai.Usage, never>(
                "Reply with exactly: ok",
                (m: baml.ai.ResponseMeta) -> baml.ai.Usage { m.usage() },
            ) catch (e) {
                let u: baml.errors.UnknownError => return "ERR:" + u.message.join(","),
            };
            if (r.meta.input_tokens > 0 && r.meta.output_tokens > 0) {
                "metered"
            } else {
                "zero-usage in=" + r.meta.input_tokens.to_string() + " out=" + r.meta.output_tokens.to_string()
            }
        }
        "#
    );
    assert_eq!(
        output.result.unwrap(),
        BexExternalValue::String("metered".into())
    );
}

/// LIVE partial structured streaming (scenario 04's structured half + 02): a user
/// function returning a class, streamed — partials arrive, final parses to the typed
/// value. Gated on `OPENAI_API_KEY`.
#[tokio::test]
async fn structured_streaming_live() {
    if std::env::var("OPENAI_API_KEY").is_err() {
        eprintln!("skipping structured_streaming_live: OPENAI_API_KEY not set");
        return;
    }
    let output = baml_test!(
        r#"
        class Person { name string, age int }
        client<llm> LiveClient {
          provider openai
          options { model "gpt-5.4-mini" api_key env.OPENAI_API_KEY }
        }
        function Extract(text: string) -> Person {
          client LiveClient
          prompt `Extract the person: ${text}
${ctx.output_format}`
        }
        function main() -> string {
            let s = Extract$stream("Ada Lovelace is 36 years old.");
            let partials = 0;
            while (partials < 500) {
                match (s.next()) {
                    baml.stream.StreamFinished => { break; },
                    _ => { partials = partials + 1; },
                }
            }
            let p: Person = s.final() catch (e) { _ => return "FINAL_ERR" };
            p.name + "|" + p.age.to_string() + "|partials>0=" + (partials > 0).to_string()
        }
        "#
    );
    let got = output.result.unwrap();
    let BexExternalValue::String(s) = got else {
        panic!("expected string, got {got:?}")
    };
    assert!(
        s.contains("Ada") && s.contains("36") && s.ends_with("true"),
        "structured stream unexpected: {s:?}"
    );
}

/// D2/D8: a NON-retryable typed error (HTTP 400) stops Retry immediately — exactly one
/// request hits the wire, and the typed OpenAiHttpError surfaces to the catch.
#[tokio::test]
async fn retry_skips_non_retryable_errors() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/chat/completions"))
        .respond_with(
            ResponseTemplate::new(400).set_body_string(r#"{"error":{"message":"bad request"}}"#),
        )
        .mount(&server)
        .await;
    let uri = server.uri();

    let output = baml_test!(&format!(
        r#"
        function main() -> string {{
            let p = baml.ai.OpenAi {{ model: "m", api_key: "k", base_url: "{uri}" }}.with_retry(3);
            p.call<string>("hi") catch (e) {{
                let he: baml.ai.OpenAiHttpError => "http:" + he.status.to_string() + " retryable=" + he.is_retryable().to_string(),
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
    let n = server.received_requests().await.unwrap().len();
    assert_eq!(
        n, 1,
        "a non-retryable 400 must not be re-driven, saw {n} requests"
    );
}

/// D2: Retry refuses to wrap an EFFECTFUL provider (OpenAiResponses stores server state /
/// bills jobs) — typed CannotRetry instead of a silent double-drive.
#[tokio::test]
async fn retry_refuses_effectful_provider() {
    let output = baml_test!(
        r#"
        function main() -> string {
            let p = baml.ai.OpenAiResponses { model: "m", api_key: "k" }.with_retry(2);
            p.call<string>("hi") catch (e) {
                let cr: baml.errors.CannotRetry => "refused: " + cr.message,
                let u: baml.errors.UnknownError => "unknown",
                let c: baml.errors.CallError => "callerr",
            }
        }
        "#
    );
    let got = output.result.unwrap();
    let BexExternalValue::String(s) = got else {
        panic!("expected string, got {got:?}")
    };
    assert!(
        s.starts_with("refused: provider is effectful"),
        "unexpected: {s:?}"
    );
}

/// LIVE multi-tool agent (scenarios 09+11): three tools registered; the model must call
/// at least two different ones (weather + local time) — possibly in parallel within one
/// turn — and compose both results into the final answer. Gated on `OPENAI_API_KEY`.
#[tokio::test]
async fn multi_tool_agent_live() {
    if std::env::var("OPENAI_API_KEY").is_err() {
        eprintln!("skipping multi_tool_agent_live: OPENAI_API_KEY not set");
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
            let tools = [
                baml.ai.Tool {
                    name: "get_weather",
                    description: "Get the current weather for a city",
                    parameters: "{\"type\":\"object\",\"properties\":{\"city\":{\"type\":\"string\"}},\"required\":[\"city\"]}",
                },
                baml.ai.Tool {
                    name: "get_local_time",
                    description: "Get the current local time in a city",
                    parameters: "{\"type\":\"object\",\"properties\":{\"city\":{\"type\":\"string\"}},\"required\":[\"city\"]}",
                },
                baml.ai.Tool {
                    name: "convert_currency",
                    description: "Convert an amount between currencies",
                    parameters: "{\"type\":\"object\",\"properties\":{\"amount\":{\"type\":\"number\"},\"from\":{\"type\":\"string\"},\"to\":{\"type\":\"string\"}},\"required\":[\"amount\",\"from\",\"to\"]}",
                },
            ];
            let called: string[] = [];
            let answer = p.run_tools<string>(
                "What is the weather AND the current local time in Paris? Use the tools, then answer in one sentence mentioning both.",
                tools,
                (calls: baml.ai.ToolCall[]) -> baml.ai.ToolResult[] {
                    let results: baml.ai.ToolResult[] = [];
                    for (let c in calls) {
                        called.push(c.name);
                        let out = if (c.name == "get_weather") {
                            "sunny, 22C"
                        } else if (c.name == "get_local_time") {
                            "14:37"
                        } else {
                            "unknown tool"
                        };
                        results.push(baml.ai.ToolResult { id: c.id, output: out });
                    }
                    results
                },
            ) catch (e) {
                let u: baml.errors.UnknownError => return "ERR:" + u.message.join(","),
            };
            let used_weather = called.includes("get_weather");
            let used_time = called.includes("get_local_time");
            "tools=" + called.length().to_string()
                + " weather=" + used_weather.to_string()
                + " time=" + used_time.to_string()
                + " || " + answer
        }
        "#
    );
    let got = output.result.unwrap();
    let BexExternalValue::String(s) = got else {
        panic!("expected string, got {got:?}")
    };
    assert!(
        s.contains("weather=true") && s.contains("time=true"),
        "model did not use both tools: {s:?}"
    );
    assert!(
        s.contains("22") && (s.contains("14:37") || s.to_lowercase().contains("2:37")),
        "final answer missing tool results: {s:?}"
    );
}

/// LIVE evaluation (scenario 33): the task is a provider call; the judge is just another
/// provider scoring the output — typed verdict via structured output. Gated on key.
#[tokio::test]
async fn eval_judge_live() {
    if std::env::var("OPENAI_API_KEY").is_err() {
        eprintln!("skipping eval_judge_live: OPENAI_API_KEY not set");
        return;
    }
    let output = baml_test!(
        r#"
        class Verdict { pass bool, reason string }
        function main() -> string {
            let task = baml.ai.OpenAi {
                model: "gpt-5.4-mini",
                api_key: baml.env.get_or_panic("OPENAI_API_KEY"),
                base_url: null,
            };
            let judge = baml.ai.OpenAi {
                model: "gpt-5.4-mini",
                api_key: baml.env.get_or_panic("OPENAI_API_KEY"),
                base_url: null,
            };
            let answer = task.call<string>("In one sentence: why is the sky blue?") catch (e) {
                let u: baml.errors.UnknownError => return "TASK_ERR",
                let c: baml.errors.CallError => return "TASK_CALLERR",
            };
            let v: Verdict = judge.call<Verdict>(
                "You are a strict grader. Does this answer correctly attribute the blue sky to Rayleigh scattering (scattering of sunlight by air molecules)? Answer: " + answer
            ) catch (e) {
                let u: baml.errors.UnknownError => return "JUDGE_ERR",
                let c: baml.errors.CallError => return "JUDGE_CALLERR",
            };
            "pass=" + v.pass.to_string() + " reason_len=" + (v.reason.length() > 0).to_string()
        }
        "#
    );
    let got = output.result.unwrap();
    let BexExternalValue::String(s) = got else {
        panic!("expected string, got {got:?}")
    };
    assert!(
        s.contains("pass=true") && s.contains("reason_len=true"),
        "judge verdict unexpected: {s:?}"
    );
}

/// LIVE multi-agent handoff (scenario 14): a specialist sub-agent IS a provider — the
/// orchestrator's tool dispatch delegates a translation tool call to a second model
/// call, and the orchestrator composes the result. Gated on `OPENAI_API_KEY`.
#[tokio::test]
async fn multi_agent_handoff_live() {
    if std::env::var("OPENAI_API_KEY").is_err() {
        eprintln!("skipping multi_agent_handoff_live: OPENAI_API_KEY not set");
        return;
    }
    let output = baml_test!(
        r#"
        function main() -> string {
            let orchestrator = baml.ai.OpenAi {
                model: "gpt-5.4-mini",
                api_key: baml.env.get_or_panic("OPENAI_API_KEY"),
                base_url: null,
            };
            let specialist: baml.ai.Provider = baml.ai.OpenAi {
                model: "gpt-5.4-mini",
                api_key: baml.env.get_or_panic("OPENAI_API_KEY"),
                base_url: null,
            };
            let tools = [
                baml.ai.Tool {
                    name: "ask_french_translator",
                    description: "Translate an English phrase to French (a specialist agent)",
                    parameters: "{\"type\":\"object\",\"properties\":{\"phrase\":{\"type\":\"string\"}},\"required\":[\"phrase\"]}",
                },
            ];
            let handoffs = 0;
            let answer = orchestrator.run_tools<string>(
                "Use the translator tool to translate 'good morning' to French, then reply with just the translation.",
                tools,
                (calls: baml.ai.ToolCall[]) -> baml.ai.ToolResult[] {
                    let results: baml.ai.ToolResult[] = [];
                    for (let c in calls) {
                        handoffs = handoffs + 1;
                        // The handoff: the specialist provider handles the delegated task.
                        let out = match (specialist) {
                            let h: baml.ai.HttpProvider => {
                                h.call<string>("Translate to French, reply with ONLY the translation: " + c.args)
                                    catch (e) { _ => "handoff failed" }
                            },
                            _ => "specialist not callable",
                        };
                        results.push(baml.ai.ToolResult { id: c.id, output: out });
                    }
                    results
                },
            ) catch (e) {
                let u: baml.errors.UnknownError => return "ERR:" + u.message.join(","),
            };
            "handoffs=" + handoffs.to_string() + " || " + answer
        }
        "#
    );
    let got = output.result.unwrap();
    let BexExternalValue::String(s) = got else {
        panic!("expected string, got {got:?}")
    };
    assert!(
        s.contains("handoffs=") && !s.starts_with("handoffs=0"),
        "no handoff happened: {s:?}"
    );
    assert!(
        s.to_lowercase().contains("bonjour"),
        "translation missing from final answer: {s:?}"
    );
}

/// LIVE typed tool loop (design D6/P7): the tool's parameter schema is LOWERED FROM A
/// BAML TYPE (`Tool.from_type` + `baml.schema.json_schema`), and the dispatcher
/// SAP-parses `ToolCall.args` back into that typed class — schema out, typed args in.
#[tokio::test]
async fn typed_tool_agent_live() {
    if std::env::var("OPENAI_API_KEY").is_err() {
        eprintln!("skipping typed_tool_agent_live: OPENAI_API_KEY not set");
        return;
    }
    let output = baml_test!(
        r#"
        class WeatherArgs { city string, unit string? }
        function main() -> string {
            let p = baml.ai.OpenAi {
                model: "gpt-5.4-mini",
                api_key: baml.env.get_or_panic("OPENAI_API_KEY"),
                base_url: null,
            };
            let weather_tool = baml.ai.Tool.from_type(
                "get_weather",
                "Get the current weather for a city",
                reflect.type_of<WeatherArgs>(),
            ) catch (e) {
                let un: baml.errors.Unsupported => return "SCHEMA_ERR:" + un.message,
            };
            let parsed_cities: string[] = [];
            let answer = p.run_tools<string>(
                "What's the weather in Tokyo? Use the tool, then answer in one short sentence.",
                [weather_tool],
                (calls: baml.ai.ToolCall[]) -> baml.ai.ToolResult[] {
                    let results: baml.ai.ToolResult[] = [];
                    for (let c in calls) {
                        // D6: coerce the wire args into the DECLARED type via SAP.
                        let args: WeatherArgs = baml.sap.parse<WeatherArgs>(c.args) catch (e) {
                            _ => WeatherArgs { city: "unknown" },
                        };
                        parsed_cities.push(args.city);
                        results.push(baml.ai.ToolResult { id: c.id, output: "cloudy, 18C in " + args.city });
                    }
                    results
                },
            ) catch (e) {
                let u: baml.errors.UnknownError => return "ERR:" + u.message.join(","),
            };
            "cities=" + parsed_cities.join(";") + " || " + answer
        }
        "#
    );
    let got = output.result.unwrap();
    let BexExternalValue::String(s) = got else {
        panic!("expected string, got {got:?}")
    };
    assert!(
        s.to_lowercase().contains("cities=tokyo"),
        "typed args not parsed: {s:?}"
    );
    assert!(s.contains("18"), "tool result missing from answer: {s:?}");
}

/// LIVE workflow graph (scenario 43): fetch → summarize + label in PARALLEL (spawn/await
/// fan-in, BEP-034 structured concurrency) → combine. Two concurrent real model calls.
#[tokio::test]
async fn workflow_graph_live() {
    if std::env::var("OPENAI_API_KEY").is_err() {
        eprintln!("skipping workflow_graph_live: OPENAI_API_KEY not set");
        return;
    }
    let output = baml_test!(
        r#"
        function model_text(p: baml.ai.Provider, prompt: string) -> string
            throws baml.errors.CallError | baml.errors.UnknownError {
            match (p) {
                let h: baml.ai.HttpProvider => h.call<string>(prompt),
                _ => throw baml.errors.Unsupported { message: "not callable" },
            }
        }
        function main() -> string {
            let p: baml.ai.Provider = baml.ai.OpenAi {
                model: "gpt-5.4-mini",
                api_key: baml.env.get_or_panic("OPENAI_API_KEY"),
                base_url: null,
            };
            let doc = "The Eiffel Tower, completed in 1889 for the World's Fair, is a wrought-iron lattice tower in Paris and one of the most recognizable structures on Earth.";
            // fan-out: two model calls run CONCURRENTLY
            let f_summary = spawn { model_text(p, "Summarize in <= 8 words: " + doc) };
            let f_label = spawn { model_text(p, "One-word topic label (just the word): " + doc) };
            let summary = await f_summary catch (e) { _ => return "SUMMARY_ERR" };
            let label = await f_label catch (e) { _ => return "LABEL_ERR" };
            "[" + label + "] " + summary
        }
        "#
    );
    let got = output.result.unwrap();
    let BexExternalValue::String(s) = got else {
        panic!("expected string, got {got:?}")
    };
    assert!(
        s.starts_with("[") && s.contains("] "),
        "pipeline shape wrong: {s:?}"
    );
    assert!(
        s.to_lowercase().contains("eiffel")
            || s.to_lowercase().contains("tower")
            || s.to_lowercase().contains("paris"),
        "content missing: {s:?}"
    );
}
