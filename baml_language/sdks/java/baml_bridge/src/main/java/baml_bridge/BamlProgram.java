package baml_bridge;

import java.util.concurrent.CompletableFuture;
import java.util.function.Supplier;

/** A generated SDK's uint64 registration and independent Java type map. */
public final class BamlProgram {
    private static final ThreadLocal<BamlProgram> ACTIVE = new ThreadLocal<>();
    private static final BamlProgram LEGACY = new BamlProgram(0, BamlProgram.class.getClassLoader());
    public final long runtimeKey;
    final ClassLoader loader;
    final TypeRegistry.Maps types = new TypeRegistry.Maps();

    public BamlProgram(long runtimeKey, ClassLoader loader) {
        this.runtimeKey = runtimeKey;
        this.loader = loader;
    }
    static BamlProgram current() { var p = ACTIVE.get(); return p == null ? LEGACY : p; }
    public Scope enter() { var previous = ACTIVE.get(); ACTIVE.set(this); return new Scope(previous); }
    public static final class Scope implements AutoCloseable {
        private final BamlProgram previous;
        private Scope(BamlProgram previous) { this.previous = previous; }
        public void close() { if (previous == null) ACTIVE.remove(); else ACTIVE.set(previous); }
    }
    <T> T within(Supplier<T> operation) { try (var scope = enter()) { return operation.get(); } }
    public Object callSync(String fqn, String[] names, Object[] args, BamlType desc) { return callSync(fqn, names, args, desc, null, null); }
    public Object callSync(String fqn, String[] names, Object[] args, BamlType desc, BamlCallContext ctx) { return callSync(fqn, names, args, desc, ctx, null); }
    public Object callSync(String fqn, String[] names, Object[] args, BamlType desc, BamlCallContext ctx, BamlTypes types) {
        return within(() -> BamlFfi.callSync(fqn, names, args, desc, ctx, types));
    }
    public CompletableFuture<Object> callAsync(String fqn, String[] names, Object[] args, BamlType desc) { return callAsync(fqn, names, args, desc, null, null); }
    public CompletableFuture<Object> callAsync(String fqn, String[] names, Object[] args, BamlType desc, BamlCallContext ctx) { return callAsync(fqn, names, args, desc, ctx, null); }
    public CompletableFuture<Object> callAsync(String fqn, String[] names, Object[] args, BamlType desc, BamlCallContext ctx, BamlTypes types) {
        return within(() -> BamlFfi.callAsync(fqn, names, args, desc, ctx, types));
    }
    static BamlProgram forArgs(Object[] args) {
        for (var arg : args) {
            if (arg instanceof BamlTypedValue typed) arg = typed.value();
            BamlHandle handle = arg instanceof BamlHandle h ? h : arg instanceof BamlStream<?, ?> s ? s.bamlHandle() : arg instanceof BamlFunctionSpec<?> s ? s.bamlHandle() : null;
            if (handle != null) return handle.program;
        }
        return current();
    }
}
