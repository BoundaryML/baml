package baml_bridge;

import java.util.Objects;

/**
 * Internal generated-call carrier pairing a Java argument with its declared
 * BAML type. It adds no wire wrapper; the encoder uses the type only for exact
 * node annotations where Java's erased runtime shape is ambiguous.
 */
public record BamlTypedValue(Object value, BamlType type) {
    public BamlTypedValue {
        Objects.requireNonNull(type, "type");
    }
}
