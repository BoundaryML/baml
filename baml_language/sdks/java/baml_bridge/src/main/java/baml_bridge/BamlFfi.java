package baml_bridge;

import baml_bridge.internal.NativeLibraryLoader;
import baml_bridge.internal.ProtoReader;
import baml_bridge.internal.ProtoWriter;

import java.util.concurrent.CompletableFuture;
import java.util.concurrent.ConcurrentHashMap;
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
 * Rust cdylib) is loaded once via {@link NativeLibraryLoader}, following a
 * first-hit-wins ladder: (1) the system property {@code baml.bridge.lib} (dev
 * override), (2) the environment variable {@code BAML_JAVA_BRIDGE_LIB} (dev/test
 * override), then (3) the bundled per-platform classpath resource
 * {@code /native/{os}-{arch}/{libname}} (extracted to a temp file and loaded) —
 * so a published {@code baml-bridge} + its {@code natives-*} jar is
 * self-contained with no environment setup. This is the JVM analog of Python's
 * {@code bridge_python} extension module: bytes-in / bytes-out over the shared
 * {@code baml_bridge.cffi.v1} protobuf envelopes, with all encode/decode on
 * this (Java) side.
 */
public final class BamlFfi {
    /** Env var / system property naming the native library to {@code System.load}. */
    public static final String LIB_ENV_VAR = "BAML_JAVA_BRIDGE_LIB";
    public static final String LIB_PROPERTY = "baml.bridge.lib";

    /**
     * In-flight async calls, keyed by their process-unique {@code call_id}.
     * {@link #callAsync} registers a raw-bytes future here before dispatching
     * {@link #nativeCallAsync}; the engine resolves it from {@link #completeCall}
     * on an engine thread, which removes the entry. Keyed by {@code call_id} (not
     * an opaque token) and cleared on completion, so a future
     * {@code cancel_function_call} can target the same id — cancellation is not
     * wired yet, but the map is already compatible with it.
     */
    private static final ConcurrentHashMap<Long, CompletableFuture<byte[]>> PENDING =
            new ConcurrentHashMap<>();

    /**
     * Fallback call-id source, used only if the native counter is somehow
     * unavailable. The Rust side owns the authoritative counter
     * ({@code sys_types::CallId::next}); a nonzero id is mandatory (the engine
     * rejects call_id 0).
     */
    private static final AtomicLong FALLBACK_CALL_ID = new AtomicLong(1);

    static {
        // First-hit-wins ladder: system property → env var → bundled classpath
        // resource. See NativeLibraryLoader for the resolution + extraction logic.
        NativeLibraryLoader.load(LIB_PROPERTY, LIB_ENV_VAR);
    }

    private BamlFfi() {}

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

    /**
     * Run a BAML function asynchronously. Encodes the identical
     * {@code CallFunctionArgs} payload as {@link #nativeCallSync} but returns
     * immediately after spawning the engine call on the shared tokio runtime; the
     * {@code BamlOutboundResult} envelope is delivered later to
     * {@link #completeCall} on an engine thread, keyed by {@code callId}. The id
     * is passed explicitly (as well as being embedded in the encoded args) so the
     * completion is routed even if the args fail to decode. A thrown
     * {@code RuntimeException} means a JNI-glue failure before hand-off.
     */
    static native void nativeCallAsync(long callId, String fqn, byte[] encodedCallFunctionArgs);

    /** Mint a process-unique, nonzero function-call id from the engine counter. */
    static native long nativeNewCallId();

    // ---- Handle lifecycle + media (baml.media.*) ---------------------------
    // The JVM analog of bridge_python's media/handle FFI: media values are
    // minted as HANDLE_TABLE rows and referenced across JNI by their u64 key
    // (returned as a long). The `kind` argument is a proto MediaTypeEnum
    // discriminant (IMAGE=1, AUDIO=2, PDF=3, VIDEO=4, OTHER=5). A native failure
    // (bad kind, invalid key, alloc) throws an unchecked RuntimeException.

    /** Mint an `Adt(Media)` row from a URL; returns its handle key. */
    static native long nativeMediaFromUrl(int kind, String url, String mimeType);

    /** Mint an `Adt(Media)` row from a local file path; returns its handle key. */
    static native long nativeMediaFromFile(int kind, String path, String mimeType);

    /** Mint an `Adt(Media)` row from a base64 payload; returns its handle key. */
    static native long nativeMediaFromBase64(int kind, String base64, String mimeType);

    /** The media's source URL, or {@code null} when it is not URL-backed. */
    static native String nativeMediaUrl(long key);

    /** The media's local file path, or {@code null} when it is not file-backed. */
    static native String nativeMediaFile(long key);

    /** The media's base64 payload (never {@code null}; empty when unavailable). */
    static native String nativeMediaBase64(long key);

    /** The media's MIME type, or {@code null} when none is set. */
    static native String nativeMediaMimeType(long key);

    /** Clone a handle key, minting a fresh owned key for the same row. */
    static native long nativeHandleClone(long key);

    /** Release one owned handle key (best-effort; a stale key is ignored). */
    static native void nativeHandleRelease(long key);

    // ---- Public surface the generated SDK targets --------------------------

    /** Initialize the runtime from embedded bytecode (idempotent; replaces). */
    public static void initFromBytecode(byte[] bytecode) {
        nativeInitFromBytecode(bytecode);
    }

    /**
     * Synchronously call the BAML function {@code fqn}, passing {@code args}
     * paired positionally with their declared parameter {@code names}. Returns
     * the decoded value, or throws {@link BamlError} / {@link BamlPanic}.
     *
     * <p>Result decode is wire-driven (no return-type descriptor): a union result
     * reifies via the wire's {@code self_type} onto its registered nominal record.
     * Generated bindings that know their declared return type call the four-arg
     * {@link #callSync(String, String[], Object[], String)} instead.
     */
    public static Object callSync(String fqn, String[] names, Object[] args) {
        return callSync(fqn, names, args, null);
    }

    /**
     * As {@link #callSync(String, String[], Object[])}, but threads a
     * type-directed decode descriptor for the declared return type (see
     * {@code ref-java-codegen-conventions.md}). The generated SDK passes the
     * descriptor string as this last argument so a union result lands on the
     * {@code Union{k}} arm family (arm chosen from the declared arm order) and
     * nested class/list/map/union results decode against their declared shape.
     * A {@code null} descriptor is exactly the three-arg (wire-driven) behavior.
     */
    public static Object callSync(String fqn, String[] names, Object[] args, String returnDesc) {
        byte[] request = ProtoWriter.encodeCallFunctionArgs(names, args, newCallId());
        byte[] response = nativeCallSync(fqn, request);
        return decodeResult(response, returnDesc);
    }

    /**
     * Asynchronous sibling of {@link #callSync(String, String[], Object[])}. See
     * {@link #callAsync(String, String[], Object[], String)}.
     */
    public static CompletableFuture<Object> callAsync(String fqn, String[] names, Object[] args) {
        return callAsync(fqn, names, args, null);
    }

    /**
     * Asynchronous sibling of
     * {@link #callSync(String, String[], Object[], String)}: the real async path.
     * A process-unique {@code call_id} is minted, a raw-bytes future registered
     * in {@link #PENDING}, and {@link #nativeCallAsync} spawns the engine call and
     * returns without blocking. When the engine completes it delivers the
     * {@code BamlOutboundResult} envelope to {@link #completeCall} (on an engine
     * thread — {@link CompletableFuture} makes that safe, so no custom executor is
     * introduced), and the {@code thenApply} stage runs the SAME
     * {@link #decodeResult} the sync path uses, so the two cannot drift. A thrown
     * error/panic surfaces as this future completing exceptionally with
     * {@link BamlError} / {@link BamlPanic}.
     */
    public static CompletableFuture<Object> callAsync(
            String fqn, String[] names, Object[] args, String returnDesc) {
        long callId = newCallId();
        CompletableFuture<byte[]> raw = new CompletableFuture<>();
        PENDING.put(callId, raw);
        try {
            byte[] request = ProtoWriter.encodeCallFunctionArgs(names, args, callId);
            nativeCallAsync(callId, fqn, request);
        } catch (Throwable t) {
            // Arg-encode / JNI-glue failure before the engine took ownership of
            // the call: it will never call completeCall for this id, so
            // unregister and fail the future here rather than leak it. (A pre-call
            // *engine* failure — uninitialized runtime, bad args — instead rides
            // an error envelope through completeCall, exactly like callSync.)
            if (PENDING.remove(callId) != null) {
                raw.completeExceptionally(t);
            }
        }
        return raw.thenApply(bytes -> decodeResult(bytes, returnDesc));
    }

    /**
     * Engine-thread completion callback for {@link #nativeCallAsync}: resolves the
     * future registered under {@code callId} with the raw
     * {@code BamlOutboundResult} envelope bytes (the identical bytes
     * {@link #nativeCallSync} returns), then removes it from {@link #PENDING}.
     * Invoked by the native bridge after attaching the completing engine thread to
     * the JVM. Never throws: an unknown / already-removed id (a double delivery,
     * or a future cancellation once wired) is ignored — the property a later
     * {@code cancel_function_call} relies on. The {@code ok}/{@code error}/
     * {@code panic} decode runs later in the {@code thenApply} stage
     * {@link #callAsync} attached.
     */
    static void completeCall(long callId, byte[] resultEnvelope) {
        CompletableFuture<byte[]> future = PENDING.remove(callId);
        if (future != null) {
            future.complete(resultEnvelope);
        }
    }

    /**
     * The single result decode shared by {@link #callSync} and {@link #callAsync}:
     * the wire-and-descriptor-driven
     * {@link ProtoReader#decodeOutboundResult(byte[], String)} that turns the
     * {@code BamlOutboundResult} envelope into the decoded value, or throws
     * {@link BamlError} / {@link BamlPanic}. Factored so the sync and async paths
     * cannot diverge in how they interpret an identical envelope.
     */
    private static Object decodeResult(byte[] response, String returnDesc) {
        return ProtoReader.decodeOutboundResult(response, returnDesc);
    }

    private static long newCallId() {
        long id = nativeNewCallId();
        // Defensive: the engine rejects call_id 0, and the native counter starts
        // at 1, so this fallback should never trigger.
        return id != 0 ? id : FALLBACK_CALL_ID.getAndIncrement();
    }
}
