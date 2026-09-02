package baml_bridge;

/**
 * Marker for the runtime-owned media wrapper classes ({@code baml.media.Image},
 * {@code Audio}, {@code Video}, {@code Pdf}) so the wire codec can detect and
 * encode a media value without importing {@code baml_sdk.baml.media}. Each media
 * class composes a single {@link BamlHandle} (the engine-side {@code Adt(Media)}
 * row) and knows its BAML stdlib FQN.
 *
 * <p>The Java objects remain native-handle-backed, but bridge traffic uses the
 * canonical portable media representation ({@code kind}, optional MIME type,
 * and a URL/file/base64 payload). This keeps runtime-local handles out of the
 * wire format; outbound decode reconstructs a fresh wrapper in this runtime.
 */
public interface BamlMedia {
    /** The engine handle backing this media value. */
    BamlHandle bamlHandle();

    /** The BAML stdlib FQN, e.g. {@code "baml.media.Image"}. */
    String bamlFqn();
}
