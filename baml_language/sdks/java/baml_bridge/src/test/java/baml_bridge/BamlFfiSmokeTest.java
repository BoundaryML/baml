package baml_bridge;

import static org.junit.jupiter.api.Assertions.assertNotNull;
import static org.junit.jupiter.api.Assertions.assertNotEquals;
import static org.junit.jupiter.api.Assertions.assertThrows;
import static org.junit.jupiter.api.Assertions.assertTrue;

import java.io.File;

import org.junit.jupiter.api.Assumptions;
import org.junit.jupiter.api.BeforeAll;
import org.junit.jupiter.api.Test;

/**
 * End-to-end smoke test against the native {@code bridge_java} library. Skipped
 * (JUnit assumption) when the {@code .so} has not been built, so
 * {@code gradle build} stays green without a Rust build; when present, it drives
 * the full encode → JNI → engine → decode path. It does <em>not</em> need
 * compiled BAML bytecode: calling into an uninitialized runtime exercises the
 * pre-call error envelope, which round-trips back as a {@link BamlError}.
 */
class BamlFfiSmokeTest {

    @BeforeAll
    static void loadNativeLibrary() {
        String path = System.getProperty(BamlFfi.LIB_PROPERTY);
        if (path == null || path.isEmpty()) {
            path = System.getenv(BamlFfi.LIB_ENV_VAR);
        }
        if (path == null || path.isEmpty()) {
            // Conventional dev location relative to this Gradle project dir.
            path = "../../../target/debug/libbridge_java.so";
        }
        File lib = new File(path);
        Assumptions.assumeTrue(lib.exists(),
                () -> "native library not built (" + lib.getAbsolutePath() + "); skipping smoke test");
        // Set the property BamlFfi's static initializer reads on first use.
        System.setProperty(BamlFfi.LIB_PROPERTY, lib.getAbsolutePath());
    }

    @Test
    void nativeNewCallId_is_nonzero_and_monotonic() {
        long a = BamlFfi.nativeNewCallId();
        long b = BamlFfi.nativeNewCallId();
        assertNotEquals(0L, a);
        assertNotEquals(0L, b);
        assertTrue(b > a, "expected monotonically increasing call ids: " + a + " then " + b);
    }

    @Test
    void callSync_on_uninitialized_runtime_surfaces_baml_error() {
        // No runtime initialized → pre-call BridgeError::NotInitialized, which
        // the engine encodes as an `error` arm carrying a
        // baml.errors.GenericSdkError class value.
        BamlError err = assertThrows(
                BamlError.class,
                () -> BamlFfi.callSync("does.not.exist", new String[0], new Object[0]));
        assertNotNull(err.class_name());
        assertTrue(err.class_name().startsWith("baml.errors."),
                "expected a baml.errors.* class name, got " + err.class_name());
    }
}
