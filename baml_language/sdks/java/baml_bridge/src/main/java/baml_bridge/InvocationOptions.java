package baml_bridge;

/** Private lowering of the generated SDK's immutable BamlOptions. */
public class InvocationOptions {
    public static final InvocationOptions EMPTY = new InvocationOptions(null, null, null);
    private final BamlHandle trace; private final Object cancel; private final Long timeoutMs;
    protected InvocationOptions(BamlHandle trace, Object cancel, Long timeoutMs) { this.trace = trace; this.cancel = cancel; this.timeoutMs = timeoutMs; }
    public final BamlHandle trace() { return trace; } public final Object cancel() { return cancel; } public final Long timeoutMs() { return timeoutMs; }
    public record TracePayload(byte[] bytes, long reservation) {}
}
