// Cancellation coverage for `throws_test.SleepMs`.
//
// Port of python_pydantic2/function_calls/customizable/test_cancellation.py.
//
// java-port notes (asyncio -> CompletableFuture), per ref-java-state-of-
// completeness.md:
//   * `_ctx=ctx` becomes a trailing `BamlCallContext` overload:
//     `Fns.SleepMs(ms, ctx)` / `Fns.SleepMs_async(ms, ctx)`  (INVENTED overload,
//     flagged in the report).
//   * Engine-driven cancellation (ctx.abort()) completes the async future
//     exceptionally with `BamlCancelledError` (carrying the decoded
//     `baml.panics.Cancelled` on `.value()`); `future.join()` surfaces it
//     wrapped in `CompletionException`. This is the analog of Python's
//     `asyncio.CancelledError` whose `.reason` is a `BamlCancelledError`.
//   * `future.cancel(true)` maps to `cancel_function_call(call_id)` and puts
//     the future in the CANCELLED state, so `join()` throws a raw
//     `CancellationException` (Python `task.cancel()` -> `CancelledError`).
//   * asyncio.TaskGroup has no Java analog; sibling cancellation is modeled
//     manually via `whenComplete`.
//   * `asyncio.wait_for(..., timeout)` -> `future.get(timeout, MILLISECONDS)`
//     -> `java.util.concurrent.TimeoutException`.
//   * Sync cancellation via a background `ctx.abort()` still surfaces as a
//     `BamlPanic` whose `.value()` is a `Cancelled` (parity with Python).

import static org.junit.jupiter.api.Assertions.assertInstanceOf;
import static org.junit.jupiter.api.Assertions.assertNull;
import static org.junit.jupiter.api.Assertions.assertThrows;
import static org.junit.jupiter.api.Assertions.assertTrue;

import baml_bridge.BamlCallContext;
import baml_bridge.BamlCancelledError;
import baml_bridge.BamlPanic;
import baml_sdk.baml.panics.Cancelled;
import baml_sdk.throws_test.Fns;
import java.util.Timer;
import java.util.TimerTask;
import java.util.concurrent.CancellationException;
import java.util.concurrent.CompletableFuture;
import java.util.concurrent.CompletionException;
import java.util.concurrent.TimeUnit;
import java.util.concurrent.TimeoutException;
import org.junit.jupiter.api.Test;

class TestCancellation {

    private static final long MAX_CANCELLATION_MILLIS = 500;

    private static void assertCancelledPanic(BamlPanic exc) {
        assertInstanceOf(Cancelled.class, exc.value());
    }

    private static void assertCancelledReason(BamlCancelledError reason) {
        assertInstanceOf(Cancelled.class, reason.value());
    }

    private static void assertFastCancellation(long startNanos) {
        double elapsedMillis = (System.nanoTime() - startNanos) / 1_000_000.0;
        assertTrue(
                elapsedMillis < MAX_CANCELLATION_MILLIS,
                "cancellation took " + elapsedMillis + "ms");
    }

    private static void sleepMillis(long millis) {
        try {
            Thread.sleep(millis);
        } catch (InterruptedException e) {
            Thread.currentThread().interrupt();
            throw new RuntimeException(e);
        }
    }

    @Test
    void test_sync_call_returns_none() {
        assertNull(Fns.SleepMs(1L));
    }

    @Test
    void test_async_call_returns_none() {
        assertNull(Fns.SleepMs_async(1L).join());
    }

    @Test
    void test_sync_cancel_via_call_context() {
        long start = System.nanoTime();
        BamlCallContext ctx = new BamlCallContext();
        Timer timer = new Timer();
        timer.schedule(
                new TimerTask() {
                    @Override
                    public void run() {
                        ctx.abort();
                    }
                },
                50);

        try {
            BamlPanic exc =
                    assertThrows(BamlPanic.class, () -> Fns.SleepMs(2000L, ctx));
            assertCancelledPanic(exc);
        } finally {
            timer.cancel();
        }

        assertFastCancellation(start);
    }

    @Test
    void test_async_cancel_via_call_context() {
        long start = System.nanoTime();
        BamlCallContext ctx = new BamlCallContext();
        CompletableFuture<Void> future = Fns.SleepMs_async(2000L, ctx);

        sleepMillis(50);
        ctx.abort();

        CompletionException ex = assertThrows(CompletionException.class, future::join);
        BamlCancelledError reason = assertInstanceOf(BamlCancelledError.class, ex.getCause());
        assertCancelledReason(reason);
        assertFastCancellation(start);
    }

    @Test
    void test_async_cancel_via_task_cancel() {
        long start = System.nanoTime();
        CompletableFuture<Void> future = Fns.SleepMs_async(2000L);

        sleepMillis(50);
        future.cancel(true);

        assertThrows(CancellationException.class, future::join);
        assertFastCancellation(start);
    }

    @Test
    void test_async_cancel_via_task_group_sibling() {
        long start = System.nanoTime();

        CompletableFuture<Void> sleep = Fns.SleepMs_async(2000L);
        CompletableFuture<Void> failSoon =
                CompletableFuture.runAsync(
                        () -> {
                            sleepMillis(50);
                            throw new RuntimeException("cancel siblings");
                        });
        // TaskGroup semantics: a failing sibling cancels the others.
        failSoon.whenComplete(
                (v, ex) -> {
                    if (ex != null) {
                        sleep.cancel(true);
                    }
                });

        assertThrows(
                CompletionException.class,
                () -> CompletableFuture.allOf(sleep, failSoon).join());
        // The sibling's RuntimeError surfaced...
        CompletionException sibEx = assertThrows(CompletionException.class, failSoon::join);
        assertInstanceOf(RuntimeException.class, sibEx.getCause());
        // ...and the sleeping sibling was cancelled.
        assertTrue(sleep.isCancelled());
        assertFastCancellation(start);
    }

    @Test
    void test_async_cancel_via_asyncio_timeout() {
        long start = System.nanoTime();
        CompletableFuture<Void> future = Fns.SleepMs_async(2000L);

        assertThrows(TimeoutException.class, () -> future.get(50, TimeUnit.MILLISECONDS));

        assertFastCancellation(start);
    }
}
