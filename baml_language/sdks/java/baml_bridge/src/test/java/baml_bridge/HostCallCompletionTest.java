package baml_bridge;

import static org.junit.jupiter.api.Assertions.assertArrayEquals;
import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertInstanceOf;
import static org.junit.jupiter.api.Assertions.assertNull;
import static org.junit.jupiter.api.Assertions.assertSame;
import static org.junit.jupiter.api.Assertions.assertThrows;
import static org.junit.jupiter.api.Assertions.assertTrue;

import baml_bridge.internal.WireReader;
import baml_bridge.internal.WireWriter;
import java.util.List;
import java.util.concurrent.CompletionException;
import java.util.function.BiFunction;
import java.util.function.Function;
import java.util.function.Supplier;
import org.junit.jupiter.api.BeforeAll;
import org.junit.jupiter.api.Test;

/**
 * The callback completion contract ({@code ABI.md}): what a host call completes
 * with for each thing a callback can do. Pure: no native library, so no value
 * here holds an engine handle.
 */
class HostCallCompletionTest {
    // HostCallCompletion outcome fields (baml_inbound.proto).
    private static final int RETURNED = 1;
    private static final int THROWN = 2;
    private static final int RAISED = 3;
    private static final int CONTRACT_VIOLATION = 4;
    private static final int BRIDGE_FAILURE = 5;

    private static final String ERROR_FQN = "user.completion.Rejected";
    private static final String PANIC_FQN = "user.completion.Broken";
    private static final BamlType ERROR = BamlType.classByFqn(ERROR_FQN);

    /** A declared error class, as the generator emits one. */
    public record Rejected(String message) {}

    /** A panic class: any class other than the declared error. */
    public record Broken(String message) {}

    @BeforeAll
    static void registerFixtures() {
        TypeRegistry.registerClass(
                ERROR_FQN, Rejected.class.getName(), new String[] {"message"}, new BamlType[] {BamlType.STRING});
        TypeRegistry.registerClass(
                PANIC_FQN, Broken.class.getName(), new String[] {"message"}, new BamlType[] {BamlType.STRING});
    }

    /** The outcome field a completion sets. */
    private static int outcome(HostCalls.Completion completion) {
        WireReader r = new WireReader(completion.bytes());
        int field = WireReader.fieldOf(r.readTag());
        r.skipField(WireWriter.WIRE_LEN);
        assertTrue(!r.hasRemaining(), "a completion carries one outcome");
        return field;
    }

    /** The payload of a completion's one outcome. */
    private static byte[] payload(HostCalls.Completion completion) {
        WireReader r = new WireReader(completion.bytes());
        r.readTag();
        return r.readBytes();
    }

    /** {@code levels} lists nested around a {@code 1}. */
    private static Object nested(int levels) {
        Object value = 1L;
        for (int i = 0; i < levels; i++) {
            value = List.of(value);
        }
        return value;
    }

    @Test
    void a_returned_value_that_fits_is_returned() {
        HostCalls.Completion done = HostCalls.returnedCompletion(5L, BamlType.INT);
        assertEquals(RETURNED, outcome(done));
        assertTrue(done.carriesReferences());
    }

    @Test
    void a_returned_value_the_bridge_can_see_does_not_fit_is_a_contract_violation() {
        assertEquals(CONTRACT_VIOLATION, outcome(HostCalls.returnedCompletion(new Object(), BamlType.UNKNOWN)));
        // An instance of another class than the declared one.
        HostCalls.Completion done = HostCalls.returnedCompletion(new Broken("x"), ERROR);
        assertEquals(CONTRACT_VIOLATION, outcome(done));
        assertTrue(!done.carriesReferences());
    }

    @Test
    void a_returned_value_nested_deeper_than_the_wire_is_a_bridge_failure() {
        // 48 lists cross under the completion; one more cannot, whatever the
        // declared type, and that is the bridge's limit, not the callable's
        // breach of contract.
        assertEquals(RETURNED, outcome(HostCalls.returnedCompletion(nested(48), BamlType.UNKNOWN)));
        for (int levels : new int[] {49, 60, 200}) {
            HostCalls.Completion done = HostCalls.returnedCompletion(nested(levels), BamlType.UNKNOWN);
            assertEquals(BRIDGE_FAILURE, outcome(done));
            assertTrue(new String(payload(done)).contains("nested too deeply"));
        }
    }

    @Test
    void a_declared_baml_error_is_thrown_against_the_declared_throws_type() {
        HostCalls.Completion done =
                HostCalls.failedCompletion(new BamlError(new Rejected("no")), ERROR);
        assertEquals(THROWN, outcome(done));
        // The value states the declared class.
        assertArrayEquals(
                baml_bridge.internal.ProtoWriter.encodeResult(new Rejected("no"), ERROR), payload(done));
        // A value the declared type does not have is the callable's breach.
        assertEquals(
                CONTRACT_VIOLATION,
                outcome(HostCalls.failedCompletion(new BamlError(new Broken("no")), ERROR)));
    }

    @Test
    void a_forwarded_panic_is_thrown_against_unknown_whatever_the_callable_declares() {
        // A panic is a panic whatever the callable declares: under a
        // single-class `throws Rejected` it is still thrown as itself.
        HostCalls.Completion done =
                HostCalls.failedCompletion(new BamlPanic(new Broken("boom"), List.of(), PANIC_FQN), ERROR);
        assertEquals(THROWN, outcome(done));
        assertArrayEquals(
                baml_bridge.internal.ProtoWriter.encodeResult(new Broken("boom"), BamlType.UNKNOWN),
                payload(done));
    }

    @Test
    void a_nested_async_cancellation_is_thrown_as_the_panic_it_is() {
        HostCalls.Completion done = HostCalls.failedCompletion(
                new BamlCancelledError(new Broken("cancelled"), List.of(), PANIC_FQN), ERROR);
        assertEquals(THROWN, outcome(done));
    }

    @Test
    void a_nested_type_mismatch_is_thrown_as_the_baml_error_it_is() {
        HostCalls.Completion done = HostCalls.failedCompletion(
                new BamlTypeMismatchError("bad argument", new Rejected("bad argument"), List.of(), ERROR_FQN),
                ERROR);
        assertEquals(THROWN, outcome(done));
    }

    @Test
    void a_baml_failure_a_completion_exception_carries_is_looked_through() {
        HostCalls.Completion done = HostCalls.failedCompletion(
                new CompletionException(new BamlError(new Rejected("no"))), ERROR);
        assertEquals(THROWN, outcome(done));
    }

    @Test
    void a_host_exception_is_raised_with_its_identity() {
        IllegalStateException mine = new IllegalStateException("mine");
        HostCalls.Completion done = HostCalls.failedCompletion(mine, ERROR);
        assertEquals(RAISED, outcome(done));
        assertTrue(done.carriesReferences());
        // HostException.handle = 5: the registry key of the original object.
        WireReader exception = new WireReader(payload(done));
        long key = 0;
        while (exception.hasRemaining()) {
            int tag = exception.readTag();
            if (WireReader.fieldOf(tag) == 5) {
                key = exception.readVarint();
            } else {
                exception.skipField(WireReader.wireOf(tag));
            }
        }
        assertSame(mine, HostCalls.lookup(key));
    }

    @Test
    void a_request_that_is_never_sent_retires_the_callables_it_registered() {
        // Keys are minted in order: the callable the refused request registers
        // takes the next one.
        BamlTypedCallable probe = new BamlTypedCallable((Supplier<Long>) () -> 1L, function(List.of()));
        long before = HostCalls.registerCallable(probe);
        BamlTypedCallable callable =
                new BamlTypedCallable((Function<Long, Long>) x -> x, function(List.of(INT_PARAM)));
        try (baml_bridge.internal.EncodingScope encoding = new baml_bridge.internal.EncodingScope()) {
            assertThrows(
                    IllegalArgumentException.class,
                    () -> baml_bridge.internal.ProtoWriter.encodeCallFunctionArgs(
                            new String[] {"callback", "value"}, new Object[] {callable, nested(5000)}, 1L));
            assertSame(callable, HostCalls.lookup(before + 1));
        }
        assertNull(HostCalls.lookup(before + 1));
        // A registration outside any request stays until the engine releases it.
        assertSame(probe, HostCalls.lookup(before));
        HostCalls.release(before, 1);
        assertNull(HostCalls.lookup(before));
    }

    /** A callable registered for one send, as the encoder registers one. */
    private static long registered(Object callable) {
        return HostCalls.registerCallable(new BamlTypedCallable(callable, function(List.of())));
    }

    @Test
    void a_key_lives_until_every_reference_to_it_is_given_back() {
        Supplier<Long> callable = () -> 1L;
        long key = registered(callable);
        BamlHandle received = new BamlHandle(key, BamlHandle.HOST_VALUE_CALLABLE);
        assertEquals(key, received.cloneKeyForWire());
        assertEquals(key, received.cloneKeyForWire());
        HostCalls.release(key, 2);
        assertTrue(HostCalls.holds(callable), "one reference is still out");
        HostCalls.release(key, 1);
        assertTrue(!HostCalls.holds(callable), "the last reference removes the key");
        assertNull(HostCalls.lookup(key));
        // A release of a key that is gone is ignored.
        HostCalls.release(key, 1);
    }

    @Test
    void a_key_that_is_no_longer_registered_is_not_sent() {
        long key = registered((Supplier<Long>) () -> 1L);
        HostCalls.release(key, 1);
        BamlHandle stale = new BamlHandle(key, BamlHandle.HOST_VALUE_OPAQUE);
        baml_bridge.internal.ProtoWriter.WireFaultException refused = assertThrows(
                baml_bridge.internal.ProtoWriter.WireFaultException.class,
                () -> baml_bridge.internal.ProtoWriter.encodeInboundValue(stale));
        assertTrue(refused.getMessage().contains("no longer registered"), refused.getMessage());
        assertNull(HostCalls.lookup(key));
    }

    @Test
    void a_request_that_is_never_sent_gives_back_only_the_counts_it_took() {
        Supplier<Long> callable = () -> 1L;
        long key = registered(callable);
        BamlHandle received = new BamlHandle(key, BamlHandle.HOST_VALUE_CALLABLE);
        try (baml_bridge.internal.EncodingScope encoding = new baml_bridge.internal.EncodingScope()) {
            // Two occurrences in one refused request: two counts taken, two given back.
            assertThrows(
                    IllegalArgumentException.class,
                    () -> baml_bridge.internal.ProtoWriter.encodeCallFunctionArgs(
                            new String[] {"a", "b", "c"}, new Object[] {received, received, nested(5000)}, 1L));
        }
        // The registration's own reference is still out: the key stays.
        assertTrue(HostCalls.holds(callable));
        HostCalls.release(key, 1);
        assertTrue(!HostCalls.holds(callable));

        // A request that reaches the engine keeps its counts for the engine to give back.
        long sent = registered(callable);
        BamlHandle again = new BamlHandle(sent, BamlHandle.HOST_VALUE_CALLABLE);
        try (baml_bridge.internal.EncodingScope encoding = new baml_bridge.internal.EncodingScope()) {
            baml_bridge.internal.ProtoWriter.encodeCallFunctionArgs(new String[] {"a"}, new Object[] {again}, 1L);
            encoding.submitted();
        }
        HostCalls.release(sent, 1);
        assertTrue(HostCalls.holds(callable), "the submitted send's count is the engine's");
        HostCalls.release(sent, 1);
        assertTrue(!HostCalls.holds(callable));
    }

    @Test
    void a_release_racing_sends_of_a_held_key_never_loses_it() throws Exception {
        // The engine holds one reference throughout (as a pin does); a sender
        // re-sends the key while the engine gives each send back. Every send
        // succeeds, and the key outlives them all.
        Supplier<Long> callable = () -> 1L;
        long key = registered(callable);
        BamlHandle received = new BamlHandle(key, BamlHandle.HOST_VALUE_CALLABLE);
        int sends = 10_000;
        java.util.concurrent.BlockingQueue<Long> inFlight = new java.util.concurrent.LinkedBlockingQueue<>();
        Thread engine = new Thread(() -> {
            try {
                for (int i = 0; i < sends; i++) {
                    HostCalls.release(inFlight.take(), 1);
                }
            } catch (InterruptedException interrupted) {
                Thread.currentThread().interrupt();
            }
        });
        engine.start();
        for (int i = 0; i < sends; i++) {
            inFlight.add(received.cloneKeyForWire());
        }
        engine.join(60_000);
        assertTrue(!engine.isAlive());
        assertSame(callable, ((BamlTypedCallable) HostCalls.lookup(key)).callable());
        HostCalls.release(key, 1);
        assertNull(HostCalls.lookup(key));
    }

    @Test
    void a_host_value_the_engine_sent_releases_its_pin_and_never_its_key() {
        Supplier<Long> callable = () -> 1L;
        long key = registered(callable);
        // Pin 0: no engine row behind it, so nothing native is touched.
        BamlHandle received = BamlHandle.hostValue(key, BamlHandle.HOST_VALUE_CALLABLE, null, 0);
        received.close();
        assertTrue(HostCalls.holds(callable), "closing the handle gives back no count of the key");
        assertThrows(IllegalArgumentException.class, () -> BamlHandle.hostValue(1, BamlHandle.FUNCTION_REF, null, 0));
        HostCalls.release(key, 1);
    }

    /** A {@code BamlToHostCall} passing {@code positional} ints, then each named optional int. */
    private static byte[] call(long[] positional, String... optional) {
        WireWriter w = new WireWriter();
        for (long value : positional) {
            w.writeMessage(1, arg(value, null));
        }
        for (String name : optional) {
            w.writeMessage(1, arg(1L, name));
        }
        return w.toByteArray();
    }

    private static byte[] arg(long value, String optionalName) {
        WireWriter outbound = new WireWriter();
        outbound.writeInt64(4, value); // BamlOutboundValue.int_value
        WireWriter arg = new WireWriter();
        arg.writeMessage(1, outbound.toByteArray());
        if (optionalName != null) {
            arg.writeString(2, optionalName);
            arg.writeBool(3, true);
        }
        return arg.toByteArray();
    }

    private static BamlType function(List<BamlType.Param> params) {
        return BamlType.function(params, BamlType.INT, BamlType.UNKNOWN);
    }

    private static final BamlType.Param INT_PARAM = new BamlType.Param(null, BamlType.INT, false);

    @Test
    void arguments_bind_to_the_declared_parameters() {
        BamlTypedCallable typed = new BamlTypedCallable(
                (BiFunction<Long, Long, Long>) Long::sum, function(List.of(INT_PARAM, INT_PARAM)));
        HostCalls.Bound.Call bound =
                assertInstanceOf(HostCalls.Bound.Call.class, HostCalls.bind(typed, call(new long[] {1, 2})));
        assertEquals(List.of(1L, 2L), bound.args().positional);
    }

    /** The bridge failure a call that cannot be bound completes with. */
    private static String refusal(BamlTypedCallable typed, byte[] call) {
        HostCalls.Bound.Refused refused =
                assertInstanceOf(HostCalls.Bound.Refused.class, HostCalls.bind(typed, call));
        WireReader r = new WireReader(refused.completion());
        assertEquals(BRIDGE_FAILURE, WireReader.fieldOf(r.readTag()));
        return r.readString();
    }

    @Test
    void arguments_the_bridge_cannot_decode_are_a_bridge_failure() {
        BamlTypedCallable typed =
                new BamlTypedCallable((Function<Long, Long>) x -> x, function(List.of(INT_PARAM)));
        // An argument message whose declared length runs past the payload.
        String message = refusal(typed, new byte[] {0x0A, 0x05, 0x0A});
        assertTrue(message.contains("could not decode the callback's arguments"), message);
    }

    @Test
    void a_call_the_adapter_cannot_pass_is_a_bridge_failure() {
        BamlTypedCallable supplier =
                new BamlTypedCallable((Supplier<Long>) () -> 1L, function(List.of()));
        String extra = refusal(supplier, call(new long[] {1}));
        assertTrue(extra.contains("has 0 positional parameters, but the engine passed 1"), extra);

        BamlTypedCallable withOptional = new BamlTypedCallable(
                (BamlHostCallable) (positional, optional) -> 1L,
                function(List.of(INT_PARAM, new BamlType.Param("y", BamlType.INT, true))));
        String unknown = refusal(withOptional, call(new long[] {1}, "z"));
        assertTrue(unknown.contains("no optional parameter `z`"), unknown);

        // A java.util.function shape bound to a type of another arity.
        BamlTypedCallable mismatched = new BamlTypedCallable(
                (Function<Long, Long>) x -> x, function(List.of(INT_PARAM, INT_PARAM)));
        String shape = refusal(mismatched, call(new long[] {1, 2}));
        assertTrue(shape.contains("java.util.function.Function takes 1 arguments"), shape);
    }
}
