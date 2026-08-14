package baml_bridge;

import java.util.Objects;

/**
 * Internal generated-call carrier pairing a host callable with the type-directed
 * decode/encode descriptors of its declared BAML callable type — the callable
 * counterpart of {@link BamlTypedValue}.
 *
 * <p>The generated SDK wraps a callable argument in this carrier so the bridge
 * can honor the generated signature on the dispatch path: when BAML invokes the
 * callable, each argument decodes against its declared parameter descriptor
 * (so e.g. a {@code baml.json.json} parameter materializes as the generated
 * sealed-union type, not the raw wire value), and the returned value encodes
 * against {@link #returnDesc()}. The wire encoder registers this carrier itself
 * in the host-value registry ({@code Handle{HOST_VALUE_CALLABLE}});
 * {@code BamlFfi.runHostDispatch} unwraps it and threads the descriptors.
 *
 * <p>Descriptor slots follow the runtime's convention: a {@code null} array or
 * a {@code null} entry means "decode/encode that slot wire-driven" (exactly the
 * pre-carrier behavior). {@link #positionalDescs()} is parallel to the
 * callable's required parameters in declared order; {@link #optionalNames()} /
 * {@link #optionalDescs()} are parallel arrays keying supplied optional
 * arguments by their BAML wire name.
 */
public final class BamlTypedCallable {
    private final Object callable;
    private final BamlType[] positionalDescs;
    private final String[] optionalNames;
    private final BamlType[] optionalDescs;
    private final BamlType returnDesc;

    public BamlTypedCallable(
            Object callable,
            BamlType[] positionalDescs,
            String[] optionalNames,
            BamlType[] optionalDescs,
            BamlType returnDesc) {
        this.callable = Objects.requireNonNull(callable, "callable");
        int optionalNameCount = optionalNames == null ? 0 : optionalNames.length;
        int optionalDescCount = optionalDescs == null ? 0 : optionalDescs.length;
        if (optionalNameCount != optionalDescCount) {
            throw new IllegalArgumentException(
                    "optional descriptor arrays length mismatch: "
                            + optionalNameCount + " names vs " + optionalDescCount + " descriptors");
        }
        this.positionalDescs = positionalDescs;
        this.optionalNames = optionalNames;
        this.optionalDescs = optionalDescs;
        this.returnDesc = returnDesc;
    }

    /** All-positional convenience: no optional parameters. */
    public BamlTypedCallable(Object callable, BamlType[] positionalDescs, BamlType returnDesc) {
        this(callable, positionalDescs, null, null, returnDesc);
    }

    /** The wrapped host callable (a {@code java.util.function.*} shape or a {@link BamlHostCallable}). */
    public Object callable() {
        return callable;
    }

    /** Required-parameter descriptors in declared order, or {@code null} (all wire-driven). */
    public BamlType[] positionalDescs() {
        return positionalDescs;
    }

    /** Optional-parameter BAML wire names, parallel to {@link #optionalDescs()}, or {@code null}. */
    public String[] optionalNames() {
        return optionalNames;
    }

    /** Optional-parameter descriptors, parallel to {@link #optionalNames()}, or {@code null}. */
    public BamlType[] optionalDescs() {
        return optionalDescs;
    }

    /** The declared return-type encode descriptor, or {@code null} (wire-driven). */
    public BamlType returnDesc() {
        return returnDesc;
    }
}
