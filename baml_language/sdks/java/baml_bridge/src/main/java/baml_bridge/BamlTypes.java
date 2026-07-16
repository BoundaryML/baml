package baml_bridge;

/**
 * An ordered bag of named {@link BamlType} bindings (the {@code _types=}
 * analog), passed at an explicit-generics call. Compile-only stub: the builder
 * methods throw {@link UnsupportedOperationException} until explicit generics
 * land.
 */
public final class BamlTypes {
    private BamlTypes() {}

    /** Start a binding set with one {@code name -> type} entry. */
    public static BamlTypes of(String name, BamlType type) {
        throw notImplemented();
    }

    /** Chain another {@code name -> type} binding. */
    public BamlTypes and(String name, BamlType type) {
        throw notImplemented();
    }

    private static UnsupportedOperationException notImplemented() {
        return new UnsupportedOperationException("capability not yet implemented: BamlTypes");
    }
}
