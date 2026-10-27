package baml_bridge;

/**
 * An instance of a standard-library class that crosses the boundary as a
 * reference to the engine's own instance (a compiled {@code reflect.Package},
 * a {@code reflect.Session}): its fields are the runtime's internals, so a host
 * holds it and passes it back, but never reads into it or builds one.
 *
 * <p>Each generated wrapper is a {@code final} subclass with a private
 * constructor that binds itself as the decoder's factory
 * ({@link TypeRegistry#bindOpaqueFactory}). It crosses as an
 * {@code ADT_RUNTIME_VALUE} handle both ways: decoded from the handle the engine
 * minted at a position typed as its class, and encoded as a fresh clone of that
 * handle (so the instance stays usable after the call). Two instances are equal
 * when they hold the same handle-table row.
 */
public abstract class BamlOpaque {
    private final BamlHandle handle;

    protected BamlOpaque(BamlHandle handle) {
        if (handle.handleType() != BamlHandle.ADT_RUNTIME_VALUE) {
            throw new IllegalArgumentException(
                    "an opaque instance is an ADT_RUNTIME_VALUE handle, received handle type "
                            + handle.handleType());
        }
        this.handle = handle;
    }

    /** The engine handle this instance owns; the encoder sends a clone of it. */
    public final BamlHandle bamlHandle() {
        return handle;
    }

    @Override
    public final boolean equals(Object o) {
        return o instanceof BamlOpaque other
                && other.getClass() == getClass()
                && other.handle.key() == handle.key();
    }

    @Override
    public final int hashCode() {
        return Long.hashCode(handle.key());
    }
}
