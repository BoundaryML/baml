package baml_sdk.baml.media;

import baml_bridge.BamlFfi;
import baml_bridge.BamlHandle;
import baml_bridge.BamlMedia;

/**
 * Runtime-owned {@code Pdf} media handle (BAML stdlib {@code baml.media.Pdf}).
 * Re-exported by the runtime, never code-generated per fixture. Wraps a single
 * {@link BamlHandle} over the engine-side {@code Adt(Media)} row.
 */
public final class Pdf implements BamlMedia {
    /** BAML stdlib FQN used to choose the portable media kind. */
    public static final String FQN = "baml.media.Pdf";
    /** Proto {@code MediaTypeEnum.PDF}. */
    private static final int KIND = 3;
    /** Wire {@code BamlHandleType.ADT_MEDIA_PDF}. */
    private static final int HANDLE_TYPE = BamlHandle.ADT_MEDIA_PDF;
    private static final String[] FROM_FILE_NAMES = {"file", "mime_type"};

    private final BamlHandle handle;

    private Pdf(BamlHandle handle) {
        this.handle = handle;
    }

    /** Wrap a decoded engine handle (used by the wire codec on the decode path). */
    public static Pdf fromHandle(BamlHandle handle) {
        return new Pdf(handle);
    }

    public static Pdf from_url(String url) {
        return from_url(url, null);
    }

    public static Pdf from_url(String url, String mimeType) {
        return new Pdf(BamlHandle.mediaFromUrl(KIND, HANDLE_TYPE, url, mimeType));
    }

    public static Pdf from_file(String path) {
        return from_file(path, null);
    }

    /**
     * Reads the file now, through {@code baml.media.Pdf.from_file}: the value
     * holds its content and base name, never its path. Throws the BAML error
     * {@code baml.errors.Io} when the file cannot be read.
     */
    public static Pdf from_file(String path, String mimeType) {
        return (Pdf)
                BamlFfi.callSync(
                        "baml.media.Pdf.from_file",
                        FROM_FILE_NAMES,
                        new Object[] {path, mimeType});
    }

    public static Pdf from_file_content(String file, String base64) {
        return from_file_content(file, base64, null);
    }

    /**
     * Base64 content that was read from {@code file}: named by its base name,
     * with the MIME type it implies unless one is given. Reads nothing.
     */
    public static Pdf from_file_content(String file, String base64, String mimeType) {
        return new Pdf(
                BamlHandle.mediaFromFileContent(KIND, HANDLE_TYPE, file, base64, mimeType));
    }

    public static Pdf from_base64(String base64) {
        return from_base64(base64, null);
    }

    public static Pdf from_base64(String base64, String mimeType) {
        return new Pdf(BamlHandle.mediaFromBase64(KIND, HANDLE_TYPE, base64, mimeType));
    }

    /** Source URL, or {@code null} when not URL-backed. */
    public String url() {
        return handle.mediaUrl();
    }

    /** Base name of the file the content was read from, or {@code null}. */
    public String name() {
        return handle.mediaName();
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
