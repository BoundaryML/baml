package baml_bridge;

import java.util.List;

/**
 * Structured BAML cancellation reason (the {@code baml.panics.Cancelled} class
 * tag). Mirrors Python's {@code BamlCancelledError(BamlError)}. Compile-only
 * stub — nothing constructs it yet; the async cancellation path will surface it
 * (as a {@code CancellationException} at the future boundary) once wired.
 */
public class BamlCancelledError extends BamlError {
    private static final long serialVersionUID = 1L;

    public BamlCancelledError(Object value, List<String> bamlTrace, String className) {
        super(value, bamlTrace, className);
    }
}
