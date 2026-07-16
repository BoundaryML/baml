package baml_bridge;

/**
 * A BAML type token, used to bind a generic function/method's TypeVars at an
 * explicit-generics call site (Java generics are erased, so the type travels as
 * a value). Compile-only stub: the primitive constants exist as opaque tokens,
 * but the {@code of(...)} factories throw {@link UnsupportedOperationException}
 * until the explicit-generics capability lands.
 */
public final class BamlType {
    public static final BamlType INT = new BamlType();
    public static final BamlType STRING = new BamlType();
    public static final BamlType BOOL = new BamlType();
    public static final BamlType FLOAT = new BamlType();

    private BamlType() {}

    /** A type token for a generated class/enum. */
    public static BamlType of(Class<?> type) {
        throw notImplemented();
    }

    /** A type token for a parameterized generic (e.g. {@code Box<int>}). */
    public static BamlType of(Class<?> type, BamlType... typeArgs) {
        throw notImplemented();
    }

    private static UnsupportedOperationException notImplemented() {
        return new UnsupportedOperationException("capability not yet implemented: BamlType");
    }
}
