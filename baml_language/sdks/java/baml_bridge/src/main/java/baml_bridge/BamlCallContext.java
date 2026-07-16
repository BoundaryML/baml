package baml_bridge;

/**
 * Handle to an in-flight call, exposing cancellation. Compile-only stub:
 * {@link #abort()} throws {@link UnsupportedOperationException} until the
 * cancellation capability lands (engine {@code cancel_function_call}).
 */
public final class BamlCallContext {
    private BamlCallContext() {}

    /** Request cancellation of the in-flight call. */
    public void abort() {
        throw new UnsupportedOperationException("capability not yet implemented: BamlCallContext.abort");
    }
}
