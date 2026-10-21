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
import baml_sdk.baml.json.PathError;
import baml_sdk.baml.json.json;
import baml_sdk.go_json_tests.Fns;
import java.math.BigInteger;
import java.util.List;
import java.util.Map;
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
}
