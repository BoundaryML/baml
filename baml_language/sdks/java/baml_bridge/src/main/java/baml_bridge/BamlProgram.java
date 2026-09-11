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
        var owners = new java.util.LinkedHashMap<Long, BamlProgram>();
        var seen = java.util.Collections.newSetFromMap(new java.util.IdentityHashMap<Object, Boolean>());
        for (var arg : args) collectOwners(arg, owners, seen);
        if (owners.size() > 1) throw new IllegalArgumentException("BAML arguments belong to different runtime registrations");
        var active = current();
        if (owners.isEmpty()) return active;
        var owner = owners.values().iterator().next();
        if (active.runtimeKey != 0) {
            if (active.runtimeKey != owner.runtimeKey) throw new IllegalArgumentException("BAML argument belongs to a different runtime registration");
            return active;
        }
        return owner;
    }

    private static void collectOwners(Object value, java.util.Map<Long, BamlProgram> owners, java.util.Set<Object> seen) {
        if (value == null || !seen.add(value)) return;
        BamlHandle handle = value instanceof BamlHandle h ? h : value instanceof BamlStream<?, ?> s ? s.bamlHandle() : value instanceof BamlFunctionSpec<?> s ? s.bamlHandle() : null;
        if (handle != null) {
            // Host values have their own namespace and carry no engine capability.
            if (handle.handleType() != BamlHandle.HOST_VALUE_CALLABLE && handle.handleType() != BamlHandle.HOST_VALUE_OPAQUE) owners.putIfAbsent(handle.program.runtimeKey, handle.program);
        } else if (value instanceof BamlTypedValue typed) {
            collectOwners(typed.value(), owners, seen);
        } else if (value instanceof java.util.List<?> list) {
            for (var item : list) collectOwners(item, owners, seen);
        } else if (value instanceof java.util.Map<?, ?> map) {
            for (var item : map.values()) collectOwners(item, owners, seen);
        } else if (value instanceof Object[] array) {
            for (var item : array) collectOwners(item, owners, seen);
        } else if (value.getClass().isRecord()) {
            try {
                for (var field : value.getClass().getRecordComponents()) collectOwners(field.getAccessor().invoke(value), owners, seen);
            } catch (ReflectiveOperationException e) {
                throw new IllegalArgumentException("Cannot inspect BAML argument record", e);
            }
        } else {
            for (var field : TypeRegistry.argumentFields(value)) collectOwners(field, owners, seen);
        }
    }
}
