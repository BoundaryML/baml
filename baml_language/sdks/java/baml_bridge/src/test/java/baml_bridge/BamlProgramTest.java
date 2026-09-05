package baml_bridge;

import java.util.List;
import java.util.Map;
import org.junit.jupiter.api.Test;
import static org.junit.jupiter.api.Assertions.*;

class BamlProgramTest {
    public static final class Box {
        private final Object value;
        public Box(Object value) { this.value = value; }
        public Object value() { return value; }
    }
    public record Arm(Object value) {}

    @Test void nestedCapabilitiesSelectTheirOriginAndRejectMixedOwners() {
        var a = new BamlProgram(Long.MIN_VALUE + 11, getClass().getClassLoader());
        var b = new BamlProgram(Long.MIN_VALUE + 22, getClass().getClassLoader());
        // Zero-key synthetic handles never own a native row, so this routing
        // test needs no native library and cannot release somebody else's row.
        BamlHandle ah = a.within(() -> new BamlHandle(0, BamlHandle.FUNCTION_REF));
        BamlHandle bh = b.within(() -> new BamlHandle(0, BamlHandle.FUNCTION_REF));
        a.within(() -> { TypeRegistry.registerClass("user.Box", Box.class.getName(), new String[]{"value"}); return null; });
        Object[] nested = {List.of(Map.of("x", new Box(new Arm(ah))))};
        assertSame(a, BamlProgram.forArgs(nested));
        assertThrows(IllegalArgumentException.class, () -> BamlProgram.forArgs(new Object[]{nested, bh}));
        assertThrows(IllegalArgumentException.class, () -> b.within(() -> BamlProgram.forArgs(nested)));
        var cycle = new java.util.ArrayList<Object>();
        cycle.add(cycle);
        cycle.add(ah);
        assertSame(a, BamlProgram.forArgs(new Object[]{cycle}));
    }

    @Test void doneIsRegisteredInEveryProgramTypeMap() {
        var a = new BamlProgram(Long.MIN_VALUE + 31, getClass().getClassLoader());
        var b = new BamlProgram(Long.MIN_VALUE + 32, getClass().getClassLoader());
        assertTrue(a.within(() -> TypeRegistry.isClass("ai.stream.Done")));
        assertTrue(b.within(() -> TypeRegistry.isClass("ai.stream.Done")));
    }
}
