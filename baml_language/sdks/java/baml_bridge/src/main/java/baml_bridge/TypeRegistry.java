package baml_bridge;

import java.lang.reflect.Constructor;
import java.lang.reflect.Method;
import java.util.LinkedHashMap;
import java.util.Map;
import java.util.concurrent.ConcurrentHashMap;

/**
 * The generated SDK's type map: BAML fully-qualified name (FQN) &harr; generated
 * Java class, with the field/variant order the wire codec needs. Populated by the
 * static initializer of the generated {@code baml_sdk.Baml} anchor (before
 * {@code initFromBytecode}) via {@link #registerClass} / {@link #registerEnum},
 * one call per user class/enum:
 *
 * <pre>{@code
 * TypeRegistry.registerClass("user.lorem.Resume", "baml_sdk.lorem.Resume",
 *                            new String[] {"name", "age"});
 * TypeRegistry.registerEnum("user.ipsum.Sentiment", "baml_sdk.ipsum.Sentiment",
 *                           new String[] {"Positive", "new$"},   // Java constants
 *                           new String[] {"Positive", "new"});   // wire variants
 * }</pre>
 *
 * <p>The two enum arrays are parallel: {@code javaConstants[i]} is the generated
 * Java {@code enum} constant name and {@code wireNames[i]} the BAML variant
 * spelling; they differ only when a variant needed Java-keyword escaping
 * (BAML {@code new} &rarr; Java {@code new$}).
 *
 * <h2>Directions</h2>
 * <ul>
 *   <li><b>Decode</b> (BAML&rarr;Java, outbound): {@link #constructClass} /
 *       {@link #resolveEnum} resolve by FQN and reify the generated class/enum.
 *       An unregistered FQN returns {@code null} so the caller keeps its lenient
 *       map/string fallback.</li>
 *   <li><b>Encode</b> (Java&rarr;BAML, inbound): {@link #classWire} /
 *       {@link #enumWire} resolve by the object's Java class and surface the FQN
 *       plus the field/variant wire payload; {@code null} means "not a registered
 *       type" so the caller can reject it.</li>
 * </ul>
 *
 * <p>Java {@link Class} objects are resolved lazily via {@link Class#forName}
 * (store the name at registration, resolve + cache on first decode use); the
 * reverse index is keyed by the registered Java binary name, so encode never
 * forces a {@code Class.forName}. All maps are {@link ConcurrentHashMap}s and
 * registration is idempotent (first registration of an FQN wins).
 */
public final class TypeRegistry {
    // Forward index (decode): BAML FQN -> entry.
    private static final ConcurrentHashMap<String, ClassEntry> classesByFqn = new ConcurrentHashMap<>();
    private static final ConcurrentHashMap<String, EnumEntry> enumsByFqn = new ConcurrentHashMap<>();

    // Reverse index (encode): generated Java binary name -> entry. Keyed by the
    // registered class name string (not the loaded Class), so encode resolves a
    // host object's type without ever loading it via Class.forName.
    private static final ConcurrentHashMap<String, ClassEntry> classesByJavaName = new ConcurrentHashMap<>();
    private static final ConcurrentHashMap<String, EnumEntry> enumsByJavaName = new ConcurrentHashMap<>();

    // Unions. Forward index (decode): the sorted arm-token signature -> entry.
    // Reverse index (encode): each generated union RECORD binary name -> a
    // reflective unwrapper for its single {@code value()} accessor.
    private static final ConcurrentHashMap<String, UnionEntry> unionsBySignature = new ConcurrentHashMap<>();
    private static final ConcurrentHashMap<String, UnionRecordEntry> unionRecordsByJavaName =
            new ConcurrentHashMap<>();

    private TypeRegistry() {}

    // -- registration --------------------------------------------------------

    /**
     * Register a generated class. {@code fieldOrder} is the class's declaration
     * order — the order of the canonical all-args constructor and the field
     * accessor methods. Idempotent: the first registration of {@code bamlFqn}
     * wins (a redundant re-registration is a no-op, preserving cached state).
     */
    public static void registerClass(String bamlFqn, String javaClassName, String[] fieldOrder) {
        ClassEntry entry = new ClassEntry(bamlFqn, javaClassName, fieldOrder);
        if (classesByFqn.putIfAbsent(bamlFqn, entry) == null) {
            classesByJavaName.putIfAbsent(javaClassName, entry);
        }
    }

    /**
     * Register a generated enum. {@code javaConstants} are the generated Java
     * {@code enum} constant names and {@code wireNames} the parallel BAML variant
     * spellings (same length). Idempotent: the first registration of
     * {@code bamlFqn} wins.
     */
    public static void registerEnum(
            String bamlFqn, String javaClassName, String[] javaConstants, String[] wireNames) {
        if (javaConstants.length != wireNames.length) {
            throw new IllegalArgumentException(
                    "enum arrays length mismatch for " + bamlFqn + ": "
                            + javaConstants.length + " constants vs " + wireNames.length + " wire names");
        }
        EnumEntry entry = new EnumEntry(bamlFqn, javaClassName, javaConstants, wireNames);
        if (enumsByFqn.putIfAbsent(bamlFqn, entry) == null) {
            enumsByJavaName.putIfAbsent(javaClassName, entry);
        }
    }

    /**
     * Register a generated union. {@code signature} is the union's arm tokens
     * sorted and joined with {@code |} (the same tokenizer the wire decoder runs
     * over the union's {@code self_type}); {@code sealedInterfaceName} the binary
     * name of the generated sealed interface (retained for completeness);
     * {@code armTokens} the per-arm tokens in <em>declaration</em> order; and
     * {@code recordNames} the parallel binary names of the wrapper records
     * (one {@code record ...Value(T value)} per arm). Decode resolves an arm from
     * the inner value's shape (see {@link #constructUnion}); encode unwraps a
     * record instance back to its bare inner value (see {@link #unionRecordInner}).
     * Idempotent: the first registration of {@code signature} wins.
     */
    public static void registerUnion(
            String signature, String sealedInterfaceName, String[] armTokens, String[] recordNames) {
        if (armTokens.length != recordNames.length) {
            throw new IllegalArgumentException(
                    "union arrays length mismatch for " + signature + ": "
                            + armTokens.length + " arm tokens vs " + recordNames.length + " records");
        }
        UnionEntry entry = new UnionEntry(signature, sealedInterfaceName, armTokens, recordNames);
        unionsBySignature.putIfAbsent(signature, entry);
        // Record-name registration is unconditional: encode must be able to
        // unwrap ANY registered record class, independent of which
        // registration won the signature slot.
        for (String recordName : recordNames) {
            unionRecordsByJavaName.putIfAbsent(recordName, new UnionRecordEntry(recordName));
        }
    }

    // -- decode (BAML FQN -> generated instance) -----------------------------

    /**
     * Construct the generated class bound to {@code bamlFqn} from a field-name
     * &rarr; value map (a field absent from the map is passed as {@code null}).
     * Returns {@code null} when {@code bamlFqn} is not registered, so the caller
     * can fall back to a plain field map.
     */
    public static Object constructClass(String bamlFqn, Map<String, Object> fields) {
        ClassEntry entry = classesByFqn.get(bamlFqn);
        return entry == null ? null : entry.instantiate(fields);
    }

    /**
     * Resolve the generated enum constant bound to {@code bamlFqn} for a given
     * wire variant name. Returns {@code null} when the FQN is not registered or
     * the wire variant is unknown, so the caller can fall back to the raw variant
     * string.
     */
    public static Object resolveEnum(String bamlFqn, String wireName) {
        EnumEntry entry = enumsByFqn.get(bamlFqn);
        return entry == null ? null : entry.constantFor(wireName);
    }

    /**
     * Construct the generated union record for a wire union value. {@code signature}
     * is the union's sorted arm-token signature; {@code discriminator} the token
     * describing the inner value's shape (a primitive/FQN token, {@code "list"} /
     * {@code "map"} for a container arm, or {@code "lit:<base>:<value>"} for a
     * literal); {@code inner} the already-decoded inner value. Returns the wrapper
     * record, or {@code null} when the signature is not a registered union or no
     * arm matches the discriminator — the caller then keeps the bare inner value
     * (literal-over-one-base unions are erased in codegen and never registered).
     */
    public static Object constructUnion(String signature, String discriminator, Object inner) {
        UnionEntry entry = unionsBySignature.get(signature);
        return entry == null ? null : entry.instantiate(discriminator, inner);
    }

    // -- encode (Java object -> FQN + wire payload) --------------------------

    /** Whether {@code obj}'s class is a registered generated union wrapper record. */
    public static boolean isUnionRecord(Object obj) {
        return unionRecordsByJavaName.containsKey(obj.getClass().getName());
    }

    /**
     * The bare inner value carried by a union wrapper record (its {@code value()}
     * accessor). Callers must gate on {@link #isUnionRecord} first; passing a
     * non-record object throws.
     */
    public static Object unionRecordInner(Object obj) {
        UnionRecordEntry entry = unionRecordsByJavaName.get(obj.getClass().getName());
        if (entry == null) {
            throw new IllegalStateException("not a registered union record: " + obj.getClass().getName());
        }
        return entry.inner(obj);
    }

    /**
     * The class-value wire payload for a host object, or {@code null} when the
     * object's class is not a registered generated class.
     */
    public static ClassWire classWire(Object obj) {
        ClassEntry entry = classesByJavaName.get(obj.getClass().getName());
        return entry == null ? null : entry.encode(obj);
    }

    /**
     * The enum-value wire payload for a host enum constant, or {@code null} when
     * the constant's enum type is not a registered generated enum.
     */
    public static EnumWire enumWire(Enum<?> constant) {
        // getDeclaringClass (not getClass): a constant with a body is an anonymous
        // subclass, but registration keys on the enum type itself.
        EnumEntry entry = enumsByJavaName.get(constant.getDeclaringClass().getName());
        if (entry == null) {
            return null;
        }
        String wire = entry.wireFor(constant.name());
        return wire == null ? null : new EnumWire(entry.bamlFqn, wire);
    }

    // -- encode payloads -----------------------------------------------------

    /** A class instance decomposed for the inbound {@code class_value} arm. */
    public static final class ClassWire {
        /** The BAML FQN, emitted on {@code InboundClassValue.class_ty.name}. */
        public final String fqn;
        /** Field names in declaration order (the {@code fields} entry keys). */
        public final String[] fieldNames;
        /** Field values in declaration order, read via the accessor methods. */
        public final Object[] fieldValues;

        ClassWire(String fqn, String[] fieldNames, Object[] fieldValues) {
            this.fqn = fqn;
            this.fieldNames = fieldNames;
            this.fieldValues = fieldValues;
        }
    }

    /** An enum constant decomposed for the inbound {@code enum_value} arm. */
    public static final class EnumWire {
        /** The BAML FQN, emitted on {@code InboundEnumValue.name}. */
        public final String fqn;
        /** The BAML variant spelling, emitted on {@code InboundEnumValue.value}. */
        public final String wireName;

        EnumWire(String fqn, String wireName) {
            this.fqn = fqn;
            this.wireName = wireName;
        }
    }

    // -- entries -------------------------------------------------------------

    private static final class ClassEntry {
        final String bamlFqn;
        final String javaClassName;
        final String[] fieldOrder;

        // Lazily resolved (decode/encode reflection). Benign races only re-resolve
        // to the same value; volatile publishes the resolved object safely.
        private volatile Class<?> resolved;
        private volatile Constructor<?> ctor;
        private volatile Method[] accessors;

        ClassEntry(String bamlFqn, String javaClassName, String[] fieldOrder) {
            this.bamlFqn = bamlFqn;
            this.javaClassName = javaClassName;
            this.fieldOrder = fieldOrder;
        }

        private Class<?> javaClass() {
            Class<?> c = resolved;
            if (c == null) {
                try {
                    c = Class.forName(javaClassName);
                } catch (ClassNotFoundException e) {
                    throw new IllegalStateException(
                            "generated class not found on the classpath: " + javaClassName, e);
                }
                resolved = c;
            }
            return c;
        }

        Object instantiate(Map<String, Object> fields) {
            Constructor<?> c = constructor();
            Object[] args = new Object[fieldOrder.length];
            for (int i = 0; i < fieldOrder.length; i++) {
                // A field absent from the wire decodes to null; primitive ctor
                // params would then NPE on unbox, but the engine always emits
                // every declared field for a well-formed class value.
                args[i] = fields.get(fieldOrder[i]);
            }
            try {
                return c.newInstance(args);
            } catch (ReflectiveOperationException e) {
                throw new IllegalStateException(
                        "failed to construct " + javaClassName + " from wire fields", e);
            }
        }

        private Constructor<?> constructor() {
            Constructor<?> c = ctor;
            if (c == null) {
                Constructor<?>[] all = javaClass().getConstructors(); // public only
                if (all.length == 1) {
                    c = all[0];
                } else {
                    // Prefer the canonical all-args constructor by arity.
                    for (Constructor<?> cand : all) {
                        if (cand.getParameterCount() == fieldOrder.length) {
                            c = cand;
                            break;
                        }
                    }
                    if (c == null) {
                        throw new IllegalStateException(
                                "no public constructor with " + fieldOrder.length
                                        + " params on " + javaClassName);
                    }
                }
                ctor = c;
            }
            return c;
        }

        ClassWire encode(Object obj) {
            Method[] acc = accessors();
            Object[] values = new Object[fieldOrder.length];
            for (int i = 0; i < fieldOrder.length; i++) {
                try {
                    values[i] = acc[i].invoke(obj);
                } catch (ReflectiveOperationException e) {
                    throw new IllegalStateException(
                            "failed to read field " + fieldOrder[i] + "() on " + javaClassName, e);
                }
            }
            return new ClassWire(bamlFqn, fieldOrder, values);
        }

        private Method[] accessors() {
            Method[] m = accessors;
            if (m == null) {
                Class<?> cls = javaClass();
                m = new Method[fieldOrder.length];
                for (int i = 0; i < fieldOrder.length; i++) {
                    try {
                        // Generated accessor: public zero-arg, named exactly the
                        // field (PreserveCase — never Field reflection).
                        m[i] = cls.getMethod(fieldOrder[i]);
                    } catch (NoSuchMethodException e) {
                        throw new IllegalStateException(
                                "no accessor " + fieldOrder[i] + "() on " + javaClassName, e);
                    }
                }
                accessors = m;
            }
            return m;
        }
    }

    private static final class EnumEntry {
        final String bamlFqn;
        final String javaClassName;
        final String[] javaConstants;
        final String[] wireNames;

        // Java constant name -> wire variant (encode). Cheap; built eagerly.
        private final Map<String, String> constToWire;

        // Wire variant -> resolved Java enum constant (decode). Lazily built so a
        // never-decoded enum never triggers Class.forName.
        private volatile Map<String, Enum<?>> wireToConstant;

        EnumEntry(String bamlFqn, String javaClassName, String[] javaConstants, String[] wireNames) {
            this.bamlFqn = bamlFqn;
            this.javaClassName = javaClassName;
            this.javaConstants = javaConstants;
            this.wireNames = wireNames;
            Map<String, String> c2w = new LinkedHashMap<>();
            for (int i = 0; i < javaConstants.length; i++) {
                c2w.put(javaConstants[i], wireNames[i]);
            }
            this.constToWire = c2w;
        }

        /** Wire variant for a Java constant name ({@code Enum.name()}); null if unknown. */
        String wireFor(String javaConstantName) {
            return constToWire.get(javaConstantName);
        }

        /** Resolved enum constant for a wire variant; null if the variant is unknown. */
        Enum<?> constantFor(String wireName) {
            return wireToConstant().get(wireName);
        }

        @SuppressWarnings({"unchecked", "rawtypes"})
        private Map<String, Enum<?>> wireToConstant() {
            Map<String, Enum<?>> m = wireToConstant;
            if (m == null) {
                Class<?> cls;
                try {
                    cls = Class.forName(javaClassName);
                } catch (ClassNotFoundException e) {
                    throw new IllegalStateException(
                            "generated enum not found on the classpath: " + javaClassName, e);
                }
                m = new LinkedHashMap<>();
                for (int i = 0; i < javaConstants.length; i++) {
                    Enum<?> constant = Enum.valueOf((Class) cls, javaConstants[i]);
                    m.put(wireNames[i], constant);
                }
                wireToConstant = m;
            }
            return m;
        }
    }

    private static final class UnionEntry {
        final String signature;
        final String sealedInterfaceName;
        final String[] armTokens;
        final String[] recordNames;

        // Lazily resolved per-arm single-arg record constructors. Benign races
        // only re-resolve a slot to an equal Constructor.
        private volatile Constructor<?>[] ctors;

        UnionEntry(String signature, String sealedInterfaceName, String[] armTokens, String[] recordNames) {
            this.signature = signature;
            this.sealedInterfaceName = sealedInterfaceName;
            this.armTokens = armTokens;
            this.recordNames = recordNames;
        }

        /** Build the wrapper record for {@code discriminator}, or null if no arm matches. */
        Object instantiate(String discriminator, Object inner) {
            int idx = pickArm(discriminator);
            if (idx < 0) {
                return null;
            }
            Constructor<?> c = constructor(idx);
            try {
                return c.newInstance(inner); // autoboxing/unboxing applies to the sole arg
            } catch (ReflectiveOperationException e) {
                throw new IllegalStateException(
                        "failed to construct union record " + recordNames[idx], e);
            }
        }

        /**
         * The declaration-order index of the arm matching {@code discriminator}, or
         * -1. A {@code "list"} / {@code "map"} discriminator picks the first arm whose
         * token is a {@code list<...>} / {@code map<...>}; a {@code "lit:<base>:..."}
         * discriminator prefers an exact literal token then the first literal arm of
         * that base; every other discriminator (primitives and class/enum FQNs)
         * matches an arm token exactly.
         */
        private int pickArm(String discriminator) {
            if (discriminator.equals("list")) {
                return firstWithPrefix("list<");
            }
            if (discriminator.equals("map")) {
                return firstWithPrefix("map<");
            }
            if (discriminator.startsWith("lit:")) {
                for (int i = 0; i < armTokens.length; i++) {
                    if (armTokens[i].equals(discriminator)) {
                        return i;
                    }
                }
                int base = discriminator.indexOf(':', 4); // after "lit:<base>"
                String prefix = base < 0 ? discriminator : discriminator.substring(0, base + 1);
                return firstWithPrefix(prefix);
            }
            for (int i = 0; i < armTokens.length; i++) {
                if (armTokens[i].equals(discriminator)) {
                    return i;
                }
            }
            return -1;
        }

        private int firstWithPrefix(String prefix) {
            for (int i = 0; i < armTokens.length; i++) {
                if (armTokens[i].startsWith(prefix)) {
                    return i;
                }
            }
            return -1;
        }

        private Constructor<?> constructor(int idx) {
            Constructor<?>[] cs = ctors;
            if (cs == null) {
                cs = new Constructor<?>[recordNames.length];
                ctors = cs;
            }
            Constructor<?> c = cs[idx];
            if (c == null) {
                c = resolveConstructor(recordNames[idx]);
                cs[idx] = c;
            }
            return c;
        }

        private static Constructor<?> resolveConstructor(String recordName) {
            Class<?> cls;
            try {
                cls = Class.forName(recordName);
            } catch (ClassNotFoundException e) {
                throw new IllegalStateException(
                        "generated union record not found on the classpath: " + recordName, e);
            }
            Constructor<?>[] all = cls.getConstructors(); // public only
            if (all.length == 1) {
                return all[0];
            }
            // Prefer the canonical single-component constructor by arity.
            for (Constructor<?> cand : all) {
                if (cand.getParameterCount() == 1) {
                    return cand;
                }
            }
            throw new IllegalStateException(
                    "no single-arg constructor on union record " + recordName);
        }
    }

    private static final class UnionRecordEntry {
        final String recordName;

        // Lazily resolved value() accessor (from the object's own Class — encode
        // never forces a Class.forName).
        private volatile Method valueAccessor;

        UnionRecordEntry(String recordName) {
            this.recordName = recordName;
        }

        Object inner(Object obj) {
            Method m = valueAccessor;
            if (m == null) {
                try {
                    m = obj.getClass().getMethod("value");
                } catch (NoSuchMethodException e) {
                    throw new IllegalStateException(
                            "no value() accessor on union record " + recordName, e);
                }
                valueAccessor = m;
            }
            try {
                return m.invoke(obj);
            } catch (ReflectiveOperationException e) {
                throw new IllegalStateException(
                        "failed to read value() on union record " + recordName, e);
            }
        }
    }
}
