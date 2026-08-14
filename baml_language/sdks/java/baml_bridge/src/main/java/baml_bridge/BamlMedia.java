package baml_bridge;

/**
 * Marker for the runtime-owned media wrapper classes ({@code baml.media.Image},
 * {@code Audio}, {@code Video}, {@code Pdf}) so the wire codec can detect and
 * encode a media value without importing {@code baml_sdk.baml.media}. Each media
 * class composes a single {@link BamlHandle} (the engine-side {@code Adt(Media)}
 * row) and knows its BAML stdlib FQN.
 *
 * <p>Inbound encode uses an exact media {@code value_type} plus a class-shaped
 * payload containing {@code { _data: handle }} with a freshly cloned wire key
 * (see {@code ProtoWriter}); outbound decode reconstructs the wrapper from the
 * decoded handle (see {@code ProtoReader}).
 */
public interface BamlMedia {
    /** The engine handle backing this media value. */
    BamlHandle bamlHandle();

    /** The BAML stdlib FQN, e.g. {@code "baml.media.Image"}. */
    String bamlFqn();
}
