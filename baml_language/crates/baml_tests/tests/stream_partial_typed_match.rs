//! Regression for the `$stream`-partial typed-match VM error (scenario 04's
//! live crash): consuming `s.next()` partials via a typed `T$stream` match arm
//! with field reads (`part.title ?? "..."`) died with `VM internal error:
//! type error: expected map, got instance` once real partials flowed.
//!
//! Root cause: MIR's `package_lowering_data` built its field/enum schema maps
//! from HIR `package_items`, which exclude PPIR-synthesized `*$stream`
//! classes — so `part.title` on `Meeting$stream` silently compiled to the
//! dynamic MAP access path while the runtime materializes SAP partials as
//! class INSTANCES.
//!
//! The mock must DRIBBLE the SSE events (separate writes with flushes and
//! delays): a single-chunk body lets the accumulator see the finish event on
//! the first `next()`, so no partial ever reaches the typed arm and the test
//! passes vacuously. Each test asserts the arm actually ran.

use std::time::Duration;

use baml_tests::baml_test;
use bex_engine::BexExternalValue;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

/// Minimal SSE server that writes each event as its own flushed TCP write with
/// a small delay in between, so the client-side stream sees real partials.
/// Returns the server's base URI.
async fn spawn_sse_dribble_server(events: Vec<String>) -> String {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind dribble server");
    let addr = listener.local_addr().expect("local addr");
    tokio::spawn(async move {
        loop {
            let Ok((mut socket, _)) = listener.accept().await else {
                return;
            };
            let events = events.clone();
            tokio::spawn(async move {
                // Read the request until the header terminator (the body, if
                // any, is irrelevant to the canned response).
                let mut buf = Vec::new();
                let mut tmp = [0u8; 4096];
                loop {
                    match socket.read(&mut tmp).await {
                        Ok(0) => break,
                        Ok(n) => {
                            buf.extend_from_slice(&tmp[..n]);
                            if buf.windows(4).any(|w| w == b"\r\n\r\n") {
                                break;
                            }
                        }
                        Err(_) => return,
                    }
                }
                let header = "HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\nconnection: close\r\n\r\n";
                if socket.write_all(header.as_bytes()).await.is_err() {
                    return;
                }
                let _ = socket.flush().await;
                for event in &events {
                    tokio::time::sleep(Duration::from_millis(20)).await;
                    if socket.write_all(event.as_bytes()).await.is_err() {
                        return;
                    }
                    let _ = socket.flush().await;
                }
                let _ = socket.shutdown().await;
            });
        }
    });
    format!("http://{addr}")
}

fn openai_sse_events() -> Vec<String> {
    [
        r#"{"choices":[{"index":0,"delta":{"role":"assistant","content":""}}]}"#,
        r#"{"choices":[{"index":0,"delta":{"content":"```json\n"}}]}"#,
        r#"{"choices":[{"index":0,"delta":{"content":"{\"title\": \"Sy"}}]}"#,
        r#"{"choices":[{"index":0,"delta":{"content":"nc\", \"date\": \"2026-0"}}]}"#,
        r#"{"choices":[{"index":0,"delta":{"content":"1-01\", \"attendees\": [\"ada\""}}]}"#,
        r#"{"choices":[{"index":0,"delta":{"content":", \"bob\"]}\n```"}}]}"#,
        r#"{"choices":[{"index":0,"delta":{},"finish_reason":"stop"}]}"#,
        "[DONE]",
    ]
    .iter()
    .map(|data| format!("data: {data}\n\n"))
    .collect()
}

/// Real gemini-flash capture shape: fenced, pretty-printed JSON; the second
/// chunk ends mid-key (`"attendees":` with no value yet).
fn gemini_sse_events() -> Vec<String> {
    [
        r#"{"candidates": [{"content": {"parts": [{"text": "```json\n{\n  "}],"role": "model"},"index": 0}],"modelVersion": "gemini-3.5-flash"}"#,
        r#"{"candidates": [{"content": {"parts": [{"text": "\"title\": \"Lunch to plan Q3\",\n  \"date\": \"Friday\",\n  \"attendees\":"}],"role": "model"},"index": 0}],"modelVersion": "gemini-3.5-flash"}"#,
        r#"{"candidates": [{"content": {"parts": [{"text": " [\n    \"Alice\",\n    \"Bob\"\n  ]\n}\n```"}],"role": "model"},"index": 0}],"modelVersion": "gemini-3.5-flash"}"#,
        r#"{"candidates": [{"content": {"parts": [{"text": ""}],"role": "model"},"finishReason": "STOP","index": 0}],"modelVersion": "gemini-3.5-flash"}"#,
    ]
    .iter()
    .map(|data| format!("data: {data}\n\n"))
    .collect()
}

/// The consumer shape from scenario 04: typed `$stream` arm with field reads.
/// `steps`/`last` in the output prove the partial arm actually executed.
fn consumer_src(client_expr: &str) -> String {
    format!(
        r##"
        class Meeting {{
            title: string
            date: string
            attendees: string[]
        }}

        function ExtractMeeting(blurb: string) -> Meeting {{
            client "openai/gpt-4o"
            prompt #"Extract the meeting from: {{{{ blurb }}}}"#
        }}

        function consume(client: baml.ai.Provider) -> string throws never {{
            let s = ExtractMeeting$stream("standup notes", client = client) catch (e) {{
                _ => return "no-stream",
            }};
            let steps: int = 0;
            let last_title: string = "";
            while (true) {{
                match (s.next()) {{
                    let part: Meeting$stream => {{
                        last_title = part.title ?? "...";
                        let _date: string = part.date ?? "...";
                        steps = steps + 1;
                    }}
                    null => {{}}
                    let _done: baml.stream.StreamFinished => {{ break; }}
                }} catch (e) {{
                    _ => return "mid-stream error",
                }}
            }}
            let m = s.final() catch (e) {{ _ => return "final error" }};
            m.title + "|" + m.date + "|" + m.attendees.length().to_string()
                + "|steps>=1=" + (steps >= 1).to_string() + "|last=" + last_title
        }}

        function main() -> string {{
            consume({client_expr})
        }}
        "##
    )
}

#[tokio::test]
async fn typed_stream_partial_match_over_openai_transport() {
    let uri = spawn_sse_dribble_server(openai_sse_events()).await;
    let client_expr =
        format!(r#"baml.ai.OpenAi {{ model: "m", api_key: "k", base_url: "{uri}" }}"#);
    let output = baml_test!(&consumer_src(&client_expr));
    let got = output.result.unwrap();
    let BexExternalValue::String(s) = got else {
        panic!("expected string, got {got:?}");
    };
    assert_eq!(
        s.as_str(),
        "Sync|2026-01-01|2|steps>=1=true|last=Sync",
        "typed-partial consumption over openai broke"
    );
}

/// Same consumer inside a NESTED NAMESPACE (`ns_ai_scenarios/` →
/// `user.ai_scenarios`) over the Gemini transport — the exact live-crash
/// configuration from scenario 04.
#[tokio::test]
async fn typed_stream_partial_match_nested_ns_over_gemini_transport() {
    let uri = spawn_sse_dribble_server(gemini_sse_events()).await;
    let client_expr = format!(
        r#"baml.ai.Gemini {{
            model: "m",
            api_key: "k",
            base_url: "{uri}",
            extra_headers: null,
            extra_body: null,
            append_output_schema: null,
        }}"#
    );
    let scenario_src = consumer_src(&client_expr);
    let program = baml_tests::engine::compile_multi_file(&[(
        "ns_ai_scenarios/repro.baml",
        scenario_src.as_str(),
    )]);
    let output = baml_tests::engine::run_compiled(
        program,
        "user.ai_scenarios.main",
        baml_tests::engine::IndexMap::new(),
        false,
    )
    .await;
    // The compiled arm must read `part.title` as an instance FIELD, not a
    // dynamic map key — the map path is exactly the regressed lowering.
    let mut in_consume = false;
    let mut consume_body = String::new();
    for line in output.bytecode.lines() {
        if line.starts_with("function ") && line.contains("consume(") {
            in_consume = true;
        }
        if in_consume {
            consume_body.push_str(line);
            consume_body.push('\n');
            if line == "}" {
                break;
            }
        }
    }
    assert!(
        !consume_body.contains("load_map_element"),
        "field access on Meeting$stream compiled to map access:\n{consume_body}"
    );
    let got = output.result.unwrap();
    let BexExternalValue::String(s) = got else {
        panic!("expected string, got {got:?}");
    };
    assert_eq!(
        s.as_str(),
        "Lunch to plan Q3|Friday|2|steps>=1=true|last=Lunch to plan Q3",
        "typed-partial consumption in nested namespace over gemini broke"
    );
}
