package baml_bridge;

import baml_bridge.internal.ProtoReader;

import java.lang.ref.Reference;
import java.lang.ref.ReferenceQueue;
import java.lang.ref.WeakReference;
import java.lang.reflect.Constructor;
import java.lang.reflect.Method;
import java.util.Collection;
import java.util.HashMap;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;
import java.util.TreeSet;
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
    static final class Maps {
    // Forward index (decode): BAML FQN -> entry.
    final ConcurrentHashMap<String, ClassEntry> classesByFqn = new ConcurrentHashMap<>();
    final ConcurrentHashMap<String, EnumEntry> enumsByFqn = new ConcurrentHashMap<>();

    // Reverse index (encode): generated Java binary name -> entry. Keyed by the
    // registered class name string (not the loaded Class), so encode resolves a
    // host object's type without ever loading it via Class.forName.
    final ConcurrentHashMap<String, ClassEntry> classesByJavaName = new ConcurrentHashMap<>();
    final ConcurrentHashMap<String, EnumEntry> enumsByJavaName = new ConcurrentHashMap<>();

    // Unions. Forward index (decode): keyed structurally by the union's arm set
    // normalized to a sorted, distinct {@code List<BamlType>} ({@code List.equals}
    // rides {@code BamlType} value equality — order- and duplicate-insensitive,
    // exactly the union-identity semantics, with no string derivation) for a
    // resolved-union {@code self_type}, and by alias FQN for a recursive alias
    // whose {@code self_type} arrives as the alias node. Reverse index (encode):
    // each generated union RECORD binary name -> a reflective unwrapper for its
    // single {@code value()} accessor.
    final ConcurrentHashMap<List<BamlType>, UnionEntry> unionsByArms = new ConcurrentHashMap<>();
    final ConcurrentHashMap<String, UnionEntry> unionsByFqn = new ConcurrentHashMap<>();
    final ConcurrentHashMap<String, UnionRecordEntry> unionRecordsByJavaName =
            new ConcurrentHashMap<>();

    }
    private static Maps maps() { return BamlProgram.current().types; }

    // Host class tokens may be constructed before entering a call's program scope.
    // Only immutable nominal identities are shared here, keyed by the actual loader.
    // Values contain strings, so the weak keys do not retain unloaded SDK loaders.
    private static final Map<ClassLoader, Map<String, String[]>> CLASS_FIELDS = new java.util.WeakHashMap<>();
    private static final Map<ClassLoader, Map<String, String>> CLASS_IDENTITIES = new java.util.WeakHashMap<>();
    private static final Map<ClassLoader, Map<String, String>> ENUM_IDENTITIES = new java.util.WeakHashMap<>();

    private static synchronized void registerIdentity(
            Map<ClassLoader, Map<String, String>> identities, String javaName, String fqn) {
        var names = identities.computeIfAbsent(BamlProgram.current().loader, ignored -> new HashMap<>());
        var previous = names.putIfAbsent(javaName, fqn);
        if (previous != null && !previous.equals(fqn)) {
            throw new IllegalArgumentException("Conflicting BAML identity for Java class " + javaName);
        }
    }

    private static synchronized String identityFor(
            Map<ClassLoader, Map<String, String>> identities, Class<?> type) {
        var names = identities.get(type.getClassLoader());
        return names == null ? null : names.get(type.getName());
    }

    static {
        // Runtime-owned stdlib types the emitter deliberately does NOT generate
        // (RUNTIME_OWNED_FQNS in sdkgen_java) — their bodies ship in this runtime
        // library, so they must be registered here rather than by generated
        // Baml.java. Done is the "finished" sentinel decoded from a
        // `class_value(ai.stream.Done, {})` in BamlStream.next().
        // (Media round-trips as a handle, so it needs no class registration; the
        // Stream wrapper likewise decodes via the ADT_TAGGED_HEAP_HANDLE arm.)
        registerClass(
                baml_sdk.ai.stream.Done.FQN,
                "baml_sdk.ai.stream.Done",
                new String[0]);
    }

    private TypeRegistry() {}

    // -- registration --------------------------------------------------------

    /**
     * Register a generated class. {@code fieldOrder} is the class's declaration
     * order — the order of the canonical all-args constructor and the field
     * accessor methods. Idempotent: the first registration of {@code bamlFqn}
     * wins (a redundant re-registration is a no-op, preserving cached state).
     *
     * <p>Delegates to {@link #registerClass(String, String, String[], String[])}
     * with no per-field descriptors ({@code null}), so decode of this class's
     * fields stays wire-driven (the pre-descriptor behavior).
     */
    public static void registerClass(String bamlFqn, String javaClassName, String[] fieldOrder) {
        registerClass(bamlFqn, javaClassName, fieldOrder, null);
    }

    /**
     * Register a generated class, carrying a parallel {@code fieldDescs} array —
     * one type-directed decode descriptor ({@link BamlType}) per
     * {@code fieldOrder} entry (see {@code ref-java-codegen-conventions.md}
     * "Type-directed decode descriptors"). A non-null entry reifies that field's
     * value through it (so e.g. a union-typed field lands on the {@code Union{k}}
     * arm family rather than the wire-driven fallback); a {@code null} entry (or a
     * {@code null} array) leaves that field's decode wire-driven. Idempotent: the
     * first registration of {@code bamlFqn} wins.
     */
    public static void registerClass(
            String bamlFqn, String javaClassName, String[] fieldOrder, BamlType[] fieldDescs) {
        if (fieldDescs != null && fieldDescs.length != fieldOrder.length) {
            throw new IllegalArgumentException(
                    "class field descriptor length mismatch for " + bamlFqn + ": "
                            + fieldOrder.length + " fields vs " + fieldDescs.length + " descriptors");
        }
        registerIdentity(CLASS_IDENTITIES, javaClassName, bamlFqn);
        synchronized (TypeRegistry.class) {
            CLASS_FIELDS.computeIfAbsent(BamlProgram.current().loader, ignored -> new HashMap<>()).putIfAbsent(javaClassName, fieldOrder.clone());
        }
        ClassEntry entry = new ClassEntry(bamlFqn, javaClassName, fieldOrder, fieldDescs);
        if (maps().classesByFqn.putIfAbsent(bamlFqn, entry) == null) {
            maps().classesByJavaName.putIfAbsent(javaClassName, entry);
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
        registerIdentity(ENUM_IDENTITIES, javaClassName, bamlFqn);
        EnumEntry entry = new EnumEntry(bamlFqn, javaClassName, javaConstants, wireNames);
        if (maps().enumsByFqn.putIfAbsent(bamlFqn, entry) == null) {
            maps().enumsByJavaName.putIfAbsent(javaClassName, entry);
        }
    }

    /**
     * Register a generated union, keyed structurally by its arm SET.
     * {@code armTokens} are the per-arm {@link BamlType} tokens in
     * <em>declaration</em> order; the registry key is the {@code Set} of them
     * ({@link BamlType} has value equality, so the set is order- and
     * duplicate-insensitive — exactly what the wire decoder derives from the
     * union's resolved {@code self_type}, no string round-trip).
     * {@code sealedInterfaceName} is the binary name of the generated sealed
     * interface; {@code recordNames} the parallel binary names of the wrapper
     * records (one {@code record ...Value(T value)} per arm). Decode resolves an
     * arm from the inner value's shape (see {@link #constructUnionForArms});
     * encode unwraps a record instance back to its bare inner value (see
     * {@link #unionRecordInner}).
     */
    public static void registerUnion(
            String sealedInterfaceName, BamlType[] armTokens, String[] recordNames) {
        UnionEntry entry = newUnionEntry(sealedInterfaceName, armTokens, recordNames);
        putUnion(maps().unionsByArms, armKey(List.of(armTokens)), entry);
    }

    /** Normalize an arm collection to the registry key: sorted (structural order) + distinct. */
    private static List<BamlType> armKey(Collection<BamlType> arms) {
        return List.copyOf(new TreeSet<>(arms));
    }

    /**
     * Register a generated union under an explicit alias-FQN key (a second key for
     * the same union). A recursive alias registers twice — once by its arm set
     * ({@link #registerUnion}) and once under its own FQN — because the engine may
     * send {@code self_type} either as the resolved member union or as the alias
     * node. An FQN is a name, not a rendered type grammar.
     */
    public static void registerUnionAlias(
            String aliasFqn, String sealedInterfaceName, BamlType[] armTokens, String[] recordNames) {
        UnionEntry entry = newUnionEntry(sealedInterfaceName, armTokens, recordNames);
        putUnion(maps().unionsByFqn, aliasFqn, entry);
    }

    /** Build a {@link UnionEntry} and register its records (unconditional — encode must unwrap any). */
    private static UnionEntry newUnionEntry(
            String sealedInterfaceName, BamlType[] armTokens, String[] recordNames) {
        if (armTokens.length != recordNames.length) {
            throw new IllegalArgumentException(
                    "union arrays length mismatch for " + sealedInterfaceName + ": "
                            + armTokens.length + " arm tokens vs " + recordNames.length + " records");
        }
        // Record-name registration is unconditional: encode must be able to unwrap
        // ANY registered record class, independent of which registration wins a key.
        for (int i = 0; i < recordNames.length; i++) {
            String recordName = recordNames[i];
            maps().unionRecordsByJavaName.putIfAbsent(recordName, new UnionRecordEntry(recordName, i));
        }
        return new UnionEntry(sealedInterfaceName, armTokens, recordNames);
    }

    /**
     * Insert {@code entry} under {@code key}. Idempotent for a re-registration
     * under the SAME {@code sealedInterfaceName} (the first wins). A re-registration
     * of an already-bound key under a <em>different</em> {@code sealedInterfaceName}
     * is a genuine identity conflict — two distinct unions sharing one key, so
     * decode would silently reify onto whichever won the slot — and throws
     * {@link IllegalStateException} rather than first-winning.
     */
    private static <K> void putUnion(ConcurrentHashMap<K, UnionEntry> map, K key, UnionEntry entry) {
        UnionEntry existing = map.putIfAbsent(key, entry);
        if (existing != null && !existing.sealedInterfaceName.equals(entry.sealedInterfaceName)) {
            throw new IllegalStateException(
                    "conflicting union registration for key '" + key
                            + "': already bound to " + existing.sealedInterfaceName
                            + ", refused rebind to " + entry.sealedInterfaceName);
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
        if (bamlFqn.equals(baml_sdk.ai.stream.Done.FQN) && !maps().classesByFqn.containsKey(bamlFqn)) registerClass(bamlFqn, "baml_sdk.ai.stream.Done", new String[0]);
        ClassEntry entry = maps().classesByFqn.get(bamlFqn);
        return entry == null ? null : entry.instantiate(fields);
    }

    /** Whether {@code bamlFqn} resolves to a registered generated class. */
    public static boolean isClass(String bamlFqn) {
        if (bamlFqn.equals(baml_sdk.ai.stream.Done.FQN)
                && !maps().classesByFqn.containsKey(bamlFqn)) {
            registerClass(bamlFqn, "baml_sdk.ai.stream.Done", new String[0]);
        }
        return maps().classesByFqn.containsKey(bamlFqn);
    }

    /**
     * Whether {@code fqn} names a registered union by FQN — a named recursive
     * alias. The type-directed FQN decode path uses this to route an FQN
     * descriptor that names a recursive alias to {@link #constructUnionForFqn}.
     */
    public static boolean isUnionKey(String fqn) {
        return maps().unionsByFqn.containsKey(fqn);
    }

    /**
     * The declaration-order field names of the registered class {@code bamlFqn},
     * or {@code null} when it is not a registered class. The parallel of
     * {@link #classFieldDescs}.
     */
    public static String[] classFieldOrder(String bamlFqn) {
        if (bamlFqn.equals(baml_sdk.ai.stream.Done.FQN) && !maps().classesByFqn.containsKey(bamlFqn)) registerClass(bamlFqn, "baml_sdk.ai.stream.Done", new String[0]);
        ClassEntry entry = maps().classesByFqn.get(bamlFqn);
        return entry == null ? null : entry.fieldOrder;
    }

    /**
     * The per-field type-directed decode descriptors ({@link BamlType}, parallel
     * to {@link #classFieldOrder}) of the registered class {@code bamlFqn}, or
     * {@code null} when it is not registered or was registered without
     * descriptors. Individual entries may be {@code null} (that field is decoded
     * wire-driven).
     */
    public static BamlType[] classFieldDescs(String bamlFqn) {
        if (bamlFqn.equals(baml_sdk.ai.stream.Done.FQN) && !maps().classesByFqn.containsKey(bamlFqn)) registerClass(bamlFqn, "baml_sdk.ai.stream.Done", new String[0]);
        ClassEntry entry = maps().classesByFqn.get(bamlFqn);
        return entry == null ? null : entry.fieldDescs;
    }

    /**
     * Resolve the generated enum constant bound to {@code bamlFqn} for a given
     * wire variant name. Returns {@code null} when the FQN is not registered or
     * the wire variant is unknown, so the caller can fall back to the raw variant
     * string.
     */
    public static Object resolveEnum(String bamlFqn, String wireName) {
        EnumEntry entry = maps().enumsByFqn.get(bamlFqn);
        return entry == null ? null : entry.constantFor(wireName);
    }

    /**
     * Construct the generated union record for a resolved-union wire value.
     * {@code arms} are the union's arm {@link BamlType}s read from the wire
     * {@code self_type} (normalized here to the arm-set key); {@code valueBytes}
     * the raw {@code BamlOutboundValue} (its shape picks the arm structurally, see
     * {@link ProtoReader#armMatchesValue}); {@code inner} the already-decoded inner
     * value. Returns the wrapper record, or {@code null} when the arm set is not a
     * registered union or no arm matches — the caller then keeps the bare inner
     * value (literal-over-one-base unions are erased in codegen and never
     * registered).
     */
    public static Object constructUnionForArms(List<BamlType> arms, byte[] valueBytes, Object inner) {
        UnionEntry entry = maps().unionsByArms.get(armKey(arms));
        return entry == null ? null : entry.instantiate(valueBytes, inner);
    }

    /** Construct the registered wrapper for the selected canonical arm type. */
    public static Object constructUnionForArmsSelected(
            List<BamlType> arms, BamlType selectedType, Object inner) {
        UnionEntry entry = maps().unionsByArms.get(armKey(arms));
        return entry == null ? null : entry.instantiateSelected(selectedType, inner);
    }

    /**
     * Construct the generated union record for a union named by {@code fqn} (a
     * recursive alias — the descriptor names it, or the wire {@code self_type}
     * arrived as the alias node). Otherwise as {@link #constructUnionForArms}.
     */
    public static Object constructUnionForFqn(String fqn, byte[] valueBytes, Object inner) {
        UnionEntry entry = maps().unionsByFqn.get(fqn);
        return entry == null ? null : entry.instantiate(valueBytes, inner);
    }

    /** Construct a named registered union wrapper for the selected arm type. */
    public static Object constructUnionForFqnSelected(
            String fqn, BamlType selectedType, Object inner) {
        UnionEntry entry = maps().unionsByFqn.get(fqn);
        return entry == null ? null : entry.instantiateSelected(selectedType, inner);
    }

    /** Construct a named alias union by its canonical selected index. */
    public static Object constructUnionForFqnAtIndex(String fqn, int index, Object inner) {
        UnionEntry entry = maps().unionsByFqn.get(fqn);
        return entry == null ? null : entry.instantiateAt(index, inner);
    }

    /**
     * The registered arm {@link BamlType} the wire value {@code valueBytes} selects
     * for the union named {@code fqn}, or {@code null} when {@code fqn} is not a
     * registered union / no arm matches. Lets the decoder reuse the arm token as a
     * type-directed descriptor for the arm's inner value (recursive aliases: nested
     * items must reify too).
     */
    public static BamlType unionArmTokenForFqn(String fqn, byte[] valueBytes) {
        UnionEntry entry = maps().unionsByFqn.get(fqn);
        if (entry == null) {
            return null;
        }
        int idx = entry.pickArm(valueBytes);
        return idx < 0 ? null : entry.armTokens[idx];
    }

    // -- encode (Java object -> FQN + wire payload) --------------------------

    /** Whether {@code obj}'s class is a registered generated union wrapper record. */
    public static boolean isUnionRecord(Object obj) {
        return maps().unionRecordsByJavaName.containsKey(obj.getClass().getName());
    }

    /**
     * The bare inner value carried by a union wrapper record (its {@code value()}
     * accessor). Callers must gate on {@link #isUnionRecord} first; passing a
     * non-record object throws.
     */
    public static Object unionRecordInner(Object obj) {
        UnionRecordEntry entry = maps().unionRecordsByJavaName.get(obj.getClass().getName());
        if (entry == null) {
            throw new IllegalStateException("not a registered union record: " + obj.getClass().getName());
        }
        return entry.inner(obj);
    }

    /** The record's declaration-order arm index, or {@code -1} when unregistered. */
    public static int unionRecordArmIndex(Object obj) {
        UnionRecordEntry entry = maps().unionRecordsByJavaName.get(obj.getClass().getName());
        return entry == null ? -1 : entry.armIndex;
    }

    /**
     * The class-value wire payload for a host object, or {@code null} when the
     * object's class is not a registered generated class.
     */
    static Object[] argumentFields(Object obj) {
        String[] fields;
        synchronized (TypeRegistry.class) {
            var names = CLASS_FIELDS.get(obj.getClass().getClassLoader());
            fields = names == null ? null : names.get(obj.getClass().getName());
        }
        if (fields == null) return new Object[0];
        Object[] values = new Object[fields.length];
        try {
            for (int i = 0; i < fields.length; i++) values[i] = obj.getClass().getMethod(fields[i]).invoke(obj);
        } catch (ReflectiveOperationException e) {
            throw new IllegalArgumentException("Cannot inspect BAML argument fields", e);
        }
        return values;
    }

    public static ClassWire classWire(Object obj) {
        ClassEntry entry = maps().classesByJavaName.get(obj.getClass().getName());
        return entry == null ? null : entry.encode(obj);
    }

    /**
     * The enum-value wire payload for a host enum constant, or {@code null} when
     * the constant's enum type is not a registered generated enum.
     */
    public static EnumWire enumWire(Enum<?> constant) {
        // getDeclaringClass (not getClass): a constant with a body is an anonymous
        // subclass, but registration keys on the enum type itself.
        EnumEntry entry = maps().enumsByJavaName.get(constant.getDeclaringClass().getName());
        if (entry == null) {
            return null;
        }
        String wire = entry.wireFor(constant.name());
        return wire == null ? null : new EnumWire(entry.bamlFqn, wire);
    }

    // -- type tokens (Java class -> BAML FQN) --------------------------------

    /**
     * The BAML FQN of the registered generated <em>class</em> whose Java type is
     * {@code javaClass}, or {@code null} when {@code javaClass} is not a
     * registered class. The reverse index the value encoder keys on
     * ({@link #classWire}), exposed for {@link BamlType#of(Class)} to resolve a
     * class token's FQN without loading anything.
     */
    public static String classFqnForJavaClass(Class<?> javaClass) {
        return identityFor(CLASS_IDENTITIES, javaClass);
    }

    /**
     * The BAML FQN of the registered generated <em>enum</em> whose Java type is
     * {@code javaClass}, or {@code null} when {@code javaClass} is not a
     * registered enum. The enum counterpart of {@link #classFqnForJavaClass}.
     */
    public static String enumFqnForJavaClass(Class<?> javaClass) {
        return identityFor(ENUM_IDENTITIES, javaClass);
    }

    // -- reified type-argument side-table ------------------------------------
    //
    // A generic class value arrives with its concrete type_args on the wire, but
    // the generated instance has no field to hold them yet (that emitter surface
    // is deferred). Until it lands, the decoder stashes the reified BamlType
    // tokens here, keyed by the instance's identity. The future emitted
    // `bamlTypeArgs()` accessor will delegate to (or replace) `typeArgsOf`.
    //
    // Weak-identity keyed: the runtime must not keep a decoded value alive, and
    // must key on identity (generated value classes may be records with value
    // equality, so two distinct-but-equal instances must not collide). The JDK
    // has no weak-identity map, so this composes a WeakReference key that hashes
    // and compares by referent identity, expunging cleared keys via a queue.

    private static final ReferenceQueue<Object> TYPE_ARGS_QUEUE = new ReferenceQueue<>();
    private static final Map<WeakIdentityKey, List<BamlType>> TYPE_ARGS = new HashMap<>();

    /**
     * Retain the reified generic {@code typeArgs} for a decoded {@code instance}.
     * A null/empty list (a non-generic instance) is a no-op. The list is
     * defensively copied; the instance is held only weakly.
     */
    public static void bindTypeArgs(Object instance, List<BamlType> typeArgs) {
        if (instance == null || typeArgs == null || typeArgs.isEmpty()) {
            return;
        }
        List<BamlType> snapshot = List.copyOf(typeArgs);
        synchronized (TYPE_ARGS) {
            expungeStaleTypeArgs();
            TYPE_ARGS.put(new WeakIdentityKey(instance, TYPE_ARGS_QUEUE), snapshot);
        }
    }

    /**
     * The reified generic type-args retained for {@code instance}, or an empty
     * list when none were bound (a non-generic instance, or one decoded without
     * type_args). Never {@code null}.
     */
    public static List<BamlType> typeArgsOf(Object instance) {
        if (instance == null) {
            return List.of();
        }
        synchronized (TYPE_ARGS) {
            expungeStaleTypeArgs();
            List<BamlType> args = TYPE_ARGS.get(new WeakIdentityKey(instance, null));
            return args == null ? List.of() : args;
        }
    }

    /** Drop entries whose instance has been collected. Caller holds the monitor. */
    private static void expungeStaleTypeArgs() {
        Reference<?> ref;
        while ((ref = TYPE_ARGS_QUEUE.poll()) != null) {
            // The enqueued reference is the key itself; remove it by identity.
            TYPE_ARGS.remove(ref);
        }
    }

    /**
     * A map key that holds its referent weakly and hashes/compares by referent
     * <em>identity</em> (not {@code equals}). The cached identity hash stays
     * valid after the referent is cleared, so a stale key still lands in its
     * original bucket for {@link #expungeStaleTypeArgs} to remove.
     */
    private static final class WeakIdentityKey extends WeakReference<Object> {
        private final int hash;

        WeakIdentityKey(Object referent, ReferenceQueue<Object> queue) {
            super(referent, queue);
            this.hash = System.identityHashCode(referent);
        }

        @Override
        public int hashCode() {
            return hash;
        }

        @Override
        public boolean equals(Object o) {
            if (this == o) {
                return true;
            }
            if (!(o instanceof WeakIdentityKey other)) {
                return false;
            }
            Object mine = get();
            // A cleared key never matches a live lookup (identity is gone).
            return mine != null && mine == other.get();
        }
    }

    // -- encode payloads -----------------------------------------------------

    /** A class instance decomposed for the inbound {@code class_value} arm. */
    public static final class ClassWire {
        /** The BAML FQN, emitted on the enclosing {@code InboundValue.value_type}. */
        public final String fqn;
        /** Field names in declaration order (the {@code fields} entry keys). */
        public final String[] fieldNames;
        /** Field values in declaration order, read via the accessor methods. */
        public final Object[] fieldValues;
        /** Declared field descriptors, parallel to the names, or {@code null}. */
        public final BamlType[] fieldDescs;

        ClassWire(String fqn, String[] fieldNames, Object[] fieldValues, BamlType[] fieldDescs) {
            this.fqn = fqn;
            this.fieldNames = fieldNames;
            this.fieldValues = fieldValues;
            this.fieldDescs = fieldDescs;
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
        // Per-field type-directed decode descriptors (parallel to fieldOrder), or
        // null when the class was registered without them (wire-driven decode). An
        // individual entry may be null (that field is decoded wire-driven).
        final BamlType[] fieldDescs;

        // Lazily resolved (decode/encode reflection). Benign races only re-resolve
        // to the same value; volatile publishes the resolved object safely.
        private volatile Class<?> resolved;
        private volatile Constructor<?> ctor;
        private volatile Method[] accessors;

        ClassEntry(String bamlFqn, String javaClassName, String[] fieldOrder, BamlType[] fieldDescs) {
            this.bamlFqn = bamlFqn;
            this.javaClassName = javaClassName;
            this.fieldOrder = fieldOrder;
            this.fieldDescs = fieldDescs;
        }

        private Class<?> javaClass() {
            Class<?> c = resolved;
            if (c == null) {
                try {
                    c = Class.forName(javaClassName, true, BamlProgram.current().loader);
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
            return new ClassWire(bamlFqn, fieldOrder, values, fieldDescs);
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
                    cls = Class.forName(javaClassName, true, BamlProgram.current().loader);
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
        final String sealedInterfaceName;
        final BamlType[] armTokens;
        final String[] recordNames;

        // Lazily resolved per-arm single-arg record constructors. Benign races
        // only re-resolve a slot to an equal Constructor.
        private volatile Constructor<?>[] ctors;

        UnionEntry(String sealedInterfaceName, BamlType[] armTokens, String[] recordNames) {
            this.sealedInterfaceName = sealedInterfaceName;
            this.armTokens = armTokens;
            this.recordNames = recordNames;
        }

        /** Build the wrapper record for the wire value {@code valueBytes}, or null if no arm matches. */
        Object instantiate(byte[] valueBytes, Object inner) {
            int idx = pickArm(valueBytes);
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

        Object instantiateSelected(BamlType selectedType, Object inner) {
            int idx = -1;
            for (int i = 0; i < armTokens.length; i++) {
                if (armTokens[i].equals(selectedType)) {
                    idx = i;
                    break;
                }
            }
            if (idx < 0) {
                throw new IllegalArgumentException(
                        "selected type " + selectedType + " is not an arm of "
                                + sealedInterfaceName);
            }
            Constructor<?> c = constructor(idx);
            try {
                return c.newInstance(inner);
            } catch (ReflectiveOperationException e) {
                throw new IllegalStateException(
                        "failed to construct union record " + recordNames[idx], e);
            }
        }

        Object instantiateAt(int idx, Object inner) {
            if (idx < 0 || idx >= armTokens.length) {
                throw new IllegalArgumentException(
                        "union selected option index " + idx + " is out of range for "
                                + sealedInterfaceName);
            }
            Constructor<?> c = constructor(idx);
            try {
                return c.newInstance(inner);
            } catch (ReflectiveOperationException e) {
                throw new IllegalStateException(
                        "failed to construct union record " + recordNames[idx], e);
            }
        }

        /**
         * The declaration-order index of the first arm whose {@link BamlType}
         * structurally matches the wire value {@code valueBytes} (see
         * {@link ProtoReader#armMatchesValue}), or -1. A typed container arm
         * matches only a same-typed (or bare/imprecise) wire container, so an empty
         * {@code int[]} no longer lands on a {@code string[]} arm declared first,
         * while a bare (pre-typed) wire still matches any container arm of that
         * base; primitives / class-enum FQNs / literals match by shape.
         */
        int pickArm(byte[] valueBytes) {
            for (int i = 0; i < armTokens.length; i++) {
                if (ProtoReader.armMatchesValue(armTokens[i], valueBytes)) {
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
                cls = Class.forName(recordName, true, BamlProgram.current().loader);
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
        final int armIndex;

        // Lazily resolved value() accessor (from the object's own Class — encode
        // never forces a Class.forName).
        private volatile Method valueAccessor;

        UnionRecordEntry(String recordName, int armIndex) {
            this.recordName = recordName;
            this.armIndex = armIndex;
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
