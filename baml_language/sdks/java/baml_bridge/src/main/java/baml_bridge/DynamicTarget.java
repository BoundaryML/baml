package baml_bridge;

/** Closed runtime lowering: a name or an owning returned closure. */
public final class DynamicTarget {
    final String name;
    final BamlHandle handle;
    private DynamicTarget(String name, BamlHandle handle) { this.name = name; this.handle = handle; }
    public static DynamicTarget named(String name) { if (name == null || name.isEmpty()) throw new IllegalArgumentException("empty invocation target"); return new DynamicTarget(name, null); }
    static DynamicTarget callable(BamlHandle handle) { return new DynamicTarget(null, handle); }
}
