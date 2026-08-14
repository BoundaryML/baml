// Roundtrip coverage for `baml_sdk.forward_refs` — forward references.
//
// Port of python_pydantic2/type_shapes/customizable/roundtrip_tests/
// test_forward_refs.py — same test names, cases, inputs, assertions.
//
// java-port note: `round_trip_node` is intentionally NOT exercised by a
// `@Test` — `class Node { next Node }` has a *required* (non-optional)
// self-reference, so no finite value can be constructed from the host side.
// Python proves the symbol is merely reachable via a module-level
// `# noqa: F401` import; the closest Java analog is a method-reference
// field that forces javac to resolve `Fns.round_trip_node`'s signature
// without ever calling it.
package roundtrip_tests;

import static org.junit.jupiter.api.Assertions.assertEquals;

import baml_sdk.forward_refs.Fns;
import baml_sdk.forward_refs.GNode;
import baml_sdk.forward_refs.Node;
import baml_sdk.forward_refs.Other;
import baml_sdk.forward_refs.RecList;
import baml_sdk.forward_refs.RecListWithOther;
import java.util.List;
import java.util.function.UnaryOperator;
import org.junit.jupiter.api.Test;

class TestForwardRefs {

    @SuppressWarnings("unused")
    private static final UnaryOperator<Node> ROUND_TRIP_NODE_REACHABILITY_PROBE =
            Fns::round_trip_node;

    @Test
    void test_forward_refs_round_trip_other() {
        Other o = new Other(7L);
        assertEquals(o, Fns.round_trip_other(o));
    }

    @Test
    void test_forward_refs_round_trip_rec_list() {
        RecList r =
                new RecList.RecListListValue(
                        List.of(
                                new RecList.IntValue(1L),
                                new RecList.RecListListValue(
                                        List.of(
                                                new RecList.IntValue(2L),
                                                new RecList.IntValue(3L)))));
        assertEquals(r, Fns.round_trip_rec_list(r));
    }

    @Test
    void test_forward_refs_round_trip_rec_list_with_other() {
        // RecListWithOther = int | Other | RecListWithOther[]
        assertEquals(
                new RecListWithOther.IntValue(1L),
                Fns.round_trip_rec_list_with_other(new RecListWithOther.IntValue(1L)));

        RecListWithOther r =
                new RecListWithOther.RecListWithOtherListValue(
                        List.of(
                                new RecListWithOther.IntValue(1L),
                                new RecListWithOther.IntValue(2L)));
        assertEquals(r, Fns.round_trip_rec_list_with_other(r));
    }

    @Test
    void test_forward_refs_round_trip_g_node_int() {
        // The leaf node carries `children=[]`; this exercises the empty-list
        // round trip fixed under Bug A (35b). See TestGenerics.java for the
        // java-port note on the `new GNode<Long>(...)` generic-constructor
        // shape this port assumes.
        GNode<Long> g = new GNode<>(List.of(new GNode<>(List.of())));
        assertEquals(g, Fns.round_trip_g_node_int(g));
    }
}
