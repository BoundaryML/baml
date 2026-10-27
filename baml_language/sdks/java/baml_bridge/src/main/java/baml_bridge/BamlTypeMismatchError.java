package baml_bridge;

import java.util.List;

/**
 * A BAML call refused an argument that is not a value of its parameter's type
 * (the engine's {@code baml.errors.TypeMismatch}). It is the caller's argument
 * error, so it is an {@link IllegalArgumentException} whose message is the
 * error's {@code message}; the thrown BAML value rides on {@link #value()}, as
 * on a {@link BamlError}, so a host callback that lets it escape forwards it to
 * BAML as the error it is.
 *
 * <p>Accessor names are snake_case to line up 1:1 with {@link BamlError}.
 */
public class BamlTypeMismatchError extends IllegalArgumentException {
    private static final long serialVersionUID = 1L;

    private final transient Object value;
    private final List<String> bamlTrace;
    private final String className;

    public BamlTypeMismatchError(String message, Object value, List<String> bamlTrace, String className) {
        super(message);
        this.value = value;
        this.bamlTrace = bamlTrace == null ? List.of() : List.copyOf(bamlTrace);
        this.className = className;
    }

    /** The decoded {@code baml.errors.TypeMismatch} value. */
    public Object value() {
        return value;
    }

    /** Pre-rendered BAML stack frame lines (most-recent-call-last). */
    public List<String> baml_trace() {
        return bamlTrace;
    }

    /** The thrown value's BAML FQN. */
    public String class_name() {
        return className;
    }
}
