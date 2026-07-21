// Roundtrip coverage for `baml_sdk.generics` — generic classes (over
// `<int>`).
//
// The generic *instance method* path (`WrapperMethods.get_value` /
// `get_value_or_marker`) is covered separately in top-level
// `TestGeneric.java`, mirroring `customizable/test_generic.py`; here we
// cover the concretely-instantiated generic class round trips.
//
// Port of python_pydantic2/type_shapes/customizable/roundtrip_tests/
// test_generics.py — same test names, cases, inputs, assertions.
//
// java-port note: the conventions doc pins `new Wrapper<Long>(5L)` /
// `Wrapper<Long> w = ...` as the generic-class construction shape but
// leaves "explicit type binding" as a trailing-type-args-parameter TBD (see
// the "Generic function/method (explicit)" row in
// ref-java-state-of-completeness.md: "Java has no `_types=` kwarg or
// subscript; shape TBD"). This port uses the plain generic constructor form
// exactly as given in the task's own example, with no trailing type-args
// parameter. How the hidden `BamlType[]` type-args side-channel mentioned
// for "Generic explicitly/implicitly reified" values gets populated from an
// ordinary `new Wrapper<Long>(...)` call (Java generics are erased at
// runtime) is unresolved and needs a real codegen decision.
package roundtrip_tests;

import static org.junit.jupiter.api.Assertions.assertEquals;

import baml_sdk.generics.Box;
import baml_sdk.generics.DifferingInstantiation;
import baml_sdk.generics.Fns;
import baml_sdk.generics.GenericBinaryTree;
import baml_sdk.generics.GenericLinkedList;
import baml_sdk.generics.NestedGenerics;
import baml_sdk.generics.Wrapper;
import java.util.List;
import org.junit.jupiter.api.Test;

class TestGenerics {

    @Test
    void test_generics_round_trip_wrapper_int() {
        Wrapper<Long> w = new Wrapper<>(5L);
        assertEquals(w, Fns.round_trip_wrapper_int(w));
    }

    @Test
    void test_generics_round_trip_generic_linked_list_int() {
        GenericLinkedList<Long> ll =
                new GenericLinkedList<>(1L, new GenericLinkedList<>(2L, null));
        assertEquals(ll, Fns.round_trip_generic_linked_list_int(ll));
    }

    @Test
    void test_generics_round_trip_generic_binary_tree_int() {
        GenericBinaryTree<Long> t = new GenericBinaryTree<>(1L, null, null);
        assertEquals(t, Fns.round_trip_generic_binary_tree_int(t));
    }

    @Test
    void test_generics_round_trip_box_int() {
        Box<Long> b = new Box<>(3L, new Wrapper<>(4L));
        assertEquals(b, Fns.round_trip_box_int(b));
    }

    @Test
    void test_generics_round_trip_nested_generics() {
        NestedGenerics n =
                new NestedGenerics(
                        new Wrapper<Wrapper<Long>>(new Wrapper<>(1L)),
                        new Wrapper<List<Long>>(List.of(1L, 2L)),
                        new Wrapper<GenericLinkedList<Long>>(new GenericLinkedList<>(9L, null)));
        assertEquals(n, Fns.round_trip_nested_generics(n));
    }

    @Test
    void test_generics_round_trip_differing_instantiation() {
        DifferingInstantiation d =
                new DifferingInstantiation(
                        new GenericLinkedList<Wrapper<Long>>(new Wrapper<>(1L), null));
        assertEquals(d, Fns.round_trip_differing_instantiation(d));
    }
}
