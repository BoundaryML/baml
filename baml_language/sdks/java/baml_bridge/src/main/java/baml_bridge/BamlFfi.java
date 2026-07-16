package baml_bridge;

import baml_bridge.internal.ProtoReader;
import baml_bridge.internal.ProtoWriter;

import java.util.concurrent.CompletableFuture;
import java.util.concurrent.ExecutorService;
import java.util.concurrent.Executors;
import java.util.concurrent.atomic.AtomicLong;

/**
 * The Java entry point into the in-process BAML engine. The generated SDK calls
 * exactly this surface:
 *
 * <pre>{@code
 *   Object v = baml_bridge.BamlFfi.callSync(fqn, paramNames, args);
 *   CompletableFuture<Object> f = baml_bridge.BamlFfi.callAsync(fqn, paramNames, args);
 * }</pre>
 *
 * <p>The native library ({@code libbridge_java.so} — the {@code bridge_java}
 * Rust cdylib) is loaded once, from the path in the system property
 * {@code baml.bridge.lib} or, failing that, the environment variable
 * {@code BAML_JAVA_BRIDGE_LIB}. This is the JVM analog of Python's
 * {@code bridge_python} extension module: bytes-in / bytes-out over the shared
 * {@code baml_bridge.cffi.v1} protobuf envelopes, with all encode/decode on
 * this (Java) side.
 */
public final class BamlFfi {
    /** Env var / system property naming the native library to {@code System.load}. */
    public static final String LIB_ENV_VAR = "BAML_JAVA_BRIDGE_LIB";
    public static final String LIB_PROPERTY = "baml.bridge.lib";

    /**
     * Interim executor for {@link #callAsync}. Daemon threads so the pool never
     * keeps the JVM alive. TODO(bridge-java): replace the supplyAsync-over-sync
     * shim with the real async path (a {@code nativeCallAsync} that completes
     * the future from the engine's result callback, and threads cancellation
     * through {@code cancel_function_call}).
     */
    private static final ExecutorService ASYNC_POOL = Executors.newCachedThreadPool(r -> {
        Thread t = new Thread(r, "baml-async");
        t.setDaemon(true);
        return t;
    });

    /**
     * Fallback call-id source, used only if the native counter is somehow
     * unavailable. The Rust side owns the authoritative counter
     * ({@code sys_types::CallId::next}); a nonzero id is mandatory (the engine
     * rejects call_id 0).
     */
    private static final AtomicLong FALLBACK_CALL_ID = new AtomicLong(1);

    static {
        System.load(resolveLibraryPath());
    }

    private BamlFfi() {}

    private static String resolveLibraryPath() {
        String path = System.getProperty(LIB_PROPERTY);
        if (path == null || path.isEmpty()) {
            path = System.getenv(LIB_ENV_VAR);
        }
        if (path == null || path.isEmpty()) {
            throw new IllegalStateException(
                    "bridge_java native library path is not set: define the system property '"
                            + LIB_PROPERTY + "' or the environment variable " + LIB_ENV_VAR
                            + " pointing at libbridge_java.so (target/debug/libbridge_java.so)");
        }
        return path;
    }

    // ---- Native methods (implemented in sdks/java/bridge_java) --------------

    /** Initialize the process-global runtime from serialized BAML bytecode. */
    static native void nativeInitFromBytecode(byte[] bytecode);

    /**
     * Run a BAML function synchronously. Takes a protobuf-encoded
     * {@code CallFunctionArgs} and returns protobuf-encoded
     * {@code BamlOutboundResult} bytes (engine errors/panics ride inside those
     * bytes; a thrown {@code RuntimeException} means a JNI-glue failure).
     */
    static native byte[] nativeCallSync(String fqn, byte[] encodedCallFunctionArgs);

    /** Mint a process-unique, nonzero function-call id from the engine counter. */
    static native long nativeNewCallId();

    // ---- Public surface the generated SDK targets --------------------------

    /** Initialize the runtime from embedded bytecode (idempotent; replaces). */
    public static void initFromBytecode(byte[] bytecode) {
        nativeInitFromBytecode(bytecode);
    }

    /**
     * Synchronously call the BAML function {@code fqn}, passing {@code args}
     * paired positionally with their declared parameter {@code names}. Returns
     * the decoded value, or throws {@link BamlError} / {@link BamlPanic}.
     */
    public static Object callSync(String fqn, String[] names, Object[] args) {
        byte[] request = ProtoWriter.encodeCallFunctionArgs(names, args, newCallId());
        byte[] response = nativeCallSync(fqn, request);
        return ProtoReader.decodeOutboundResult(response);
    }

    /**
     * Asynchronous sibling of {@link #callSync}. For now this runs the sync
     * path on a background daemon thread; see {@link #ASYNC_POOL} for the real
     * async-path TODO.
     */
    public static CompletableFuture<Object> callAsync(String fqn, String[] names, Object[] args) {
        return CompletableFuture.supplyAsync(() -> callSync(fqn, names, args), ASYNC_POOL);
    }

    private static long newCallId() {
        long id = nativeNewCallId();
        // Defensive: the engine rejects call_id 0, and the native counter starts
        // at 1, so this fallback should never trigger.
        return id != 0 ? id : FALLBACK_CALL_ID.getAndIncrement();
    }
}
