// Roundtrip coverage for `baml_sdk.aliases` — type aliases (incl. recursive).
//
// Port of python_pydantic2/type_shapes/customizable/roundtrip_tests/
// test_aliases.py — same test names, cases, inputs, assertions.
//
// java-port note: BAML non-recursive type aliases (`type StringList =
// string[]`) have no Java analog — Java has no alias mechanism — so this
// port assumes codegen erases them to their underlying type at every use
// site (`StringList` -> `List<String>`). A *recursive* alias
// (`type RecList = int | RecList[]`) can't be inlined that way (it would be
// infinite), so it must still emit a real type; this port assumes it gets
// the same treatment as an ad-hoc union (see TestUnions.java) — a generated
// sealed interface named after the alias itself (`RecList`), one record arm
// per union member (`RecList.IntValue`, `RecList.RecListListValue`). Neither the
// non-recursive-alias erasure nor the recursive-alias-as-named-union shape
// is pinned down in the conventions doc; both are invented here.
package roundtrip_tests;

import static org.junit.jupiter.api.Assertions.assertEquals;

import baml_sdk.aliases.AliasContainer;
import baml_sdk.aliases.Fns;
import baml_sdk.aliases.RecList;
import java.util.List;
import org.junit.jupiter.api.Test;

class TestAliases {

    @Test
    void test_aliases_round_trip_string_list() {
        assertEquals(List.of("a", "b"), Fns.round_trip_string_list(List.of("a", "b")));
    }

    @Test
    void test_aliases_round_trip_rec_list() {
        // RecList = int | RecList[]
        assertEquals(
                new RecList.IntValue(1L), Fns.round_trip_rec_list(new RecList.IntValue(1L)));

        RecList nested =
                new RecList.RecListListValue(
                        List.of(
                                new RecList.IntValue(1L),
                                new RecList.RecListListValue(
                                        List.of(
                                                new RecList.IntValue(2L),
                                                new RecList.IntValue(3L)))));
        assertEquals(nested, Fns.round_trip_rec_list(nested));
    }

    @Test
    void test_aliases_round_trip_alias_container() {
        AliasContainer c =
                new AliasContainer(
                        List.of("x"),
                        new RecList.RecListListValue(
                                List.of(
                                        new RecList.IntValue(1L),
                                        new RecList.RecListListValue(List.of(new RecList.IntValue(2L))))));
        assertEquals(c, Fns.round_trip_alias_container(c));
    }
}
