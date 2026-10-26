package baml_bridge;

import static org.junit.jupiter.api.Assertions.assertArrayEquals;
import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertInstanceOf;
import static org.junit.jupiter.api.Assertions.assertNotNull;
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

    // A call is allocated by the runtime: with none initialized, as here,
    // `nativeNewCallId` returns the ABI's failure value 0 and no call can be
    // made, sync or async. These pin that refusal; the encode → engine →
    // decode path itself runs in the sdk_test_java fixtures.
    @Test
    void nativeNewCallId_without_a_runtime_is_zero() {
        assertEquals(0L, BamlFfi.nativeNewCallId());
        assertEquals(0L, BamlFfi.nativeNewCallId());
    }

    @Test
    void callSync_on_uninitialized_runtime_is_refused_before_anything_is_sent() {
        IllegalStateException err = assertThrows(
                IllegalStateException.class,
                () -> BamlFfi.callSync("does.not.exist", new String[0], new Object[0]));
        assertEquals("Could not allocate a BAML invocation", err.getMessage());
    }

    @Test
    void callAsync_on_uninitialized_runtime_is_refused_before_anything_is_sent() {
        // Refused synchronously: no future is registered for a call that was
        // never allocated.
        IllegalStateException err = assertThrows(
                IllegalStateException.class,
                () -> BamlFfi.callAsync("does.not.exist", new String[0], new Object[0]));
        assertEquals("Could not allocate a BAML invocation", err.getMessage());
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

    // -- callback completion: faults only the native table can produce -------

    @Test
    void a_returned_handle_the_bridge_cannot_clone_is_a_bridge_failure() {
        // A handle whose row is gone: the bridge cannot hand the engine a
        // reference to it, which is its own failure, not the callable's
        // breach of contract.
        try (BamlHandle gone = new BamlHandle(Long.MAX_VALUE, BamlHandle.ADT_MEDIA_IMAGE)) {
            HostCalls.Completion done = HostCalls.returnedCompletion(gone, BamlType.UNKNOWN);
            WireReader r = new WireReader(done.bytes());
            assertEquals(5, WireReader.fieldOf(r.readTag())); // bridge_failure
            String message = r.readString();
            assertTrue(message.contains("could not clone handle"), message);
        }
    }

    @Test
    void a_baml_closure_refused_where_it_arrives_is_released() {
        // A media row stands in for the closure the engine hands over: one
        // reference, which a closure the position refuses must not keep.
        Image closure = Image.from_url("https://example.com/closure", "image/png");
        long key = BamlFfi.nativeHandleClone(closure.bamlHandle().key());
        WireWriter handle = new WireWriter();
        handle.writeInt64(1, key); // BamlOutboundHandle.key
        handle.writeInt64(2, BamlHandle.FUNCTION_REF); // handle_type
        WireWriter outbound = new WireWriter();
        outbound.writeMessage(16, handle.toByteArray()); // BamlOutboundValue.handle_value
        // A position whose callable takes an optional parameter, which a
        // generated interface cannot pass yet: refused.
        BamlType withOptional = BamlType.function(
                java.util.function.Function.class,
                java.util.List.of(new BamlType.Param("x", BamlType.INT, true)),
                BamlType.INT,
                BamlType.UNKNOWN);
        assertThrows(
                BamlError.class,
                () -> ProtoReader.decodeWithDesc(outbound.toByteArray(), withOptional, false));
        assertThrows(RuntimeException.class, () -> BamlFfi.nativeMediaUrl(key));
    }

    /** A `BamlTyClass` whose head is the live row {@code key}. */
    private static byte[] liveClass(long key) {
        WireWriter cls = new WireWriter();
        cls.writeInt64(3, key); // BamlTyClass.live
        WireWriter ty = new WireWriter();
        ty.writeMessage(2, cls.toByteArray()); // BamlTy.class_ty
        return ty.toByteArray();
    }

    /** Every `live` key a stated `BamlTy` carries, in order. */
    private static java.util.List<Long> liveKeys(byte[] bamlTy) {
        java.util.List<Long> keys = new java.util.ArrayList<>();
        collectLiveKeys(new WireReader(bamlTy), keys);
        return keys;
    }

    private static void collectLiveKeys(WireReader ty, java.util.List<Long> keys) {
        while (ty.hasRemaining()) {
            int tag = ty.readTag();
            int field = WireReader.fieldOf(tag);
            if (field == 2) { // class_ty: live = 3
                WireReader cls = ty.readMessage();
                while (cls.hasRemaining()) {
                    int clsTag = cls.readTag();
                    if (WireReader.fieldOf(clsTag) == 3) {
                        keys.add(cls.readVarint());
                    } else {
                        cls.skipField(WireReader.wireOf(clsTag));
                    }
                }
            } else if (field == 4 || field == 7) { // list.item = 1, union.options = 1
                WireReader inner = ty.readMessage();
                while (inner.hasRemaining()) {
                    int innerTag = inner.readTag();
                    if (WireReader.fieldOf(innerTag) == 1) {
                        collectLiveKeys(inner.readMessage(), keys);
                    } else {
                        inner.skipField(WireReader.wireOf(innerTag));
                    }
                }
            } else {
                ty.skipField(WireReader.wireOf(tag));
            }
        }
    }

    @Test
    void a_returned_type_value_holds_a_reference_per_live_head_and_restates_each() {
        // A media row stands in for a runtime declaration: each is a handle
        // table row the engine hands over one reference to per occurrence.
        Image declaration = Image.from_url("https://example.com/declaration", "image/png");
        long first = BamlFfi.nativeHandleClone(declaration.bamlHandle().key());
        long second = BamlFfi.nativeHandleClone(declaration.bamlHandle().key());
        // `W | W[]`: the declaration twice, one reference each.
        WireWriter list = new WireWriter();
        list.writeMessage(1, liveClass(second));
        WireWriter listTy = new WireWriter();
        listTy.writeMessage(4, list.toByteArray());
        WireWriter union = new WireWriter();
        union.writeMessage(1, liveClass(first));
        union.writeMessage(1, listTy.toByteArray());
        WireWriter ty = new WireWriter();
        ty.writeMessage(7, union.toByteArray());
        WireWriter outbound = new WireWriter();
        outbound.writeMessage(21, ty.toByteArray()); // BamlOutboundValue.ty_value
        WireWriter result = new WireWriter();
        result.writeMessage(1, outbound.toByteArray()); // BamlOutboundResult.ok

        BamlType held = assertInstanceOf(
                BamlType.class, ProtoReader.decodeOutboundResult(result.toByteArray()));
        assertEquals(BamlType.Kind.UNION, held.kind());
        assertNull(held.unionOptions().get(0).fqn());
        assertTrue(held.isStatable());
        // The type holds the two references it was handed.
        assertEquals("https://example.com/declaration", BamlFfi.nativeMediaUrl(first));
        assertEquals("https://example.com/declaration", BamlFfi.nativeMediaUrl(second));

        // Stating it hands the engine a fresh reference per occurrence; a
        // request that is never sent takes them back.
        java.util.List<Long> stated;
        try (baml_bridge.internal.EncodingScope encoding = new baml_bridge.internal.EncodingScope()) {
            stated = liveKeys(held.toWireTy());
            assertEquals(2, stated.size());
            for (long key : stated) {
                assertEquals("https://example.com/declaration", BamlFfi.nativeMediaUrl(key));
            }
        }
        for (long key : stated) {
            assertThrows(RuntimeException.class, () -> BamlFfi.nativeMediaUrl(key));
        }
        // The held type still has its own.
        assertEquals("https://example.com/declaration", BamlFfi.nativeMediaUrl(first));
        // Beside a value, a live head is no type the decoder holds.
        assertNull(BamlType.fromWireTy(ty.toByteArray()));
    }

    // -- media (baml.media.*) native round-trip ------------------------------

    /** A URL-backed Image exposes its source URL / mime via the native accessors. */
    @Test
    void media_from_url_exposes_accessors() {
        Image img = Image.from_url("https://example.com/asset", "image/png");
        assertEquals("https://example.com/asset", img.url());
        assertEquals("image/png", img.mime_type());
        assertNull(img.name()); // not read from a file
        assertEquals("", img.base64()); // no base64 payload for a bare URL
    }

    /** A container of media states the media type its values have. */
    @Test
    void a_list_of_media_states_its_media_type() {
        byte[] encoded = ProtoWriter.encodeInboundValue(java.util.List.of(
                Image.from_url("https://example.com/a", "image/png"),
                Image.from_url("https://example.com/b", "image/png")));
        assertArrayEquals(
                BamlType.list(BamlType.media(1)).toWireTy(), subMessage(new WireReader(encoded), 1));
    }

    /** Encoding a native-backed media wrapper copies its portable payload onto field 15. */
    @Test
    void media_encodes_as_portable_value() {
        Image img = Image.from_url("https://example.com/asset", "image/png");
        long originalKey = img.bamlHandle().key();

        assertPortableMedia(img, 1, 3, "https://example.com/asset", null, "image/png");
        assertPortableMedia(Audio.from_base64("YXVkaW8=", "audio/mpeg"),
                2, 4, "YXVkaW8=", null, "audio/mpeg");
        // File content keeps the file's name beside its payload.
        assertPortableMedia(Pdf.from_file_content("/reports/document.pdf", "JVBERi0xLjc="),
                3, 6, "JVBERi0xLjc=", "document.pdf", "application/pdf");
        assertPortableMedia(Video.from_url("https://example.com/video", "video/mp4"),
                4, 3, "https://example.com/video", null, "video/mp4");

        // Portable serialization neither clones nor drains the source handle.
        assertEquals(originalKey, img.bamlHandle().key());
        assertEquals("https://example.com/asset", img.url());
    }

    private static void assertPortableMedia(
            BamlMedia value, int expectedKind, int expectedSource, String expectedValue,
            String expectedName, String expectedMimeType) {
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
        String name = null;
        while (media.hasRemaining()) {
            int tag = media.readTag();
            int field = WireReader.fieldOf(tag);
            int wire = WireReader.wireOf(tag);
            switch (field) {
                case 1 -> kind = (int) media.readVarint();
                case 2 -> mimeType = media.readString();
                case 3, 4 -> {
                    source = field;
                    payload = media.readString();
                }
                case 6 -> {
                    // BamlValueMediaFileContent: name = 1, base64 = 2.
                    source = field;
                    WireReader content = media.readMessage();
                    while (content.hasRemaining()) {
                        int contentTag = content.readTag();
                        switch (WireReader.fieldOf(contentTag)) {
                            case 1 -> name = content.readString();
                            case 2 -> payload = content.readString();
                            default -> content.skipField(WireReader.wireOf(contentTag));
                        }
                    }
                }
                default -> media.skipField(wire);
            }
        }
        assertEquals(expectedKind, kind);
        assertEquals(expectedMimeType, mimeType);
        assertEquals(expectedSource, source);
        assertEquals(expectedValue, payload);
        assertEquals(expectedName, name);
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
                Pdf.class, decodePortableMedia(3, 6, "JVBERi0xLjc=", null, "document.pdf"));
        // Content read from a file keeps the file's name.
        assertEquals("document.pdf", pdf.name());
        assertEquals("application/pdf", pdf.mime_type());

        Video video = assertInstanceOf(
                Video.class,
                decodePortableMedia(4, 3, "https://example.com/video", "video/mp4"));
        assertEquals("https://example.com/video", video.url());
        assertEquals("video/mp4", video.mime_type());
    }

    private static Object decodePortableMedia(
            int kind, int sourceField, String value, String mimeType) {
        return decodePortableMedia(kind, sourceField, value, mimeType, null);
    }

    private static Object decodePortableMedia(
            int kind, int sourceField, String value, String mimeType, String name) {
        WireWriter media = new WireWriter();
        media.writeInt64(1, kind); // BamlValueMedia.media
        if (mimeType != null) {
            media.writeString(2, mimeType); // BamlValueMedia.mime_type
        }
        if (name == null) {
            media.writeString(sourceField, value); // BamlValueMedia.value oneof
        } else {
            WireWriter content = new WireWriter();
            content.writeString(1, name); // BamlValueMediaFileContent.name
            content.writeString(2, value); // BamlValueMediaFileContent.base64
            media.writeMessage(sourceField, content.toByteArray()); // BamlValueMedia.file_content
        }

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
