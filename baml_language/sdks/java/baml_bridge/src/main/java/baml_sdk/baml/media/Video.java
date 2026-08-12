package baml_sdk.baml.media;

import baml_bridge.BamlHandle;
import baml_bridge.BamlMedia;

/**
 * Runtime-owned {@code Video} media handle (BAML stdlib {@code baml.media.Video}).
 * Re-exported by the runtime, never code-generated per fixture. Wraps a single
 * {@link BamlHandle} over the engine-side {@code Adt(Media)} row.
 */
public final class Video implements BamlMedia {
    /** BAML stdlib FQN, emitted on the inbound {@code class_value}. */
    public static final String FQN = "baml.media.Video";
    /** Proto {@code MediaTypeEnum.VIDEO}. */
    private static final int KIND = 4;
    /** Wire {@code BamlHandleType.ADT_MEDIA_VIDEO}. */
    private static final int HANDLE_TYPE = BamlHandle.ADT_MEDIA_VIDEO;

    private final BamlHandle handle;

    private Video(BamlHandle handle) {
        this.handle = handle;
    }

    /** Wrap a decoded engine handle (used by the wire codec on the decode path). */
    public static Video fromHandle(BamlHandle handle) {
        return new Video(handle);
    }

    public static Video from_url(String url) {
        return from_url(url, null);
    }

    public static Video from_url(String url, String mimeType) {
        return new Video(BamlHandle.mediaFromUrl(KIND, HANDLE_TYPE, url, mimeType));
    }

    public static Video from_file(String path) {
        return from_file(path, null);
    }

    public static Video from_file(String path, String mimeType) {
        return new Video(BamlHandle.mediaFromFile(KIND, HANDLE_TYPE, path, mimeType));
    }

    public static Video from_base64(String base64) {
        return from_base64(base64, null);
    }

    public static Video from_base64(String base64, String mimeType) {
        return new Video(BamlHandle.mediaFromBase64(KIND, HANDLE_TYPE, base64, mimeType));
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
