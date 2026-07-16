package baml_sdk.baml.media;

/**
 * Runtime-owned {@code Video} media handle (BAML stdlib path {@code baml.media}).
 * These classes are re-exported by the runtime, never code-generated (per the
 * conventions doc's "Media" row). Compile-only stub: the factories throw
 * {@link UnsupportedOperationException} until the media capability lands
 * (native {@code baml_media_from_url/file/base64} handle constructors).
 */
public final class Video {
    private Video() {}

    public static Video from_url(String url) {
        throw notImplemented();
    }

    public static Video from_url(String url, String mimeType) {
        throw notImplemented();
    }

    public static Video from_file(String path) {
        throw notImplemented();
    }

    public static Video from_file(String path, String mimeType) {
        throw notImplemented();
    }

    public static Video from_base64(String base64) {
        throw notImplemented();
    }

    public static Video from_base64(String base64, String mimeType) {
        throw notImplemented();
    }

    private static UnsupportedOperationException notImplemented() {
        return new UnsupportedOperationException("capability not yet implemented: Video media");
    }
}
