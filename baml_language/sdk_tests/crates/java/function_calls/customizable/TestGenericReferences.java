// A generic class the runtime keeps to itself (`baml.sap._ParseCache<T>`): an
// instance crosses as a reference that carries its type arguments, so its
// methods bind `T` from the receiver, and two references to one engine
// instance are equal.

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertNotEquals;

import baml_bridge.BamlType;
import baml_bridge.BamlTypes;
import baml_sdk.baml.sap.Fns;
import baml_sdk.baml.sap._ParseCache;
import java.util.List;
import org.junit.jupiter.api.Test;

class TestGenericReferences {

    @Test
    void test_generic_references_method_binds_the_class_type_argument_from_its_receiver() throws Exception {
        _ParseCache<Long> ints = Fns._new_parse_cache(BamlTypes.of("T", BamlType.INT));
        assertEquals(41L, ints._parse_final("41"));
        assertEquals(41L, ints._parse_final_async("41").get());
        // The same text read at another type argument: the binding is the
        // receiver's, not a default.
        _ParseCache<String> strings = Fns._new_parse_cache(BamlTypes.of("T", BamlType.STRING));
        assertEquals("41", strings._parse_final("41"));
    }

    @Test
    void test_generic_references_reference_carries_its_type_arguments() {
        // The type argument a reference came with binds another call: a cache
        // built at the strings cache's `T` reads text as a string.
        _ParseCache<String> strings = Fns._new_parse_cache(BamlTypes.of("T", BamlType.STRING));
        assertEquals(List.of(BamlType.STRING), strings.bamlTypeArgs());
        _ParseCache<Object> rebuilt =
                Fns._new_parse_cache(BamlTypes.of("T", strings.bamlTypeArgs().get(0)));
        assertEquals("41", rebuilt._parse_final("41"));
    }

    @Test
    void test_generic_references_one_engine_instance_is_one_reference() {
        _ParseCache<Long> cache = Fns._new_parse_cache(BamlTypes.of("T", BamlType.INT));
        assertEquals(
                cache,
                baml_sdk.generic_tests.Fns.identity(
                        cache,
                        BamlTypes.of("T", BamlType.of(_ParseCache.class, BamlType.INT))));
        assertNotEquals(Fns._new_parse_cache(BamlTypes.of("T", BamlType.INT)), cache);
    }
}
