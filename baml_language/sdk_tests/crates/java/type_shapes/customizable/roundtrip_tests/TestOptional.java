// Roundtrip coverage for `baml_sdk.optional` — optional Ty variants.
//
// Port of python_pydantic2/type_shapes/customizable/roundtrip_tests/
// test_optional.py — same test names, cases, inputs, assertions.
//
// java-port note: for the union-shaped values below, see TestUnions.java
// for the sealed-interface shape (`UnionIntOrString`) this port assumes.
// `T? | string` collapsing to a nullable `UnionIntOrString` matches the
// conventions doc's "Null-bearing unions collapse to `@Nullable T`" rule.
package roundtrip_tests;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertNull;

import baml_sdk.optional.Fns;
import baml_sdk.optional.OptionalContainer;
import baml_sdk.optional.Resume;
import baml_sdk.unions$.UnionIntOrString;
import org.junit.jupiter.api.Test;

class TestOptional {

    @Test
    void test_round_trip_optional_int() {
        assertEquals(5L, Fns.round_trip_optional_int(5L));
        assertNull(Fns.round_trip_optional_int(null));
    }

    @Test
    void test_round_trip_optional_resume() {
        Resume r = new Resume("ada");
        assertEquals(r, Fns.round_trip_optional_resume(r));
        assertNull(Fns.round_trip_optional_resume(null));
    }

    @Test
    void test_round_trip_optional_union() {
        assertEquals(
                new UnionIntOrString.IntValue(3L),
                Fns.round_trip_optional_union(new UnionIntOrString.IntValue(3L)));
        assertEquals(
                new UnionIntOrString.StringValue("s"),
                Fns.round_trip_optional_union(new UnionIntOrString.StringValue("s")));
        assertNull(Fns.round_trip_optional_union(null));
    }

    @Test
    void test_round_trip_resume() {
        Resume r = new Resume("grace");
        assertEquals(r, Fns.round_trip_resume(r));
    }

    @Test
    void test_round_trip_optional_container() {
        OptionalContainer c =
                new OptionalContainer(
                        null, new Resume("x"), new UnionIntOrString.StringValue("y"));
        assertEquals(c, Fns.round_trip_optional_container(c));
    }
}
