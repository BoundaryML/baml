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
// before StreamFinished (asserted below).
//
// ===========================================================================
// java-port notes on the streaming surface (INVENTED shapes — need review):
//   * `BamlStream<TPartial, TFinal>` is the runtime wrapper (baml_bridge). Its
//     `next()` returns `Object` — either a `TPartial` partial (nullable) or a
//     `StreamFinished` sentinel — because Java generics can't express the
//     `TPartial | StreamFinished` union and the `if (v instanceof
//     StreamFinished)` control flow must compile. A sealed `StreamItem<T>`
//     return type is the better long-term shape; `Object` is the faithful port
//     of Python's `isinstance(v, StreamFinished)` duck-typing.
//   * Method names follow the codegen-conventions doc: next()/next_async() and
//     get_final()/get_final_async(). The Python source spells the final
//     accessor `final()`/`final_async()`; the rename is provisional.
//   * `await x_async()` ports to `x_async().join()` per the conventions doc.
//   * `hasattr(v, "title")` ports to a reflective accessor-presence check
//     (`hasAccessor`) so the partial's duck-typed shape is pinned without
//     importing the name-clashing `stream_types.lorem.StreamingDoc`.
//   * The replay env-var plumbing is only a structural port — see the
//     `java-port TODO` in ReplayHarness.java (needs a native setenv reachable
//     by the JNI engine).
// ===========================================================================

import static org.junit.jupiter.api.Assertions.assertInstanceOf;
import static org.junit.jupiter.api.Assertions.assertTrue;

import baml_bridge.BamlStream;
import baml_sdk.baml.stream.StreamFinished;
import baml_sdk.lorem.Fns;
import baml_sdk.lorem.StreamE2ECollectResult;
import baml_sdk.lorem.StreamingDoc;
import org.junit.jupiter.api.Test;

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
    void test_stream() throws Exception {
        // Sync `next()` yields a stream of partials and drains to `StreamFinished`.
        try (ReplayHarness h = ReplayHarness.start("replay_extract_string")) {
            BamlStream<String, String> stream =
                    Fns.stream_e2e_extract_stream("ignored-by-replay-server");
            int results = 0;
            while (true) {
                Object v = stream.next();
                if (v instanceof StreamFinished) {
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
    void test_stream_async() throws Exception {
        // Async sibling over the CompletableFuture path: next_async() / get_final_async().
        try (ReplayHarness h = ReplayHarness.start("replay_extract_string")) {
            BamlStream<String, String> stream =
                    Fns.stream_e2e_extract_stream_async("ignored-by-replay-server").join();
            int results = 0;
            while (true) {
                Object v = stream.next_async().join();
                if (v instanceof StreamFinished) {
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
    void test_stream_collect_in_baml() throws Exception {
        // BAML-driven counterpart: the `S | StreamFinished` union stays engine-side.
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
    void test_stream_doc() throws Exception {
        // Sync `next()` yields >= 10 doc partials; `get_final()` is a typed `StreamingDoc`.
        try (ReplayHarness h = ReplayHarness.start("replay_extract_doc")) {
            BamlStream<baml_sdk.stream_types.lorem.StreamingDoc, StreamingDoc> stream =
                    Fns.stream_e2e_extract_doc_stream("ignored-by-replay-server");
            int results = 0;
            while (true) {
                Object v = stream.next();
                if (v instanceof StreamFinished) {
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
    void test_stream_doc_async() throws Exception {
        // Async sibling over the CompletableFuture path for a class `T`.
        try (ReplayHarness h = ReplayHarness.start("replay_extract_doc")) {
            BamlStream<baml_sdk.stream_types.lorem.StreamingDoc, StreamingDoc> stream =
                    Fns.stream_e2e_extract_doc_stream_async("ignored-by-replay-server").join();
            int results = 0;
            while (true) {
                Object v = stream.next_async().join();
                if (v instanceof StreamFinished) {
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
    void test_stream_doc_collect_in_baml() throws Exception {
        // BAML-driven counterpart: the `S | StreamFinished` union stays
        // engine-side; only the concrete `StreamingDoc` crosses the FFI boundary.
        try (ReplayHarness h = ReplayHarness.start("replay_extract_doc")) {
            Object result = Fns.stream_e2e_collect_doc("ignored-by-replay-server");
            assertTrue(
                    result instanceof StreamingDoc
                            || result instanceof baml_sdk.stream_types.lorem.StreamingDoc,
                    "expected a StreamingDoc (final or partial)");
            assertTrue(hasAccessor(result, "title"));
        }
    }
}
