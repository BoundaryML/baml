// Host-supplied json must materialize with `json` container typing.
//
// Port of python_pydantic2/function_calls/customizable/test_json.py (the
// typed-narrowing half; the Go analog lives in go/function_calls/customizable/
// test_json_test.go).
//
// Inbound json values from the Java bridge carry no element-type annotation on
// the wire; the engine must re-annotate them with the `baml.json.json` alias so
// typed narrowing inside BAML — `match (j) { let m: map<string, json> => ... }`,
// and therefore `baml.json.path` / `path_or` — treats them exactly like
// BAML-born `baml.json.parse` values.
//
// java-port note: where Python/Go pass plain dicts/maps, Java's generated
// surface types json as the sealed interface `baml_sdk.baml.json.json`, so the
// fixtures are built from its record arms (jsonMapValue / jsonListValue /
// StringValue / IntValue).

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertInstanceOf;
import static org.junit.jupiter.api.Assertions.assertThrows;
import static org.junit.jupiter.api.Assertions.assertTrue;

import baml_bridge.BamlError;
import baml_bridge.BamlPanic;
import baml_sdk.baml.json.PathError;
import baml_sdk.baml.json.json;
import baml_sdk.baml.panics.SdkPanic;
import baml_sdk.go_json_tests.Fns;
import java.math.BigInteger;
import java.util.List;
import java.util.Map;
import java.util.concurrent.ExecutionException;
import java.util.concurrent.TimeUnit;
import java.util.function.Function;
import org.junit.jupiter.api.Test;

class TestJson {

    /** {"type": "ok", "nested": {"list": [1, {"deep": "found"}]}} */
    private static json narrowingFixture() {
        json deep = new json.jsonMapValue(Map.of("deep", new json.StringValue("found")));
        json list = new json.jsonListValue(List.of(new json.IntValue(1L), deep));
        json nested = new json.jsonMapValue(Map.of("list", list));
        return new json.jsonMapValue(
                Map.of("type", new json.StringValue("ok"), "nested", nested));
    }

    @Test
    void test_host_supplied_json_supports_typed_narrowing() {
        json obj = narrowingFixture();

        assertEquals("object", Fns.json_kind(obj));
        assertEquals("array", Fns.json_kind(new json.jsonListValue(List.of(new json.IntValue(1L)))));
        assertEquals("string", Fns.json_kind(new json.StringValue("text")));
        assertEquals("other", Fns.json_kind(new json.IntValue(3L)));

        assertEquals("ok", Fns.json_path_string(obj, ".type"));
        assertEquals("found", Fns.json_path_string(obj, ".nested.list[1].deep"));
        assertEquals("fallback", Fns.json_path_string_or(obj, ".missing", "fallback"));

        // java-port note: Python matches str(exc) against "missing field";
        // the generated Java PathError does not render its fields into
        // BamlError's message, so assert on the decoded error's message field.
        BamlError exc =
                assertThrows(BamlError.class, () -> Fns.json_path_string(obj, ".absent"));
        PathError pathError = assertInstanceOf(PathError.class, exc.value());
        assertTrue(pathError.message().contains("missing field"), pathError.message());
    }

    @Test
    void test_json_returned_from_host_callback_supports_typed_narrowing() {
        // json returned from a host callback converts on the host-return path
        // (no argument coercion pass); it must narrow identically.
        //
        // The generated slot types the callback as `Function<json, json>` and
        // the bridge honors that signature: the callback receives the sealed
        // `json` union the parameter declares (here a `json.StringValue`), and
        // returns a `json` value the bridge encodes back to BAML.
        Function<json, json> cb = v -> new json.jsonMapValue(Map.of("wrapped", v));

        assertEquals("object", Fns.json_callback_kind(cb, new json.StringValue("payload")));
    }

    @Test
    void test_empty_json_containers_round_trip() {
        // An empty container has no elements to type it: under `json` it is
        // the alias's own list (`json[]`) or map (`map<string, json>`) member,
        // at any depth, so BAML narrows it like a parsed `[]` / `{}`.
        json emptyArray = new json.jsonListValue(List.of());
        json emptyObject = new json.jsonMapValue(Map.of());
        assertEquals(emptyArray, Fns.round_trip_json(emptyArray));
        assertEquals(emptyObject, Fns.round_trip_json(emptyObject));
        assertEquals("array", Fns.json_kind(emptyArray));
        assertEquals("object", Fns.json_kind(emptyObject));
        assertEquals(emptyArray, Fns.round_trip_json_alias(emptyArray));

        // [[]] and an empty container inside a non-empty one.
        json nested = new json.jsonListValue(List.of(emptyArray));
        assertEquals(nested, Fns.round_trip_json(nested));
        json mixed = new json.jsonMapValue(
                Map.of("items", emptyArray, "meta", emptyObject, "n", new json.IntValue(1L)));
        assertEquals(mixed, Fns.round_trip_json(mixed));

        // Under an exact `json[]` / `map<string, json>` slot, the elements'
        // slot is the alias.
        assertEquals(List.of(), Fns.round_trip_json_list(List.of()));
        assertEquals(List.of(emptyArray, emptyObject), Fns.round_trip_json_list(List.of(emptyArray, emptyObject)));
        assertEquals(Map.of("k", emptyArray), Fns.round_trip_json_map(Map.of("k", emptyArray)));

        // A host callback's empty json return is typed the same way.
        assertEquals("array", Fns.json_callback_kind(v -> new json.jsonListValue(List.of()), emptyObject));
    }

    @Test
    void test_json_bigint_round_trips_as_bigint() {
        // `json` has both `int` and `bigint` members; a BigInteger is a
        // `bigint` whatever its magnitude, so it comes back as one.
        json small = new json.BigintValue(BigInteger.valueOf(5));
        assertEquals(small, Fns.round_trip_json(small));
    }

    /** A json value whose lists nest {@code depth} deep around a {@code 1}. */
    private static json nestedJson(int depth) {
        json value = new json.IntValue(1L);
        for (int i = 0; i < depth; i++) {
            value = new json.jsonListValue(List.of(value));
        }
        return value;
    }

    /** A json value whose single-entry objects nest {@code depth} deep around a {@code 1}. */
    private static json nestedJsonObjects(int depth) {
        json value = new json.IntValue(1L);
        for (int i = 0; i < depth; i++) {
            value = new json.jsonMapValue(Map.of("k", value));
        }
        return value;
    }

    /**
     * The encoder refused an argument nested deeper than the wire carries
     * before sending it, with its ordinary error for an argument it cannot
     * encode, naming the argument.
     */
    private static void assertRefusedAsTooDeep(Throwable error) {
        IllegalArgumentException refused = assertInstanceOf(IllegalArgumentException.class, error);
        assertTrue(
                refused.getMessage().startsWith("cannot encode argument 'value': ")
                        && refused.getMessage().contains("nested too deeply"),
                refused.getMessage());
    }

    /** The call failed with the engine's panic for a bridge failure, naming the depth. */
    private static void assertBridgeFailure(BamlPanic panic) {
        assertEquals("baml.panics.SdkPanic", panic.class_name());
        SdkPanic failure = assertInstanceOf(SdkPanic.class, panic.value());
        assertTrue(failure.message().contains("nested too deeply"), failure.message());
    }

    @Test
    void test_json_nested_deeper_than_the_wire_carries_is_refused_as_an_argument() {
        // Well inside the bound, the value crosses both ways.
        assertEquals(nestedJson(30), Fns.round_trip_json(nestedJson(30)));
        // Past it, and far past it: refused before anything is sent, never by
        // exhausting the stack.
        for (int depth : new int[] {60, 1000}) {
            json value = nestedJson(depth);
            assertRefusedAsTooDeep(assertThrows(Throwable.class, () -> Fns.round_trip_json(value)));
        }
        // The async form refuses through its future.
        ExecutionException failed = assertThrows(
                ExecutionException.class,
                () -> Fns.round_trip_json_async(nestedJson(60)).get(60, TimeUnit.SECONDS));
        assertRefusedAsTooDeep(failed.getCause());
    }

    @Test
    void test_json_nested_deeper_than_the_wire_carries_is_refused_as_a_callback_result() {
        // Well inside the bound, the callback's result crosses back.
        assertEquals(
                nestedJson(30),
                Fns.call_json_callback(v -> nestedJson(30), new json.IntValue(1L)));
        // A result the wire cannot carry is refused where the callback's
        // result is encoded: the bridge's own failure, not the callable's
        // breach of contract and nothing BAML can catch.
        for (int depth : new int[] {60, 200}) {
            assertBridgeFailure(assertThrows(
                    BamlPanic.class,
                    () -> Fns.call_json_callback(v -> nestedJson(depth), new json.IntValue(1L))));
        }
    }

    // SDK_PARITY_LINT(skip): pins a typed SDK encoder's exact message count at the wire's limit
    @Test
    void test_json_nested_at_the_wire_limit_crosses_and_one_level_more_is_refused() {
        // The wire carries 100 nested messages: a json list level costs two,
        // an object level three, and the outermost statement adds two. So 48
        // lists or 32 objects cross as an argument (two messages below the
        // request) and as a callback's result (one below the completion); one
        // level more does not.
        assertEquals(nestedJson(48), Fns.round_trip_json(nestedJson(48)));
        assertEquals(nestedJsonObjects(32), Fns.round_trip_json(nestedJsonObjects(32)));
        assertRefusedAsTooDeep(assertThrows(Throwable.class, () -> Fns.round_trip_json(nestedJson(49))));
        assertRefusedAsTooDeep(
                assertThrows(Throwable.class, () -> Fns.round_trip_json(nestedJsonObjects(33))));

        assertEquals(nestedJson(48), Fns.call_json_callback(v -> nestedJson(48), new json.IntValue(1L)));
        assertEquals(
                nestedJsonObjects(32),
                Fns.call_json_callback(v -> nestedJsonObjects(32), new json.IntValue(1L)));
        assertBridgeFailure(assertThrows(
                BamlPanic.class,
                () -> Fns.call_json_callback(v -> nestedJson(49), new json.IntValue(1L))));
        assertBridgeFailure(assertThrows(
                BamlPanic.class,
                () -> Fns.call_json_callback(v -> nestedJsonObjects(33), new json.IntValue(1L))));
    }
}
