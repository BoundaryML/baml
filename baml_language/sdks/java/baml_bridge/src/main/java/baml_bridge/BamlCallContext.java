package baml_bridge;

import java.util.ArrayList;
import java.util.List;

/**
 * A call context for cancelling in-flight BAML function calls. The JVM analog
 * of {@code bridge_python}'s {@code BamlCallContext}
 * ({@code bridge_python/src/baml_call_context.rs}), with the same semantics.
 *
 * <p>Pass one to any generated binding's trailing-{@code ctx} overload
 * ({@code Fns.MyFunc(args, ctx)} / {@code Fns.MyFunc_async(args, ctx)}); a
 * single context may govern several concurrent calls. {@link #abort()} cancels
 * every call currently bound to it and latches the aborted flag, so a call that
 * {@linkplain #attach attaches} after the abort is cancelled immediately
 * (abort-before-start). All state is guarded by an internal monitor, so a
 * context is safe to abort from any thread.
 */
public final class BamlCallContext {
    private final Object lock = new Object();
    private boolean aborted = false;
    private final List<Long> activeCallIds = new ArrayList<>();

    public BamlCallContext() {}

    /**
     * Request cancellation of every call currently bound to this context and
     * latch the aborted state. A running call is interrupted at its next
     * cancellation check point (before HTTP calls, between retries, etc.).
     * Idempotent — calling {@code abort()} repeatedly is harmless, and the
     * per-id engine cancel is itself idempotent.
     */
    public void abort() {
        List<Long> toCancel;
        synchronized (lock) {
            aborted = true;
            toCancel = new ArrayList<>(activeCallIds);
        }
        for (long callId : toCancel) {
            BamlFfi.nativeCancelFunctionCall(callId);
        }
    }

    /** Whether {@link #abort()} has been called. */
    public boolean aborted() {
        synchronized (lock) {
            return aborted;
        }
    }

    /**
     * Bind this context to an in-flight call id for the duration of one call.
     * Registers the id (so a later {@link #abort()} reaches it) and, if this
     * context is already aborted, cancels the id immediately so an abort that
     * happened before the call started still takes effect. Package-private:
     * the runtime ({@link BamlFfi}) calls it around each ctx-carrying call.
     */
    void attach(long callId) {
        boolean cancelNow;
        synchronized (lock) {
            if (!activeCallIds.contains(callId)) {
                activeCallIds.add(callId);
            }
            cancelNow = aborted;
        }
        if (cancelNow) {
            BamlFfi.nativeCancelFunctionCall(callId);
        }
    }

    /** Remove a call-id binding installed by {@link #attach(long)} (on completion). */
    void detach(long callId) {
        synchronized (lock) {
            activeCallIds.remove(Long.valueOf(callId));
        }
    }
}
