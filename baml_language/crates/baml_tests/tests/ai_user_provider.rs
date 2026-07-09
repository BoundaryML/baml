//! End-to-end proof that a provider can be authored ENTIRELY in a user package.
//!
//! This was the top blocker in `_plan/baml_gotchas.md`: a user-package class
//! implementing `baml.ai.HttpProvider` (which `requires Provider`) was rejected
//! with a false-positive E0125, forcing all providers into the stdlib. With the
//! cross-package `requires`-satisfaction fix in `baml_lsp2_actions/src/check.rs`,
//! everything below compiles and runs from user code alone: the provider, its
//! `ResponseMeta`, and its typed error (Failure + CallError) — driven through the
//! stdlib's default `call`/`call_with` pipeline against a wiremock server.

use baml_tests::baml_test;
use bex_engine::BexExternalValue;
use wiremock::{
    Mock, MockServer, ResponseTemplate,
    matchers::{method, path},
};

/// The user-authored provider: a toy "echo" wire protocol (POST /echo with
/// `{"input": …}`, replies `{"reply": …, "tokens": n}`). Spliced into each test's
/// BAML source. Implements the marker + the full `HttpProvider` required surface;
/// `call`/`call_with`/`call_messages*` are inherited stdlib defaults.
const ECHO_PROVIDER: &str = r#"
    class EchoRequestWire { input string }
    class EchoResponseWire { reply string, tokens int }

    class EchoMeta {
        tokens: int,
        implements baml.ai.ResponseMeta {
            function finish_reason(self) -> string throws never { "stop" }
            function usage(self) -> baml.ai.Usage throws never {
                baml.ai.Usage { input_tokens: 0, output_tokens: self.tokens }
            }
            function reasoning(self) -> string | baml.ai.Unavailable throws never {
                baml.ai.Unavailable { reason: "echo has no reasoning" }
            }
            function logprobs(self) -> baml.ai.Logprob[] | baml.ai.Unavailable throws never {
                baml.ai.Unavailable { reason: "echo has no logprobs" }
            }
            function citations(self) -> baml.ai.Citation[] | baml.ai.Unavailable throws never {
                baml.ai.Unavailable { reason: "echo has no citations" }
            }
        }
    }

    class EchoHttpError {
        status: int,
        body: string,
        implements baml.errors.Failure {
            function is_retryable(self) -> bool { self.status == 429 || self.status >= 500 }
            function is_effectful(self) -> bool { false }
            function is_policy_refusal(self) -> bool { false }
            function is_resumable(self) -> bool { false }
            function is_unsupported(self) -> bool { false }
        }
        implements baml.errors.CallError {
            function is_network_error(self) -> bool { false }
            function is_rate_limit(self) -> bool { self.status == 429 }
            function is_parse_error(self) -> bool { false }
        }
    }

    class EchoProvider {
        base_url: string,

        function _prompt_text(self, messages: baml.ai.ChatMessage[]) -> string throws never {
            match (messages.at(0)) {
                null => "",
                let m: baml.ai.ChatMessage => match (m.parts.at(0)) {
                    null => "",
                    let p: baml.ai.MessagePart => {
                        let t: string = p.text ?? "";
                        t
                    },
                },
            }
        }

        implements baml.ai.Provider {}

        implements baml.ai.HttpProvider {
            type Body = string

            function build_request<T>(self, messages: baml.ai.ChatMessage[]) -> baml.http.Request
                throws baml.errors.CallError | baml.errors.UnknownError {
                let body: map<string, json> = { "input": self._prompt_text(messages) };
                baml.http.Request {
                    method: "POST",
                    url: self.base_url + "/echo",
                    headers: { "Content-Type": "application/json" },
                    body: baml.json.stringify(body),
                }
            }

            function send(self, request: baml.http.Request) -> string
                throws baml.errors.CallError | baml.errors.UnknownError {
                let resp = baml.http.send(request) catch (e) {
                    _ => throw baml.errors.UnknownError { data: e, message: ["echo send failed"] },
                };
                let text = resp.text() catch (e) {
                    _ => throw baml.errors.UnknownError { data: e, message: ["echo body read failed"] },
                };
                if (resp.ok()) {
                    text
                } else {
                    throw EchoHttpError { status: resp.status_code, body: text }
                }
            }

            function parse<T>(self, from: string) -> T
                throws baml.errors.CallError | baml.errors.UnknownError {
                let wire: EchoResponseWire = baml.json.from_json<EchoResponseWire>(baml.json.parse(from)) catch (e) {
                    _ => throw baml.errors.UnknownError { data: e, message: ["echo parse failed"] },
                };
                baml.sap.parse<T>(wire.reply) catch (e) {
                    _ => throw baml.errors.UnknownError { data: e, message: ["echo SAP parse failed"] },
                }
            }

            function parse_meta(self, from: string) -> baml.ai.ResponseMeta
                throws baml.errors.CallError | baml.errors.UnknownError {
                let wire: EchoResponseWire = baml.json.from_json<EchoResponseWire>(baml.json.parse(from)) catch (e) {
                    _ => throw baml.errors.UnknownError { data: e, message: ["echo meta failed"] },
                };
                EchoMeta { tokens: wire.tokens }
            }
        }
    }
"#;

/// The previously-E0125-blocked shape compiles, and the user provider negotiates
/// as `HttpProvider` through the `Provider` existential at runtime.
#[tokio::test]
async fn user_provider_satisfies_stdlib_requires() {
    let output = baml_test!(&format!(
        r#"
        {ECHO_PROVIDER}
        function main() -> string {{
            let p: baml.ai.Provider = EchoProvider {{ base_url: "http://unused" }};
            let cap = match (p) {{
                let h: baml.ai.HttpProvider => "http",
                _ => "none",
            }};
            cap + "|effectful=" + p.is_effectful().to_string()
        }}
        "#
    ));
    assert_eq!(
        output.result.unwrap(),
        BexExternalValue::String("http|effectful=false".into())
    );
}

/// The stdlib's inherited `call<string>` default drives the user-authored
/// build_request → send → parse pipeline against a mock server.
#[tokio::test]
async fn user_provider_call_via_mock() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/echo"))
        .respond_with(ResponseTemplate::new(200).set_body_string(r#"{"reply":"pong","tokens":7}"#))
        .mount(&server)
        .await;
    let uri = server.uri();

    let output = baml_test!(&format!(
        r#"
        {ECHO_PROVIDER}
        function main() -> string {{
            let p = EchoProvider {{ base_url: "{uri}" }};
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
}

/// Typed extraction + meta projection through `call_with<T, V, E2>`: SAP decodes
/// the user wire's reply into a user class, and the projection reads usage from
/// the user-authored `ResponseMeta`.
#[tokio::test]
async fn user_provider_structured_and_meta_via_mock() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/echo"))
        .respond_with(ResponseTemplate::new(200).set_body_string(
            r#"{"reply":"```json\n{\"subject\": \"answer\", \"value\": 42}\n```","tokens":9}"#,
        ))
        .mount(&server)
        .await;
    let uri = server.uri();

    let output = baml_test!(&format!(
        r#"
        {ECHO_PROVIDER}
        class Fact {{ subject string, value int }}
        function main() -> string {{
            let p = EchoProvider {{ base_url: "{uri}" }};
            let r = p.call_with<Fact, baml.ai.Usage, never>(
                "extract the fact",
                (m: baml.ai.ResponseMeta) -> baml.ai.Usage {{ m.usage() }},
            ) catch (e) {{
                let u: baml.errors.UnknownError => return "ERR:" + u.message.join(","),
                let c: baml.errors.CallError => return "CALLERR",
            }};
            r.value.subject + "=" + r.value.value.to_string()
                + "|out=" + r.meta.output_tokens.to_string()
        }}
        "#
    ));
    assert_eq!(
        output.result.unwrap(),
        BexExternalValue::String("answer=42|out=9".into())
    );
}

/// A non-2xx response surfaces as the user's typed error, triaged through BOTH
/// stdlib classifier axes (`Failure.is_retryable`, `CallError.is_rate_limit`)
/// without the call site naming the concrete class in its channel.
#[tokio::test]
async fn user_provider_error_is_typed_and_triaged() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/echo"))
        .respond_with(ResponseTemplate::new(429).set_body_string("slow down"))
        .mount(&server)
        .await;
    let uri = server.uri();

    let output = baml_test!(&format!(
        r#"
        {ECHO_PROVIDER}
        function main() -> string {{
            let p = EchoProvider {{ base_url: "{uri}" }};
            p.call<string>("ping") catch (e) {{
                let ehe: EchoHttpError => "status=" + ehe.status.to_string()
                    + "|retryable=" + ehe.is_retryable().to_string()
                    + "|ratelimit=" + ehe.is_rate_limit().to_string(),
                let u: baml.errors.UnknownError => "ERR:" + u.message.join(","),
                let c: baml.errors.CallError => "CALLERR",
            }}
        }}
        "#
    ));
    assert_eq!(
        output.result.unwrap(),
        BexExternalValue::String("status=429|retryable=true|ratelimit=true".into())
    );
}

/// REGRESSION (pkg-alias-query perf merge): an interface `match` compiled in
/// the BUILTINS package (stdlib `drive_call`'s negotiation) must see USER-package
/// implementors — `package_lowering_data` unions implementor relations across
/// every session package, not just the package + its deps. When this breaks,
/// every LLM function called with a user-authored provider throws
/// `Unsupported: "client's provider supports neither HttpProvider nor Streaming"`.
#[tokio::test]
async fn stdlib_match_sees_user_package_implementors() {
    let output = baml_test!(
        r##"
        class InlineEcho {
            reply: string,

            implements baml.ai.Provider {}

            implements baml.ai.HttpProvider {
                type Body = string
                function build_request<T>(self, messages: baml.ai.ChatMessage[]) -> baml.http.Request throws baml.errors.UnknownError {
                    throw baml.errors.UnknownError { data: null, message: ["no"] }
                }
                function send(self, request: baml.http.Request) -> string throws baml.errors.UnknownError {
                    throw baml.errors.UnknownError { data: null, message: ["no"] }
                }
                function parse<T>(self, from: string) -> T throws baml.errors.UnknownError {
                    baml.sap.parse<T>(from) catch (e) { _ => throw baml.errors.UnknownError { data: e, message: ["p"] } }
                }
                function parse_meta(self, from: string) -> baml.ai.ResponseMeta throws never {
                    baml.ai.LegacyMeta {}
                }
                function call_messages_with<T, V, E2>(
                    self,
                    messages: baml.ai.ChatMessage[],
                    project: (baml.ai.ResponseMeta) -> V throws E2,
                ) -> baml.ai.CallResult<T, V> throws baml.errors.CallError | baml.errors.UnknownError | E2 {
                    let value: T = baml.sap.parse<T>(self.reply) catch (e) {
                        _ => throw baml.errors.UnknownError { data: e, message: ["p"] },
                    };
                    baml.ai.CallResult<T, V> { value: value, meta: project(baml.ai.LegacyMeta {}) }
                }
            }
        }

        function Greet(name: string) -> string {
            client "openai/gpt-4o"
            prompt #"Say hi to {{ name }}"#
        }

        function main() -> string {
            Greet("world", client = InlineEcho { reply: "hi from echo" }) catch (e) {
                let u: baml.errors.Unsupported => "UNSUPPORTED: " + u.message,
                _ => "OTHER_ERR",
            }
        }
        "##
    );
    assert_eq!(
        output.result.unwrap(),
        BexExternalValue::String("hi from echo".into())
    );
}
