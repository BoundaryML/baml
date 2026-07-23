package baml_bridge;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertNotNull;
import static org.junit.jupiter.api.Assertions.assertNotEquals;
import static org.junit.jupiter.api.Assertions.assertNull;
import static org.junit.jupiter.api.Assertions.assertThrows;
import static org.junit.jupiter.api.Assertions.assertTrue;

import baml_bridge.internal.ProtoWriter;
import baml_bridge.internal.WireReader;
import baml_sdk.baml.media.Image;

import java.io.File;
import java.util.concurrent.CompletableFuture;
import java.util.concurrent.ExecutionException;
import java.util.concurrent.TimeUnit;

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

    @Test
    void callAsync_on_uninitialized_runtime_completes_with_baml_error() throws Exception {
        // The real async path end to end: callAsync mints a call id, registers
        // the future, and nativeCallAsync spawns the call — capturing the JVM +
        // BamlFfi class ref, then (no runtime initialized) encoding a pre-call
        // BridgeError::NotInitialized as an `error` arm and delivering it back
        // through the JNI completeCall route. The thenApply decode then raises
        // BamlError: the async sibling of
        // callSync_on_uninitialized_runtime_surfaces_baml_error.
        CompletableFuture<Object> future =
                BamlFfi.callAsync("does.not.exist", new String[0], new Object[0]);
        assertNotNull(future, "callAsync must hand back a future rather than block");

        ExecutionException ex = assertThrows(
                ExecutionException.class,
                () -> future.get(10, TimeUnit.SECONDS));
        assertTrue(ex.getCause() instanceof BamlError,
                "expected a BamlError cause, got " + ex.getCause());
        BamlError err = (BamlError) ex.getCause();
        assertNotNull(err.class_name());
        assertTrue(err.class_name().startsWith("baml.errors."),
                "expected a baml.errors.* class name, got " + err.class_name());
    }

    @Test
    void completeCall_with_unknown_call_id_is_ignored() {
        // No future is registered under this id, so a late/stale delivery must be
        // a harmless no-op — the removal-on-completion property a future
        // cancel_function_call depends on. (Also references BamlFfi, so it loads
        // the native library under the same @BeforeAll assumption guard.)
        long unusedId = BamlFfi.nativeNewCallId();
        BamlFfi.completeCall(unusedId, new byte[0]);
    }

    // -- os-exit telemetry-flush hooks ---------------------------------------

    /**
     * The flush drain {@code decodePanic} runs just before {@code Runtime.halt} on
     * an os-exit panic, exercised in isolation (no halt): registered hooks run in
     * order, and a throwing hook is swallowed without aborting the drain. Lives
     * here (not in an offline test) only because touching any {@link BamlFfi}
     * member triggers its native-loading static initializer — the same
     * {@code @BeforeAll} assumption guard covers it. The halt itself stays covered
     * by the {@code function_calls TestErrors} subprocess exit-code tests.
     */
    @Test
    void exit_flush_hooks_run_best_effort_and_swallow_exceptions() {
        java.util.List<String> order = new java.util.ArrayList<>();
        BamlFfi.registerExitFlushHooks(() -> order.add("a"));
        BamlFfi.registerExitFlushHooks(() -> {
            throw new RuntimeException("boom"); // must be swallowed, not propagate
        });
        BamlFfi.registerExitFlushHooks(() -> order.add("b"));

        // The factored-out flush step — driven directly, deliberately without any
        // Runtime.halt so the test JVM survives.
        BamlFfi.runExitFlushHooks();

        // Both non-throwing hooks ran, in registration order; the throwing hook
        // neither aborted the drain nor propagated.
        assertEquals(java.util.List.of("a", "b"), order);
    }

    // -- media (baml.media.*) native round-trip ------------------------------

    /** A URL-backed Image exposes its source URL / mime via the native accessors. */
    @Test
    void media_from_url_exposes_accessors() {
        Image img = Image.from_url("https://example.com/asset", "image/png");
        assertEquals("https://example.com/asset", img.url());
        assertEquals("image/png", img.mime_type());
        assertNull(img.file()); // not file-backed
        assertEquals("", img.base64()); // no base64 payload for a bare URL
    }

    /**
     * Encoding a media value produces {@code class_value(baml.media.Image, {_data:
     * handle})} with a <em>freshly cloned</em> wire key — the original handle
     * survives the encode so the Java object stays usable after the call.
     */
    @Test
    void media_encodes_as_class_value_with_cloned_data_handle() {
        Image img = Image.from_url("https://example.com/asset", null);
        long originalKey = img.bamlHandle().key();

        byte[] encoded = ProtoWriter.encodeInboundValue(img);

        // InboundValue.class_value = 8.
        byte[] classBytes = subMessage(new WireReader(encoded), 8);
        assertNotNull(classBytes, "expected InboundValue.class_value (field 8)");

        byte[] valueType = subMessage(new WireReader(encoded), 1);
        assertNotNull(valueType, "expected InboundValue.value_type (field 1)");
        byte[] mediaType = subMessage(new WireReader(valueType), 11);
        assertNotNull(mediaType, "expected BamlTy.media (field 11)");
        int mediaKind = 0;
        WireReader ty = new WireReader(mediaType);
        while (ty.hasRemaining()) {
            int tag = ty.readTag();
            if (WireReader.fieldOf(tag) == 1) {
                mediaKind = (int) ty.readVarint();
            } else {
                ty.skipField(WireReader.wireOf(tag));
            }
        }

        // InboundClassValue: fields = 2 (repeated InboundMapEntry).
        WireReader cv = new WireReader(classBytes);
        String dataKey = null;
        byte[] dataValueBytes = null;
        while (cv.hasRemaining()) {
            int tag = cv.readTag();
            int f = WireReader.fieldOf(tag);
            int wt = WireReader.wireOf(tag);
            if (f == 2) {
                WireReader entry = cv.readMessage();
                while (entry.hasRemaining()) {
                    int et = entry.readTag();
                    int ef = WireReader.fieldOf(et);
                    if (ef == 1) {
                        dataKey = entry.readString(); // InboundMapEntry.string_key
                    } else if (ef == 6) {
                        dataValueBytes = entry.readBytes(); // InboundMapEntry.value
                    } else {
                        entry.skipField(WireReader.wireOf(et));
                    }
                }
            } else {
                cv.skipField(wt);
            }
        }
        assertEquals(1, mediaKind); // BAML_TY_MEDIA_KIND_IMAGE
        assertEquals("_data", dataKey);
        assertNotNull(dataValueBytes, "expected a _data field value");

        // _data InboundValue.handle = 10 → BamlHandle { key = 1, handle_type = 2 }.
        byte[] handleBytes = subMessage(new WireReader(dataValueBytes), 10);
        assertNotNull(handleBytes, "expected InboundValue.handle (field 10)");
        WireReader hb = new WireReader(handleBytes);
        long wireKey = 0;
        int handleType = 0;
        while (hb.hasRemaining()) {
            int tag = hb.readTag();
            int f = WireReader.fieldOf(tag);
            if (f == 1) {
                wireKey = hb.readVarint();
            } else if (f == 2) {
                handleType = (int) hb.readVarint();
            } else {
                hb.skipField(WireReader.wireOf(tag));
            }
        }
        assertEquals(BamlHandle.ADT_MEDIA_IMAGE, handleType);
        assertNotEquals(0L, wireKey);
        assertNotEquals(originalKey, wireKey, "wire key must be a fresh clone");

        // The original handle survived the clone: its accessors still resolve.
        assertEquals("https://example.com/asset", img.url());

        // The engine would drain the wire key on decode; we never sent it, so
        // release it here to avoid leaking the row.
        BamlFfi.nativeHandleRelease(wireKey);
    }

    /** Read the first length-delimited sub-message payload for {@code wantField}. */
    private static byte[] subMessage(WireReader r, int wantField) {
        byte[] found = null;
        while (r.hasRemaining()) {
            int tag = r.readTag();
            int f = WireReader.fieldOf(tag);
            int wt = WireReader.wireOf(tag);
            if (f == wantField) {
                found = r.readBytes();
            } else {
                r.skipField(wt);
            }
        }
        return found;
    }
}
