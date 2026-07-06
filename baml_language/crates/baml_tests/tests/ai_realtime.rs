//! Tests for the `baml.ws` WebSocket surface and the LIVE `baml.ai.OpenAiRealtime`
//! realtime provider (scenario 22).
//!
//! - Offline: `baml.ws.connect` to an unreachable port throws `Io` (fast, deterministic).
//! - Live (gated on `OPENAI_API_KEY`): a real text-mode exchange against OpenAI's
//!   Realtime API, driven through the `baml.ws` transport, negotiated via `match`.

use baml_tests::baml_test;
use bex_engine::BexExternalValue;

/// A `connect` to a closed port must surface as `baml.errors.Io` — no network, no key,
/// no hang. `Io` is the only error `connect` throws, so catching it is exhaustive.
#[tokio::test]
async fn ws_connect_unreachable_throws_io() {
    let output = baml_test!(
        r#"
        function main() -> string {
            let headers: map<string, string> = {};
            let ws = baml.ws.connect("ws://127.0.0.1:1/x", headers) catch (e) {
                let io: baml.errors.Io => return "io",
            };
            ws.close();
            "connected"
        }
        "#
    );
    assert_eq!(
        output.result.unwrap(),
        BexExternalValue::String("io".into())
    );
}

/// Live smoke test against the real OpenAI Realtime API. Skipped unless `OPENAI_API_KEY`
/// is set. Exercises the whole stack: `baml.ws` transport → `OpenAiRealtime.run` →
/// prompt/response.create events → text-delta accumulation → `Transcript`, with the
/// provider's raw event feed forwarded to a user-package `Channel`.
#[tokio::test]
async fn realtime_text_exchange_live() {
    if std::env::var("OPENAI_API_KEY").is_err() {
        eprintln!("skipping realtime_text_exchange_live: OPENAI_API_KEY not set");
        return;
    }
    let output = baml_test!(
        r#"
        // A user-package channel: records every raw event the provider forwards.
        class RecordingChannel {
            events: string[],

            implements baml.ai.Channel {
                function on(self, handler: (string) -> void) -> null throws never {
                    null
                }
                function send(self, data: string) -> null throws baml.errors.Io {
                    self.events.push(data);
                    null
                }
                function close(self) -> null throws never {
                    null
                }
            }
        }

        function main() -> string {
            let ch = RecordingChannel { events: [] };
            let p: baml.ai.Provider = baml.ai.OpenAiRealtime {
                model: "gpt-realtime",
                voice: "alloy",
                api_key: baml.env.get_or_panic("OPENAI_API_KEY"),
            };
            let t = match (p) {
                let r: baml.ai.Realtime => r.run("Reply with exactly the lowercase word: pong", ch),
                _ => return "no_realtime",
            } catch (e) {
                let u: baml.errors.UnknownError => return "ERR:" + u.message.join(","),
                let re: baml.errors.RealtimeError => return "REALTIMEERR",
            };
            t.text + "|events=" + ch.events.length().to_string()
        }
        "#
    );

    let result = output.result.expect("live realtime run should not error");
    let s = match result {
        BexExternalValue::String(s) => s.to_string(),
        other => panic!("expected a string transcript, got {other:?}"),
    };
    eprintln!("realtime live result: {s}");
    assert!(
        s.to_lowercase().contains("pong"),
        "transcript should contain 'pong', got: {s}"
    );
    assert!(
        !s.contains("events=0") && s.contains("events="),
        "channel should have received the raw event feed, got: {s}"
    );
}
