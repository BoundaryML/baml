package baml_bridge.internal;

import baml_bridge.BamlHandle;
import java.util.ArrayList;
import java.util.List;

/**
 * The references one request hands the engine, taken back if the request
 * never reaches it: a handle cloned for the wire is released, and a host
 * callable registered for it is retired. Once the engine has the request it
 * owns them ({@link #submitted}), and nothing is taken back.
 */
public final class EncodingScope implements AutoCloseable {
    private static final ThreadLocal<EncodingScope> CURRENT = new ThreadLocal<>();
    private final EncodingScope prior;
    private final List<Runnable> rollbacks = new ArrayList<>();
    private boolean submitted;

    public EncodingScope() {
        prior = CURRENT.get();
        CURRENT.set(this);
    }

    /** The handle key {@code key} of kind {@code kind} was cloned for the active request. */
    public static void own(long key, int kind) {
        if (key != 0) {
            onRollback(() -> new BamlHandle(key, kind).close());
        }
    }

    /** {@code rollback} undoes a reference the active request took, if the request is never sent. */
    public static void onRollback(Runnable rollback) {
        EncodingScope active = CURRENT.get();
        if (active != null) {
            active.rollbacks.add(rollback);
        }
    }

    /** The engine has the request, and with it every reference the request carries. */
    public void submitted() {
        submitted = true;
    }

    @Override
    public void close() {
        if (prior == null) {
            CURRENT.remove();
        } else {
            CURRENT.set(prior);
        }
        if (!submitted) {
            for (Runnable rollback : rollbacks) {
                rollback.run();
            }
        }
        rollbacks.clear();
    }
}
