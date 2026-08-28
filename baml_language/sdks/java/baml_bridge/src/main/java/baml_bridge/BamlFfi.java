package baml_bridge;

import baml_bridge.internal.BamlTraceback;
import baml_bridge.internal.NativeLibraryLoader;
import baml_bridge.internal.ProtoReader;
import baml_bridge.internal.ProtoWriter;

import java.lang.reflect.InvocationHandler;
import java.lang.reflect.Proxy;
import java.util.List;
import java.util.Map;
import java.util.Objects;
import java.util.concurrent.CancellationException;
import java.util.concurrent.CompletableFuture;
import java.util.concurrent.CompletionException;
import java.util.concurrent.ConcurrentHashMap;
import java.util.concurrent.CopyOnWriteArrayList;
import java.util.concurrent.ExecutionException;
import java.util.concurrent.ExecutorService;
import java.util.concurrent.Executors;
import java.util.concurrent.ThreadFactory;
import java.util.concurrent.TimeUnit;
import java.util.concurrent.TimeoutException;
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
     * an opaque token) and cleared on completion, so {@code cancel_function_call}
     * targets the same id: a host {@code future.cancel(true)} fires the engine
     * cancel, and the late completion envelope that follows is a no-op
     * ({@link #completeCall} on an already-removed id is ignored, and
     * {@code complete()} on an already-cancelled future does nothing).
     */
    private static final ConcurrentHashMap<Long, CompletableFuture<byte[]>> PENDING =
            new ConcurrentHashMap<>();

    /**
     * The BAML FQN of the {@code baml.panics.Cancelled} class. An async call
     * whose panic envelope carries this class name is remapped from
     * {@link BamlPanic} to {@link BamlCancelledError} (matched by string so this
     * runtime library never references a generated class — see
     * {@link #mapAsyncFailure(Throwable)}). Kept in sync with Python's
     * {@code _CANCELLED_PANIC_CLASS}.
     */
    private static final String CANCELLED_PANIC_CLASS = "baml.panics.Cancelled";

    /**
     * Fallback call-id source, used only if the native counter is somehow
     * unavailable. The Rust side owns the authoritative counter
     * ({@code sys_types::CallId::next}); a nonzero id is mandatory (the engine
     * rejects call_id 0).
     */
    private static final AtomicLong FALLBACK_CALL_ID = new AtomicLong(1);

    /**
     * Java-side host-value registry: callables handed to BAML as function
     * arguments AND native throwables raised inside a callable, keyed by a
     * process-unique {@code long}. Rust is a pure router — it forwards a key back
     * to {@link #hostDispatch}/{@link #hostRelease}; storing the objects here (not
     * behind a JNI {@code GlobalRef}) makes identity a plain JVM reference, so a
     * rehydrated exception is {@code ==} the original (design points A + D).
     * Callables and thrown objects share one keyspace ({@link #HOST_VALUE_KEY}) so
     * keys never collide and a single {@link #hostRelease} clears either kind.
     */
    private static final ConcurrentHashMap<Long, Object> HOST_VALUES = new ConcurrentHashMap<>();

    /** Shared key source for {@link #HOST_VALUES}; starts at 1 (0 is reserved). */
    private static final AtomicLong HOST_VALUE_KEY = new AtomicLong(1);

    /**
     * Dedicated dispatch executor: BAML→host callable invocations run here, off
     * the engine's runtime threads and the JNI attach frame, so a slow or blocking
     * user callable cannot starve the engine and concurrent dispatches (allowed by
     * the C ABI) get real parallelism (design point B). Daemon threads so the pool
     * never keeps the JVM alive. A submitted callable must not synchronously
     * re-enter BAML (api.rs: no blocking re-entry).
     */
    private static final ExecutorService HOST_DISPATCH_EXECUTOR =
            Executors.newCachedThreadPool(hostDispatchThreadFactory());

    private static ThreadFactory hostDispatchThreadFactory() {
        AtomicLong seq = new AtomicLong(0);
        return runnable -> {
            Thread t = new Thread(runnable, "baml-host-dispatch-" + seq.getAndIncrement());
            t.setDaemon(true);
            return t;
        };
    }

    /** Empty payload: an {@code isError} completion with no bytes ⇒ BridgeFailure. */
    private static final byte[] EMPTY_PAYLOAD = new byte[0];

    /**
     * Best-effort telemetry-flush hooks, run immediately before an {@code os-exit}
     * panic hard-halts the process. A clean {@code baml.sys.exit} surfaces as an
     * exit panic that terminates the JVM via {@link Runtime#halt(int)} — bypassing
     * JVM shutdown hooks (the analog of Python's {@code os._exit}) — so any
     * buffered telemetry would otherwise be dropped. This is the flush socket the
     * spec calls for: registered hooks run just before the halt (see
     * {@link #runExitFlushHooks()}, invoked from the outbound decoder). No
     * telemetry ships in this slice, so the list is empty by default and the halt
     * behavior is unchanged. {@link CopyOnWriteArrayList} so registration and the
     * exit-time drain are safe under concurrency.
     */
    private static final CopyOnWriteArrayList<Runnable> EXIT_FLUSH_HOOKS =
            new CopyOnWriteArrayList<>();

    static {
        // First-hit-wins ladder: system property → env var → bundled classpath
        // resource. See NativeLibraryLoader for the resolution + extraction logic.
        NativeLibraryLoader.load(LIB_PROPERTY, LIB_ENV_VAR);
        Runtime.getRuntime()
                .addShutdownHook(new Thread(BamlFfi::nativeShutdownRuntime, "baml-runtime-shutdown"));
    }

    private BamlFfi() {}

    // ---- os-exit telemetry-flush hooks --------------------------------------

    /**
     * Register a best-effort hook to run just before an {@code os-exit} panic
     * hard-halts the process — the place to flush a telemetry buffer that JVM
     * shutdown hooks would otherwise miss (the halt bypasses them, mirroring
     * Python's {@code os._exit}). Hooks run in registration order; a hook's
     * exception is swallowed, because nothing may prevent (or delay) the halt.
     * Registering the same {@link Runnable} twice runs it twice. Thread-safe.
     */
    public static void registerExitFlushHooks(Runnable hook) {
        EXIT_FLUSH_HOOKS.add(java.util.Objects.requireNonNull(hook, "hook"));
    }

    /**
     * Run every registered {@linkplain #registerExitFlushHooks exit-flush hook},
     * best-effort: each hook's {@link Throwable} is caught and swallowed so a
     * misbehaving hook can neither prevent nor reorder the process halt. The
     * outbound decoder calls this on an {@code os-exit} panic immediately before
     * {@link Runtime#halt(int)}. Factored out of the halt itself so the hook
     * mechanics stay unit-testable without terminating the test JVM.
     */
    public static void runExitFlushHooks() {
        for (Runnable hook : EXIT_FLUSH_HOOKS) {
            try {
                hook.run();
            } catch (Throwable ignored) {
                // Best-effort: nothing a hook does may hold up the halt.
            }
        }
    }

    // ---- Native methods (implemented in sdks/java/bridge_java) --------------

    /** Initialize the process-global runtime from serialized BAML bytecode. */
    static native void nativeInitFromBytecode(
            byte[] bytecode,
            String embeddedBamlToml,
            String bridgeRuntimeVersion,
            String toolchainVersion);

    static native void nativeShutdownRuntime();

    /** Wait for spawned work, report unreachable errors, and release the runtime. */
    public static void shutdownRuntime() {
        nativeShutdownRuntime();
    }

    /**
     * Run a BAML function synchronously. Takes a protobuf-encoded
     * {@code CallFunctionArgs} and returns protobuf-encoded
     * {@code BamlOutboundResult} bytes (engine errors/panics ride inside those
     * bytes; a thrown {@code RuntimeException} means a JNI-glue failure).
     */
    static native byte[] nativeCallSync(byte[] encodedCallFunctionArgs);

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
    static native void nativeCallAsync(long callId, byte[] encodedCallFunctionArgs);

    /** Mint a process-unique, nonzero function-call id from the engine counter. */
    static native long nativeNewCallId();

    /**
     * Cancel an in-flight function call by its {@code call_id}
     * ({@code bridge_cffi::cancel_function_call_by_id}). Returns {@code true}
     * when the runtime accepted the cancel, {@code false} otherwise (unknown /
     * already-completed id, id 0, or an uninitialized runtime). Never throws —
     * both {@link BamlCallContext#abort()} and a host {@code future.cancel(true)}
     * fire it and tolerate a {@code false}.
     */
    static native boolean nativeCancelFunctionCall(long callId);

    /**
     * Complete an in-flight host-callable dispatch
     * ({@code bridge_cffi::complete_host_call}). {@code content} is a
     * protobuf-encoded {@code InboundValue} (the callable's result on success, or
     * the thrown value on error); an empty {@code content} with {@code isError}
     * true is the bridge-failure signal. Called from the dispatch executor once a
     * host callable finishes; unknown / cancelled call ids are ignored
     * engine-side.
     */
    static native void nativeCompleteHostCall(long callId, boolean isError, byte[] content);

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

    /** Set a process env var so the in-process engine (native getenv) observes it. */
    static native void nativeEnvSet(String name, String value);

    /** Remove a process env var (teardown half of {@link #nativeEnvSet}). */
    static native void nativeEnvUnset(String name);

    // ---- Public surface the generated SDK targets --------------------------

    /** Initialize the runtime from embedded bytecode (idempotent; replaces). */
    public static void initFromBytecode(byte[] bytecode) {
        nativeInitFromBytecode(
                bytecode,
                null,
                BamlVersion.BRIDGE_RUNTIME_VERSION,
                BamlVersion.TOOLCHAIN_VERSION);
    }

    public static void initFromBytecode(byte[] bytecode, String embeddedBamlToml) {
        nativeInitFromBytecode(
                bytecode,
                embeddedBamlToml,
                BamlVersion.BRIDGE_RUNTIME_VERSION,
                BamlVersion.TOOLCHAIN_VERSION);
    }

    public static String getToolchainVersion() {
        return BamlVersion.TOOLCHAIN_VERSION;
    }

    public static String getBridgeRuntimeVersion() {
        return BamlVersion.BRIDGE_RUNTIME_VERSION;
    }

    /**
     * Synchronously call the BAML function {@code fqn}, passing {@code args}
     * paired positionally with their declared parameter {@code names}. Returns
     * the decoded value, or throws {@link BamlError} / {@link BamlPanic}.
     *
     * <p>Result decode is wire-driven (no return-type descriptor): a union result
     * reifies via the wire's {@code self_type} onto its registered nominal record.
     * Generated bindings that know their declared return type call the four-arg
     * {@link #callSync(String, String[], Object[], BamlType)} instead.
     */
    public static Object callSync(String fqn, String[] names, Object[] args) {
        return callSync(fqn, names, args, null, null);
    }

    public static Object callHandleSync(
            BamlHandle handle, String[] names, Object[] args, BamlType returnDesc) {
        Objects.requireNonNull(handle, "handle");
        Objects.requireNonNull(names, "names");
        Objects.requireNonNull(args, "args");
        if (handle.handleType() != BamlHandle.FUNCTION_REF) {
            throw new IllegalArgumentException(
                    "expected a FUNCTION_REF handle, received " + handle.handleType());
        }
        if (names.length != args.length) {
            throw new IllegalArgumentException("function handle argument names and values differ in length");
        }
        long callId = newCallId();
        byte[] request =
                ProtoWriter.encodeHandleCallFunctionArgs(handle.key(), names, args, callId);
        return decodeResult(nativeCallSync(request), returnDesc);
    }

    public static <T> T returnedClosure(
            Class<T> callableType,
            Object value,
            String[] names,
            BamlType returnDesc) {
        Objects.requireNonNull(callableType, "callableType");
        if (!(value instanceof BamlHandle handle)) {
            throw new IllegalStateException(
                    "Expected a returned BAML function handle, received "
                            + (value == null ? "null" : value.getClass().getName()));
        }
        InvocationHandler handler =
                (proxy, method, arguments) -> {
                    if (method.getDeclaringClass() == Object.class) {
                        return switch (method.getName()) {
                            case "toString" -> "BAML closure " + handle.key();
                            case "hashCode" -> System.identityHashCode(proxy);
                            case "equals" -> proxy == arguments[0];
                            default -> throw new UnsupportedOperationException(method.toString());
                        };
                    }
                    Object[] supplied = arguments == null ? new Object[0] : arguments;
                    if (supplied.length != names.length) {
                        throw new IllegalArgumentException(
                                "BAML closure expected "
                                        + names.length
                                        + " arguments, received "
                                        + supplied.length);
                    }
                    return callHandleSync(handle, names, supplied, returnDesc);
                };
        Object proxy =
                Proxy.newProxyInstance(
                        callableType.getClassLoader(),
                        new Class<?>[] {callableType},
                        handler);
        return callableType.cast(proxy);
    }

    public static <T> CompletableFuture<T> returnedClosureAsync(
            CompletableFuture<Object> source,
            Class<T> callableType,
            String[] names,
            BamlType returnDesc) {
        Objects.requireNonNull(source, "source");
        CompletableFuture<T> mapped = new CompletableFuture<>();
        source.whenComplete(
                (value, error) -> {
                    if (error != null) {
                        mapped.completeExceptionally(error);
                    } else {
                        try {
                            mapped.complete(returnedClosure(callableType, value, names, returnDesc));
                        } catch (Throwable failure) {
                            mapped.completeExceptionally(failure);
                        }
                    }
                });
        mapped.whenComplete(
                (ignored, error) -> {
                    if (mapped.isCancelled()) {
                        source.cancel(true);
                    }
                });
        return mapped;
    }

    /**
     * As {@link #callSync(String, String[], Object[])}, but threads a
     * type-directed decode descriptor for the declared return type (see
     * {@code ref-java-codegen-conventions.md}). The generated SDK passes the
     * descriptor {@link BamlType} as this last argument so a union result lands on the
     * {@code Union{k}} arm family (arm chosen from the declared arm order) and
     * nested class/list/map/union results decode against their declared shape.
     * A {@code null} descriptor is exactly the three-arg (wire-driven) behavior.
     */
    public static Object callSync(String fqn, String[] names, Object[] args, BamlType returnDesc) {
        return callSync(fqn, names, args, returnDesc, null);
    }

    /**
     * As {@link #callSync(String, String[], Object[], BamlType)}, but bound to an
     * optional {@link BamlCallContext} for cancellation: the minted
     * {@code call_id} is attached to {@code ctx} for the duration of the call and
     * detached in a {@code finally}, so a concurrent {@link BamlCallContext#abort()}
     * (or an abort that already happened) cancels this call engine-side. A sync
     * cancellation surfaces as a {@link BamlPanic} whose {@code value()} is a
     * {@code baml.panics.Cancelled} (the async path remaps that to
     * {@link BamlCancelledError}; sync keeps the panic). A {@code null} {@code ctx}
     * is exactly the four-arg behavior.
     */
    public static Object callSync(
            String fqn, String[] names, Object[] args, BamlType returnDesc, BamlCallContext ctx) {
        return callSync(fqn, names, args, returnDesc, ctx, null);
    }

    /**
     * Explicit-generics variant of
     * {@link #callSync(String, String[], Object[], BamlType, BamlCallContext)}:
     * threads a {@link BamlTypes} bag of TypeVar bindings into
     * {@code CallFunctionArgs.type_args} so a generic function/method's TypeVars
     * are bound at the call. Reached from the generated explicit-generics
     * overloads; a {@code null} bag is exactly the five-arg (non-generic)
     * behavior, and a {@code null} {@code ctx} is exactly the four-arg behavior.
     */
    public static Object callSync(
            String fqn,
            String[] names,
            Object[] args,
            BamlType returnDesc,
            BamlCallContext ctx,
            BamlTypes typeArgs) {
        return callSyncOperation(
                fqn,
                names,
                args,
                returnDesc,
                ctx,
                typeArgs,
                BamlFunctionOperation.DIRECT);
    }

    public static Object callSyncOperation(
            String fqn,
            String[] names,
            Object[] args,
            BamlType returnDesc,
            BamlCallContext ctx,
            BamlTypes typeArgs,
            BamlFunctionOperation operation) {
        long callId = newCallId();
        if (ctx != null) {
            ctx.attach(callId);
        }
        try {
            byte[] request =
                    ProtoWriter.encodeNamedCallFunctionArgs(
                            fqn, names, args, callId, typeArgs, operation);
            byte[] response = nativeCallSync(request);
            return decodeResult(response, returnDesc);
        } finally {
            if (ctx != null) {
                ctx.detach(callId);
            }
        }
    }

    /**
     * Asynchronous sibling of {@link #callSync(String, String[], Object[])}. See
     * {@link #callAsync(String, String[], Object[], BamlType)}.
     */
    public static CompletableFuture<Object> callAsync(String fqn, String[] names, Object[] args) {
        return callAsync(fqn, names, args, null, null);
    }

    /**
     * Asynchronous sibling of
     * {@link #callSync(String, String[], Object[], BamlType)}. See
     * {@link #callAsync(String, String[], Object[], BamlType, BamlCallContext)}.
     */
    public static CompletableFuture<Object> callAsync(
            String fqn, String[] names, Object[] args, BamlType returnDesc) {
        return callAsync(fqn, names, args, returnDesc, null);
    }

    /**
     * The real async path, bound to an optional {@link BamlCallContext}. A
     * process-unique {@code call_id} is minted, a raw-bytes future registered in
     * {@link #PENDING}, and {@link #nativeCallAsync} spawns the engine call and
     * returns without blocking. When the engine completes it delivers the
     * {@code BamlOutboundResult} envelope to {@link #completeCall} (on an engine
     * thread — {@link CompletableFuture} makes that safe), whose {@code whenComplete}
     * hook runs the SAME {@link #decodeResult} the sync path uses (so the two
     * cannot drift) and settles the returned future.
     *
     * <p>The returned object is a {@link CancellableCall} that owns its
     * {@code call_id}: {@code future.cancel(true)} fires the engine cancel for that
     * id before marking the future cancelled, so a host cancellation actually stops
     * the engine call (not just the JVM-side future). Because decode happens inside
     * this method rather than in a derived {@code thenApply} stage, the returned
     * future — the one the caller can cancel — is the one wired to the engine.
     *
     * <p>Cancellation surfacing differs from sync: an engine-driven abort produces
     * a {@code baml.panics.Cancelled} panic which is remapped to
     * {@link BamlCancelledError} (a {@link java.util.concurrent.CancellationException}),
     * so {@code future.isCancelled()} is true and {@code join()}/{@code get()}
     * surface it directly. Other thrown errors/panics complete the future
     * exceptionally with {@link BamlError} / {@link BamlPanic} unchanged.
     */
    public static CompletableFuture<Object> callAsync(
            String fqn, String[] names, Object[] args, BamlType returnDesc, BamlCallContext ctx) {
        return callAsync(fqn, names, args, returnDesc, ctx, null);
    }

    /**
     * Explicit-generics variant of
     * {@link #callAsync(String, String[], Object[], BamlType, BamlCallContext)}:
     * threads a {@link BamlTypes} bag of TypeVar bindings into
     * {@code CallFunctionArgs.type_args}. Reached from the generated
     * explicit-generics overloads; a {@code null} bag is exactly the five-arg
     * (non-generic) behavior, and a {@code null} {@code ctx} is exactly the
     * four-arg behavior.
     */
    public static CompletableFuture<Object> callAsync(
            String fqn,
            String[] names,
            Object[] args,
            BamlType returnDesc,
            BamlCallContext ctx,
            BamlTypes typeArgs) {
        return callAsyncOperation(
                fqn,
                names,
                args,
                returnDesc,
                ctx,
                typeArgs,
                BamlFunctionOperation.DIRECT);
    }

    public static CompletableFuture<Object> callAsyncOperation(
            String fqn,
            String[] names,
            Object[] args,
            BamlType returnDesc,
            BamlCallContext ctx,
            BamlTypes typeArgs,
            BamlFunctionOperation operation) {
        long callId = newCallId();
        CompletableFuture<byte[]> raw = new CompletableFuture<>();
        PENDING.put(callId, raw);
        CancellableCall result = new CancellableCall(callId);
        if (ctx != null) {
            ctx.attach(callId);
        }
        try {
            byte[] request =
                    ProtoWriter.encodeNamedCallFunctionArgs(
                            fqn, names, args, callId, typeArgs, operation);
            nativeCallAsync(callId, request);
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
        // Decode + settle the caller-visible future. `whenComplete` fires whether
        // `raw` was completed by the engine (completeCall) or by the catch above,
        // and runs immediately if `raw` is already done. `complete*` on an
        // already-cancelled `result` (host `future.cancel(true)`) is a no-op, so a
        // late engine envelope after a host cancel is harmlessly dropped.
        raw.whenComplete(
                (bytes, err) -> {
                    if (ctx != null) {
                        ctx.detach(callId);
                    }
                    if (err != null) {
                        result.completeExceptionally(err);
                    } else {
                        try {
                            result.complete(decodeResult(bytes, returnDesc));
                        } catch (Throwable t) {
                            result.completeExceptionally(mapAsyncFailure(t));
                        }
                    }
                });
        return result;
    }

    /**
     * A caller-visible async future that owns its engine {@code call_id}. A host
     * {@code future.cancel(true)} fires {@link #nativeCancelFunctionCall} for that
     * id before the standard {@link CompletableFuture#cancel} bookkeeping, so the
     * engine call is actually stopped; the engine's late completion envelope then
     * no-ops against the already-cancelled future.
     *
     * <p>When an engine-driven abort completes this future exceptionally with a
     * {@link BamlCancelledError} (a {@link CancellationException}),
     * {@link #isCancelled()} is true and the {@code join()} / {@code get()} family
     * surface that {@code BamlCancelledError} <em>directly</em>. On JDK 19+ the base
     * {@code CompletableFuture} re-wraps a stored {@code CancellationException} in a
     * fresh one at report time (JDK ≤17 threw it as-is), so these overrides unwrap
     * back to the original — the design's contract ("{@code join()}/{@code get()}
     * throw it directly, unwrapped") held across JDKs. A host {@code cancel(true)}
     * (whose stored value is a plain {@code CancellationException}) still surfaces
     * as a {@code CancellationException}.
     */
    private static final class CancellableCall extends CompletableFuture<Object> {
        private final long callId;

        CancellableCall(long callId) {
            this.callId = callId;
        }

        @Override
        public boolean cancel(boolean mayInterruptIfRunning) {
            nativeCancelFunctionCall(callId);
            return super.cancel(mayInterruptIfRunning);
        }

        @Override
        public Object join() {
            try {
                return super.join();
            } catch (CancellationException e) {
                throw unwrapCancellation(e);
            }
        }

        @Override
        public Object get() throws InterruptedException, ExecutionException {
            try {
                return super.get();
            } catch (CancellationException e) {
                throw unwrapCancellation(e);
            }
        }

        @Override
        public Object get(long timeout, TimeUnit unit)
                throws InterruptedException, ExecutionException, TimeoutException {
            try {
                return super.get(timeout, unit);
            } catch (CancellationException e) {
                throw unwrapCancellation(e);
            }
        }

        @Override
        public Object getNow(Object valueIfAbsent) {
            try {
                return super.getNow(valueIfAbsent);
            } catch (CancellationException e) {
                throw unwrapCancellation(e);
            }
        }

        /**
         * Recover the original {@link BamlCancelledError} from a
         * report-time-wrapped {@link CancellationException} (JDK 19+ wraps the
         * stored cancellation in a fresh one, carrying the original as its cause).
         * A host {@code cancel(true)} has no {@code BamlCancelledError} cause, so it
         * is returned unchanged as a {@code CancellationException}.
         */
        private static CancellationException unwrapCancellation(CancellationException wrapped) {
            return wrapped.getCause() instanceof BamlCancelledError cancelled ? cancelled : wrapped;
        }
    }

    /**
     * Async-only remap of an engine-driven cancellation: a {@link BamlPanic}
     * carrying the {@code baml.panics.Cancelled} class (matched by name so this
     * library never touches a generated class) becomes a {@link BamlCancelledError}
     * — a {@link java.util.concurrent.CancellationException} — so the future reads
     * as cancelled. Every other throwable passes through unchanged (the sync path
     * never calls this, so it keeps the raw {@link BamlPanic}).
     */
    private static Throwable mapAsyncFailure(Throwable t) {
        if (t instanceof BamlPanic panic && CANCELLED_PANIC_CLASS.equals(panic.class_name())) {
            // Prepend the BAML frames onto the freshly-minted cancellation, the
            // same synthetic-stack splice decodeError/decodePanic apply (the
            // remap builds a new exception, so it re-splices rather than
            // inheriting the panic's stack).
            return BamlTraceback.splice(
                    new BamlCancelledError(panic.value(), panic.baml_trace(), panic.class_name()),
                    panic.baml_trace());
        }
        return t;
    }

    /**
     * Engine-thread completion callback for {@link #nativeCallAsync}: resolves the
     * raw-bytes future registered under {@code callId} with the raw
     * {@code BamlOutboundResult} envelope bytes (the identical bytes
     * {@link #nativeCallSync} returns), then removes it from {@link #PENDING}.
     * Invoked by the native bridge after attaching the completing engine thread to
     * the JVM. Never throws: an unknown / already-removed id (a double delivery, or
     * a host cancellation that already dropped the entry) is ignored. The
     * {@code ok}/{@code error}/{@code panic} decode runs later in the
     * {@code whenComplete} hook {@link #callAsync} attached.
     */
    static void completeCall(long callId, byte[] resultEnvelope) {
        CompletableFuture<byte[]> future = PENDING.remove(callId);
        if (future != null) {
            future.complete(resultEnvelope);
        }
    }

    static void unhandledSpawnError(byte[] errorEnvelope, boolean cancelled) {
        UnhandledSpawnErrors.report(errorEnvelope, cancelled);
    }

    // ---- Host-callable registry + dispatch ---------------------------------
    // A BAML function parameter typed as a callable receives a host function;
    // mid-call the engine dispatches back into the host to invoke it. Encode
    // registers the callable here and emits a Handle{HOST_VALUE_CALLABLE}; the
    // engine's dispatch fires the Rust trampoline, which lands on hostDispatch;
    // the result (or thrown value) flows back via nativeCompleteHostCall.

    /**
     * Register a host callable handed to BAML as a function argument; returns its
     * process-unique key (emitted as {@code Handle{key, HOST_VALUE_CALLABLE}} by
     * {@code ProtoWriter}). The engine binds the key to an {@code Object::HostClosure}
     * and dispatches back through {@link #hostDispatch} when BAML invokes it.
     */
    public static long registerHostCallable(Object callable) {
        long key = HOST_VALUE_KEY.getAndIncrement();
        HOST_VALUES.put(key, callable);
        return key;
    }

    /**
     * Register an arbitrary host object — a native {@link Throwable} raised inside
     * a callable — so a {@code baml.errors.HostCallable}'s {@code _handle} can
     * resolve back to the original object by identity on round trip. Shares the
     * keyspace with callables so keys never collide.
     */
    private static long registerHostOpaque(Object value) {
        long key = HOST_VALUE_KEY.getAndIncrement();
        HOST_VALUES.put(key, value);
        return key;
    }

    /**
     * Resolve a host-value key back to its object (callable or thrown), or
     * {@code null} when the key is absent — released, or foreign to this runtime.
     * Used by {@code ProtoReader} to rehydrate a host-thrown exception by
     * identity, and never mints or mutates state.
     */
    public static Object lookupHostValue(long key) {
        return HOST_VALUES.get(key);
    }

    /**
     * Engine-driven release: drop the registry entry for {@code key} (the engine
     * dropped the last reference to the host value). Called from the Rust release
     * trampoline; a stale/absent key is a harmless no-op. Rarely fires per call
     * (GC-driven) — hence the {@code @Disabled} release test.
     */
    static void hostRelease(long key) {
        HOST_VALUES.remove(key);
    }

    /**
     * Engine→host callable dispatch entry point, called from the Rust dispatch
     * trampoline. Submits the decode / invoke / complete work to the dedicated
     * executor and returns promptly (api.rs: the callback must return promptly;
     * dispatches may be concurrent). Never throws across the JNI boundary.
     */
    static void hostDispatch(long hostValueKey, long callId, byte[] bamlToHostCall) {
        try {
            HOST_DISPATCH_EXECUTOR.execute(
                    () -> runHostDispatch(hostValueKey, callId, bamlToHostCall));
        } catch (Throwable t) {
            // Could not even schedule (executor shut down / resource exhaustion):
            // complete with a bridge failure so the engine never awaits forever.
            safeComplete(callId, true, EMPTY_PAYLOAD);
        }
    }

    /**
     * Run one dispatch on the executor: resolve the callable, decode the args,
     * invoke, and complete the call with the result (awaiting a returned
     * {@link CompletableFuture}) or the thrown value. Wrapped so a dropped
     * dispatch can never leave the engine awaiting: any thrown value routes to
     * {@link #completeHostError}, and the call is always completed exactly once.
     *
     * <p>A callable registered wrapped in a {@link BamlTypedCallable} (the
     * generated SDK's carrier) is unwrapped here: its declared parameter
     * descriptors drive a type-directed arg decode — so the user's typed lambda
     * receives the generated types its slot declares (e.g. the sealed
     * {@code baml.json.json} union), never the raw wire value — and its return
     * descriptor types the encode of the result.
     */
    private static void runHostDispatch(long hostValueKey, long callId, byte[] bamlToHostCall) {
        Object registered = HOST_VALUES.get(hostValueKey);
        if (registered == null) {
            // The engine dispatched a key the bridge no longer holds — a bridge
            // fault, not a user exception → BridgeFailure (empty error payload).
            safeComplete(callId, true, EMPTY_PAYLOAD);
            return;
        }
        BamlTypedCallable typed = registered instanceof BamlTypedCallable t ? t : null;
        Object callable = typed != null ? typed.callable() : registered;
        BamlType returnDesc = typed != null ? typed.returnDesc() : null;
        Object result;
        try {
            ProtoReader.HostCallArgs args = typed != null
                    ? ProtoReader.decodeBamlToHostCall(
                            bamlToHostCall,
                            typed.positionalDescs(),
                            typed.optionalNames(),
                            typed.optionalDescs())
                    : ProtoReader.decodeBamlToHostCall(bamlToHostCall);
            result = invokeHostCallable(callable, args);
        } catch (Throwable userError) {
            completeHostError(callId, userError);
            return;
        }
        // Async detection at the VALUE level (design point C): await a returned
        // CompletableFuture, then encode; anything else encodes immediately. No
        // typed async surface yet — this future-proofs the Object-typed slot.
        if (result instanceof CompletableFuture<?> future) {
            future.whenComplete(
                    (value, err) -> {
                        if (err != null) {
                            completeHostError(callId, unwrapCompletion(err));
                        } else {
                            completeHostSuccess(callId, value, returnDesc);
                        }
                    });
        } else {
            completeHostSuccess(callId, result, returnDesc);
        }
    }

    /**
     * Invoke {@code callable} with the reshaped args — the invoke ladder. A
     * generated {@link BamlHostCallable} folds the required + optional buckets
     * back into its SAM ({@code __bamlDispatch}); the {@code java.util.function.*}
     * shapes take their positional args directly (codegen only uses them for plain
     * arity-&le;-2 all-required callables, so their optional bucket is empty).
     */
    private static Object invokeHostCallable(Object callable, ProtoReader.HostCallArgs args) {
        List<Object> pos = args.positional;
        if (callable instanceof BamlHostCallable hc) {
            return hc.__bamlDispatch(pos, args.optional);
        }
        if (callable instanceof java.util.function.Function<?, ?>) {
            @SuppressWarnings("unchecked")
            java.util.function.Function<Object, Object> f =
                    (java.util.function.Function<Object, Object>) callable;
            return f.apply(pos.get(0));
        }
        if (callable instanceof java.util.function.BiFunction<?, ?, ?>) {
            @SuppressWarnings("unchecked")
            java.util.function.BiFunction<Object, Object, Object> f =
                    (java.util.function.BiFunction<Object, Object, Object>) callable;
            return f.apply(pos.get(0), pos.get(1));
        }
        if (callable instanceof java.util.function.Supplier<?> s) {
            return s.get();
        }
        if (callable instanceof java.util.function.Consumer<?>) {
            @SuppressWarnings("unchecked")
            java.util.function.Consumer<Object> c = (java.util.function.Consumer<Object>) callable;
            c.accept(pos.get(0));
            return null;
        }
        if (callable instanceof java.util.function.BiConsumer<?, ?>) {
            @SuppressWarnings("unchecked")
            java.util.function.BiConsumer<Object, Object> c =
                    (java.util.function.BiConsumer<Object, Object>) callable;
            c.accept(pos.get(0), pos.get(1));
            return null;
        }
        if (callable instanceof Runnable r) {
            r.run();
            return null;
        }
        // Defensive: only callables are registered, so this is unreachable.
        throw new IllegalStateException(
                "host-value key resolved to a non-callable object: " + callable.getClass());
    }

    /**
     * Encode a successful callable result and complete the call.
     * {@code returnDesc} (nullable) is the declared return-type descriptor a
     * {@link BamlTypedCallable} carries; it types the encode exactly like a
     * generated binding's argument descriptor ({@link BamlTypedValue}).
     */
    private static void completeHostSuccess(long callId, Object value, BamlType returnDesc) {
        byte[] payload;
        try {
            Object encodable =
                    returnDesc != null && value != null
                            ? new BamlTypedValue(value, returnDesc)
                            : value;
            payload = ProtoWriter.encodeInboundValue(encodable);
        } catch (Throwable encodeError) {
            // The callable returned a value the encoder can't map to a BAML value:
            // a bridge fault (the declared return type should have matched).
            safeComplete(callId, true, EMPTY_PAYLOAD);
            return;
        }
        safeComplete(callId, false, payload);
    }

    /**
     * Complete the call with a thrown value, via the three-way error branch
     * (design points D + F):
     *
     * <ul>
     *   <li>A {@link BamlError} / {@link BamlPanic} wrapping a codegenned BAML
     *       value → unwrap {@code .value()} and encode it as its real BAML class,
     *       so BAML's typed {@code catch (e: ValidationError)} matches
     *       structurally.
     *   <li>Anything else → register the {@link Throwable} under a fresh key and
     *       encode a {@code baml.errors.HostCallable} carrying {@code _handle}, so
     *       the decoder rehydrates the SAME object by identity on round trip.
     * </ul>
     */
    private static void completeHostError(long callId, Throwable error) {
        Object unwrapped = null;
        if (error instanceof BamlError be) {
            unwrapped = be.value();
        } else if (error instanceof BamlPanic bp) {
            unwrapped = bp.value();
        }
        if (unwrapped != null) {
            try {
                byte[] payload = ProtoWriter.encodeInboundValue(unwrapped);
                safeComplete(callId, true, payload);
                return;
            } catch (Throwable notABamlValue) {
                // Not encodable as a BAML value — fall through to the opaque path.
            }
        }
        long handleKey = registerHostOpaque(error);
        String className = error.getClass().getSimpleName();
        String message = error.getMessage() != null ? error.getMessage() : error.toString();
        byte[] payload =
                ProtoWriter.encodeHostCallableError(className, message, renderTraceback(error), handleKey);
        safeComplete(callId, true, payload);
    }

    /** Render a throwable's stack trace as a string for the {@code HostCallable.traceback} field. */
    private static String renderTraceback(Throwable error) {
        java.io.StringWriter sw = new java.io.StringWriter();
        try (java.io.PrintWriter pw = new java.io.PrintWriter(sw)) {
            error.printStackTrace(pw);
        }
        return sw.toString();
    }

    /**
     * A {@code whenComplete} error is a {@link CompletionException} wrapping the
     * real cause; unwrap it so the callable's actual exception drives the error
     * branch (and round-trips by identity).
     */
    private static Throwable unwrapCompletion(Throwable err) {
        return err instanceof CompletionException && err.getCause() != null
                ? err.getCause()
                : err;
    }

    /** Complete a host call, swallowing any native-completion failure (nothing more to do). */
    private static void safeComplete(long callId, boolean isError, byte[] content) {
        try {
            nativeCompleteHostCall(callId, isError, content);
        } catch (Throwable ignored) {
            // The native completion is written never to throw; if it somehow does,
            // the executor task must not propagate it.
        }
    }

    /**
     * The single result decode shared by {@link #callSync} and {@link #callAsync}:
     * the wire-and-descriptor-driven
     * {@link ProtoReader#decodeOutboundResult(byte[], BamlType)} that turns the
     * {@code BamlOutboundResult} envelope into the decoded value, or throws
     * {@link BamlError} / {@link BamlPanic}. Factored so the sync and async paths
     * cannot diverge in how they interpret an identical envelope. The async path's
     * Cancelled remap ({@link #mapAsyncFailure}) is layered on top of this shared
     * decode, not baked into it, so sync keeps the raw {@link BamlPanic}.
     */
    private static Object decodeResult(byte[] response, BamlType returnDesc) {
        return ProtoReader.decodeOutboundResult(response, returnDesc);
    }

    private static long newCallId() {
        long id = nativeNewCallId();
        // Defensive: the engine rejects call_id 0, and the native counter starts
        // at 1, so this fallback should never trigger.
        return id != 0 ? id : FALLBACK_CALL_ID.getAndIncrement();
    }
}
