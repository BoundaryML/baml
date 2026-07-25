// Roundtrip coverage for the symbol-collision suite — three distinct `Bar`
// classes at different namespace depths plus the consumers (`Ipsum`,
// `Deep`) that compose all three.
//
// Port of python_pydantic2/type_shapes/customizable/roundtrip_tests/
// test_symbol_collisions.py — same test names, cases, inputs, assertions.
//
// java-port note: Python's collision is class-only (`Bar` appears in three
// namespaces) because free functions keep distinct BAML-declared names
// (`make_foo_bar`, `make_fizz_foo_bar`, `make_fizz_buzz_foo_bar`) that never
// collide, so each imports cleanly by name. In Java, free functions are
// static methods on a per-namespace holder that is *always* named `Fns`
// (per the conventions doc), so the same-name collision also applies to the
// function holders here, not just the `Bar` classes. Every symbol below is
// therefore referenced fully qualified rather than imported, to keep all
// five namespaces unambiguous side by side.
package roundtrip_tests;

import static org.junit.jupiter.api.Assertions.assertEquals;

import org.junit.jupiter.api.Test;

class TestSymbolCollisions {

    @Test
    void test_symbol_collisions_round_trip_foo_bar() {
        baml_sdk.symbol_collisions.foo.Bar bar =
                baml_sdk.symbol_collisions.foo.Fns.make_foo_bar("hi", 2L);
        assertEquals(bar, baml_sdk.symbol_collisions.foo.Fns.round_trip_foo_bar(bar));
    }

    @Test
    void test_symbol_collisions_round_trip_fizz_foo_bar() {
        baml_sdk.symbol_collisions.fizz.foo.Bar bar =
                baml_sdk.symbol_collisions.fizz.foo.Fns.make_fizz_foo_bar("t", 1.5);
        assertEquals(
                bar, baml_sdk.symbol_collisions.fizz.foo.Fns.round_trip_fizz_foo_bar(bar));
    }

    @Test
    void test_symbol_collisions_round_trip_fizz_buzz_foo_bar() {
        baml_sdk.symbol_collisions.fizz.buzz.foo.Bar bar =
                baml_sdk.symbol_collisions.fizz.buzz.foo.Fns.make_fizz_buzz_foo_bar(
                        "f", 2.5, true);
        assertEquals(
                bar,
                baml_sdk.symbol_collisions.fizz.buzz.foo.Fns.round_trip_fizz_buzz_foo_bar(bar));
    }

    @Test
    void test_symbol_collisions_round_trip_ipsum() {
        baml_sdk.symbol_collisions.lorem.Ipsum ipsum =
                baml_sdk.symbol_collisions.lorem.Fns.make_ipsum(
                        baml_sdk.symbol_collisions.foo.Fns.make_foo_bar("a", 1L),
                        baml_sdk.symbol_collisions.fizz.foo.Fns.make_fizz_foo_bar("b", 2.0),
                        baml_sdk.symbol_collisions.fizz.buzz.foo.Fns.make_fizz_buzz_foo_bar(
                                "c", 3.0, false));
        assertEquals(ipsum, baml_sdk.symbol_collisions.lorem.Fns.round_trip_ipsum(ipsum));
    }

    @Test
    void test_symbol_collisions_round_trip_deep() {
        baml_sdk.symbol_collisions.lorem.Ipsum ipsum =
                baml_sdk.symbol_collisions.lorem.Fns.make_ipsum(
                        baml_sdk.symbol_collisions.foo.Fns.make_foo_bar("a", 1L),
                        baml_sdk.symbol_collisions.fizz.foo.Fns.make_fizz_foo_bar("b", 2.0),
                        baml_sdk.symbol_collisions.fizz.buzz.foo.Fns.make_fizz_buzz_foo_bar(
                                "c", 3.0, false));
        baml_sdk.symbol_collisions.a.b.c.d.Deep deep =
                baml_sdk.symbol_collisions.a.b.c.d.Fns.make_deep(
                        baml_sdk.symbol_collisions.foo.Fns.make_foo_bar("h", 9L),
                        baml_sdk.symbol_collisions.fizz.foo.Fns.make_fizz_foo_bar("th", 4.0),
                        baml_sdk.symbol_collisions.fizz.buzz.foo.Fns.make_fizz_buzz_foo_bar(
                                "fu", 5.0, true),
                        ipsum);
        assertEquals(deep, baml_sdk.symbol_collisions.a.b.c.d.Fns.round_trip_deep(deep));
    }
}
