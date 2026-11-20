// A host's own value (`baml.host.Value`) crosses as the host's key to it and
// comes back as the same value. Java spells one as a `BamlHandle` over a key
// in the host-value registry, and has no constructor of its own for one yet:
// a key registered for a callable stands in, held by the test (the one count
// its registration takes) and given back at the end.

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertSame;
import static org.junit.jupiter.api.Assertions.assertTrue;

import baml_bridge.BamlHandle;
import baml_bridge.BamlType;
import baml_bridge.BamlTypedCallable;
import baml_bridge.HostCalls;
import baml_sdk.host_value_tests.Fns;
import java.util.List;
import java.util.concurrent.TimeUnit;
import java.util.function.Supplier;
import org.junit.jupiter.api.Test;
import org.junit.jupiter.api.Timeout;

class TestHostValues {
    /** A host value: a fresh registry key, held by the caller until {@link #giveBack}. */
    private static BamlHandle hostValue(Supplier<Long> value) {
        long key = HostCalls.registerCallable(
                new BamlTypedCallable(value, BamlType.function(List.of(), BamlType.INT, BamlType.UNKNOWN)));
        return new BamlHandle(key, BamlHandle.HOST_VALUE_OPAQUE);
    }

    /** Give back the count {@link #hostValue} took; the key goes once BAML gives back its own. */
    private static void giveBack(BamlHandle sent, Supplier<Long> value) throws InterruptedException {
        HostCalls.release(sent.key(), 1);
        TestHostCallables.awaitReleased(value);
    }

    @Test
    @Timeout(value = 60, unit = TimeUnit.SECONDS)
    void test_host_value_back_decodes_after_baml_collects_garbage() throws InterruptedException {
        baml_sdk.baml.sys.Fns.collect_garbage();
        for (int i = 0; i < 16; i++) {
            long round = i;
            Supplier<Long> value = () -> round;
            BamlHandle sent = hostValue(value);
            BamlHandle back = Fns.echo_host_value(sent);
            assertEquals(sent.key(), back.key());
            // The test lets go of its own count, and BAML of its references:
            // only the pin `back` holds keeps the key registered.
            HostCalls.release(sent.key(), 1);
            baml_sdk.baml.sys.Fns.collect_garbage();
            assertTrue(HostCalls.holds(value), "the pin keeps the key");
            // Sent again after the collection, it is still one value to BAML.
            assertTrue(Fns.same_host_value(back, back));
            assertSame(value, ((BamlTypedCallable) HostCalls.lookup(back.key())).callable());
            // With the pin given back, every count comes back and the key goes.
            back.close();
            TestHostCallables.awaitReleased(value);
        }
    }

    @Test
    @Timeout(value = 60, unit = TimeUnit.SECONDS)
    void test_host_value_decoded_twice_is_one_value() throws InterruptedException {
        Supplier<Long> value = () -> 2L;
        BamlHandle sent = hostValue(value);
        List<BamlHandle> back = Fns.twice_host_value(sent);
        assertEquals(2, back.size());
        assertEquals(back.get(0).key(), back.get(1).key());
        assertSame(HostCalls.lookup(back.get(0).key()), HostCalls.lookup(back.get(1).key()));
        assertTrue(Fns.same_host_value(back.get(0), back.get(1)));
        back.forEach(BamlHandle::close);
        giveBack(sent, value);
    }
}
