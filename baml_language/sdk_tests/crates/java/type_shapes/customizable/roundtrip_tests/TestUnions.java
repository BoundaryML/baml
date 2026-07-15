// Roundtrip coverage for `baml_sdk.unions` — union normalization variants.
//
// Port of python_pydantic2/type_shapes/customizable/roundtrip_tests/
// test_unions.py — same test names, cases, inputs, assertions.
//
// java-port note: per the conventions doc, a BAML union renders in Java as
// a generated sealed interface + one record per arm (Python instead leaves
// the value bare — "metadata dropped" — since `typing.Union` needs no
// runtime wrapper). This is the canonical file for the naming scheme this
// port invents (not pinned down by the doc beyond "shared cross-language
// decision, legacy Go `UnionNKaOrKb…` precedent"): an ad-hoc union
// `A | B | ...` gets a synthesized sealed interface `UnionAOrBOr...` (arms
// PascalCased: `int`->`Int`, `string`->`String`, `bool`->`Boolean`, a class
// `T` stays `T`), with nested records `<Arm>Value(<ArmType> value)`. A
// null-bearing union collapses to `@Nullable UnionAOrB...` per the doc
// (not an extra union arm). A union that dedups/collapses to one member
// (`int | int` -> `int`) renders as the bare underlying type, no wrapper —
// same as Python's "no `typing.Union[int]`" behavior. Where Python asserts
// directly on the raw unwrapped value, this port constructs/reads through
// the record arm instead.
package roundtrip_tests;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertNull;

import baml_sdk.unions.Fns;
import baml_sdk.unions.T;
import baml_sdk.unions.UnionContainer;
import baml_sdk.unions.UnionIntOrString;
import baml_sdk.unions.UnionTOrString;
import org.junit.jupiter.api.Test;

class TestUnions {

    @Test
    void test_round_trip_null_to_end() {
        assertEquals(
                new UnionIntOrString.IntValue(1L),
                Fns.round_trip_null_to_end(new UnionIntOrString.IntValue(1L)));
        assertEquals(
                new UnionIntOrString.StringValue("s"),
                Fns.round_trip_null_to_end(new UnionIntOrString.StringValue("s")));
        assertNull(Fns.round_trip_null_to_end(null));
    }

    @Test
    void test_round_trip_dedup() {
        assertEquals(
                new UnionIntOrString.IntValue(2L),
                Fns.round_trip_dedup(new UnionIntOrString.IntValue(2L)));
        assertEquals(
                new UnionIntOrString.StringValue("x"),
                Fns.round_trip_dedup(new UnionIntOrString.StringValue("x")));
    }

    @Test
    void test_round_trip_singleton_unwrap() {
        // `int | int` collapses to plain `int` after dedup — no union
        // wrapper.
        assertEquals(7L, Fns.round_trip_singleton_unwrap(7L));
    }

    @Test
    void test_round_trip_optional_plus_null() {
        assertEquals(
                new UnionTOrString.TValue(new T(1L)),
                Fns.round_trip_optional_plus_null(new UnionTOrString.TValue(new T(1L))));
        assertEquals(
                new UnionTOrString.StringValue("s"),
                Fns.round_trip_optional_plus_null(new UnionTOrString.StringValue("s")));
        assertNull(Fns.round_trip_optional_plus_null(null));
    }

    @Test
    void test_round_trip_t() {
        assertEquals(new T(4L), Fns.round_trip_t(new T(4L)));
    }

    @Test
    void test_round_trip_union_container() {
        UnionContainer c =
                new UnionContainer(
                        null,
                        new UnionIntOrString.StringValue("d"),
                        5L,
                        new UnionTOrString.TValue(new T(2L)));
        assertEquals(c, Fns.round_trip_union_container(c));
    }
}
