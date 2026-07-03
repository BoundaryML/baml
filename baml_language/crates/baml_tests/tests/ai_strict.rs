//! Tests for OpenAI strict-mode structured outputs:
//!
//! - `baml.schema.json_schema` — the `type` → JSON Schema host function (P7),
//!   exercised purely in BAML and asserted with `baml.json.path`.
//! - `baml.ai.OpenAiStrict` — the Chat Completions provider that sends
//!   `response_format: json_schema` with `"strict": true`.
//!
//! Offline: schema lowering + mocked request-shape capture (no key needed).
//! Live (gated on `OPENAI_API_KEY`): a real `gpt-5.4-mini` strict extraction.

use baml_tests::baml_test;
use bex_engine::BexExternalValue;
use wiremock::{
    Mock, MockServer, ResponseTemplate,
    matchers::{method, path},
};

/// Pure-BAML unit test of `baml.schema.json_schema`: lower a class (with an
/// optional field) and an enum in strict mode, then assert the emitted schema
/// with jq-style `baml.json.path` selectors — object type, closed objects
/// (`additionalProperties: false`), ALL fields required (optional `nickname`
/// included), the optional field's `["string","null"]` nullable form, and the
/// enum's string+variants shape.
#[tokio::test]
async fn schema_lowering_unit() {
    let output = baml_test!(
        r#"
        enum Status { Active, Inactive }
        class Person { name string, age int, nickname string? }
        function main() -> string {
            let schema = baml.schema.json_schema(reflect.type_of<Person>(), true) catch (e) {
                let u: baml.errors.Unsupported => return "UNSUPPORTED",
            };
            let j = baml.json.parse(schema) catch (e) { _ => return "PARSE_ERR" };
            let t: string = baml.json.path<string>(j, ".type") catch (e) { _ => return "no-type" };
            let ap: bool = baml.json.path<bool>(j, ".additionalProperties") catch (e) {
                _ => return "no-ap"
            };
            let req: string[] = baml.json.path<string[]>(j, ".required") catch (e) {
                _ => return "no-req"
            };
            let nn: string[] = baml.json.path<string[]>(j, ".properties.nickname.type") catch (e) {
                _ => return "no-nn"
            };
            let es = baml.schema.json_schema(reflect.type_of<Status>(), true) catch (e) {
                let u: baml.errors.Unsupported => return "E_UNSUPPORTED",
            };
            let ej = baml.json.parse(es) catch (e) { _ => return "E_PARSE" };
            let et: string = baml.json.path<string>(ej, ".type") catch (e) { _ => return "no-et" };
            let ev: string[] = baml.json.path<string[]>(ej, ".enum") catch (e) {
                _ => return "no-ev"
            };
            `${t}|${ap}|${req.join(",")}|${nn.join(",")}|${et}|${ev.join(",")}`
        }
        "#
    );
    assert_eq!(
        output.result.unwrap(),
        BexExternalValue::String(
            "object|false|name,age,nickname|string,null|string|Active,Inactive".into()
        )
    );
}

/// Request-shape capture via wiremock: a structured `call<Person>` must send a
/// `response_format` with `"strict":true` and a closed
/// (`"additionalProperties":false`) schema, while a `call<string>` must NOT send
/// `response_format` at all (free-form text).
#[tokio::test]
async fn strict_request_shape_via_mock() {
    // --- structured call: response_format present ---
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_string(
            r#"{"choices":[{"message":{"content":"{\"name\":\"Ada\",\"age\":36,\"nickname\":null}"}}]}"#,
        ))
        .mount(&server)
        .await;
    let uri = server.uri();

    let output = baml_test!(&format!(
        r#"
        class Person {{ name string, age int, nickname string? }}
        function main() -> string {{
            let p = baml.ai.OpenAiStrict {{ model: "gpt-5.4-mini", api_key: "k", base_url: "{uri}" }};
            let per: Person = p.call<Person>("Extract a person") catch (e) {{
                let u: baml.errors.UnknownError => return "ERR:" + u.message.join(","),
                let c: baml.errors.CallError => return "CALLERR",
            }};
            per.name
        }}
        "#
    ));
    assert_eq!(
        output.result.unwrap(),
        BexExternalValue::String("Ada".into())
    );

    let reqs = server.received_requests().await.unwrap();
    assert_eq!(reqs.len(), 1, "expected exactly one request");
    let body = String::from_utf8_lossy(&reqs[0].body).to_string();
    assert!(
        body.contains(r#""response_format""#),
        "structured call must send response_format; body: {body}"
    );
    assert!(
        body.contains(r#""strict":true"#),
        "structured call must set strict:true; body: {body}"
    );
    assert!(
        body.contains(r#""additionalProperties":false"#),
        "strict schema must close objects; body: {body}"
    );

    // --- string call: NO response_format ---
    let server2 = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/chat/completions"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string(r#"{"choices":[{"message":{"content":"pong"}}]}"#),
        )
        .mount(&server2)
        .await;
    let uri2 = server2.uri();

    let out2 = baml_test!(&format!(
        r#"
        function main() -> string {{
            let p = baml.ai.OpenAiStrict {{ model: "m", api_key: "k", base_url: "{uri2}" }};
            p.call<string>("Reply with exactly: pong") catch (e) {{
                let u: baml.errors.UnknownError => "ERR:" + u.message.join(","),
                let c: baml.errors.CallError => "CALLERR",
            }}
        }}
        "#
    ));
    assert_eq!(
        out2.result.unwrap(),
        BexExternalValue::String("pong".into())
    );

    let reqs2 = server2.received_requests().await.unwrap();
    assert_eq!(reqs2.len(), 1, "expected exactly one request");
    let body2 = String::from_utf8_lossy(&reqs2[0].body).to_string();
    assert!(
        !body2.contains("response_format"),
        "string call must NOT send response_format; body: {body2}"
    );
}

/// LIVE strict extraction (gated on `OPENAI_API_KEY`): `gpt-5.4-mini` constrained
/// by the strict `Person` schema returns exact JSON that SAP parses into the class.
#[tokio::test]
async fn strict_live_extraction() {
    if std::env::var("OPENAI_API_KEY").is_err() {
        eprintln!("skipping strict_live_extraction: OPENAI_API_KEY not set");
        return;
    }
    let output = baml_test!(
        r#"
        class Person { name string, age int, nickname string? }
        function main() -> string {
            let p = baml.ai.OpenAiStrict {
                model: "gpt-5.4-mini",
                api_key: baml.env.get_or_panic("OPENAI_API_KEY"),
                base_url: null,
            };
            let per: Person = p.call<Person>(
                "Extract: Ada Lovelace, 36, nickname unknown",
            ) catch (e) {
                let u: baml.errors.UnknownError => return "ERR:" + u.message.join(","),
                let c: baml.errors.CallError => return "CALLERR",
            };
            `${per.name}|${per.age}`
        }
        "#
    );
    let got = output.result.unwrap();
    let BexExternalValue::String(s) = got else {
        panic!("expected string, got {got:?}");
    };
    assert!(
        s.to_lowercase().contains("ada"),
        "live strict extraction did not return the name: {s:?}"
    );
    assert!(
        s.contains("36"),
        "live strict extraction did not return the age: {s:?}"
    );
}
