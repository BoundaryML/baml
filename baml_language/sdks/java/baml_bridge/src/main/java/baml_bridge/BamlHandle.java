package baml_bridge;

/**
 * Opaque handle to an engine-side object (a {@code BexExternalValue} kept in the
 * handle table). Compile-only stub for the handle-backed capabilities
 * (media, streams, {@code $rust_type} shells, host callables). Not constructible
 * yet.
 */
public final class BamlHandle {
    private BamlHandle() {
        throw new UnsupportedOperationException("capability not yet implemented: BamlHandle");
    }
}
