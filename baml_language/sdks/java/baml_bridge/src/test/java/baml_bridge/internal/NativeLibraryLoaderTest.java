package baml_bridge.internal;

import static org.junit.jupiter.api.Assertions.assertArrayEquals;
import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertNotNull;
import static org.junit.jupiter.api.Assertions.assertNull;
import static org.junit.jupiter.api.Assertions.assertTrue;

import java.io.File;
import java.io.InputStream;
import java.nio.file.Files;

import org.junit.jupiter.api.Test;

/**
 * Offline tests for the self-contained native loader — no {@code System.load},
 * no native library required. They pin the platform-string mapping (the
 * classpath-resource layout of the published {@code natives-*} jars) and the
 * temp-file extraction helper. {@link NativeLibraryLoader} has no native static
 * initializer, so exercising it never attempts a real load; the env-var-based
 * {@code BamlFfiSmokeTest} is unaffected.
 */
class NativeLibraryLoaderTest {

    // -- os.name → {linux, macos, windows} -----------------------------------

    @Test
    void mapOs_normalizes_known_names() {
        assertEquals("linux", NativeLibraryLoader.mapOs("Linux"));
        assertEquals("macos", NativeLibraryLoader.mapOs("Mac OS X"));
        assertEquals("macos", NativeLibraryLoader.mapOs("Darwin"));
        assertEquals("windows", NativeLibraryLoader.mapOs("Windows 11"));
        assertEquals("windows", NativeLibraryLoader.mapOs("Windows Server 2022"));
    }

    // -- os.arch → {x86_64, aarch64} (amd64→x86_64, arm64/aarch64→aarch64) ----

    @Test
    void mapArch_normalizes_known_arches() {
        assertEquals("x86_64", NativeLibraryLoader.mapArch("amd64"));
        assertEquals("x86_64", NativeLibraryLoader.mapArch("x86_64"));
        assertEquals("x86_64", NativeLibraryLoader.mapArch("x64"));
        assertEquals("aarch64", NativeLibraryLoader.mapArch("aarch64"));
        assertEquals("aarch64", NativeLibraryLoader.mapArch("arm64"));
    }

    @Test
    void nativeResourcePath_is_wellformed_for_this_host() {
        String path = NativeLibraryLoader.nativeResourcePath();
        // /native/<os>-<arch>/<mapped-lib-name>
        assertTrue(path.startsWith("/native/"), "expected a /native/ resource path, got " + path);
        assertTrue(
                path.endsWith("/" + System.mapLibraryName(NativeLibraryLoader.LIB_BASE_NAME)),
                "expected the host mapped-lib-name suffix, got " + path);
        assertEquals(
                "/native/" + NativeLibraryLoader.platformDir() + "/" + NativeLibraryLoader.libFileName(),
                path);
    }

    // -- extraction helper ---------------------------------------------------

    @Test
    void extractResource_copies_classpath_resource_to_temp_file() throws Exception {
        String resource = "/native/testos-testarch/testlib.txt";
        byte[] expected;
        try (InputStream in = getClass().getResourceAsStream(resource)) {
            assertNotNull(in, "test fixture resource missing on classpath: " + resource);
            expected = in.readAllBytes();
        }

        File extracted = NativeLibraryLoader.extractResource(resource, "testlib.txt");
        assertNotNull(extracted, "expected a non-null extracted file for a present resource");
        assertTrue(extracted.exists(), "extracted file should exist: " + extracted);
        assertEquals("testlib.txt", extracted.getName());
        assertTrue(
                extracted.getParentFile().getName().startsWith("baml-bridge-native"),
                "expected extraction under a baml-bridge-native temp dir, got "
                        + extracted.getParentFile());
        assertArrayEquals(expected, Files.readAllBytes(extracted.toPath()));
    }

    @Test
    void extractResource_returns_null_when_resource_absent() throws Exception {
        assertNull(NativeLibraryLoader.extractResource("/native/nope-nope/missing.so", "missing.so"));
    }
}
