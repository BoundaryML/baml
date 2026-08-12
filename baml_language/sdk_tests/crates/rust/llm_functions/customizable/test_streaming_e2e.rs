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

// PROVISIONAL(rust-codegen): the streaming surface has no final Rust naming.
// This port assumes:
//   * `$stream` companions bind as `<function>_stream` /
//     `<function>_stream_async` (mirroring the python binding names);
//   * the stream's `next()` returns `Result<Option<S>, _>`, with `None`
//     adapting the engine's `ai.stream.Done` sentinel, and `final_()` returns
//     `Result<T, _>` (`final` is a reserved keyword);
//   * the `_async` companion returns a stream whose `next()`/`final_()` are
//     themselves `async` — python names those methods `next_async` /
//     `final_async` instead.

use crate::replay_harness::{replay_server, replay_server_async};

// ---------------------------------------------------------------------------
// String-typed `T` — Stream<null | string, string>.
// ---------------------------------------------------------------------------

/// Sync `next()` yields a stream of partials and drains to `None`.
#[test]
fn test_streaming_e2e_stream() {
    use baml_sdk::lorem::stream_e2e_extract_stream;

    replay_server("replay_extract_string", || {
        let mut stream = stream_e2e_extract_stream("ignored-by-replay-server".to_string()).unwrap();
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
        let mut stream = stream_e2e_extract_stream_async("ignored-by-replay-server".to_string())
            .await
            .unwrap();
        let mut results = 0;
        while let Some(v) = stream.next().await.unwrap() {
            results += 1;
            let _: Option<String> = v;
            assert!(results < 10_000, "stream.next() failed to terminate");
        }
        assert!(
            results >= 10,
            "expected stream.next() to yield at least 10 partials"
        );
        let _: String = stream.final_().await.unwrap();
    })
    .await;
}

/// BAML-driven counterpart: the `S | ai.stream.Done` union stays engine-side.
#[test]
fn test_streaming_e2e_stream_collect_in_baml() {
    use baml_sdk::lorem::{StreamE2ECollectResult, stream_e2e_collect};

    replay_server("replay_extract_string", || {
        // python: `isinstance(result, StreamE2ECollectResult)` — the
        // annotated binding pins it at compile time.
        let result: StreamE2ECollectResult =
            stream_e2e_collect("ignored-by-replay-server".to_string()).unwrap();
        assert!(
            result.next_calls.len() >= 10,
            "expected at least 10 collected partials"
        );
        for item in &result.next_calls {
            // python: `item is None or isinstance(item, str)` — compile-time:
            let _: &Option<String> = item;
        }
        // python: `isinstance(result.final_call, str)` — compile-time:
        let _: String = result.final_call;
    });
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
        let mut stream =
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
        let mut stream =
            stream_e2e_extract_doc_stream_async("ignored-by-replay-server".to_string())
                .await
                .unwrap();
        let mut results = 0;
        while let Some(v) = stream.next().await.unwrap() {
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
        let _: StreamingDoc = stream.final_().await.unwrap();
    })
    .await;
}

/// BAML-driven counterpart: the `S | ai.stream.Done` union stays engine-side;
/// only the concrete `StreamingDoc` crosses the FFI boundary.
#[test]
fn test_streaming_e2e_stream_doc_collect_in_baml() {
    use baml_sdk::lorem::{StreamingDoc, stream_e2e_collect_doc};

    replay_server("replay_extract_doc", || {
        // DIVERGENCE(rust): python tolerates either the complete class or
        // its `$stream` partial crossing the boundary
        // (`isinstance(result, (StreamingDoc, StreamingDocPartial))`); the
        // typed Rust signature pins the declared return, exactly
        // `StreamingDoc`.
        let result: StreamingDoc =
            stream_e2e_collect_doc("ignored-by-replay-server".to_string()).unwrap();
        // python: `hasattr(result, "title")` — the field access pins it.
        let _ = &result.title;
    });
}
