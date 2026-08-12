// Roundtrip coverage for `baml_sdk.optional` — optional Ty variants.
//
// Port of python_pydantic2/type_shapes/customizable/roundtrip_tests/
// test_optional.py — same test names, cases, inputs, assertions.
//
// java-port note: for the union-shaped values below, see TestUnions.java
// for the generic-family shape (`(int | string)` -> `Union2<Long, String>`,
// int = Arm0, string = Arm1) this port assumes. `(int | string)?` collapsing
// to a nullable `Union2<Long, String>` matches the conventions doc's
// "Null-bearing unions collapse to `@Nullable T`" rule.
package roundtrip_tests;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertNull;

import baml_bridge.Union2;
import baml_sdk.optional.Fns;
import baml_sdk.optional.OptionalContainer;
import baml_sdk.optional.Resume;
import org.junit.jupiter.api.Test;

class TestOptional {

    @Test
    void test_optional_round_trip_optional_int() {
        assertEquals(5L, Fns.round_trip_optional_int(5L));
        assertNull(Fns.round_trip_optional_int(null));
    }

    @Test
    void test_optional_round_trip_optional_resume() {
        Resume r = new Resume("ada");
        assertEquals(r, Fns.round_trip_optional_resume(r));
        assertNull(Fns.round_trip_optional_resume(null));
    }

    @Test
    void test_optional_round_trip_optional_union() {
        assertEquals(
                new Union2.Arm0<Long, String>(3L),
                Fns.round_trip_optional_union(new Union2.Arm0<Long, String>(3L)));
        assertEquals(
                new Union2.Arm1<Long, String>("s"),
                Fns.round_trip_optional_union(new Union2.Arm1<Long, String>("s")));
        assertNull(Fns.round_trip_optional_union(null));
    }

    @Test
    void test_optional_round_trip_resume() {
        Resume r = new Resume("grace");
        assertEquals(r, Fns.round_trip_resume(r));
    }

    @Test
    void test_optional_round_trip_optional_container() {
        OptionalContainer c =
                new OptionalContainer(
                        null, new Resume("x"), new Union2.Arm1<Long, String>("y"));
        assertEquals(c, Fns.round_trip_optional_container(c));
    }
}
