// Generic *function-call* coverage (ns_generic_tests) — the INFERENCE variant.
//
// Port of python_pydantic2/function_calls/customizable/test_generic_inference.py.
//
// Sibling of TestGenericCalls (the explicit-binding suite). Here every call is
// BARE: no trailing `BamlTypes`. The engine solves each TypeVar from the
// argument *values*. Java's own generic inference binds the return type from
// the value arguments in lockstep (`<T> T identity(T x)` etc.).
//
// See TestGenericCalls' header for the full INVENTED explicit-generics surface
// (BamlType / BamlTypes / reified factories / `new$`); this file reuses it only
// for the partial/explicit-seed cases (§D, §E, §C C4) and for constructing
// reified argument instances.
//
// Engine type-mismatch rejections surface as native `IllegalArgumentException`
// (state doc: `baml.errors.TypeMismatch` -> `IllegalArgumentException`,
// mirroring Python's `TypeError`); `assertTypeError` is the analog of the
// Python `_assert_type_error` helper.

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertInstanceOf;
import static org.junit.jupiter.api.Assertions.assertNull;
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
import baml_sdk.generic_tests.SomeEnum;
import baml_sdk.generic_tests.StringIntPair;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;
import org.junit.jupiter.api.Disabled;
import org.junit.jupiter.api.Test;
import org.junit.jupiter.api.function.Executable;

class TestGenericInference {

    private static void assertTypeError(Executable call, String... needles) {
        IllegalArgumentException ex = assertThrows(IllegalArgumentException.class, call);
        String message = String.valueOf(ex.getMessage());
        for (String needle : needles) {
            assertTrue(message.contains(needle), "missing '" + needle + "' in: " + message);
        }
    }

    // =======================================================================
    // §A — single TypeVar inferred from one argument value
    // =======================================================================

    @Test
    void test_generic_inference_identity_infers_primitives() {
        assertEquals(5L, Fns.identity(5L));
        assertEquals("hi", Fns.identity("hi"));
        assertTrue(Fns.identity(true));
    }

    @Test
    void test_generic_inference_identity_infers_user_class() {
        StringIntPair pair = new StringIntPair("a", 1L);
        assertEquals(pair, Fns.identity(pair));
    }

    @Test
    void test_generic_inference_identity_infers_generic_instance() {
        GenericBox<Long> box = GenericBox.of(BamlType.INT, 5L);
        assertEquals(box, Fns.identity(box));

        GenericBox<GenericBox<String>> nested =
                GenericBox.of(
                        BamlType.of(GenericBox.class, BamlType.STRING),
                        GenericBox.of(BamlType.STRING, "hello"));
        assertEquals(nested, Fns.identity(nested));
    }

    @Test
    void test_generic_inference_identity_async_infers() {
        assertEquals(7L, Fns.identity_async(7L).join());
    }

    @Test
    void test_generic_inference_identity_null_round_trips() {
        // §I I4: a `null` actual is no inference evidence (not bound as `T=null`);
        // `T` defaults to host-only `rust_type` and the value round-trips.
        assertNull(Fns.identity(null));
    }

    @Test
    void test_generic_inference_identity_unbound_generic_instance_round_trips() {
        // §G G2: an UNBOUND generic instance carries no wire type-args, so it is
        // host-only and rides through opaquely, round-tripping unchanged.
        GenericBox<Long> unbound = new GenericBox<>(5L);
        assertEquals(unbound, Fns.identity(unbound));
    }

    // =======================================================================
    // §B — structural / container solving across one or more arguments
    // =======================================================================

    @Test
    void test_generic_inference_make_triple_infers_multiple_typevars() {
        var t = Fns.make_triple(1L, List.of("a", "b"), Map.of("k", true));
        assertInstanceOf(GenericTriple.class, t);
        assertEquals(1L, t.first());
        assertEquals(List.of("a", "b"), t.second());
        assertEquals(Map.of("k", true), t.third());
    }

    @Test
    void test_generic_inference_second_of_infers_from_nested_generic() {
        assertEquals("hi", Fns.second_of(GenericPair.of(BamlType.INT, BamlType.STRING, 1L, "hi")));
        StringIntPair pair = new StringIntPair("z", 9L);
        GenericPair<Long, StringIntPair> p =
                GenericPair.of(BamlType.INT, BamlType.of(StringIntPair.class), 0L, pair);
        assertEquals(pair, Fns.second_of(p));
    }

    @Test
    void test_generic_inference_read_items_infers_from_instance_wire_args() {
        ContainerShapes<Long> container =
                ContainerShapes.of(BamlType.INT, 1L, List.of(1L, 2L, 3L), Map.of("k", 4L), null, null);
        assertEquals(List.of(1L, 2L, 3L), Fns.read_items(container));

        ContainerShapes<Long> emptyFields =
                ContainerShapes.of(BamlType.INT, 1L, List.of(), Map.of(), null, null);
        assertEquals(List.of(), Fns.read_items(emptyFields));
    }

    @Test
    void test_generic_inference_list_head_infers_from_recursive_generic() {
        GenericRecursive<Long> linked =
                GenericRecursive.of(BamlType.INT, 7L, GenericRecursive.of(BamlType.INT, 8L, null));
        assertEquals(7L, Fns.list_head(linked));
    }

    @Test
    void test_generic_inference_extract_infers_four_typevars_from_nesting() {
        GenericPair<GenericPair<Long, String>, GenericPair<Boolean, Double>> pair =
                GenericPair.of(
                        BamlType.of(GenericPair.class, BamlType.INT, BamlType.STRING),
                        BamlType.of(GenericPair.class, BamlType.BOOL, BamlType.FLOAT),
                        GenericPair.of(BamlType.INT, BamlType.STRING, 1L, "a"),
                        GenericPair.of(BamlType.BOOL, BamlType.FLOAT, true, 1.5));
        assertEquals("int | string | bool | float", Fns.extract(pair));
    }

    // =======================================================================
    // §C — union unification: one TypeVar across two argument positions
    // =======================================================================

    @Test
    void test_generic_inference_choose_infers_unified_typevar() {
        assertEquals(5L, Fns.choose(5L, 6L));
        assertEquals("a", Fns.choose("a", "b"));
    }

    @Test
    void test_generic_inference_choose_infers_divergent_union() {
        // choose(5, "asdf") => T = int | string; body returns `left` = 5.
        assertEquals(5L, Fns.choose(5L, "asdf"));
    }

    // =======================================================================
    // §D — partial binding: explicit seed for one TypeVar, infer the rest
    // =======================================================================

    @Test
    void test_generic_inference_make_triple_partial_explicit_then_infer() {
        // Bind A explicitly via a partial `BamlTypes`; B and C are inferred.
        var t =
                Fns.make_triple(
                        1L, List.of("x", "y"), Map.of("k", true), BamlTypes.of("A", BamlType.INT));
        assertInstanceOf(GenericTriple.class, t);
        assertEquals(1L, t.first());
        assertEquals(List.of("x", "y"), t.second());
        assertEquals(Map.of("k", true), t.third());
    }

    // =======================================================================
    // §G/outbound — infer T, return a generic over it
    // =======================================================================

    @Test
    void test_generic_inference_wrap_infers_and_returns_generic() {
        GenericBox<Long> w = Fns.wrap(5L);
        assertInstanceOf(GenericBox.class, w);
        assertEquals(5L, w.value());
    }

    // =======================================================================
    // §K — methods: class T from the receiver, method vars inferred from args
    // =======================================================================

    @Test
    void test_generic_inference_genericbox_pair_with_infers_method_typevar() {
        GenericBox<Long> b = GenericBox.of(BamlType.INT, 5L);
        assertEquals("int | string", b.pair_with("hello world"));
    }

    @Test
    void test_generic_inference_generic_static_infers_own_typevar() {
        GenericBox<Long> box = GenericBox.new$(5L);
        assertInstanceOf(GenericBox.class, box);
        assertEquals(5L, box.value());
    }

    @Test
    void test_generic_inference_named_static_infers_distinct_typevars() {
        assertEquals("int | string", NamedStatic.make(1L, "x"));
    }

    // =======================================================================
    // Out-of-scope / must-specify and union sibling routing
    // =======================================================================

    @Test
    void test_generic_inference_union_with_concrete_sibling_infers_typevar() {
        // A TypeVar beside concrete union members (`x: T | string | null`) is
        // solved by inference: the int actual routes to `T` => T=int.
        assertEquals("int", Fns.tag_or_value(5L));
    }

    @Test
    void test_generic_inference_union_concrete_sibling_absorbs_value_binds_rust_type() {
        // §H H3: a `string` actual is absorbed by the concrete `string` sibling,
        // so nothing routes to `T` => it defaults to host-only `rust_type`.
        assertEquals("$rust_type", Fns.tag_or_value("hi"));
    }

    @Test
    void test_generic_inference_union_null_actual_binds_rust_type() {
        // §H H3 / §I I4: a `null` actual is no evidence, and the `null` sibling
        // absorbs it, so `T` defaults to `rust_type`.
        assertEquals("$rust_type", Fns.tag_or_value(null));
    }

    @Test
    void test_generic_inference_return_only_var_still_requires_binding() {
        // §E: parse_as<T>(source) -> T is return-position-only; inference finds
        // nothing and the engine rejects.
        assertTypeError(() -> Fns.parse_as("42"), "could not infer a type", "parse_as");
    }

    @Test
    void test_generic_inference_body_only_var_still_requires_binding() {
        // §E: one_type_arg<T>() reflects T but takes no argument => uninferable.
        assertTypeError(() -> Fns.one_type_arg(), "could not infer a type", "one_type_arg");
    }

    // =======================================================================
    // §J — variance soundness: conflicting occurrences reject; agreeing bind
    // =======================================================================

    @Test
    void test_generic_inference_pair_invariant_list_conflict_rejects() {
        assertTypeError(
                () -> Fns.pair(List.of(1L, 2L), List.of("a", "b")),
                "can't be reconciled",
                "pair",
                "int",
                "string");
    }

    @Test
    void test_generic_inference_pair_invariant_list_agree_binds() {
        assertEquals("int", Fns.pair(List.of(1L, 2L), List.of(3L, 4L)));
    }

    @Test
    void test_generic_inference_choose_union_outside_container_is_sound() {
        // Both occurrences are covariant (bare `T`), so the union forms OUTSIDE
        // the container (T = int[] | string[]) and the call SUCCEEDS.
        assertEquals(List.of(1L, 2L), Fns.choose(List.of(1L, 2L), List.of("a")));
    }

    @Test
    void test_generic_inference_merge_invariant_map_value_conflict_rejects() {
        assertTypeError(
                () -> Fns.merge(Map.of("k", 1L), Map.of("k", "a")), "can't be reconciled", "merge");
    }

    @Test
    void test_generic_inference_combine_invariant_class_arg_conflict_rejects() {
        assertTypeError(
                () -> Fns.combine(GenericBox.of(BamlType.INT, 1L), GenericBox.of(BamlType.STRING, "x")),
                "can't be reconciled",
                "combine");
    }

    @Test
    void test_generic_inference_glue_invariant_vs_covariant_conflict_rejects() {
        assertTypeError(() -> Fns.glue(1L, List.of("a")), "can't be reconciled", "glue");
    }

    @Test
    void test_generic_inference_glue_invariant_and_covariant_agree_binds() {
        assertEquals("int", Fns.glue(1L, List.of(2L, 3L)));
    }

    @Test
    void test_generic_inference_two_typevar_union_is_uninferrable_rejects() {
        assertTypeError(() -> Fns.two_in_union("hello"), "two_in_union");
    }

    // =======================================================================
    // §D — n-ary covariant join, and §B heterogeneous container element
    // =======================================================================

    @Test
    void test_generic_inference_triple_choose_three_covariant_join() {
        assertEquals("int | string | bool", Fns.triple_choose(5L, "asdf", true));
    }

    @Test
    void test_generic_inference_make_triple_heterogeneous_list_element_unions() {
        var t = Fns.make_triple(1L, List.<Object>of(1L, "x"), Map.of("k", true));
        assertInstanceOf(GenericTriple.class, t);
        assertEquals(List.<Object>of(1L, "x"), t.second());
    }

    @Test
    void test_generic_inference_choose_divergent_generic_instances_union() {
        GenericBox<Long> left = GenericBox.of(BamlType.INT, 1L);
        assertEquals(left, Fns.choose(left, GenericBox.of(BamlType.STRING, "x")));
    }

    @Test
    void test_generic_inference_tag_or_value_binds_generic_instance() {
        String rendered = Fns.tag_or_value(GenericBox.of(BamlType.STRING, "asdf"));
        assertTrue(rendered.contains("GenericBox") && rendered.contains("string"));
    }

    // =======================================================================
    // §B — empty collections on a FREE function (low-evidence => rust_type)
    // =======================================================================

    @Test
    void test_generic_inference_first_or_empty_list_round_trips_none() {
        assertNull(Fns.first_or(List.of()));
    }

    @Test
    void test_generic_inference_first_or_nonempty_infers_element() {
        assertEquals(7L, Fns.first_or(List.of(7L, 8L, 9L)));
    }

    @Test
    void test_generic_inference_values_of_empty_map_round_trips_empty_list() {
        assertEquals(List.of(), Fns.values_of(Map.of()));
    }

    @Test
    void test_generic_inference_values_of_nonempty_returns_values() {
        // LinkedHashMap preserves insertion order so values() is deterministic.
        Map<String, Long> m = new LinkedHashMap<>();
        m.put("a", 1L);
        m.put("b", 2L);
        assertEquals(List.of(1L, 2L), Fns.values_of(m));
    }

    // =======================================================================
    // §C — caller-specified & partial binding via the explicit surface
    // =======================================================================

    @Disabled(
            "java-port note: Python's subscript form requires FULL arity (a partial "
                    + "`make_triple[int]` raises host-side before the call). Java has a "
                    + "single `BamlTypes` surface that (like Python's `_types=`) permits "
                    + "partial binding, so `make_triple(.., BamlTypes.of(\"A\", INT))` "
                    + "SUCCEEDS (see test_make_triple_partial_explicit_then_infer). The "
                    + "subscript-only arity restriction has no Java analog.")
    @Test
    void test_generic_inference_make_triple_partial_subscript_requires_full_arity() {
        // Intentionally empty — see @Disabled reason.
    }

    @Test
    void test_generic_inference_make_triple_subscript_fully_bound() {
        var t =
                Fns.make_triple(
                        5L,
                        List.of("x"),
                        Map.of("k", true),
                        BamlTypes.of("A", BamlType.INT)
                                .and("B", BamlType.STRING)
                                .and("C", BamlType.BOOL));
        assertInstanceOf(GenericTriple.class, t);
        assertEquals(5L, t.first());
        assertEquals(List.of("x"), t.second());
        assertEquals(Map.of("k", true), t.third());
    }

    // =======================================================================
    // §E — must-specify (negatives above); the explicit binding succeeds.
    // =======================================================================

    @Test
    void test_generic_inference_one_type_arg_explicit_types_succeeds() {
        assertEquals("int", Fns.one_type_arg(BamlTypes.of("T", BamlType.INT)));
    }

    @Test
    void test_generic_inference_parse_as_explicit_types_succeeds() {
        assertEquals(42L, Fns.<Long>parse_as("42", BamlTypes.of("T", BamlType.INT)));
    }

    // =======================================================================
    // §G — unbound generic instances recovered when the formal forces recursion
    // =======================================================================

    @Test
    void test_generic_inference_second_of_unbound_instance_recovers_field_type() {
        // An UNBOUND GenericPair; the formal `GenericPair<int, T>` forces
        // inference into the second slot => T=string from the field VALUE.
        assertEquals("hi", Fns.second_of(new GenericPair<>(1L, "hi")));
    }

    @Test
    void test_generic_inference_identity_nested_unbound_round_trips() {
        GenericBox<GenericBox<String>> nested = new GenericBox<>(new GenericBox<>("hello"));
        assertEquals(nested, Fns.identity(nested));
    }

    @Test
    void test_generic_inference_wrap_infers_and_returns_bound_generic() {
        // `wrap(5)` infers T=int and returns a bound `GenericBox[int]`. Value
        // equality ignores parameterization, so bound == the bound literal.
        assertEquals(GenericBox.of(BamlType.INT, 5L), Fns.wrap(5L));
    }

    // =======================================================================
    // §I — nullable param, literal/enum widening edges.
    // =======================================================================

    @Test
    void test_generic_inference_maybe_id_present_value_infers() {
        assertEquals(5L, Fns.maybe_id(5L));
    }

    @Test
    void test_generic_inference_maybe_id_null_round_trips() {
        assertNull(Fns.maybe_id(null));
    }

    @Test
    void test_generic_inference_identity_enum_round_trips() {
        // An enum value rides through inference and round-trips as a real enum
        // member (T binds to the enum type SomeEnum, not `string`). The
        // instanceof check is load-bearing.
        SomeEnum result = Fns.identity(SomeEnum.VARIANT);
        assertEquals(SomeEnum.VARIANT, result);
        assertInstanceOf(SomeEnum.class, result);
    }

    // =======================================================================
    // §F — host-only object boundary: an arbitrary Java object is unencodable.
    // =======================================================================

    @Test
    void test_generic_inference_host_only_object_not_encodable_from_python() {
        // An arbitrary Java object has no wire encoding and is rejected at encode
        // time with an IllegalArgumentException BEFORE the call reaches the engine
        // (state doc: encoder throws IllegalArgumentException naming the arg).
        class HostThing {
            final long n;

            HostThing(long n) {
                this.n = n;
            }
        }

        IllegalArgumentException excinfo =
                assertThrows(IllegalArgumentException.class, () -> Fns.identity(new HostThing(3)));
        assertTrue(String.valueOf(excinfo.getMessage()).toLowerCase().contains("encode"));
    }

    // =======================================================================
    // §J J13 — a function-typed (host callable) argument poisons its TypeVars.
    // =======================================================================

    @Test
    void test_generic_inference_apply_closure_poisons_typevars_must_specify() {
        // `T` is poisoned by its occurrence in the closure parameter `(T)` and
        // `R` lives only in the closure's return, so both must be specified;
        // bare => rejected.
        assertTypeError(
                () -> Fns.apply((Long v) -> v + 1, 5L), "could not infer a type", "apply");
    }

    @Test
    void test_generic_inference_apply_closure_typevars_specified_succeeds() {
        assertEquals(
                6L,
                Fns.apply(
                        (Long v) -> v + 1,
                        5L,
                        BamlTypes.of("T", BamlType.INT).and("R", BamlType.INT)));
    }

    // =======================================================================
    // §L — methods: class T from the receiver, method vars from method args.
    // =======================================================================

    @Test
    void test_generic_inference_genericbox_get_infers_class_var_from_receiver() {
        assertEquals("int", GenericBox.of(BamlType.INT, 5L).get());
    }

    @Test
    void test_generic_inference_genericbox_pair_with_unbound_receiver_recovers_class_var() {
        // A BARE method call on an UNBOUND receiver: the method's
        // `self: GenericBox<T>` formal forces recursion into `value` => class
        // T=int recovered from `value=5`, unioned with method var U=string.
        assertEquals("int | string", new GenericBox<>(5L).pair_with("x"));
    }

    // =======================================================================
    // §C C4 — a caller-specified binding contradicted by the actual rejects.
    // =======================================================================

    @Test
    void test_generic_inference_make_triple_types_kwarg_contradicted_by_actual_rejects() {
        // Partial binding fixes A=int, but `a="nope"` is a string => the engine's
        // per-arg structural check rejects at call time.
        assertTypeError(
                () ->
                        Fns.make_triple(
                                "nope",
                                List.of("x"),
                                Map.of("k", true),
                                BamlTypes.of("A", BamlType.INT)),
                "make_triple");
    }

    @Test
    void test_generic_inference_make_triple_full_subscript_contradicted_by_actual_rejects() {
        assertTypeError(
                () ->
                        Fns.make_triple(
                                "nope",
                                List.of("x"),
                                Map.of("k", true),
                                BamlTypes.of("A", BamlType.INT)
                                        .and("B", BamlType.STRING)
                                        .and("C", BamlType.BOOL)),
                "make_triple");
    }

    // =======================================================================
    // §B/§D — heterogeneous array unification yields a union element type.
    // =======================================================================

    @Test
    void test_generic_inference_elem_type_heterogeneous_array_unifies() {
        assertEquals("int | string", Fns.elem_type(List.<Object>of(1L, "x")));
    }

    @Test
    void test_generic_inference_elem_type_homogeneous_array_is_single_type() {
        assertEquals("int", Fns.elem_type(List.of(1L, 2L, 3L)));
    }

    @Test
    void test_generic_inference_elem_type_three_way_heterogeneous_array_unifies() {
        String rendered = Fns.elem_type(List.<Object>of(1L, "x", true));
        assertTrue(rendered.contains("int") && rendered.contains("string") && rendered.contains("bool"));
    }

    // =======================================================================
    // §G generalized — unbound generic instances inferrable via formal recursion
    // =======================================================================

    @Test
    void test_generic_inference_read_items_unbound_container_recovers_t_from_fields() {
        ContainerShapes<Long> unbound =
                new ContainerShapes<>(1L, List.of(1L, 2L, 3L), Map.of("k", 4L), null, null);
        assertEquals(List.of(1L, 2L, 3L), Fns.read_items(unbound));
    }

    @Test
    void test_generic_inference_list_head_unbound_recursive_recovers_t_from_fields() {
        GenericRecursive<Long> unbound =
                new GenericRecursive<>(7L, new GenericRecursive<>(8L, null));
        assertEquals(7L, Fns.list_head(unbound));
    }

    @Test
    void test_generic_inference_extract_fully_unbound_nested_pair_recovers_all_vars() {
        GenericPair<GenericPair<Long, String>, GenericPair<Boolean, Double>> fullyUnbound =
                new GenericPair<>(new GenericPair<>(1L, "a"), new GenericPair<>(true, 1.5));
        assertEquals("int | string | bool | float", Fns.extract(fullyUnbound));
    }

    // =======================================================================
    // §D concrete-type join — non-primitive actuals participate in the join.
    // =======================================================================

    @Test
    void test_generic_inference_triple_choose_join_includes_concrete_class() {
        String rendered = Fns.triple_choose(5L, new StringIntPair("a", 1L), "x");
        assertTrue(
                rendered.contains("int")
                        && rendered.contains("StringIntPair")
                        && rendered.contains("string"));
    }

    @Test
    void test_generic_inference_triple_choose_join_includes_enum_variant() {
        String rendered = Fns.triple_choose(5L, SomeEnum.VARIANT, new StringIntPair("a", 1L));
        assertTrue(
                rendered.contains("int")
                        && rendered.contains("SomeEnum")
                        && rendered.contains("StringIntPair"));
    }
}
