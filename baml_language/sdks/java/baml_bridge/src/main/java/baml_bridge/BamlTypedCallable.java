package baml_bridge;

import java.util.Objects;

/**
 * Internal generated-call carrier pairing a host callable with the function type
 * its slot declares — the callable counterpart of {@link BamlTypedValue}.
 *
 * <p>The generated SDK wraps a callable argument in this carrier so the bridge
 * can honor the generated signature on the dispatch path: when BAML invokes the
 * callable, each argument decodes against its declared parameter descriptor
 * (so e.g. a {@code baml.json.json} parameter materializes as the generated
 * sealed-union type, not the raw wire value), and the returned value encodes
 * against {@link #returnDesc()}. The wire encoder registers this carrier itself
 * in the host-value registry ({@code Handle{HOST_VALUE_CALLABLE}}) and states
 * {@link #functionType()} on the node; {@code BamlFfi.runHostDispatch} unwraps
 * it and threads the parameter and return types.
 *
 * <p>{@link #positionalDescs()} is parallel to the callable's required
 * parameters in declared order; {@link #optionalNames()} / {@link #optionalDescs()}
 * are parallel arrays keying supplied optional arguments by their BAML wire name.
 */
public final class BamlTypedCallable {
    private final Object callable;
    private final BamlType functionType;

    /**
     * Wrap {@code callable} with the function type its slot declares. The type
     * is stated on the wire beside the callable's handle, each argument BAML
     * passes back decodes against its parameter, and the result encodes against
     * the return type.
     */
    public BamlTypedCallable(Object callable, BamlType functionType) {
        this.callable = Objects.requireNonNull(callable, "callable");
        this.functionType = Objects.requireNonNull(functionType, "functionType");
        if (functionType.kind() != BamlType.Kind.FUNCTION) {
            throw new IllegalArgumentException(
                    "a callable's type must be a function type, not " + functionType);
        }
    }

    public Object callable() {
        return callable;
    }

    public BamlType functionType() {
        return functionType;
    }

    /** The declared types of the positional parameters, in order. */
    public BamlType[] positionalDescs() {
        return functionType.functionParams().stream()
                .filter(param -> !param.optional())
                .map(BamlType.Param::type)
                .toArray(BamlType[]::new);
    }

    /** The wire names of the optional (by-name) parameters, in declaration order. */
    public String[] optionalNames() {
        return functionType.functionParams().stream()
                .filter(BamlType.Param::optional)
                .map(BamlType.Param::name)
                .toArray(String[]::new);
    }

    /** The declared types of the optional parameters, aligned with {@link #optionalNames}. */
    public BamlType[] optionalDescs() {
        return functionType.functionParams().stream()
                .filter(BamlType.Param::optional)
                .map(BamlType.Param::type)
                .toArray(BamlType[]::new);
    }

    public BamlType returnDesc() {
        return functionType.functionReturn();
    }
}
