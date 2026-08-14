// Roundtrip coverage for `baml_sdk.maps` — map Ty variants.
//
// NOTE: enum-keyed maps don't round-trip yet. proto.py encodes an enum map
// key as a typed `enum_key`, but the OUTBOUND map entry carries only a
// scalar `entry.key`, so the engine renders an enum key as the string
// `"<fqn>::<variant>"` and decode hands it back as that raw string rather
// than the enum member. Finishing it needs a typed outbound map key (proto
// schema + engine emit + decode), not just a proto.py tweak. The
// `round_trip_enum_keyed_map` and `round_trip_map_container` tests (the
// latter has a required `enum_keyed: map<Sentiment, Resume>` field) are
// dropped until that lands; enum *values* still round-trip
// (test_round_trip_sentiment).
//
// Port of python_pydantic2/type_shapes/customizable/roundtrip_tests/
// test_maps.py — same test names, cases, inputs, assertions.
package roundtrip_tests;

import static org.junit.jupiter.api.Assertions.assertEquals;

import baml_sdk.maps.Fns;
import baml_sdk.maps.Resume;
import baml_sdk.maps.Sentiment;
import java.util.List;
import java.util.Map;
import org.junit.jupiter.api.Test;

class TestMaps {

    @Test
    void test_maps_round_trip_simple_map() {
        assertEquals(
                Map.of("a", 1L, "b", 2L), Fns.round_trip_simple_map(Map.of("a", 1L, "b", 2L)));
    }

    @Test
    void test_maps_round_trip_list_valued_map() {
        assertEquals(
                Map.of("k", List.of(1L, 2L)),
                Fns.round_trip_list_valued_map(Map.of("k", List.of(1L, 2L))));
    }

    @Test
    void test_maps_round_trip_sentiment() {
        assertEquals(Sentiment.Positive, Fns.round_trip_sentiment(Sentiment.Positive));
    }

    @Test
    void test_maps_round_trip_resume() {
        Resume r = new Resume("n");
        assertEquals(r, Fns.round_trip_resume(r));
    }
}
