package baml_bridge;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertInstanceOf;
import static org.junit.jupiter.api.Assertions.assertNotNull;
import static org.junit.jupiter.api.Assertions.assertNotEquals;
import static org.junit.jupiter.api.Assertions.assertNull;
import static org.junit.jupiter.api.Assertions.assertThrows;
import static org.junit.jupiter.api.Assertions.assertTrue;

import baml_bridge.internal.ProtoReader;
import baml_bridge.internal.ProtoWriter;
import baml_bridge.internal.WireReader;
import baml_bridge.internal.WireWriter;
import baml_sdk.baml.media.Audio;
import baml_sdk.baml.media.Image;
import baml_sdk.baml.media.Pdf;
import baml_sdk.baml.media.Video;

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

    /** Encoding a native-backed media wrapper copies its portable payload onto field 15. */
    @Test
    void media_encodes_as_portable_value() {
        Image img = Image.from_url("https://example.com/asset", "image/png");
        long originalKey = img.bamlHandle().key();

        assertPortableMedia(img, 1, 3, "https://example.com/asset", "image/png");
        assertPortableMedia(Audio.from_base64("YXVkaW8=", "audio/mpeg"),
                2, 4, "YXVkaW8=", "audio/mpeg");
        assertPortableMedia(Pdf.from_file("document.pdf"),
                3, 5, "document.pdf", null);
        assertPortableMedia(Video.from_url("https://example.com/video", "video/mp4"),
                4, 3, "https://example.com/video", "video/mp4");

        // Portable serialization neither clones nor drains the source handle.
        assertEquals(originalKey, img.bamlHandle().key());
        assertEquals("https://example.com/asset", img.url());
    }

    private static void assertPortableMedia(
            BamlMedia value, int expectedKind, int expectedSource, String expectedValue,
            String expectedMimeType) {
        byte[] encoded = ProtoWriter.encodeInboundValue(value);
        assertNull(subMessage(new WireReader(encoded), 1), "media needs no value_type");
        assertNull(subMessage(new WireReader(encoded), 8), "media is not a class_value");
        byte[] mediaBytes = subMessage(new WireReader(encoded), 15);
        assertNotNull(mediaBytes, "expected InboundValue.media_value (field 15)");

        WireReader media = new WireReader(mediaBytes);
        int kind = 0;
        String mimeType = null;
        int source = 0;
        String payload = null;
        while (media.hasRemaining()) {
            int tag = media.readTag();
            int field = WireReader.fieldOf(tag);
            int wire = WireReader.wireOf(tag);
            switch (field) {
                case 1 -> kind = (int) media.readVarint();
                case 2 -> mimeType = media.readString();
                case 3, 4, 5 -> {
                    source = field;
                    payload = media.readString();
                }
                default -> media.skipField(wire);
            }
        }
        assertEquals(expectedKind, kind);
        assertEquals(expectedMimeType, mimeType);
        assertEquals(expectedSource, source);
        assertEquals(expectedValue, payload);
    }

    /** Portable outbound media reifies all four Java wrappers and all three source arms. */
    @Test
    void portable_media_decode_reifies_kind_and_source() {
        Image image = assertInstanceOf(
                Image.class,
                decodePortableMedia(1, 3, "https://example.com/image", "image/png"));
        assertEquals("https://example.com/image", image.url());
        assertEquals("image/png", image.mime_type());

        Audio audio = assertInstanceOf(
                Audio.class, decodePortableMedia(2, 4, "YXVkaW8=", "audio/mpeg"));
        assertEquals("YXVkaW8=", audio.base64());
        assertEquals("audio/mpeg", audio.mime_type());

        Pdf pdf = assertInstanceOf(
                Pdf.class, decodePortableMedia(3, 5, "document.pdf", null));
        assertEquals("document.pdf", pdf.file());
        assertNull(pdf.mime_type());

        Video video = assertInstanceOf(
                Video.class,
                decodePortableMedia(4, 3, "https://example.com/video", "video/mp4"));
        assertEquals("https://example.com/video", video.url());
        assertEquals("video/mp4", video.mime_type());
    }

    private static Object decodePortableMedia(
            int kind, int sourceField, String value, String mimeType) {
        WireWriter media = new WireWriter();
        media.writeInt64(1, kind); // BamlValueMedia.media
        if (mimeType != null) {
            media.writeString(2, mimeType); // BamlValueMedia.mime_type
        }
        media.writeString(sourceField, value); // BamlValueMedia.value oneof

        WireWriter outbound = new WireWriter();
        outbound.writeMessage(17, media.toByteArray()); // BamlOutboundValue.media_value
        WireWriter result = new WireWriter();
        result.writeMessage(1, outbound.toByteArray()); // BamlOutboundResult.ok
        return ProtoReader.decodeOutboundResult(result.toByteArray());
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
