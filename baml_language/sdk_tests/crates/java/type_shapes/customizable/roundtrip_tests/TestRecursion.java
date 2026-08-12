// Roundtrip coverage for `baml_sdk.recursion` — recursive classes / SCCs.
//
// All recursive child fields are optional, so finite values are built by
// terminating recursion with `null`.
//
// Port of python_pydantic2/type_shapes/customizable/roundtrip_tests/
// test_recursion.py — same test names, cases, inputs, assertions.
package roundtrip_tests;

import static org.junit.jupiter.api.Assertions.assertEquals;

import baml_sdk.recursion.A;
import baml_sdk.recursion.B;
import baml_sdk.recursion.Fns;
import baml_sdk.recursion.IntBinaryTree;
import baml_sdk.recursion.T1;
import baml_sdk.recursion.T2;
import baml_sdk.recursion.T3;
import baml_sdk.recursion.T4;
import baml_sdk.recursion.T5;
import baml_sdk.recursion.T6;
import org.junit.jupiter.api.Test;

class TestRecursion {

    @Test
    void test_recursion_round_trip_int_binary_tree() {
        IntBinaryTree t = new IntBinaryTree(1L, new IntBinaryTree(2L, null, null), null);
        assertEquals(t, Fns.round_trip_int_binary_tree(t));
    }

    @Test
    void test_recursion_round_trip_mutual_recursion() {
        A a = new A(new B(null));
        B b = new B(new A(null));
        assertEquals(a, Fns.round_trip_a(a));
        assertEquals(b, Fns.round_trip_b(b));
    }

    @Test
    void test_recursion_round_trip_scc_t1_t2_t3() {
        T1 t1 = new T1(new T2(null, null), null);
        T2 t2 = new T2(null, new T3(null, null));
        T3 t3 = new T3(null, null);
        assertEquals(t1, Fns.round_trip_t1(t1));
        assertEquals(t2, Fns.round_trip_t2(t2));
        assertEquals(t3, Fns.round_trip_t3(t3));
    }

    @Test
    void test_recursion_round_trip_scc_t4_t5_t6() {
        T4 t4 = new T4(new T5(null, null), null);
        T5 t5 = new T5(null, new T6(null, null));
        T6 t6 = new T6(null, null);
        assertEquals(t4, Fns.round_trip_t4(t4));
        assertEquals(t5, Fns.round_trip_t5(t5));
        assertEquals(t6, Fns.round_trip_t6(t6));
    }
}
