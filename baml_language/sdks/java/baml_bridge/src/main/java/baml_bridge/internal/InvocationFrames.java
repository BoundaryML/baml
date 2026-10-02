package baml_bridge.internal;

import baml_bridge.BamlHandle;

/** Private transport carrier. Generated invocation bindings provide explicit capture. */
public final class InvocationFrames {
    public static final ThreadLocal<BamlHandle> CURRENT = new ThreadLocal<>();
    private InvocationFrames() {}
    public static long currentState() {
        BamlHandle state = CURRENT.get();
        return state == null ? 0 : state.key();
    }
}
