package baml_bridge;

import java.util.List;

/**
 * Unchecked wrapper raised when a BAML function surfaces a thrown error value
 * (the {@code error} arm of a {@code BamlOutboundResult}). The Java analog of
 * Python's {@code BamlError} (errors.py).
 *
 * <p>A BAML error type cannot itself be an exception, so the decoded thrown
 * value rides inside this wrapper via {@link #value()}. {@link #baml_trace()}
 * carries the pre-rendered {@code File "...", line N, in fn} frame strings from
 * the BAML stack, and {@link #class_name()} is the thrown value's BAML FQN when
 * known (e.g. {@code baml.errors.GenericSdkError}).
 *
 * <p>Accessor names are snake_case to line up 1:1 with the Python reference and
 * the cross-language parity tests.
 */
public class BamlError extends RuntimeException {
    private static final long serialVersionUID = 1L;

    private final transient Object value;
    private final List<String> bamlTrace;
    private final String className;

    public BamlError(Object value, List<String> bamlTrace, String className) {
        super(formatMessage(className, value));
        this.value = value;
        this.bamlTrace = bamlTrace == null ? List.of() : List.copyOf(bamlTrace);
        this.className = className;
    }

    /** The decoded thrown value. */
    public Object value() {
        return value;
    }

    /** Pre-rendered BAML stack frame lines (most-recent-call-last). */
    public List<String> baml_trace() {
        return bamlTrace;
    }

    /** The thrown value's BAML FQN, or null when unknown. */
    public String class_name() {
        return className;
    }

    static String formatMessage(String className, Object value) {
        String name = className != null ? className : (value == null ? "BamlError" : value.getClass().getName());
        return name + ": " + value;
    }
}
