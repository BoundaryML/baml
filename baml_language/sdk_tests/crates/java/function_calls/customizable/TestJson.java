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
}
