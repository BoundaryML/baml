// A type value naming a declaration the runtime created (`reflect.Class.new`)
// holds the engine's reference to that declaration, and states it again, a
// fresh reference per occurrence, when it is sent back.

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertInstanceOf;
import static org.junit.jupiter.api.Assertions.assertNull;
import static org.junit.jupiter.api.Assertions.assertTrue;

import baml_bridge.BamlHandle;
import baml_bridge.BamlType;
import baml_bridge.BamlTypes;
import java.util.List;
import org.junit.jupiter.api.Test;

class TestRuntimeTypes {

    private static final String DOC = "{\"title\": \"t\", \"body\": null, \"word_count\": 2}";

    // SDK_PARITY_LINT(skip): pins the Java BamlType holding a runtime declaration's engine reference
    @Test
    void test_a_runtime_declarations_type_value_holds_it_and_states_it_again() {
        Object returned = baml_sdk.lorem.Fns.hs7_collision_type();
        BamlType collision = assertInstanceOf(BamlType.class, returned);
        assertEquals(BamlType.Kind.CLASS, collision.kind());
        // No name resolves a runtime declaration: the type holds the
        // declaration itself, and can state it.
        assertNull(collision.fqn());
        assertTrue(collision.isStatable());

        // Sent back as an explicit type argument, it names the same
        // declaration: the engine builds an instance of it (a value of a
        // runtime declaration crosses as the engine's own), not of the
        // compiled class that shares its display name.
        Object value = baml_sdk.baml.json.Fns.decode(DOC, BamlTypes.of("T", collision));
        BamlHandle instance = assertInstanceOf(BamlHandle.class, value);
        assertEquals(BamlHandle.ADT_RUNTIME_VALUE, instance.handleType());

        // Named twice in one type (`W | W[]`), each occurrence is its own
        // reference, and the type can be stated as often as it is used.
        BamlType either = BamlType.union(collision, BamlType.list(collision));
        for (int i = 0; i < 3; i++) {
            Object many = baml_sdk.baml.json.Fns.decode("[" + DOC + ", " + DOC + "]", BamlTypes.of("T", either));
            List<?> items = assertInstanceOf(List.class, many);
            assertEquals(2, items.size());
            Object one = baml_sdk.baml.json.Fns.decode(DOC, BamlTypes.of("T", either));
            assertInstanceOf(BamlHandle.class, one);
        }
    }
}
