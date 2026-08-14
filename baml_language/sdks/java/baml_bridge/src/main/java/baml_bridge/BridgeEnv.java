package baml_bridge;

/**
 * Native environment shim: mutate the <em>engine's</em> view of the process
 * environment. The in-process engine reads env via native {@code getenv}
 * (Rust {@code std::env::var}), which JVM-side patching — {@code System.getenv}
 * caching, junit-pioneer's {@code @SetEnvironmentVariable} — does not reach.
 * {@link #set}/{@link #unset} route through {@code bridge_java}'s native
 * {@code setenv}/{@code unsetenv} so the change is visible to the engine, the
 * exact parity of {@code bridge_python} relying on Python's
 * {@code os.environ[...] = ...} (which calls {@code setenv(3)}).
 *
 * <p>Used by the replay-harness tests to point the env-driven {@code StreamStub}
 * client at the local replay server.
 */
public final class BridgeEnv {
    private BridgeEnv() {}

    /** Set process env var {@code name} to {@code value}, visible to the engine. */
    public static void set(String name, String value) {
        BamlFfi.nativeEnvSet(name, value);
    }

    /** Remove process env var {@code name}. */
    public static void unset(String name) {
        BamlFfi.nativeEnvUnset(name);
    }
}
