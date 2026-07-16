package baml_bridge;

import java.util.concurrent.CompletableFuture;

/**
 * Runtime-owned streaming wrapper. Compile-only stub: the streaming capability
 * is not implemented yet, so every method throws
 * {@link UnsupportedOperationException}. See ref-java-state-of-completeness.md
 * ("Stream" row) for the target shape.
 *
 * <p>{@code get_final} / {@code get_final_async} escape Python's
 * {@code final} / {@code final_async} because {@code final} is a Java reserved
 * word (per the conventions doc).
 *
 * @param <TPartial> the partial (in-flight) element type
 * @param <TFinal>   the final (completed) value type
 */
public final class BamlStream<TPartial, TFinal> {
    private BamlStream() {
        throw notImplemented();
    }

    public TPartial next() {
        throw notImplemented();
    }

    public CompletableFuture<TPartial> next_async() {
        throw notImplemented();
    }

    public TFinal get_final() {
        throw notImplemented();
    }

    public CompletableFuture<TFinal> get_final_async() {
        throw notImplemented();
    }

    private static UnsupportedOperationException notImplemented() {
        return new UnsupportedOperationException("capability not yet implemented: BamlStream");
    }
}
