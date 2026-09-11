package baml_bridge;

import java.lang.ref.Cleaner;
import java.util.concurrent.atomic.AtomicBoolean;

/**
 * Owning wrapper around a {@code HANDLE_TABLE} row — a {@code (key, handleType)}
 * pair, where {@code key} is the engine-side {@code u64} table key and
 * {@code handleType} the wire {@code BamlHandleType} discriminant. The Java
 * analog of {@code bridge_python}'s {@code BamlPyHandle}: the actual
 * {@code CffiHandleTableEntry} lives in the Rust {@code HANDLE_TABLE}; this
 * object is just a token that owns the row's lifetime.
 *
 * <h2>Lifecycle</h2>
 * <ul>
 *   <li><b>Construct</b> (decode side): the wire codec builds a
 *       {@code BamlHandle} over the key the engine minted. This object now owns
 *       the row.</li>
 *   <li><b>Clone for the wire</b> (encode side): {@link #cloneKeyForWire} mints
 *       a fresh key via {@code baml_handle_clone} so the engine can
 *       {@code drain} its copy on decode while this object keeps its own — never
 *       sharing a key would double-release.</li>
 *   <li><b>Release</b>: a {@link Cleaner} calls {@code baml_handle_release} once
 *       the {@code BamlHandle} is unreachable; {@link #close()} releases eagerly.
 *       JVM finalizers are deprecated, so a {@code Cleaner} is the durable
 *       lifecycle hook (mirrors the .NET/JVM blueprint for handle-backed
 *       values).</li>
 * </ul>
 *
 * <p>Host-owned handles ({@code HOST_VALUE_CALLABLE} / {@code HOST_VALUE_OPAQUE})
 * are <em>not</em> tracked in {@code HANDLE_TABLE}; releasing them by key could
 * evict an unrelated engine row (the two keyspaces share the {@code u64} value
 * space), so the release path skips them — same guard as {@code BamlPyHandle}.
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
    public static final int ADT_MEDIA_GENERIC = 10;
    public static final int HOST_VALUE_CALLABLE = 15;
    public static final int HOST_VALUE_OPAQUE = 16;

    final BamlProgram program = BamlProgram.current();
    private final long key;
    private final int handleType;
    /** Root class identity carried by an outbound tagged handle's {@code ty}. */
    private final String classFqn;
    private final State state;
    private final Cleaner.Cleanable cleanable;

    /**
     * The cleaning action. Kept in a separate class that captures only the key +
     * type (never the enclosing {@code BamlHandle}) so the referent stays
     * collectible — a common {@code Cleaner} pitfall is capturing {@code this}.
     */
    private static final class State implements Runnable {
        private final long key;
        private final int handleType;
        private final AtomicBoolean released = new AtomicBoolean(false);

        State(long key, int handleType) {
            this.key = key;
            this.handleType = handleType;
        }

        @Override
        public void run() {
            // Per-instance atomic latch: release exactly once across a possible
            // race between explicit close() and the Cleaner.
            if (!released.compareAndSet(false, true)) {
                return;
            }
            // Host-value handles live in a per-bridge registry, not HANDLE_TABLE.
            if (handleType == HOST_VALUE_CALLABLE || handleType == HOST_VALUE_OPAQUE) {
                return;
            }
            try {
                BamlFfi.nativeHandleRelease(key);
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
     * heap handles use it to derive their instance-method FQNs.
     */
    public BamlHandle(long key, int handleType, String classFqn) {
        this.key = key;
        this.handleType = handleType;
        this.classFqn = classFqn;
        this.state = new State(key, handleType);
        this.cleanable = CLEANER.register(this, state);
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
     * Mint a fresh owned key over the same row for inbound wire ownership. The
     * engine {@code drain}s the returned key on decode; this object keeps its own
     * key, so the original media/handle value stays valid after the call.
     */
    public long cloneKeyForWire() {
        return BamlFfi.nativeHandleClone(key);
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

    /** Mint a media handle from a local file path. */
    public static BamlHandle mediaFromFile(int kind, int handleType, String path, String mimeType) {
        return new BamlHandle(BamlFfi.nativeMediaFromFile(kind, path, mimeType), handleType);
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

    /** The media's local file path, or {@code null} when not file-backed. */
    public String mediaFile() {
        return BamlFfi.nativeMediaFile(key);
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
