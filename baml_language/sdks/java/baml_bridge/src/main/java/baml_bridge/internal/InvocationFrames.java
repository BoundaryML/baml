package baml_bridge.internal;

import baml_bridge.BamlHandle;
import java.util.concurrent.Executor;
import java.util.function.Function;

/** Owned inheritance state; captured frames never reuse a callback identity. */
public final class InvocationFrames {
    public record Frame(BamlHandle state, Object cancel) {
        public Frame capture() { return new Frame(state.cloneOwned(), cancel); }
        public <T> T run(java.util.function.Supplier<T> body) {
            Frame prior = CURRENT.get(); CURRENT.set(this);
            try { return body.get(); } finally { if (prior == null) CURRENT.remove(); else CURRENT.set(prior); }
        }
        public <T,R> Function<T,R> wrapFunction(Function<T,R> body) { return value -> run(() -> body.apply(value)); }
        public Runnable wrapRunnable(Runnable body) { return () -> run(() -> { body.run(); return null; }); }
        public Executor executor(Executor delegate) { return body -> delegate.execute(wrapRunnable(body)); }
    }
    public static final ThreadLocal<Frame> CURRENT = new ThreadLocal<>();
    private InvocationFrames() {}
    public static long currentState() { Frame frame = CURRENT.get(); return frame == null ? 0 : frame.state().key(); }
}
