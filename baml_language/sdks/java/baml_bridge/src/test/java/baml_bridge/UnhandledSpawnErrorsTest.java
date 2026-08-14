package baml_bridge;

import static org.junit.jupiter.api.Assertions.assertTrue;

import baml_bridge.internal.WireWriter;

import java.util.concurrent.CountDownLatch;
import java.util.concurrent.TimeUnit;
import java.util.concurrent.atomic.AtomicReference;

import org.junit.jupiter.api.Test;

class UnhandledSpawnErrorsTest {
    @Test
    void unhandled_spawn_error_uses_host_default() throws InterruptedException {
        WireWriter value = new WireWriter();
        value.writeString(3, "boom");
        WireWriter error = new WireWriter();
        error.writeMessage(1, value.toByteArray());
        WireWriter envelope = new WireWriter();
        envelope.writeMessage(2, error.toByteArray());

        CountDownLatch reported = new CountDownLatch(1);
        AtomicReference<Throwable> seen = new AtomicReference<>();
        Thread.UncaughtExceptionHandler original = Thread.getDefaultUncaughtExceptionHandler();
        Thread.setDefaultUncaughtExceptionHandler(
                (thread, failure) -> {
                    seen.set(failure);
                    reported.countDown();
                });
        try {
            UnhandledSpawnErrors.report(envelope.toByteArray(), false);
            assertTrue(reported.await(1, TimeUnit.SECONDS));
        } finally {
            Thread.setDefaultUncaughtExceptionHandler(original);
        }

        assertTrue(seen.get().getMessage().contains("boom"));
    }
}
