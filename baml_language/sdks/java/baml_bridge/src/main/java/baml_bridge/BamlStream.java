package baml_bridge;

import java.util.concurrent.CompletableFuture;

/**
 * Runtime-owned streaming wrapper around a {@code baml.llm.Stream} handle — the
 * Java analog of {@code bridge_python}'s {@code BamlStream} ({@code _stream.py}).
 *
 * <p>Holds a single {@link BamlHandle} whose {@code HANDLE_TABLE} row is an
 * {@code Adt(TaggedHeapHandle{ty, heap_handle})} (wire tag
 * {@code ADT_TAGGED_HEAP_HANDLE}). {@link #next()} / {@link #get_final()} (and
 * their {@code _async} siblings) re-enter the engine via ordinary
 * {@link BamlFfi#callSync}/{@link BamlFfi#callAsync} on the stdlib functions
 * {@code baml.llm.Stream.next} / {@code baml.llm.Stream.final}, passing this
 * wrapper as the {@code self} receiver — exactly like any codegen-emitted
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
 * until it returns a {@link baml_sdk.baml.stream.StreamFinished} VALUE — no
 * {@code null} sentinel, no exception. Because Java generics cannot express
 * {@code TPartial | StreamFinished}, the declared return is {@code TPartial}
 * (erased to {@code Object}); callers dispatch with
 * {@code if (v instanceof StreamFinished)} — the faithful port of Python's
 * {@code isinstance(v, StreamFinished)} duck-typing.
 *
 * @param <TPartial> the partial (in-flight) element type
 * @param <TFinal>   the final (completed) value type
 */
public final class BamlStream<TPartial, TFinal> {
    private static final String STREAM_NEXT_FN = "baml.llm.Stream.next";
    private static final String STREAM_FINAL_FN = "baml.llm.Stream.final";

    /** The single positional arg name for the receiver, mirroring Python's {@code {"self": self}}. */
    private static final String[] SELF_NAMES = {"self"};

    private final BamlHandle handle;

    private BamlStream(BamlHandle handle) {
        this.handle = handle;
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

    /**
     * The next partial value, or a {@link baml_sdk.baml.stream.StreamFinished}
     * value once the stream is exhausted. The result is decoded wire-driven (null
     * descriptor): a partial is a registered {@code $stream} companion (or a bare
     * primitive), and {@code StreamFinished} is a registered runtime class.
     */
    @SuppressWarnings("unchecked")
    public TPartial next() {
        return (TPartial) BamlFfi.callSync(STREAM_NEXT_FN, SELF_NAMES, new Object[] {this}, null);
    }

    /** Asynchronous sibling of {@link #next()}. */
    @SuppressWarnings("unchecked")
    public CompletableFuture<TPartial> next_async() {
        return (CompletableFuture<TPartial>) (CompletableFuture<?>)
                BamlFfi.callAsync(STREAM_NEXT_FN, SELF_NAMES, new Object[] {this}, null);
    }

    /** The stream's final (completed) value. */
    @SuppressWarnings("unchecked")
    public TFinal get_final() {
        return (TFinal) BamlFfi.callSync(STREAM_FINAL_FN, SELF_NAMES, new Object[] {this}, null);
    }

    /** Asynchronous sibling of {@link #get_final()}. */
    @SuppressWarnings("unchecked")
    public CompletableFuture<TFinal> get_final_async() {
        return (CompletableFuture<TFinal>) (CompletableFuture<?>)
                BamlFfi.callAsync(STREAM_FINAL_FN, SELF_NAMES, new Object[] {this}, null);
    }
}
