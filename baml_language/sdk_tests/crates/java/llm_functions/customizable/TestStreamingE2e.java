// Keyless streaming smokes — string-typed and class-typed `T`.
//
// Port of python_pydantic2/llm_functions/customizable/test_streaming_e2e.py —
// same test names, cases, assertions.
//
// Exercises the full streaming path — bridge -> BAML LLM client -> HTTP -> SSE
// -> StreamAccumulator -> SAP -> BamlStream.next()/get_final() -> bridge —
// without hitting OpenAI. `ReplayHarness.start(...)` (see ReplayHarness.java, the
// port of the `@replay_server` decorator) runs each test against an in-process
// BAML server replaying a checked-in SSE recording, with the env-driven
// `StreamStub` client pointed at it:
//
//   * string `T` — stream_e2e_extract(text) -> string
//                  (BamlStream<@Nullable String, String>)
//   * class  `T` — stream_e2e_extract_doc(text) -> StreamingDoc
//                  (BamlStream<StreamingDoc$stream, StreamingDoc>)
//
// The recordings stream many SSE chunks, so each next() yields >= 10 partials
// before Done (asserted below).
//
// ===========================================================================
// java-port notes on the streaming surface:
//   * `BamlStream<TPartial, TFinal>` is the runtime wrapper (baml_bridge). Its
//     `next()` is declared `TPartial` but callers bind the result to `Object`:
//     it is either a `TPartial` partial (nullable) or an `ai.stream.Done`
//     sentinel — Java generics can't express the `TPartial | Done`
//     union, and the `if (v instanceof Done)` control flow must compile. This
//     is the faithful port of Python's sentinel duck-typing (a sealed
//     `StreamItem<T>` is a possible future shape).
//   * Method names follow the codegen-conventions doc: next()/next_async() and
//     get_final()/get_final_async() (get_final escapes Python's `final`, a Java
//     reserved word — OWNER decision 2026-07-18).
//   * `await x_async()` ports to `x_async().join()` per the conventions doc.
//   * `$stream` companions keep the BAML name verbatim ($ is legal in Java):
//     the streaming factory is `stream_e2e_extract$stream(...)` (not Python's
//     `_stream`), and a class partial is the in-package companion
//     `baml_sdk.lorem.StreamingDoc$stream` (not Python's `stream_types.lorem.*`
//     legacy layout) — the same retarget TestStreams got (GAP B, 2026-07-17).
//   * `hasattr(v, "title")` ports to a reflective accessor-presence check
//     (`hasAccessor`), matching Python's duck-typed shape probe.
//   * The replay env-var plumbing routes through the native `BridgeEnv` setenv
//     shim (see ReplayHarness.java) so the JNI-linked engine observes it.
// ===========================================================================

import static org.junit.jupiter.api.Assertions.assertInstanceOf;
import static org.junit.jupiter.api.Assertions.assertTrue;

import baml_bridge.BamlStream;
import baml_sdk.ai.stream.Done;
import baml_sdk.lorem.Fns;
import baml_sdk.lorem.StreamE2ECollectResult;
import baml_sdk.lorem.StreamingDoc;
import java.util.concurrent.TimeUnit;
import org.junit.jupiter.api.Test;
import org.junit.jupiter.api.Timeout;

// A blocking `next()` / `join()` that never drains (a mis-behaving replay
// server, a stuck SSE stream) would otherwise hang the whole suite; a
// class-level per-test timeout bounds every streaming test.
@Timeout(value = 30, unit = TimeUnit.SECONDS)
class TestStreamingE2e {

    /** Java analog of Python's `hasattr(obj, name)` for a no-arg accessor. */
    private static boolean hasAccessor(Object obj, String name) {
        if (obj == null) {
            return false;
        }
        try {
            obj.getClass().getMethod(name);
            return true;
        } catch (NoSuchMethodException e) {
            return false;
        }
    }

    // -----------------------------------------------------------------------
    // String-typed `T` — BamlStream<@Nullable String, String>.
    // -----------------------------------------------------------------------

    @Test
    void test_streaming_e2e_stream() throws Exception {
        // Sync `next()` yields a stream of partials and drains to `Done`.
        try (ReplayHarness h = ReplayHarness.start("replay_extract_string")) {
            BamlStream<String, String> stream =
                    Fns.stream_e2e_extract$stream("ignored-by-replay-server");
            int results = 0;
            while (true) {
                Object v = stream.next();
                if (v instanceof Done) {
                    break;
                }
                results += 1;
                assertTrue(v == null || v instanceof String);
                assertTrue(results < 10_000, "stream.next() failed to terminate");
            }
            assertTrue(results >= 10, "expected stream.next() to yield at least 10 partials");
            assertInstanceOf(String.class, stream.get_final());
        }
    }

    @Test
    void test_streaming_e2e_stream_async() throws Exception {
        // Async sibling over the CompletableFuture path: next_async() / get_final_async().
        try (ReplayHarness h = ReplayHarness.start("replay_extract_string")) {
            BamlStream<String, String> stream =
                    Fns.stream_e2e_extract$stream_async("ignored-by-replay-server").join();
            int results = 0;
            while (true) {
                Object v = stream.next_async().join();
                if (v instanceof Done) {
                    break;
                }
                results += 1;
                assertTrue(v == null || v instanceof String);
                assertTrue(results < 10_000, "stream.next_async() failed to terminate");
            }
            assertTrue(results >= 10, "expected stream.next_async() to yield at least 10 partials");
            assertInstanceOf(String.class, stream.get_final_async().join());
        }
    }

    @Test
    void test_streaming_e2e_stream_collect_in_baml() throws Exception {
        // BAML-driven counterpart: the `S | Done` union stays engine-side.
        // java-port note: `result`/`item` are statically typed, so the
        // `assertInstanceOf` / `instanceof String` checks are the compile-time
        // guaranteed analogs of Python's runtime `isinstance` assertions.
        try (ReplayHarness h = ReplayHarness.start("replay_extract_string")) {
            StreamE2ECollectResult result = Fns.stream_e2e_collect("ignored-by-replay-server");
            assertInstanceOf(StreamE2ECollectResult.class, result);
            assertTrue(result.next_calls().size() >= 10, "expected at least 10 collected partials");
            for (String item : result.next_calls()) {
                assertTrue(item == null || item instanceof String);
            }
            assertInstanceOf(String.class, result.final_call());
        }
    }

    // -----------------------------------------------------------------------
    // Class-typed `T` — BamlStream<StreamingDoc$stream, StreamingDoc>. The
    // regression guard for the class-typed streaming bug (doc 00).
    // -----------------------------------------------------------------------

    @Test
    void test_streaming_e2e_stream_doc() throws Exception {
        // Sync `next()` yields >= 10 doc partials; `get_final()` is a typed `StreamingDoc`.
        try (ReplayHarness h = ReplayHarness.start("replay_extract_doc")) {
            BamlStream<baml_sdk.lorem.StreamingDoc$stream, StreamingDoc> stream =
                    Fns.stream_e2e_extract_doc$stream("ignored-by-replay-server");
            int results = 0;
            while (true) {
                Object v = stream.next();
                if (v instanceof Done) {
                    break;
                }
                results += 1;
                if (v != null) {
                    assertTrue(hasAccessor(v, "title"), "unexpected partial: " + v);
                }
                assertTrue(results < 10_000, "stream.next() failed to terminate");
            }
            assertTrue(results >= 10, "expected stream.next() to yield at least 10 partials");
            assertInstanceOf(StreamingDoc.class, stream.get_final());
        }
    }

    @Test
    void test_streaming_e2e_stream_doc_async() throws Exception {
        // Async sibling over the CompletableFuture path for a class `T`.
        try (ReplayHarness h = ReplayHarness.start("replay_extract_doc")) {
            BamlStream<baml_sdk.lorem.StreamingDoc$stream, StreamingDoc> stream =
                    Fns.stream_e2e_extract_doc$stream_async("ignored-by-replay-server").join();
            int results = 0;
            while (true) {
                Object v = stream.next_async().join();
                if (v instanceof Done) {
                    break;
                }
                results += 1;
                if (v != null) {
                    assertTrue(hasAccessor(v, "title"), "unexpected partial: " + v);
                }
                assertTrue(results < 10_000, "stream.next_async() failed to terminate");
            }
            assertTrue(results >= 10, "expected stream.next_async() to yield at least 10 partials");
            assertInstanceOf(StreamingDoc.class, stream.get_final_async().join());
        }
    }

    @Test
    void test_streaming_e2e_stream_doc_collect_in_baml() throws Exception {
        // BAML-driven counterpart: the `S | Done` union stays
        // engine-side; only the concrete `StreamingDoc` crosses the FFI boundary.
        try (ReplayHarness h = ReplayHarness.start("replay_extract_doc")) {
            Object result = Fns.stream_e2e_collect_doc("ignored-by-replay-server");
            assertTrue(
                    result instanceof StreamingDoc
                            || result instanceof baml_sdk.lorem.StreamingDoc$stream,
                    "expected a StreamingDoc (final or partial)");
            assertTrue(hasAccessor(result, "title"));
        }
    }
}
