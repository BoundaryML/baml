// Tuples as stream items, over `ns_tuple_streams`: each function's client is
// a scripted `StreamingClient` replaying fixed deltas, so no replay server or
// key is needed. A partial is the declared type parsed from the text so far,
// so every partial of a tuple-typed stream is a tuple: here the generated
// record registered for its tuple identity, never a list fallback.

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertInstanceOf;
import static org.junit.jupiter.api.Assertions.assertTrue;

import baml_bridge.BamlStream;
import baml_sdk.Tuple2_IntString;
import baml_sdk.Tuple2_StringInt;
import baml_sdk.ai.stream.Done;
import baml_sdk.tuple_streams.Fns;
import java.util.ArrayList;
import java.util.List;
import java.util.concurrent.TimeUnit;
import java.util.function.Supplier;
import org.junit.jupiter.api.Test;
import org.junit.jupiter.api.Timeout;

@Timeout(value = 30, unit = TimeUnit.SECONDS)
class TestTupleStreams {

    /** Drain `next` until `Done`, returning every partial in order. */
    private static List<Object> drain(Supplier<Object> next) {
        List<Object> partials = new ArrayList<>();
        while (true) {
            Object partial = next.get();
            if (partial instanceof Done) {
                return partials;
            }
            partials.add(partial);
            assertTrue(partials.size() < 1_000, "stream failed to terminate");
        }
    }

    private static void assertTupleListPartials(List<Object> partials) {
        assertFalse(partials.isEmpty(), "expected at least one partial");
        for (Object partial : partials) {
            List<?> items = assertInstanceOf(List.class, partial);
            for (Object item : items) {
                assertInstanceOf(Tuple2_StringInt.class, item);
            }
        }
        assertEquals(List.of(new Tuple2_StringInt("a", 1)), partials.get(0));
    }

    // SDK_PARITY_LINT(skip): C# streams tuples from its own tuples fixture
    @Test
    void test_tuples_stream_items() {
        List<Tuple2_StringInt> expected = List.of(new Tuple2_StringInt("a", 1), new Tuple2_StringInt("b", 2));

        BamlStream<List<Tuple2_StringInt>> stream = Fns.stream_tuple_items_stream("x");
        assertTupleListPartials(drain(() -> stream.next()));
        assertEquals(expected, stream.get_final());

        BamlStream<List<Tuple2_StringInt>> async = Fns.stream_tuple_items_stream_async("x").join();
        assertTupleListPartials(drain(() -> async.next_async().join()));
        assertEquals(expected, async.get_final_async().join());

        // The non-streaming call settles to the same tuples.
        assertEquals(expected, Fns.stream_tuple_items("x"));
    }

    // SDK_PARITY_LINT(skip): C# streams tuples from its own tuples fixture
    @Test
    void test_tuples_stream_final() {
        Tuple2_IntString expected = new Tuple2_IntString(1, "a");

        BamlStream<Tuple2_IntString> stream = Fns.stream_tuple_final_stream("x");
        List<Object> partials = drain(() -> stream.next());
        assertFalse(partials.isEmpty(), "expected at least one partial");
        for (Object partial : partials) {
            assertInstanceOf(Tuple2_IntString.class, partial);
        }
        assertEquals(expected, partials.get(partials.size() - 1));
        assertEquals(expected, stream.get_final());

        BamlStream<Tuple2_IntString> async = Fns.stream_tuple_final_stream_async("x").join();
        List<Object> asyncPartials = drain(() -> async.next_async().join());
        asyncPartials.forEach(partial -> assertInstanceOf(Tuple2_IntString.class, partial));
        assertEquals(expected, async.get_final_async().join());

        assertEquals(expected, Fns.stream_tuple_final("x"));
    }
}
