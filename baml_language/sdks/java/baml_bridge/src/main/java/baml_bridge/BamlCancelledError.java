package baml_bridge;

import java.util.List;
import java.util.concurrent.CancellationException;

/**
 * Structured BAML cancellation reason (the {@code baml.panics.Cancelled} class
 * tag), surfaced when an <em>async</em> call is aborted engine-side.
 *
 * <p>It extends {@link CancellationException} (not {@link BamlError}) so it
 * behaves as the JVM analog of Python's {@code asyncio.CancelledError}: when a
 * {@link java.util.concurrent.CompletableFuture} completes exceptionally with
 * this value the JDK reports {@code isCancelled() == true}, and
 * {@code join()} / {@code get()} surface it <em>directly</em> (unwrapped — a
 * {@code CancellationException} is not re-wrapped in a
 * {@code CompletionException} / {@code ExecutionException}). The decoded
 * {@code baml.panics.Cancelled} value rides on {@link #value()}, mirroring
 * Python's {@code BamlCancelledError.value} reachable via the raised
 * {@code CancelledError.reason}.
 *
 * <p>A <em>sync</em> cancellation still surfaces as a {@link BamlPanic} whose
 * {@code value()} is a {@code Cancelled}; only the async engine-driven abort is
 * remapped to this type. Accessor names are snake_case to line up 1:1 with
 * {@link BamlError} / {@link BamlPanic} and the cross-language parity tests.
 */
public class BamlCancelledError extends CancellationException {
    private static final long serialVersionUID = 1L;

    private final transient Object value;
    private final List<String> bamlTrace;
    private final String className;

    public BamlCancelledError(Object value, List<String> bamlTrace, String className) {
        super(BamlError.formatMessage(className, value));
        this.value = value;
        this.bamlTrace = bamlTrace == null ? List.of() : List.copyOf(bamlTrace);
        this.className = className;
    }

    /** The decoded cancellation value (a {@code baml.panics.Cancelled}). */
    public Object value() {
        return value;
    }

    /** Pre-rendered BAML stack frame lines (most-recent-call-last). */
    public List<String> baml_trace() {
        return bamlTrace;
    }

    /** The cancellation value's BAML FQN, or null when unknown. */
    public String class_name() {
        return className;
    }
}
