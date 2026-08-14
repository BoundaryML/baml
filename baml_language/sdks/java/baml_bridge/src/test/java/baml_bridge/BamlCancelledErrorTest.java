package baml_bridge;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertInstanceOf;
import static org.junit.jupiter.api.Assertions.assertNotNull;
import static org.junit.jupiter.api.Assertions.assertThrows;
import static org.junit.jupiter.api.Assertions.assertTrue;

import java.util.List;
import java.util.concurrent.CancellationException;
import java.util.concurrent.CompletableFuture;
import org.junit.jupiter.api.Test;

/**
 * Offline classification tests for the Design B {@link BamlCancelledError}: it
 * is a {@link CancellationException} (not a {@link BamlError}), so a future
 * completed exceptionally with it reads as cancelled and surfaces it directly.
 * No native library required — this is pure JDK behavior.
 */
class BamlCancelledErrorTest {

    @Test
    void isCancellationExceptionNotBamlError() {
        BamlCancelledError err =
                new BamlCancelledError("cancelled", List.of("frame"), "baml.panics.Cancelled");
        assertInstanceOf(CancellationException.class, err);
        assertInstanceOf(RuntimeException.class, err); // unchecked, like the siblings
        // Design B drops the BamlError parentage. (A literal `err instanceof
        // BamlError` no longer even compiles — proof enough — so check
        // reflectively.)
        assertFalse(BamlError.class.isInstance(err), "Design B drops the BamlError parentage");
        // Fields + accessors are preserved.
        assertEquals("cancelled", err.value());
        assertEquals(List.of("frame"), err.baml_trace());
        assertEquals("baml.panics.Cancelled", err.class_name());
    }

    @Test
    void futureCompletedWithItReadsAsCancelled() {
        // The core Design B classification: because BamlCancelledError IS a
        // CancellationException, completing a future exceptionally with it makes
        // isCancelled() true and surfaces it as a CancellationException on join()
        // — not a CompletionException. (BamlFfi's own CancellableCall additionally
        // unwraps join()/get() to the BamlCancelledError itself across JDKs; that
        // end-to-end direct-throw is covered by the TestCancellation suite.)
        BamlCancelledError err =
                new BamlCancelledError("cancelled", List.of(), "baml.panics.Cancelled");
        CompletableFuture<Object> future = new CompletableFuture<>();
        future.completeExceptionally(err);

        assertTrue(future.isCancelled(), "a CancellationException completion reads as cancelled");
        CancellationException thrown = assertThrows(CancellationException.class, future::join);
        // JDK ≤17 throws it as-is; JDK 19+ wraps it with the original as the cause.
        assertTrue(
                thrown == err || thrown.getCause() == err,
                "join() must surface the BamlCancelledError (directly or as cause)");
    }

    @Test
    void nullTraceBecomesEmptyList() {
        BamlCancelledError err = new BamlCancelledError(null, null, null);
        assertNotNull(err.baml_trace());
        assertTrue(err.baml_trace().isEmpty());
    }
}
