package baml_sdk.baml.media;

import baml_bridge.BamlHandle;
import baml_bridge.BamlMedia;

/**
 * Runtime-owned {@code Image} media handle (BAML stdlib {@code baml.media.Image}).
 * Re-exported by the runtime, never code-generated per fixture (per the
 * conventions doc's "Media" row). Wraps a single {@link BamlHandle} over the
 * engine-side {@code Adt(Media)} row; static factories dispatch natively via the
 * {@code baml_media_from_*} constructors rather than round-tripping through the
 * BAML engine.
 */
public final class Image implements BamlMedia {
    /** BAML stdlib FQN, emitted on the inbound {@code class_value}. */
    public static final String FQN = "baml.media.Image";
    /** Proto {@code MediaTypeEnum.IMAGE}. */
    private static final int KIND = 1;
    /** Wire {@code BamlHandleType.ADT_MEDIA_IMAGE}. */
    private static final int HANDLE_TYPE = BamlHandle.ADT_MEDIA_IMAGE;

    private final BamlHandle handle;

    private Image(BamlHandle handle) {
        this.handle = handle;
    }

    /** Wrap a decoded engine handle (used by the wire codec on the decode path). */
    public static Image fromHandle(BamlHandle handle) {
        return new Image(handle);
    }

    public static Image from_url(String url) {
        return from_url(url, null);
    }

    public static Image from_url(String url, String mimeType) {
        return new Image(BamlHandle.mediaFromUrl(KIND, HANDLE_TYPE, url, mimeType));
    }

    public static Image from_file(String path) {
        return from_file(path, null);
    }

    public static Image from_file(String path, String mimeType) {
        return new Image(BamlHandle.mediaFromFile(KIND, HANDLE_TYPE, path, mimeType));
    }

    public static Image from_base64(String base64) {
        return from_base64(base64, null);
    }

    public static Image from_base64(String base64, String mimeType) {
        return new Image(BamlHandle.mediaFromBase64(KIND, HANDLE_TYPE, base64, mimeType));
    }

    /** Source URL, or {@code null} when not URL-backed. */
    public String url() {
        return handle.mediaUrl();
    }

    /** Local file path, or {@code null} when not file-backed. */
    public String file() {
        return handle.mediaFile();
    }

    /** Base64 payload (never {@code null}). */
    public String base64() {
        return handle.mediaBase64();
    }

    /** MIME type, or {@code null} when none is set. */
    public String mime_type() {
        return handle.mediaMimeType();
    }

    @Override
    public BamlHandle bamlHandle() {
        return handle;
    }

    @Override
    public String bamlFqn() {
        return FQN;
    }
}
