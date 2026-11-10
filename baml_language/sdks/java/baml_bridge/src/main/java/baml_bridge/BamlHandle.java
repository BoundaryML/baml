package baml_bridge;

import java.lang.ref.Cleaner;
import java.util.concurrent.atomic.AtomicBoolean;

/**
 * Owning wrapper around a {@code HANDLE_TABLE} row — a {@code (key, handleType)}
 * pair, where {@code key} is the engine-side {@code u64} table key and
 * {@code handleType} the wire {@code BamlHandleType} discriminant. The Java
 * analog of {@code bridge_python}'s {@code BamlPyHandle}: the actual
 * {@code CffiHandleTableEntry} lives in the Rust {@code HANDLE_TABLE}; this
 * object is just a token that owns one ownership of the row.
 *
 * <h2>Lifecycle</h2>
 * <ul>
 *   <li><b>Construct</b> (decode side): the wire codec builds a
 *       {@code BamlHandle} over the key the engine handed over. This object now
 *       owns that ownership.</li>
 *   <li><b>Clone for the wire</b> (encode side): {@link #cloneKeyForWire} takes
 *       one more ownership via {@code baml_handle_clone} for the engine to
 *       {@code drain} on decode, while this object keeps its own: the same key
 *       for a row naming an engine object, a fresh key for an identity-free row
 *       (media).</li>
 *   <li><b>Release</b>: a {@link Cleaner} calls {@code baml_handle_release} once
 *       the {@code BamlHandle} is unreachable; {@link #close()} releases eagerly.
 *       JVM finalizers are deprecated, so a {@code Cleaner} is the durable
 *       lifecycle hook (mirrors the .NET/JVM blueprint for handle-backed
 *       values).</li>
 * </ul>
 *
 * <p>Host-value handles ({@code HOST_VALUE_CALLABLE} / {@code HOST_VALUE_OPAQUE})
 * are the engine's references to objects this host registered, numbered by the
 * host registry ({@link HostCalls}) and never present in the engine's handle
 * table. Sending one takes one more count of its key in the registry; the
 * engine gives the counts back. One the engine sent ({@link #hostValue}) owns
 * the engine's {@code HOST_VALUE_PIN} row instead: the pin keeps the engine's
 * references to the key, and with them its registration, until this object is
 * released.
 */
public final class BamlHandle implements AutoCloseable {
    /** Shared cleaner; daemon-threaded, so it never keeps the JVM alive. */
    private static final Cleaner CLEANER = Cleaner.create();

    // BamlHandleType discriminants (baml_handle.proto). Only the values the Java
    // bridge needs to reason about are named here.
    /** {@code HANDLE_UNSPECIFIED} — skips handle-type validation on accessors. */
    public static final int HANDLE_UNSPECIFIED = 0;
    public static final int FUNCTION_REF = 5;
    public static final int ADT_MEDIA_IMAGE = 6;
    public static final int ADT_MEDIA_AUDIO = 7;
    public static final int ADT_MEDIA_VIDEO = 8;
    public static final int ADT_MEDIA_PDF = 9;
    public static final int HOST_VALUE_CALLABLE = 15;
    public static final int HOST_VALUE_OPAQUE = 16;
    public static final int ADT_RUNTIME_VALUE = 18;

    private final long key;
    private final int handleType;
    /** Root class identity carried by an outbound tagged handle's {@code ty}. */
    private final String classFqn;
    private final State state;
    private final Cleaner.Cleanable cleanable;

    /**
     * The cleaning action. Kept in a separate class that captures only the row
     * it releases (never the enclosing {@code BamlHandle}) so the referent stays
     * collectible — a common {@code Cleaner} pitfall is capturing {@code this}.
     */
    private static final class State implements Runnable {
        /**
         * The {@code HANDLE_TABLE} row this handle owns: its own key for an
         * engine handle; a host value's pin, or {@code 0} for none.
         */
        private final long owned;
        private final AtomicBoolean released = new AtomicBoolean(false);

        State(long owned) {
            this.owned = owned;
        }

        @Override
        public void run() {
            // Per-instance atomic latch: release exactly once across a possible
            // race between explicit close() and the Cleaner.
            if (!released.compareAndSet(false, true) || owned == 0) {
                return;
            }
            try {
                BamlFfi.nativeHandleRelease(owned);
            } catch (Throwable ignored) {
                // Best-effort: during JVM teardown (or if the native library is
                // unavailable in an offline unit-test JVM) the release is a no-op
                // rather than a propagated failure — matches BamlPyHandle::drop
                // ignoring the release status.
            }
        }
    }

    /**
     * Wrap an existing engine row. The caller must own {@code key} (i.e. the
     * engine minted it and no other {@code BamlHandle} owns it), since this
     * object will release it on cleanup.
     */
    public BamlHandle(long key, int handleType) {
        this(key, handleType, null);
    }

    /**
     * Wrap an engine row together with the root class identity carried on the
     * outbound handle. Ordinary/media handles have no class identity; tagged
     * heap handles use it to derive their instance-method FQNs. A host-value
     * type names the host's key and owns no row.
     */
    public BamlHandle(long key, int handleType, String classFqn) {
        this(key, handleType, classFqn, isHostValue(handleType) ? 0 : key);
    }

    private BamlHandle(long key, int handleType, String classFqn, long owned) {
        this.key = key;
        this.handleType = handleType;
        this.classFqn = classFqn;
        this.state = new State(owned);
        this.cleanable = CLEANER.register(this, state);
    }

    /**
     * A host value the engine sent: the host's {@code key}, and the engine's
     * {@code pin} row ({@code 0} for none), which this handle owns. While the
     * pin is held the key stays registered, so the value can be read and sent
     * again.
     *
     * @throws IllegalArgumentException when {@code handleType} is not a host-value type
     */
    public static BamlHandle hostValue(long key, int handleType, String classFqn, long pin) {
        if (!isHostValue(handleType)) {
            throw new IllegalArgumentException("handle type " + handleType + " is not a host value");
        }
        return new BamlHandle(key, handleType, classFqn, pin);
    }

    /** The engine-side {@code HANDLE_TABLE} key. */
    public long key() {
        return key;
    }

    /** The wire {@code BamlHandleType} discriminant. */
    public int handleType() {
        return handleType;
    }

    /** Root BAML class FQN carried by this handle, or {@code null} if absent. */
    public String classFqn() {
        return classFqn;
    }

    /**
     * The key to send this handle under, holding a reference of its own for
     * the request, given back if the request never reaches the engine
     * ({@code EncodingScope}). An engine row takes one more ownership
     * ({@code baml_handle_clone}: the same key for a row naming an engine
     * object, a fresh key for an identity-free row), which the engine
     * {@code drain}s on decode; this object keeps its own, so the value stays
     * valid after the call. A host value goes under its own key, taking one
     * more count of it in the host registry ({@link HostCalls#sendAgain}).
     *
     * @throws baml_bridge.internal.ProtoWriter.WireFaultException when the row
     *     cannot be cloned, or the host key is no longer registered
     */
    public long cloneKeyForWire() {
        if (state.released.get()) throw new IllegalStateException("handle is closed");
        if (isHostValue()) {
            HostCalls.sendAgain(key);
            return key;
        }
        long cloned;
        try {
            cloned = BamlFfi.nativeHandleClone(key);
        } catch (RuntimeException failure) {
            // The row is ours and alive, so this is the bridge's own failure,
            // not anything the value did.
            throw new baml_bridge.internal.ProtoWriter.WireFaultException(
                    "the Java bridge could not clone handle " + key + " for the wire: "
                            + failure.getMessage(),
                    failure);
        }
        baml_bridge.internal.EncodingScope.own(cloned, handleType);
        return cloned;
    }

    /** Keep an independently owned reference to this row (for a host value, to its pin). */
    public BamlHandle cloneOwned() {
        if (state.released.get()) throw new IllegalStateException("handle is closed");
        if (isHostValue()) {
            long pin = state.owned == 0 ? 0 : BamlFfi.nativeHandleClone(state.owned);
            return new BamlHandle(key, handleType, classFqn, pin);
        }
        return new BamlHandle(BamlFfi.nativeHandleClone(key), handleType, classFqn);
    }

    /**
     * Whether this handle is the engine's reference to a host object (a
     * registered callable or opaque value), as opposed to this host's reference
     * to an engine object. The two are numbered independently and never meet
     * in the engine's handle table.
     */
    public boolean isHostValue() {
        return isHostValue(handleType);
    }

    private static boolean isHostValue(int handleType) {
        return handleType == HOST_VALUE_CALLABLE || handleType == HOST_VALUE_OPAQUE;
    }

    /** Release the owned row eagerly (idempotent). */
    @Override
    public void close() {
        cleanable.clean();
    }

    // -- media (baml.media.*) native accessors -------------------------------
    // The media wrapper classes in baml_sdk.baml.media are thin shells over a
    // BamlHandle; they delegate construction and field access here so the
    // package-private BamlFfi natives stay encapsulated in this package.

    /**
     * Mint a media handle from a URL. {@code kind} is a proto MediaTypeEnum
     * discriminant; {@code handleType} the matching {@code ADT_MEDIA_*} tag.
     */
    public static BamlHandle mediaFromUrl(int kind, int handleType, String url, String mimeType) {
        return new BamlHandle(BamlFfi.nativeMediaFromUrl(kind, url, mimeType), handleType);
    }

    /** Mint a media handle from base64 content read from {@code file}. Reads nothing. */
    public static BamlHandle mediaFromFileContent(
            int kind, int handleType, String file, String base64, String mimeType) {
        return new BamlHandle(
                BamlFfi.nativeMediaFromFileContent(kind, file, base64, mimeType), handleType);
    }

    /** Mint a media handle from a base64 payload. */
    public static BamlHandle mediaFromBase64(
            int kind, int handleType, String base64, String mimeType) {
        return new BamlHandle(BamlFfi.nativeMediaFromBase64(kind, base64, mimeType), handleType);
    }

    /** The media's source URL, or {@code null} when not URL-backed. */
    public String mediaUrl() {
        return BamlFfi.nativeMediaUrl(key);
    }

    /** The base name of the file the content was read from, or {@code null}. */
    public String mediaName() {
        return BamlFfi.nativeMediaName(key);
    }

    /** The media's base64 payload (never {@code null}). */
    public String mediaBase64() {
        return BamlFfi.nativeMediaBase64(key);
    }

    /** The media's MIME type, or {@code null} when none is set. */
    public String mediaMimeType() {
        return BamlFfi.nativeMediaMimeType(key);
    }
}
