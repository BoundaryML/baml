package baml_bridge;

import baml_bridge.internal.InvocationFrames;
import java.util.concurrent.CompletableFuture;
import org.junit.jupiter.api.Test;
import static org.junit.jupiter.api.Assertions.*;

class InvocationFramesTest {
    @Test void explicit_wrappers_restore_frame_java_only() {
        var original = new InvocationFrames.Frame(new BamlHandle(1, BamlHandle.HOST_VALUE_OPAQUE), "original");
        var captured = new InvocationFrames.Frame(new BamlHandle(2, BamlHandle.HOST_VALUE_OPAQUE), "captured");
        InvocationFrames.CURRENT.set(original);
        try {
            assertSame(captured, captured.wrapFunction(value -> InvocationFrames.CURRENT.get()).apply(7));
            assertSame(original, InvocationFrames.CURRENT.get());
            assertThrows(IllegalStateException.class, () -> captured.wrapRunnable(() -> { throw new IllegalStateException(); }).run());
            assertSame(original, InvocationFrames.CURRENT.get());
            assertSame(captured, CompletableFuture.supplyAsync(() -> captured.run(() -> InvocationFrames.CURRENT.get())).join());
        } finally { InvocationFrames.CURRENT.remove(); }
        assertNull(InvocationFrames.CURRENT.get());
    }
}
