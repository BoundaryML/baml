// Roundtrip coverage for `baml_sdk.unions` — union normalization variants.
//
// Port of python_pydantic2/type_shapes/customizable/roundtrip_tests/
// test_unions.py — same test names, cases, inputs, assertions.
//
// java-port note: per the conventions doc (Unions, TEAM DECISION
// 2026-07-16), an anonymous BAML union `A | B | ...` renders in Java as
// the runtime generic arity family `baml_bridge.Union2<A, B>` …
// `Union10<...>` — a sealed interface with one nested positional record
// `Arm0..Arm{n-1}` per arm in BAML declaration order, each holding its
// payload under `.value()`. (Python instead leaves the value bare —
// "metadata dropped" — since `typing.Union` needs no runtime wrapper.)
// Normalization runs before arm assignment: the `null` arm is stripped
// (a null-bearing union collapses to a `@Nullable UnionN<...>`, not an
// extra arm), structurally-equal arms dedup, and a union that collapses
// to a single member (`int | int` -> `int`) renders as the bare
// underlying type with no wrapper — same as Python's "no
// `typing.Union[int]`" behavior. Type args are always boxed (`Long`,
// `Double`, `Boolean`). Where Python asserts directly on the raw
// unwrapped value, this port constructs/reads through the arm record
// instead. Arm ordering here: `int | null | string` -> [int, string] ->
// `Union2<Long, String>` (int = Arm0, string = Arm1); `T? | string` ->
// [T, string] -> `Union2<T, String>` (T = Arm0, string = Arm1).
package roundtrip_tests;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertNull;

import baml_bridge.Union2;
import baml_sdk.unions.Fns;
import baml_sdk.unions.T;
import baml_sdk.unions.UnionContainer;
import java.util.List;
import org.junit.jupiter.api.Test;

class TestUnions {

    @Test
    void test_unions_round_trip_null_to_end() {
        assertEquals(
                new Union2.Arm0<Long, String>(1L),
                Fns.round_trip_null_to_end(new Union2.Arm0<Long, String>(1L)));
        assertEquals(
                new Union2.Arm1<Long, String>("s"),
                Fns.round_trip_null_to_end(new Union2.Arm1<Long, String>("s")));
        assertNull(Fns.round_trip_null_to_end(null));
    }

    @Test
    void test_unions_round_trip_dedup() {
        assertEquals(
                new Union2.Arm0<Long, String>(2L),
                Fns.round_trip_dedup(new Union2.Arm0<Long, String>(2L)));
        assertEquals(
                new Union2.Arm1<Long, String>("x"),
                Fns.round_trip_dedup(new Union2.Arm1<Long, String>("x")));
    }

    @Test
    void test_unions_round_trip_singleton_unwrap() {
        // `int | int` collapses to plain `int` after dedup — no union
        // wrapper.
        assertEquals(7L, Fns.round_trip_singleton_unwrap(7L));
    }

    @Test
    void test_unions_round_trip_optional_plus_null() {
        assertEquals(
                new Union2.Arm0<T, String>(new T(1L)),
                Fns.round_trip_optional_plus_null(new Union2.Arm0<T, String>(new T(1L))));
        assertEquals(
                new Union2.Arm1<T, String>("s"),
                Fns.round_trip_optional_plus_null(new Union2.Arm1<T, String>("s")));
        assertNull(Fns.round_trip_optional_plus_null(null));
    }

    @Test
    void test_unions_round_trip_str_or_int_list() {
        // Differently-typed list arms: the generated arm wrapper selects the
        // contextual list type. The encoder places that exact type on the list
        // node, so neither non-empty nor empty values depend on element inference.
        assertEquals(
                new Union2.Arm1<List<String>, List<Long>>(List.of(1L, 2L, 3L)),
                Fns.round_trip_str_or_int_list(
                        new Union2.Arm1<List<String>, List<Long>>(List.of(1L, 2L, 3L))));
        assertEquals(
                new Union2.Arm0<List<String>, List<Long>>(List.of("a", "b")),
                Fns.round_trip_str_or_int_list(
                        new Union2.Arm0<List<String>, List<Long>>(List.of("a", "b"))));
        // Empty containers are the regression case: payload shape alone cannot
        // distinguish these arms, so each must retain its host-selected identity.
        assertEquals(
                new Union2.Arm0<List<String>, List<Long>>(List.of()),
                Fns.round_trip_str_or_int_list(
                        new Union2.Arm0<List<String>, List<Long>>(List.of())));
        assertEquals(
                new Union2.Arm1<List<String>, List<Long>>(List.of()),
                Fns.round_trip_str_or_int_list(
                        new Union2.Arm1<List<String>, List<Long>>(List.of())));
    }

    @Test
    void test_unions_round_trip_t() {
        assertEquals(new T(4L), Fns.round_trip_t(new T(4L)));
    }

    @Test
    void test_unions_round_trip_union_container() {
        UnionContainer c =
                new UnionContainer(
                        null,
                        new Union2.Arm1<Long, String>("d"),
                        5L,
                        new Union2.Arm0<T, String>(new T(2L)));
        assertEquals(c, Fns.round_trip_union_container(c));
    }
}
