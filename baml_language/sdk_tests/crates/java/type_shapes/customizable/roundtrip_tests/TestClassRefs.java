// Roundtrip coverage for `baml_sdk.class_refs` — class composition.
//
// Port of python_pydantic2/type_shapes/customizable/roundtrip_tests/
// test_class_refs.py — same test names, cases, inputs, assertions.
package roundtrip_tests;

import static org.junit.jupiter.api.Assertions.assertEquals;

import baml_sdk.class_refs.Fns;
import baml_sdk.class_refs.Inner;
import baml_sdk.class_refs.Outer;
import org.junit.jupiter.api.Test;

class TestClassRefs {

    @Test
    void test_class_refs_make_outer() {
        Outer o = Fns.make_outer(5L);
        assertEquals(5L, o.inner().value());
    }

    @Test
    void test_class_refs_round_trip_inner() {
        Inner i = new Inner(3L);
        assertEquals(i, Fns.round_trip_inner(i));
    }

    @Test
    void test_class_refs_round_trip_outer() {
        Outer o = new Outer(new Inner(9L));
        assertEquals(o, Fns.round_trip_outer(o));
    }
}
