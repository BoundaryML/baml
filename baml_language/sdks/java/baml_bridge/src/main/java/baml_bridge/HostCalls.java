package baml_bridge;

import baml_bridge.internal.EncodingScope;
import baml_bridge.internal.HostAdapter;
import baml_bridge.internal.ProtoReader;
import baml_bridge.internal.ProtoWriter;
import java.util.List;
import java.util.Objects;
import java.util.concurrent.CompletionException;
import java.util.concurrent.ConcurrentHashMap;
import java.util.concurrent.atomic.AtomicLong;

/**
 * Internal: the host side of a BAML→host callable call, without the native
 * glue {@link BamlFfi} adds around it. It holds the host values BAML references
 * (callables handed to it, exceptions raised to it), binds a dispatched call's
 * arguments, and decides what a call completes with: the callback completion
 * contract of {@code ABI.md}, one outcome per fault.
 */
public final class HostCalls {
    private HostCalls() {}

    /**
     * Java-side host-value registry: callables handed to BAML as function
     * arguments AND native throwables raised inside a callable, keyed by a
     * process-unique {@code long}. Rust is a pure router — it forwards a key back
     * to {@code BamlFfi.hostDispatch}/{@code BamlFfi.hostRelease}; storing the objects here (not
     * behind a JNI {@code GlobalRef}) makes identity a plain JVM reference, so a
     * rehydrated exception is {@code ==} the original (design points A + D).
     * Callables and thrown objects share one keyspace ({@link #HOST_VALUE_KEY}) so
     * keys never collide and a single {@link #release} clears either kind.
     */
    private static final ConcurrentHashMap<Long, Object> HOST_VALUES = new ConcurrentHashMap<>();

    /** Shared key source for {@link #HOST_VALUES}; starts at 1 (0 is reserved). */
    private static final AtomicLong HOST_VALUE_KEY = new AtomicLong(1);

    /**
     * Register a host callable handed to BAML as a function argument; returns its
     * process-unique key (emitted as {@code Handle{key, HOST_VALUE_CALLABLE}} by
     * {@code ProtoWriter}). The engine binds the key to an {@code Object::HostClosure},
     * dispatches back through {@code BamlFfi.hostDispatch} when BAML invokes it,
     * and retires the registration through {@link #release}. A request that never
     * reaches the engine retires it itself ({@link EncodingScope}).
     */
    public static long registerCallable(BamlTypedCallable callable) {
        Objects.requireNonNull(callable, "callable");
        long key = HOST_VALUE_KEY.getAndIncrement();
        HOST_VALUES.put(key, callable);
        EncodingScope.onRollback(() -> HOST_VALUES.remove(key));
        return key;
    }

    /**
     * Register a native {@link Throwable} raised inside a callable, so the
     * {@code baml.errors.HostCallable} the engine builds around its key resolves
     * back to the original object by identity on round trip. Shares the keyspace
     * with callables so keys never collide.
     */
    private static long registerThrowable(Throwable error) {
        long key = HOST_VALUE_KEY.getAndIncrement();
        HOST_VALUES.put(key, error);
        EncodingScope.onRollback(() -> HOST_VALUES.remove(key));
        return key;
    }

    /**
     * Resolve a host-value key back to its object (a {@link BamlTypedCallable}
     * or a raised {@link Throwable}), or {@code null} when the key is absent —
     * released, or foreign to this runtime. Used by {@code ProtoReader} to
     * rehydrate a host-thrown exception by identity, and never mints or mutates
     * state.
     */
    public static Object lookup(long key) {
        return HOST_VALUES.get(key);
    }

    /**
     * Engine-driven release: drop the registry entry for {@code key}. A
     * stale/absent key is a harmless no-op.
     */
    static void release(long key) {
        HOST_VALUES.remove(key);
    }

    /** A dispatched call, bound: ready to run, or refused before it ran. */
    public sealed interface Bound permits Bound.Call, Bound.Refused {
        /** The call's arguments, decoded and checked against the Java shape that takes them. */
        record Call(HostAdapter adapter, ProtoReader.HostCallArgs args) implements Bound {}

        /** The {@code bridge_failure} completion of a call whose arguments could not be bound. */
        record Refused(byte[] completion) implements Bound {}
    }

    /**
     * Decode a {@code BamlToHostCall} against the callable's declared parameters
     * and check that its Java shape takes them. Neither step runs the user's
     * code, so a failure of either is the bridge's own: a payload it cannot
     * decode, or a call its adapter cannot pass (an arity or argument name the
     * declared type does not have).
     */
    public static Bound bind(BamlTypedCallable typed, byte[] bamlToHostCall) {
        HostAdapter adapter = HostAdapter.of(typed.callable());
        if (adapter == null) {
            return new Bound.Refused(ProtoWriter.hostCallBridgeFailure(
                    "the registered host callable is not callable: " + typed.callable().getClass().getName()));
        }
        ProtoReader.HostCallArgs args;
        try {
            args = ProtoReader.decodeBamlToHostCall(
                    bamlToHostCall, typed.positionalDescs(), typed.optionalNames(), typed.optionalDescs());
        } catch (Throwable undecodable) {
            return new Bound.Refused(ProtoWriter.hostCallBridgeFailure(
                    "the Java bridge could not decode the callback's arguments: " + describe(undecodable)));
        }
        String refusal = argumentRefusal(typed, adapter, args);
        if (refusal != null) {
            return new Bound.Refused(ProtoWriter.hostCallBridgeFailure(
                    "the Java bridge could not pass the callback's arguments: " + refusal));
        }
        return new Bound.Call(adapter, args);
    }

    /** Why the declared type and Java shape of {@code typed} cannot take {@code args}, or {@code null}. */
    private static String argumentRefusal(BamlTypedCallable typed, HostAdapter adapter, ProtoReader.HostCallArgs args) {
        int declared = typed.positionalDescs().length;
        if (args.positional.size() != declared) {
            return "the callable's type has " + declared + " positional parameters, but the engine passed "
                    + args.positional.size();
        }
        List<String> optionalNames = List.of(typed.optionalNames());
        for (String name : args.optional.keySet()) {
            if (!optionalNames.contains(name)) {
                return "the callable's type has no optional parameter `" + name + "`";
            }
        }
        return adapter.refusal(args.positional.size(), typed.optionalNames());
    }

    /**
     * What a callback that returned {@code value} completes with, encoded against
     * its declared return type: {@code returned} when the value is one;
     * {@code contract_violation} when the encoder can see it is not (an instance
     * of another class, a Java object that is no BAML value); and
     * {@code bridge_failure} when the bridge cannot carry it whatever its type
     * (nested deeper than the wire, a handle it could not clone).
     */
    public static Completion returnedCompletion(Object value, BamlType returnDesc) {
        return encodeOutcome(
                value,
                returnDesc,
                ProtoWriter::hostCallReturned,
                "the callable's return value does not inhabit its declared type: ");
    }

    /**
     * What a callback that threw {@code error} completes with:
     *
     * <ul>
     *   <li>a BAML panic in hand ({@link BamlPanic}, or a nested async call's
     *       {@link BamlCancelledError}) is {@code thrown}, encoded against
     *       {@code unknown}: a panic is a panic whatever the callable declares;
     *   <li>a BAML error in hand ({@link BamlError}, or a nested call's
     *       {@link BamlTypeMismatchError}) is {@code thrown}, encoded against the
     *       declared throws type, so BAML's typed {@code catch} matches;
     *   <li>anything else is the host's own exception: {@code raised} with its
     *       identity, under a fresh key, so the {@code baml.errors.HostCallable}
     *       the engine builds rehydrates the SAME object on round trip.
     * </ul>
     *
     * <p>A {@link CompletionException} (a nested async call's {@code join()})
     * wraps the failure it carries and is looked through.
     */
    public static Completion failedCompletion(Throwable error, BamlType throwsDesc) {
        Throwable cause = unwrapCompletion(error);
        String doesNotFit = "the callable's thrown value does not inhabit its declared throws type: ";
        if (cause instanceof BamlPanic panic) {
            return encodeOutcome(panic.value(), BamlType.UNKNOWN, ProtoWriter::hostCallThrown, doesNotFit);
        }
        if (cause instanceof BamlCancelledError cancelled) {
            return encodeOutcome(cancelled.value(), BamlType.UNKNOWN, ProtoWriter::hostCallThrown, doesNotFit);
        }
        if (cause instanceof BamlError bamlError) {
            return encodeOutcome(bamlError.value(), throwsDesc, ProtoWriter::hostCallThrown, doesNotFit);
        }
        if (cause instanceof BamlTypeMismatchError mismatch) {
            return encodeOutcome(mismatch.value(), throwsDesc, ProtoWriter::hostCallThrown, doesNotFit);
        }
        long handleKey = registerThrowable(cause);
        String className = cause.getClass().getSimpleName();
        String message = cause.getMessage() != null ? cause.getMessage() : cause.toString();
        return new Completion(
                ProtoWriter.hostCallRaised(className, message, renderTraceback(cause), handleKey), true);
    }

    /**
     * One {@code HostCallCompletion}, and whether it hands the engine the
     * references its encoding took (a value's handle clones, a raised
     * exception's registration). A completion that does not (a
     * {@code contract_violation} or {@code bridge_failure} after a refused
     * encode) leaves them to be taken back.
     */
    public record Completion(byte[] bytes, boolean carriesReferences) {}

    /**
     * {@code value} encoded against {@code slot} into the completion
     * {@code outcome} builds, or the completion for a value that cannot be one.
     */
    private static Completion encodeOutcome(
            Object value,
            BamlType slot,
            java.util.function.Function<byte[], byte[]> outcome,
            String doesNotFit) {
        byte[] payload;
        try {
            payload = ProtoWriter.encodeResult(value, slot);
        } catch (ProtoWriter.WireFaultException cannotCarry) {
            return new Completion(ProtoWriter.hostCallBridgeFailure(cannotCarry.getMessage()), false);
        } catch (IllegalArgumentException misfit) {
            return new Completion(ProtoWriter.hostCallContractViolation(doesNotFit + misfit.getMessage()), false);
        } catch (Throwable bridgeFault) {
            return new Completion(ProtoWriter.hostCallBridgeFailure(
                    "the Java bridge could not encode the callable's value: " + describe(bridgeFault)), false);
        }
        return new Completion(outcome.apply(payload), true);
    }

    /** {@code error}'s class and message, for a bridge failure's text. */
    static String describe(Throwable error) {
        return error.getClass().getSimpleName() + ": " + error.getMessage();
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
     * A {@link CompletionException} (a {@code whenComplete} error, a nested
     * async call's {@code join()}) wraps the real cause; unwrap it so the
     * callable's actual exception drives the outcome (and round-trips by
     * identity).
     */
    private static Throwable unwrapCompletion(Throwable err) {
        Throwable cause = err;
        while (cause instanceof CompletionException && cause.getCause() != null) {
            cause = cause.getCause();
        }
        return cause;
    }
}
