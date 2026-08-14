package baml_bridge;

import java.util.Collections;
import java.util.LinkedHashMap;
import java.util.Map;
import java.util.Objects;
import java.util.Set;

/**
 * An ordered, immutable bag of named {@link BamlType} bindings (the JVM analog
 * of Python's {@code _types=} keyword), passed at an explicit-generics call to
 * bind a generic function/method's TypeVars. Each binding names a TypeVar (e.g.
 * {@code "T"}) and the concrete {@link BamlType} it resolves to.
 *
 * <p>Built fluently and immutably: {@link #of(String, BamlType)} starts a bag
 * with one entry and {@link #and(String, BamlType)} returns a <em>new</em> bag
 * with an additional entry (the receiver is unchanged). Insertion order is
 * preserved — the encoder sends bindings in that order (De Bruijn order:
 * enclosing class params first, then the callee's own {@code <...>} params).
 * A duplicate name is rejected with {@link IllegalArgumentException}.
 */
public final class BamlTypes {
    // Insertion-ordered so the encoder emits bindings in De Bruijn order.
    private final LinkedHashMap<String, BamlType> bindings;

    private BamlTypes(LinkedHashMap<String, BamlType> bindings) {
        this.bindings = bindings;
    }

    /** Start a binding bag with one {@code name -> type} entry. */
    public static BamlTypes of(String name, BamlType type) {
        LinkedHashMap<String, BamlType> m = new LinkedHashMap<>();
        m.put(requireName(name), requireType(type, name));
        return new BamlTypes(m);
    }

    /**
     * Return a new bag with an additional {@code name -> type} entry appended
     * after this bag's entries. Rejects a name already bound in this bag.
     */
    public BamlTypes and(String name, BamlType type) {
        requireName(name);
        requireType(type, name);
        if (bindings.containsKey(name)) {
            throw new IllegalArgumentException("duplicate type binding name: " + name);
        }
        LinkedHashMap<String, BamlType> next = new LinkedHashMap<>(bindings);
        next.put(name, type);
        return new BamlTypes(next);
    }

    /** Whether this bag has no bindings (never true for a bag built via {@link #of}). */
    public boolean isEmpty() {
        return bindings.isEmpty();
    }

    /**
     * The ordered {@code name -> type} bindings, for the call encoder. Internal:
     * iteration order is insertion (De Bruijn) order; the returned view is
     * unmodifiable.
     */
    public Set<Map.Entry<String, BamlType>> bindings() {
        return Collections.unmodifiableMap(bindings).entrySet();
    }

    private static String requireName(String name) {
        Objects.requireNonNull(name, "binding name");
        if (name.isEmpty()) {
            throw new IllegalArgumentException("binding name must be non-empty");
        }
        return name;
    }

    private static BamlType requireType(BamlType type, String name) {
        if (type == null) {
            throw new IllegalArgumentException("null type for binding '" + name + "'");
        }
        return type;
    }

    // -- value semantics (order-insensitive, like the underlying map) ---------

    @Override
    public boolean equals(Object o) {
        return o instanceof BamlTypes other && bindings.equals(other.bindings);
    }

    @Override
    public int hashCode() {
        return bindings.hashCode();
    }

    @Override
    public String toString() {
        return "BamlTypes" + bindings;
    }
}
