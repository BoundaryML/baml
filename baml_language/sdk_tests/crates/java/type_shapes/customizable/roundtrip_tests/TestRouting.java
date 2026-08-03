// Roundtrip coverage for the cross-namespace routing-rules suite: root
// (`baml_sdk`), `a`, `a.b`, `lorem`, and `ipsum` leaves.
//
// The `baml.http.Response`-typed round trips in `lorem` are covered in
// TestStreams.java (they need an engine-minted handle and can't be built
// host-side).
//
// Port of python_pydantic2/type_shapes/customizable/roundtrip_tests/
// test_routing.py — same test names, cases, inputs, assertions.
//
// java-port note: every namespace here generates its own free-function
// holder class named `Fns` (per the conventions doc), so at most one `Fns`
// can be imported by simple name in a single file. Only the root
// `baml_sdk.Fns` is imported; the leaf-namespace calls below are fully
// qualified (`baml_sdk.a.Fns...`, `baml_sdk.a.b.Fns...`,
// `baml_sdk.lorem.Fns...`, `baml_sdk.ipsum.Fns...`).
package roundtrip_tests;

import static org.junit.jupiter.api.Assertions.assertEquals;

import baml_sdk.Fns;
import baml_sdk.Foo;
import baml_sdk.a.b.Thing;
import baml_sdk.lorem.Resume;
import org.junit.jupiter.api.Test;

class TestRouting {

    @Test
    void test_routing_make_foo() {
        assertEquals(3L, Fns.make_foo(3L).v());
    }

    @Test
    void test_routing_round_trip_foo() {
        Foo f = new Foo(10L);
        assertEquals(f, Fns.round_trip_foo(f));
    }

    @Test
    void test_routing_round_trip_thing_from_ab() {
        Thing t = new Thing(1L);
        assertEquals(t, baml_sdk.a.b.Fns.round_trip_thing_from_ab(t));
    }

    @Test
    void test_routing_round_trip_root_foo_from_ab() {
        Foo f = new Foo(2L);
        assertEquals(f, baml_sdk.a.b.Fns.round_trip_root_foo_from_ab(f));
    }

    @Test
    void test_routing_round_trip_deep_thing_from_a() {
        Thing t = new Thing(4L);
        assertEquals(t, baml_sdk.a.Fns.round_trip_deep_thing_from_a(t));
    }

    @Test
    void test_routing_round_trip_deep_thing_from_lorem() {
        Thing t = new Thing(5L);
        assertEquals(t, baml_sdk.lorem.Fns.round_trip_deep_thing_from_lorem(t));
    }

    @Test
    void test_routing_round_trip_resume() {
        Resume r = new Resume("ada", null);
        assertEquals(r, baml_sdk.lorem.Fns.round_trip_resume(r));
    }

    @Test
    void test_routing_round_trip_root_foo() {
        Foo f = new Foo(6L);
        assertEquals(f, baml_sdk.lorem.Fns.round_trip_root_foo(f));
    }

    @Test
    void test_routing_round_trip_lorem_resume_from_ipsum() {
        Resume r = new Resume("grace", "g@x.com");
        assertEquals(r, baml_sdk.ipsum.Fns.round_trip_lorem_resume_from_ipsum(r));
    }
}
