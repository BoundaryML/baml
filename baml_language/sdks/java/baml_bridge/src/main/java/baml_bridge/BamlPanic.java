package baml_bridge;

import java.util.List;

/**
 * Unchecked wrapper raised for a BAML panic (the non-exit {@code panic} arm of
 * a {@code BamlOutboundResult}). The Java analog of Python's {@code BamlPanic}
 * (errors.py). A clean {@code baml.sys.exit} never reaches here — the decoder
 * terminates the process via {@code Runtime.getRuntime().halt(exit_code)}.
 *
 * <p>Extends {@link Error}, not {@link RuntimeException} — the JVM analog of
 * Python's {@code BamlPanic} subclassing {@code BaseException} rather than
 * {@code Exception}: a panic is not an ordinary recoverable error, so a bare
 * {@code catch (Exception)} (Java's {@code except Exception}) does not swallow
 * it. Callers that genuinely want to intercept a panic catch {@link BamlPanic}
 * (or {@link Throwable}) explicitly.
 *
 * <p>Accessor names are snake_case to line up 1:1 with the Python reference.
 */
public class BamlPanic extends Error {
    private static final long serialVersionUID = 1L;

    private final transient Object value;
    private final List<String> bamlTrace;
    private final String className;

    public BamlPanic(Object value, List<String> bamlTrace, String className) {
        super(BamlError.formatMessage(className, value));
        this.value = value;
        this.bamlTrace = bamlTrace == null ? List.of() : List.copyOf(bamlTrace);
        this.className = className;
    }

    /** The decoded panic value. */
    public Object value() {
        return value;
    }

    /** Pre-rendered BAML stack frame lines (most-recent-call-last). */
    public List<String> baml_trace() {
        return bamlTrace;
    }

    /** The panic value's BAML FQN, or null when unknown. */
    public String class_name() {
        return className;
    }
}
