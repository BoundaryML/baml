//! Engine-level transport pin for the streaming replay harness
//! (thoughts/sam-projects/bridge-generics/streaming/02).
//!
//! Compiles `data/replay_server.baml` and runs `replay_sse_roundtrip` through
//! the engine helpers: a BAML `http.Server` serves an OpenAI-shaped SSE body
//! and the engine's own SSE client (`baml.http.fetch_sse`) consumes it. This
//! proves a BAML-implemented server can stand in for WireMock before any
//! bridge/SDK machinery is involved — so a bridge-level replay failure
//! (pytest/vitest) can only be a bridge/orchestration issue, not transport.
//!
//! Asserting in Rust (rather than a BAML `test` block run via the `baml test`
//! CLI) keeps the pin co-located with the recorder and sidesteps an unrelated
//! CLI-path miscompile of `String.split(multi-char).length()`.

use baml_tests::engine::{IndexMap, compile_multi_file, run_compiled};
use bex_engine::BexExternalValue;

#[tokio::test(flavor = "multi_thread")]
async fn replay_sse_roundtrip() {
    let src = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/data/replay_server.baml"
    ))
    .expect("read data/replay_server.baml");

    let program = compile_multi_file(&[("ns_replay_server/replay_server.baml", src.as_str())]);
    let out = run_compiled(
        program,
        "replay_server.replay_sse_roundtrip",
        IndexMap::new(),
        false,
    )
    .await;

    let acc = match out.result.expect("replay_sse_roundtrip should succeed") {
        BexExternalValue::String(s) => s.to_string(),
        other => panic!("expected string, got {other:?}"),
    };

    // Each SSE event is serialized as a JSON object with a `data` field, so the
    // number of `"data":` keys is the event count: 3 content + 1 finish + [DONE].
    let event_count = acc.matches("\"data\":").count();
    assert_eq!(event_count, 5, "expected 5 SSE events, acc = {acc}");

    // The content chunks, the finish event, and the [DONE] sentinel all arrived
    // intact through the BAML server -> fetch_sse transport...
    for marker in ["Hello", ", world", "finish_reason", "[DONE]"] {
        assert!(acc.contains(marker), "missing {marker:?} in {acc}");
    }
    // ...and in the order they were sent.
    let idx = |needle: &str| acc.find(needle).unwrap_or_else(|| panic!("no {needle:?}"));
    assert!(idx("Hello") < idx("world"));
    assert!(idx("world") < idx("finish_reason"));
    assert!(idx("finish_reason") < idx("[DONE]"));
}
