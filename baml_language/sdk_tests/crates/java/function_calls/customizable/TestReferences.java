// Instances the runtime keeps to itself (`baml.hash.DefaultHasher`) cross as
// references: one passed back is the same instance, wherever a class can sit.
// Its state shows which instance a call reached: one hasher fed "a" and then
// "b" hashes as a fresh one fed "ab".

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertInstanceOf;
import static org.junit.jupiter.api.Assertions.assertNotEquals;
import static org.junit.jupiter.api.Assertions.assertNotNull;
import static org.junit.jupiter.api.Assertions.assertNull;

import baml_bridge.Union2;
import baml_sdk.baml.hash.DefaultHasher;
import baml_sdk.references.Fns;
import baml_sdk.references.HasherHolder;
import baml_sdk.references.HasherMarker;
import java.util.ArrayList;
import java.util.List;
import java.util.Map;
import org.junit.jupiter.api.Test;

class TestReferences {

    @Test
    void test_references_a_returned_instance_passed_back_is_the_same_instance() {
        DefaultHasher hasher = Fns.make_hasher();
        Fns.hash_after(hasher, "a");
        long fedTwice = Fns.hash_after(hasher, "b");
        assertEquals(Fns.hash_after(Fns.make_hasher(), "ab"), fedTwice);
        // Negative control: a fresh hasher fed only "b" differs.
        assertNotEquals(Fns.hash_after(Fns.make_hasher(), "b"), fedTwice);
    }

    @Test
    void test_references_one_engine_instance_is_one_reference() {
        DefaultHasher hasher = Fns.make_hasher();
        assertEquals(hasher, Fns.same_hasher(hasher));
        assertNotEquals(hasher, Fns.make_hasher());
    }

    @Test
    void test_references_containers_hold_the_instance() {
        DefaultHasher hasher = Fns.make_hasher();
        assertNotNull(Fns.maybe_hasher(true));
        assertNull(Fns.maybe_hasher(false));
        assertEquals(List.of(hasher, hasher), Fns.hasher_twice(hasher));
        assertEquals(Map.of("key", hasher), Fns.hasher_under("key", hasher));
    }

    @Test
    void test_references_a_field_holds_the_instance() {
        DefaultHasher hasher = Fns.make_hasher();
        HasherHolder holder = Fns.hold_hasher("label", hasher);
        assertEquals("label", holder.label());
        assertEquals(hasher, holder.hasher());
    }

    @Test
    void test_references_a_union_selects_the_instance_arm() {
        assertInstanceOf(DefaultHasher.class,
                assertInstanceOf(Union2.Arm0.class, Fns.hasher_or_marker(true)).value());
        assertEquals(new HasherMarker("marker"),
                assertInstanceOf(Union2.Arm1.class, Fns.hasher_or_marker(false)).value());
        assertEquals("hasher",
                Fns.describe_hasher_or_marker(new Union2.Arm0<>(Fns.make_hasher())));
        assertEquals("marker",
                Fns.describe_hasher_or_marker(new Union2.Arm1<>(new HasherMarker("marker"))));
    }

    @Test
    void test_references_a_callback_receives_and_returns_the_instance() {
        DefaultHasher hasher = Fns.make_hasher();
        List<DefaultHasher> received = new ArrayList<>();
        DefaultHasher returned = Fns.hasher_through_callback(hasher, value -> {
            received.add(value);
            return value;
        });
        assertEquals(hasher, returned);
        assertEquals(List.of(hasher), received);
    }
}
