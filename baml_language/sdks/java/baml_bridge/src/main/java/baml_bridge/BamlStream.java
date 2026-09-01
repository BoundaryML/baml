package baml_bridge;

import java.util.concurrent.CompletableFuture;

/**
 * Runtime-owned streaming wrapper around an {@code ai.stream.Stream} handle — the
 * Java analog of {@code bridge_python}'s {@code BamlStream} ({@code _stream.py}).
 *
 * <p>Holds a single {@link BamlHandle} whose {@code HANDLE_TABLE} row is an
 * {@code Adt(TaggedHeapHandle{ty, heap_handle})} (wire tag
 * {@code ADT_TAGGED_HEAP_HANDLE}). {@link #next()} / {@link #get_final()} (and
 * their {@code _async} siblings) re-enter the engine via ordinary
 * {@link BamlFfi#callSync}/{@link BamlFfi#callAsync} on the stdlib functions
 * method FQNs derived from the tagged handle's carried class identity, passing
 * this wrapper as the {@code self} receiver — exactly like any codegen-emitted
 * instance method. The args encoder emits a {@code handle_value} for the
 * receiver (see {@code ProtoWriter}), the engine substitutes the generics
 * {@code TPartial} / {@code TFinal}, and the result decodes typed.
 *
 * <p>{@code get_final} / {@code get_final_async} escape Python's
 * {@code final} / {@code final_async}: {@code final} is a Java reserved word, so
 * per the OWNER decision (2026-07-18) the getter is spelled {@code get_final}
 * (an explicit override of the {@code $}-escape default; documented in the
 * conventions doc).
 *
 * <p>Exhaustion contract (Python parity): {@link #next()} returns partial values
 * until it returns an {@link baml_sdk.ai.stream.Done} VALUE — no
 * {@code null} sentinel, no exception. Because Java generics cannot express
 * {@code TPartial | Done}, the declared return is {@code TPartial}
 * (erased to {@code Object}); callers dispatch with
 * {@code if (v instanceof Done)} — the faithful port of Python's sentinel
 * duck-typing.
 *
 * @param <TPartial> the partial (in-flight) element type
 * @param <TFinal>   the final (completed) value type
 */
public final class BamlStream<TPartial, TFinal> {
    /** The single positional arg name for the receiver, mirroring Python's {@code {"self": self}}. */
    private static final String[] SELF_NAMES = {"self"};

    private final BamlHandle handle;
    private final String classFqn;

    private BamlStream(BamlHandle handle) {
        if (handle.classFqn() == null || handle.classFqn().isBlank()) {
            throw new IllegalArgumentException(
                    "tagged stream handle is missing its carried BAML class identity");
        }
        this.handle = handle;
        this.classFqn = handle.classFqn();
    }

    /**
     * Wrap a decoded engine handle (used by the wire codec's
     * {@code ADT_TAGGED_HEAP_HANDLE} decode arm). The engine minted {@code handle}
     * as the streaming call's result; this wrapper now owns that row.
     */
    public static BamlStream<?, ?> fromHandle(BamlHandle handle) {
        return new BamlStream<>(handle);
    }

    /**
     * The engine handle backing this stream. Read by {@code ProtoWriter}'s
     * {@code BamlStream} encode arm to emit the receiver as a
     * {@code handle_value(ADT_TAGGED_HEAP_HANDLE)} with a freshly cloned wire key
     * (the engine drains its copy on decode; this wrapper keeps its own row so it
     * stays valid across {@code next}/{@code final} calls).
     */
    public BamlHandle bamlHandle() {
        return handle;
    }

    /** The exact BAML class identity carried by the tagged handle. */
    public String bamlClassFqn() {
        return classFqn;
    }

    /** Package-private seam used by codec tests; runtime calls use this exact FQN. */
    String methodFqn(String method) {
        return classFqn + "." + method;
    }

    /**
     * The next partial value, or an {@link baml_sdk.ai.stream.Done}
     * value once the stream is exhausted. The result is decoded wire-driven (null
     * descriptor): a partial is a registered PPIR {@code $stream} model (or a bare
     * primitive), and {@code Done} is a registered runtime class.
     */
    @SuppressWarnings("unchecked")
    public TPartial next() {
        return (TPartial) BamlFfi.callSync(methodFqn("next"), SELF_NAMES, new Object[] {this}, null);
    }

    /** Asynchronous sibling of {@link #next()}. */
    @SuppressWarnings("unchecked")
    public CompletableFuture<TPartial> next_async() {
        return (CompletableFuture<TPartial>) (CompletableFuture<?>)
                BamlFfi.callAsync(methodFqn("next"), SELF_NAMES, new Object[] {this}, null);
    }

    /** The stream's final (completed) value. */
    @SuppressWarnings("unchecked")
    public TFinal get_final() {
        return (TFinal) BamlFfi.callSync(methodFqn("final"), SELF_NAMES, new Object[] {this}, null);
    }

    /** Asynchronous sibling of {@link #get_final()}. */
    @SuppressWarnings("unchecked")
    public CompletableFuture<TFinal> get_final_async() {
        return (CompletableFuture<TFinal>) (CompletableFuture<?>)
                BamlFfi.callAsync(methodFqn("final"), SELF_NAMES, new Object[] {this}, null);
    }
}
