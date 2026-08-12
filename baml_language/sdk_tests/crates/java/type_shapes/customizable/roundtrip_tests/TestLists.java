// Roundtrip coverage for `baml_sdk.lists` — list Ty variants.
//
// Port of python_pydantic2/type_shapes/customizable/roundtrip_tests/
// test_lists.py — same test names, cases, inputs, assertions.
//
// java-port note: for the union-shaped elements below, see TestUnions.java
// for the generic-family shape (`(int | string)` -> `Union2<Long, String>`,
// int = Arm0, string = Arm1) this port assumes.
package roundtrip_tests;

import static org.junit.jupiter.api.Assertions.assertEquals;

import baml_bridge.Union2;
import baml_sdk.lists.Fns;
import baml_sdk.lists.ListContainer;
import java.util.Arrays;
import java.util.List;
import org.junit.jupiter.api.Test;

class TestLists {

    @Test
    void test_lists_round_trip_ints() {
        assertEquals(List.of(1L, 2L, 3L), Fns.round_trip_ints(List.of(1L, 2L, 3L)));
    }

    @Test
    void test_lists_round_trip_empty_list() {
        // Regression for Bug A (35b), fixed by `SetInParent()` on the
        // `list_value` oneof arm in proto.py: an empty list used to encode
        // as an unset oneof, which the engine read as null and returned as
        // `None`.
        assertEquals(List.of(), Fns.round_trip_ints(List.of()));
    }

    @Test
    void test_lists_round_trip_optional_strings() {
        // java-port note: `List.of` rejects null elements; `Arrays.asList`
        // permits them and is used wherever a list literal carries a
        // `null` element.
        List<String> xs = Arrays.asList("a", null, "b");
        assertEquals(xs, Fns.round_trip_optional_strings(xs));
    }

    @Test
    void test_lists_round_trip_union_list() {
        List<Union2<Long, String>> xs =
                List.of(
                        new Union2.Arm0<Long, String>(1L),
                        new Union2.Arm1<Long, String>("two"),
                        new Union2.Arm0<Long, String>(3L));
        assertEquals(xs, Fns.round_trip_union_list(xs));
    }

    @Test
    void test_lists_round_trip_list_container() {
        ListContainer c =
                new ListContainer(
                        List.of(1L, 2L),
                        Arrays.asList(null, "z"),
                        List.of(
                                new Union2.Arm0<Long, String>(1L),
                                new Union2.Arm1<Long, String>("x")));
        assertEquals(c, Fns.round_trip_list_container(c));
    }
}
