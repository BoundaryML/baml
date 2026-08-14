package baml_bridge;

import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertTrue;

import java.io.File;
import org.junit.jupiter.api.Assumptions;
import org.junit.jupiter.api.Test;

/**
 * Semantics of {@link BamlCallContext}, the JVM analog of {@code bridge_python}'s
 * {@code BamlCallContext}.
 *
 * <p>The state-machine tests are pure Java: a not-yet-aborted context never
 * fires a native cancel on {@code attach}, and {@code abort} with no live ids
 * loops over nothing, so those paths never touch {@link BamlFfi} (hence never
 * load the native library). The two tests that DO drive the native cancel are
 * guarded by a library-availability assumption, mirroring {@code BamlFfiSmokeTest}
 * — with no runtime initialized the cancel returns {@code false} and is a no-op,
 * so they assert only that the path runs cleanly.
 */
class BamlCallContextTest {

    @Test
    void freshContextIsNotAborted() {
        assertFalse(new BamlCallContext().aborted());
    }

    @Test
    void abortLatchesAbortedFlag() {
        // No active calls attached → abort touches no native cancel, pure state.
        BamlCallContext ctx = new BamlCallContext();
        ctx.abort();
        assertTrue(ctx.aborted());
    }

    @Test
    void abortIsIdempotent() {
        BamlCallContext ctx = new BamlCallContext();
        ctx.abort();
        ctx.abort();
        assertTrue(ctx.aborted());
    }

    @Test
    void attachThenDetachOnLiveContextIsPureBookkeeping() {
        // A not-yet-aborted context never cancels on attach, so attach/detach of
        // a call id needs no native library. After detach, a later abort has no
        // live id to cancel either.
        BamlCallContext ctx = new BamlCallContext();
        ctx.attach(42L);
        ctx.detach(42L);
        assertFalse(ctx.aborted());
        ctx.abort();
        assertTrue(ctx.aborted());
    }

    @Test
    void abortCancelsAttachedCallsWithoutThrowing() {
        assumeNativeLibrary();
        // attach (not yet aborted → no cancel) then abort → snapshots the live id
        // and fires nativeCancelFunctionCall, which no-ops (false) with no runtime.
        BamlCallContext ctx = new BamlCallContext();
        ctx.attach(1234L);
        ctx.abort();
        assertTrue(ctx.aborted());
        ctx.detach(1234L);
    }

    @Test
    void attachWhileAbortedCancelsImmediatelyWithoutThrowing() {
        assumeNativeLibrary();
        // abort-before-start: attaching to an already-aborted context cancels the
        // id immediately (again a no-op false without a runtime).
        BamlCallContext ctx = new BamlCallContext();
        ctx.abort();
        ctx.attach(5678L);
        assertTrue(ctx.aborted());
    }

    /**
     * Skip (JUnit assumption) unless the native {@code bridge_java} library is
     * available, so the state-driving cancel path can be exercised without a
     * hard dependency on a Rust build. Resolves the same ladder
     * {@code BamlFfiSmokeTest} uses; on a hit it sets the property the
     * {@link BamlFfi} static initializer reads.
     */
    private static void assumeNativeLibrary() {
        String path = System.getProperty(BamlFfi.LIB_PROPERTY);
        if (path == null || path.isEmpty()) {
            path = System.getenv(BamlFfi.LIB_ENV_VAR);
        }
        if (path == null || path.isEmpty()) {
            path = "../../../target/debug/libbridge_java.so";
        }
        File lib = new File(path);
        Assumptions.assumeTrue(
                lib.exists(),
                () -> "native library not built (" + lib.getAbsolutePath() + "); skipping");
        System.setProperty(BamlFfi.LIB_PROPERTY, lib.getAbsolutePath());
    }
}
