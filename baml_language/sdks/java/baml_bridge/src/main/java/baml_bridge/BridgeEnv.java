package baml_bridge;

/**
 * Native environment shim: mutate the <em>engine's</em> view of the process
 * environment (the in-process engine reads native {@code getenv}, which
 * JVM-side patching such as junit-pioneer does not reach — see the
 * conventions doc's "Native env hook").
 *
 * <p>Compile-only stub: {@link #set}/{@link #unset} throw
 * {@link UnsupportedOperationException} for now. The real native
 * {@code setenv}/{@code unsetenv} shim in {@code bridge_java} is a follow-up.
 */
public final class BridgeEnv {
    private BridgeEnv() {}

    public static void set(String name, String value) {
        throw notImplemented();
    }

    public static void unset(String name) {
        throw notImplemented();
    }

    private static UnsupportedOperationException notImplemented() {
        return new UnsupportedOperationException("capability not yet implemented: BridgeEnv native setenv");
    }
}
