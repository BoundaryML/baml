// Generic *function-call* coverage (ns_generic_tests) — the EXPLICIT-binding
// variant.
//
// Port of python_pydantic2/function_calls/customizable/test_generic_calls.py.
//
// ===========================================================================
// INVENTED API SHAPE (flagged prominently in the report — Java has no `_types=`
// kwarg or `fn[T]` subscript, and the state-of-completeness doc leaves the
// explicit-type-args surface "TBD"). This port proposes ONE minimal, consistent
// shape and uses it everywhere:
//
//   * `baml_bridge.BamlType` — a type token:
//       BamlType.INT / STRING / BOOL / FLOAT      (primitive tokens)
//       BamlType.of(StringIntPair.class)          (user class / enum)
//       BamlType.of(GenericBox.class, BamlType.INT) (reified generic)
//   * `baml_bridge.BamlTypes` — a NAMED type-arg binding set (the wire form the
//     engine actually uses; also covers partial binding, replacing Python's
//     `_types=`):
//       BamlTypes.of("T", BamlType.INT)
//       BamlTypes.of("A", BamlType.INT).and("B", BamlType.STRING)
//   * Explicit generic CALLS take a trailing `BamlTypes` argument on a dedicated
//     overload: `Fns.identity(5L, BamlTypes.of("T", BamlType.INT))`,
//     `b.pair_with("x", BamlTypes.of("U", BamlType.STRING))`,
//     `GenericBox.new$(5L, BamlTypes.of("V", BamlType.INT))`.
//   * Generic VALUE classes:
//       - unbound instance (no reified type args): plain ctor `new GenericBox<>(5L)`;
//       - bound/reified instance: static factory taking the reified type
//         token(s) positionally, then the fields:
//         `GenericBox.of(BamlType.INT, 5L)`,
//         `GenericPair.of(BamlType.INT, BamlType.STRING, 1L, "hi")`;
//       - `instance.bamlTypeArgs()` -> `List<BamlType>` reads the reified type
//         args back (the analog of Python's
//         `__pydantic_generic_metadata__["args"]`).
//   * BAML reserved-word method `new` escapes to `new$` (the doc's `$` escaping
//     precedent; also flagged).
//   * Engine type-mismatch rejections surface as native `IllegalArgumentException`
//     (state doc: `baml.errors.TypeMismatch` -> `IllegalArgumentException`,
//     mirroring Python's `TypeError`).
// ===========================================================================

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertInstanceOf;
import static org.junit.jupiter.api.Assertions.assertThrows;
import static org.junit.jupiter.api.Assertions.assertTrue;

import baml_bridge.BamlType;
import baml_bridge.BamlTypes;
import baml_sdk.generic_tests.ContainerShapes;
import baml_sdk.generic_tests.Fns;
import baml_sdk.generic_tests.GenericBox;
import baml_sdk.generic_tests.GenericPair;
import baml_sdk.generic_tests.GenericRecursive;
import baml_sdk.generic_tests.GenericTriple;
import baml_sdk.generic_tests.NamedStatic;
import baml_sdk.generic_tests.StringIntPair;
import java.util.List;
import java.util.Map;
import org.junit.jupiter.api.Test;

class TestGenericCalls {

    // =======================================================================
    // basic cases, free functions
    // =======================================================================

    @Test
    void test_generic_calls_identity_explicit() {
        assertEquals(5L, Fns.identity(5L, BamlTypes.of("T", BamlType.INT)));
        assertEquals("hi", Fns.identity("hi", BamlTypes.of("T", BamlType.STRING)));

        StringIntPair pair = new StringIntPair("a", 1L);
        assertEquals(pair, Fns.identity(pair, BamlTypes.of("T", BamlType.of(StringIntPair.class))));

        GenericBox<GenericBox<String>> box =
                GenericBox.of(
                        BamlType.of(GenericBox.class, BamlType.STRING),
                        GenericBox.of(BamlType.STRING, "hello"));
        assertEquals(
                box,
                Fns.identity(
                        box,
                        BamlTypes.of(
                                "T",
                                BamlType.of(
                                        GenericBox.class,
                                        BamlType.of(GenericBox.class, BamlType.STRING)))));

        GenericTriple<GenericBox<String>, Double, Boolean> triple =
                GenericTriple.of(
                        BamlType.of(GenericBox.class, BamlType.STRING),
                        BamlType.FLOAT,
                        BamlType.BOOL,
                        GenericBox.of(BamlType.STRING, "hello"),
                        List.of(1.1, 2.2),
                        Map.of("lorem", true, "ipsum", false));
        assertEquals(
                triple,
                Fns.identity(
                        triple,
                        BamlTypes.of(
                                "T",
                                BamlType.of(
                                        GenericTriple.class,
                                        BamlType.of(GenericBox.class, BamlType.STRING),
                                        BamlType.FLOAT,
                                        BamlType.BOOL))));
    }

    @Test
    void test_generic_calls_identity_async_explicit() {
        assertEquals(7L, Fns.identity_async(7L, BamlTypes.of("T", BamlType.INT)).join());
    }

    @Test
    void test_generic_calls_tag_or_value_explicit() {
        // `tag_or_value` reflects its bound `T` back as a string; `x` must
        // inhabit the substituted `T | string | null` (a union arg -> `Object`).
        assertEquals("int", Fns.tag_or_value(5L, BamlTypes.of("T", BamlType.INT)));
        assertEquals("string", Fns.tag_or_value("plain", BamlTypes.of("T", BamlType.STRING)));
        StringIntPair pair = new StringIntPair("b", 2L);
        assertTrue(
                Fns.tag_or_value(pair, BamlTypes.of("T", BamlType.of(StringIntPair.class)))
                        .contains("StringIntPair"));
    }

    @Test
    void test_generic_calls_make_triple_explicit() {
        // A=int, B=str, C=bool, bound positionally by name.
        var t =
                Fns.make_triple(
                        1L,
                        List.of("a", "b"),
                        Map.of("k", true),
                        BamlTypes.of("A", BamlType.INT)
                                .and("B", BamlType.STRING)
                                .and("C", BamlType.BOOL));
        assertInstanceOf(GenericTriple.class, t);
        assertEquals(1L, t.first());
        assertEquals(List.of("a", "b"), t.second());
        assertEquals(Map.of("k", true), t.third());
    }

    @Test
    void test_generic_calls_one_type_arg_explicit() {
        assertEquals("int", Fns.one_type_arg(BamlTypes.of("T", BamlType.INT)));
        assertEquals("string", Fns.one_type_arg(BamlTypes.of("T", BamlType.STRING)));
        // Nested generic binding must encode fully (base class + concrete arg).
        String nested =
                Fns.one_type_arg(BamlTypes.of("T", BamlType.of(GenericBox.class, BamlType.INT)));
        assertTrue(nested.contains("GenericBox") && nested.contains("int"));
    }

    @Test
    void test_generic_calls_two_type_args_explicit() {
        assertEquals(
                "int | string",
                Fns.two_type_args(BamlTypes.of("A", BamlType.INT).and("B", BamlType.STRING)));
    }

    @Test
    void test_generic_calls_generic_free_fn_requires_binding() {
        // `one_type_arg<T>()` / `two_type_args<A,B>()` are return/body-only: NO
        // argument carries the TypeVar, so a bare call finds no evidence and the
        // engine (Gate A) rejects it. `baml.errors.TypeMismatch` surfaces as a
        // native `IllegalArgumentException` (Python's `TypeError`).
        IllegalArgumentException excOne =
                assertThrows(IllegalArgumentException.class, () -> Fns.one_type_arg());
        assertTrue(excOne.getMessage().contains("could not infer a type"));
        assertTrue(excOne.getMessage().contains("one_type_arg"));
        IllegalArgumentException excTwo =
                assertThrows(IllegalArgumentException.class, () -> Fns.two_type_args());
        assertTrue(excTwo.getMessage().contains("could not infer a type"));
    }

    @Test
    void test_generic_calls_subscript_wrong_arity_raises() {
        // java-port note: Python's subscript form requires FULL arity and raises
        // a host-side message "expected 2 type argument(s) for ['A', 'B'], got 1".
        // Java has a single `BamlTypes` surface that (like Python's `_types=`)
        // permits partial binding, so there is no separate subscript arity check.
        // Binding only "A" for `two_type_args<A,B>` leaves body-only `B` with no
        // evidence, so the engine rejects it (could-not-infer) — the meaningful
        // Java analog of the under-specified call.
        assertThrows(
                IllegalArgumentException.class,
                () -> Fns.two_type_args(BamlTypes.of("A", BamlType.INT)));
    }

    // =======================================================================
    // basic cases, generic classes
    // =======================================================================

    @Test
    void test_generic_calls_consume_int_wrapper_baseline() {
        // A concretely-instantiated `GenericBox<int>` flows in; the `int` field
        // flows back out. Anchors the suite.
        assertEquals(9L, Fns.consume_int_wrapper(GenericBox.of(BamlType.INT, 9L)));
    }

    @Test
    void test_generic_calls_genericbox_get_explicit() {
        // `GenericBox.of(BamlType.INT, ...)` carries the type arg; the host
        // recovers it from the receiver as the method frame's class-level `T`.
        GenericBox<Long> b = GenericBox.of(BamlType.INT, 5L);
        assertEquals("int", b.get());
    }

    @Test
    void test_generic_calls_genericbox_pair_with_explicit() {
        // `T` from the reified receiver, `U` from the method's explicit binding.
        GenericBox<Long> b = GenericBox.of(BamlType.INT, 5L);
        assertEquals("int | string", b.pair_with("hello world", BamlTypes.of("U", BamlType.STRING)));
    }

    @Test
    void test_generic_calls_genericbox_new_static_explicit() {
        // Generic STATIC method (`new` -> `new$`); only the static's own `V` is
        // bound, no class type args ride along.
        GenericBox<Long> box = GenericBox.new$(5L, BamlTypes.of("V", BamlType.INT));
        assertInstanceOf(GenericBox.class, box);
        assertEquals(5L, box.value());
    }

    @Test
    void test_generic_calls_generic_static_infers_binding() {
        // A generic static's own `V` appears in a parameter, so a bare call
        // INFERS it from the value — no explicit binding needed.
        GenericBox<Long> box = GenericBox.new$(5L);
        assertInstanceOf(GenericBox.class, box);
        assertEquals(5L, box.value());
    }

    @Test
    void test_generic_calls_named_static_distinct_typevar_names() {
        // Static TypeVar names differ from the class's (`make<D,E>`); each binds
        // by NAME into the static frame's own params.
        assertEquals(
                "int | string",
                NamedStatic.make(1L, "x", BamlTypes.of("D", BamlType.INT).and("E", BamlType.STRING)));
    }

    @Test
    void test_generic_calls_instance_method_unparameterized_receiver_raises() {
        // java-port note: `new GenericBox<>(5L)` is UNBOUND (no reified type
        // args), so `pair_with`'s class `T` can't be recovered host-side. Python
        // raises a pydantic-specific message; the Java analog is a host-side
        // `IllegalArgumentException` about the receiver needing to be reified.
        GenericBox<Long> unbound = new GenericBox<>(5L);
        IllegalArgumentException exc =
                assertThrows(
                        IllegalArgumentException.class,
                        () -> unbound.pair_with("x", BamlTypes.of("U", BamlType.STRING)));
        assertTrue(exc.getMessage().contains("receiver"));
    }

    @Test
    void test_generic_calls_extract_explicit() {
        // Nested generic; body is `reflect.type_of<A|B|C|D>()`.
        GenericPair<GenericPair<Long, String>, GenericPair<Boolean, Double>> pair =
                GenericPair.of(
                        BamlType.of(GenericPair.class, BamlType.INT, BamlType.STRING),
                        BamlType.of(GenericPair.class, BamlType.BOOL, BamlType.FLOAT),
                        GenericPair.of(BamlType.INT, BamlType.STRING, 1L, "a"),
                        GenericPair.of(BamlType.BOOL, BamlType.FLOAT, true, 1.5));
        assertEquals(
                "int | string | bool | float",
                Fns.extract(
                        pair,
                        BamlTypes.of("A", BamlType.INT)
                                .and("B", BamlType.STRING)
                                .and("C", BamlType.BOOL)
                                .and("D", BamlType.FLOAT)));
    }

    // =======================================================================
    // basic case, `T` in return position only
    // =======================================================================

    @Test
    void test_generic_calls_parse_as_explicit() {
        // `T` bound by the host explicitly. Return-position-only, so the call
        // site needs a target/witness type for the erased `<T>`.
        StringIntPair pair =
                Fns.parse_as(
                        "{\"my_string\": \"x\", \"my_int\": 3}",
                        BamlTypes.of("T", BamlType.of(StringIntPair.class)));
        assertEquals(new StringIntPair("x", 3L), pair);
        assertEquals(42L, Fns.<Long>parse_as("42", BamlTypes.of("T", BamlType.INT)));
    }

    // =======================================================================
    // complex cases
    // =======================================================================

    @Test
    void test_generic_calls_second_of_explicit() {
        assertEquals(
                "hi",
                Fns.second_of(
                        GenericPair.of(BamlType.INT, BamlType.STRING, 1L, "hi"),
                        BamlTypes.of("T", BamlType.STRING)));
        StringIntPair pair = new StringIntPair("z", 9L);
        GenericPair<Long, StringIntPair> p =
                GenericPair.of(BamlType.INT, BamlType.of(StringIntPair.class), 0L, pair);
        assertEquals(pair, Fns.second_of(p, BamlTypes.of("T", BamlType.of(StringIntPair.class))));
    }

    @Test
    void test_generic_calls_list_head_explicit() {
        GenericRecursive<Long> linkedList =
                GenericRecursive.of(
                        BamlType.INT, 7L, GenericRecursive.of(BamlType.INT, 8L, null));
        assertEquals(7L, Fns.list_head(linkedList, BamlTypes.of("T", BamlType.INT)));
    }

    @Test
    void test_generic_calls_choose_explicit() {
        assertEquals(1L, Fns.choose(1L, 2L, BamlTypes.of("T", BamlType.INT)));
        assertEquals("a", Fns.choose("a", "b", BamlTypes.of("T", BamlType.STRING)));
    }

    @Test
    void test_generic_calls_read_items_explicit() {
        ContainerShapes<Long> container =
                ContainerShapes.of(BamlType.INT, 1L, List.of(1L, 2L, 3L), Map.of("k", 4L), null, null);
        assertEquals(List.of(1L, 2L, 3L), Fns.read_items(container, BamlTypes.of("T", BamlType.INT)));
    }

    // =======================================================================
    // outbound generics
    // =======================================================================

    @Test
    void test_generic_calls_wrap_explicit() {
        GenericBox<Long> w = Fns.wrap(5L, BamlTypes.of("T", BamlType.INT));
        assertInstanceOf(GenericBox.class, w);
        assertEquals(5L, w.value());
    }

    // =======================================================================
    // reified generics returned by NON-generic functions
    // =======================================================================

    @Test
    void test_generic_calls_make_int_box_reified() {
        GenericBox<Long> box = Fns.make_int_box();
        assertInstanceOf(GenericBox.class, box);
        assertEquals(List.of(BamlType.INT), box.bamlTypeArgs());
        assertEquals(7L, box.value());
    }

    @Test
    void test_generic_calls_make_int_container_reified() {
        ContainerShapes<Long> c = Fns.make_int_container();
        assertInstanceOf(ContainerShapes.class, c);
        assertEquals(List.of(BamlType.INT), c.bamlTypeArgs());
        assertEquals(1L, c.item());
        assertEquals(List.of(1L, 2L, 3L), c.items());
        assertEquals(Map.of("k", 4L), c.by_key());
        org.junit.jupiter.api.Assertions.assertNull(c.maybe());
        assertEquals(5L, c.mixed());
    }

    @Test
    void test_generic_calls_make_nested_box_reified() {
        GenericBox<GenericBox<Long>> outer = Fns.make_nested_box();
        assertInstanceOf(GenericBox.class, outer);
        // The outer box's single type arg is itself the reified `GenericBox<int>`.
        assertEquals(List.of(BamlType.of(GenericBox.class, BamlType.INT)), outer.bamlTypeArgs());
        GenericBox<Long> inner = outer.value();
        assertInstanceOf(GenericBox.class, inner);
        assertEquals(List.of(BamlType.INT), inner.bamlTypeArgs());
        assertEquals(9L, inner.value());
    }

    @Test
    void test_generic_calls_make_int_str_bool_triple_reified() {
        GenericTriple<Long, String, Boolean> t = Fns.make_int_str_bool_triple();
        assertInstanceOf(GenericTriple.class, t);
        assertEquals(List.of(BamlType.INT, BamlType.STRING, BamlType.BOOL), t.bamlTypeArgs());
        assertEquals(1L, t.first());
        assertEquals(List.of("a", "b"), t.second());
        assertEquals(Map.of("k", true), t.third());
    }
}
