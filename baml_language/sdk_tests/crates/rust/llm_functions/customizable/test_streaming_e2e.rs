//! Keyless streaming smokes — string-typed and class-typed `T`.
//!
//! Exercises the full streaming path — bridge → BAML LLM client → HTTP → SSE →
//! StreamAccumulator → SAP → `Stream.next()/final()` → bridge — without hitting
//! OpenAI. The [`replay_server`] wrapper (see `replay_harness.rs`; python
//! applies it as the `@replay_server` decorator) runs each test against an
//! in-process BAML server replaying a checked-in SSE recording, with the
//! env-driven `StreamStub` client pointed at it:
//!
//!   * string `T` — `stream_e2e_extract(text) -> string` (Stream<null | string, string>)
//!   * class  `T` — `stream_e2e_extract_doc(text) -> StreamingDoc { title, body, word_count }`
//!
//! The recordings stream many SSE chunks, so each `next()` yields >= 10 partials
//! before the end marker (asserted below). The class-typed tests are the
//! deterministic, bridge-level regression guard for the class-typed streaming bug
//! (thoughts/sam-projects/bridge-generics/streaming, doc 00).
//!
//! Re-record the SSE fixtures (needs a real key):
//!
//!     INSTA_UPDATE=always infisical run -- cargo nextest run -p sdk_test_llm_recordings

// Flat `_stream` bindings invoke the compiler-private `Fn@stream` projection
// through the authored function FQN plus the Stream boundary operation. The
// `Out$stream` type still carries every partial-output rule.

use crate::replay_harness::{replay_server, replay_server_async};

// ---------------------------------------------------------------------------
// String-typed `T` — Stream<null | string, string>.
// ---------------------------------------------------------------------------

/// Sync `next()` yields a stream of partials and drains to `None`.
#[test]
fn test_streaming_e2e_stream() {
    use baml_sdk::lorem::stream_e2e_extract_stream;

    replay_server("replay_extract_string", || {
        let stream = stream_e2e_extract_stream("ignored-by-replay-server".to_string()).unwrap();
        let mut results = 0;
        while let Some(v) = stream.next().unwrap() {
            results += 1;
            // python: `v is None or isinstance(v, str)` — compile-time here:
            // the partial type is `Option<String>`.
            let _: Option<String> = v;
            assert!(results < 10_000, "stream.next() failed to terminate");
        }
        assert!(
            results >= 10,
            "expected stream.next() to yield at least 10 partials"
        );
        // python: `isinstance(stream.final(), str)` — compile-time here.
        let _: String = stream.final_().unwrap();
    });
}

/// Async sibling (python routes it over the pyo3-tokio path as
/// `next_async()` / `final_async()`).
#[tokio::test]
async fn test_streaming_e2e_stream_async() {
    use baml_sdk::lorem::stream_e2e_extract_stream_async;

    replay_server_async("replay_extract_string", async {
        let stream = stream_e2e_extract_stream_async("ignored-by-replay-server".to_string())
            .await
            .unwrap();
        let mut results = 0;
        while let Some(v) = stream.next_async().await.unwrap() {
            results += 1;
            let _: Option<String> = v;
            assert!(results < 10_000, "stream.next() failed to terminate");
        }
        assert!(
            results >= 10,
            "expected stream.next() to yield at least 10 partials"
        );
        let _: String = stream.final_async().await.unwrap();
    })
    .await;
}

// ---------------------------------------------------------------------------
// Class-typed `T` — Stream<StreamingDoc$stream, StreamingDoc>. The case the
// plain-`string` tests above deliberately avoid; the regression guard for the
// class-typed streaming bug (doc 00).
// ---------------------------------------------------------------------------

/// Sync `next()` yields >= 10 doc partials; `final()` is a typed `StreamingDoc`.
#[test]
fn test_streaming_e2e_stream_doc() {
    use baml_sdk::lorem::{StreamingDoc, stream_e2e_extract_doc_stream};

    replay_server("replay_extract_doc", || {
        let stream =
            stream_e2e_extract_doc_stream("ignored-by-replay-server".to_string()).unwrap();
        let mut results = 0;
        while let Some(v) = stream.next().unwrap() {
            results += 1;
            if let Some(partial) = &v {
                // python: `hasattr(v, "title")` — the field access pins it.
                let _ = &partial.title;
            }
            assert!(results < 10_000, "stream.next() failed to terminate");
        }
        assert!(
            results >= 10,
            "expected stream.next() to yield at least 10 partials"
        );
        // python: `isinstance(stream.final(), StreamingDoc)` — compile-time:
        let _: StreamingDoc = stream.final_().unwrap();
    });
}

/// Async sibling for a class `T` (python's pyo3-tokio path).
#[tokio::test]
async fn test_streaming_e2e_stream_doc_async() {
    use baml_sdk::lorem::{StreamingDoc, stream_e2e_extract_doc_stream_async};

    replay_server_async("replay_extract_doc", async {
        let stream =
            stream_e2e_extract_doc_stream_async("ignored-by-replay-server".to_string())
                .await
                .unwrap();
        let mut results = 0;
        while let Some(v) = stream.next_async().await.unwrap() {
            results += 1;
            if let Some(partial) = &v {
                let _ = &partial.title;
            }
            assert!(results < 10_000, "stream.next() failed to terminate");
        }
        assert!(
            results >= 10,
            "expected stream.next() to yield at least 10 partials"
        );
        let _: StreamingDoc = stream.final_async().await.unwrap();
    })
    .await;
}
