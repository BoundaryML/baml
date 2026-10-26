package baml_bridge;

import baml_bridge.internal.WireReader;
import baml_bridge.internal.WireWriter;

import java.math.BigInteger;
import java.util.ArrayList;
import java.util.List;
import java.util.Map;
import java.util.Objects;
import java.util.stream.Collectors;

/**
 * An immutable BAML type token, used to bind a generic function/method's
 * TypeVars at an explicit-generics call site (Java generics are erased, so the
 * type travels as a value). The JVM analog of Python's {@code _types=} bindings
 * lowered to a wire {@code BamlTy} (see {@code proto.py} {@code python_type_to_wire_ty}).
 *
 * <h2>Grammar</h2>
 * <ul>
 *   <li>the primitive constants {@link #INT}, {@link #BIGINT}, {@link #STRING},
 *       {@link #BOOL}, {@link #FLOAT};</li>
 *   <li>{@link #of(Class)} for a registered generated class or enum (the BAML
 *       FQN is resolved via {@link TypeRegistry} — the same lookup the value
 *       encoder uses);</li>
 *   <li>{@link #of(Class, BamlType...)} for a reified generic class (e.g.
 *       {@code Box<int>}), carrying its concrete args in declaration order;</li>
 *   <li>the structural constructors {@link #list(BamlType)},
 *       {@link #map(BamlType, BamlType)}, {@link #optional(BamlType)}, and
 *       {@link #union(BamlType...)}; and</li>
 *   <li>the literal-type constructors {@link #literalString(String)},
 *       {@link #literalInt(long)}, {@link #literalBool(boolean)},
 *       {@link #literalFloat(String)}, {@link #literalBigint(BigInteger)}.</li>
 * </ul>
 *
 * <p>Value semantics: two tokens are {@link #equals equal} when their kind, and
 * (per kind) primitive kind / FQN / nested tokens / literal arm+value match, with
 * a <em>structural</em> total order ({@link #compareTo}) over those same fields.
 * So a union's arm set keys the registry directly — a sorted, distinct
 * {@code List<BamlType>} whose {@link java.util.List#equals} rides {@code BamlType}
 * value equality — with no string derivation (and thus no
 * literal-value-collision hazard a rendered key would carry).
 *
 * <h2>Function types</h2>
 * {@link #function(List, BamlType, BamlType)} is the type of a callable: its
 * parameters (each a {@link Param} with an optional wire name, a type and
 * whether it is an optional, by-name parameter), its return type and its
 * throws type. A host callable has no type of its own, so the one it is bound
 * to is stated on the wire beside its handle.
 *
 * <p>{@link #function(Class, List, BamlType, BamlType)} also names the Java
 * functional interface a generated position spells the type as, so a BAML
 * closure decoded there becomes that interface. The interface is how one host
 * position spells the type, not part of the BAML type: it takes no part in
 * {@link #equals}, {@link #compareTo} or {@link #toWireTy}.
 *
 * <h2>Decode-only hints</h2>
 * {@link #typeVar} (a bound TypeVar's decode placeholder) exists purely to drive
 * host-side decode and never rides the encode wire: {@link #toWireTy} on it
 * throws {@link IllegalStateException}. {@link #UNKNOWN} is both a decode hint
 * (decode this value wire-driven, matches any arm) and the type {@code unknown}
 * on the wire, as the element type of a list of unalike values or a callable's
 * {@code unknown} return. {@link #classByFqn} builds a named-type token straight
 * from a BAML FQN (no {@link TypeRegistry} lookup, so it names runtime-owned /
 * not-yet-loaded types too); decode resolves the FQN against the registry.
 *
 * <h2>Wire {@code BamlTy} ({@code baml_type.proto})</h2>
 * A token renders to the shared {@code BamlTy} oneof:
 * <pre>
 * BamlTy oneof:      primitive = 1, class_ty = 2, enum = 3, list = 4, map = 5,
 *                    optional = 6, union = 7, literal = 8, type_alias = 9,
 *                    unknown = 10, media = 11, interface = 12, enum_variant = 13,
 *                    function = 14, meta_type = 17, prompt_ast = 19, never = 24
 * BamlTyFunction:    params = 3 (repeated BamlTyFunctionParam), ret = 4, throws = 5
 * BamlTyFunctionParam: name = 1 (optional string), ty = 2, mode = 3 (REQUIRED=1, OPTIONAL=2)
 * BamlTyPrimitive:   kind = 1 (BamlTyPrimitiveKind: STRING=1, INT=2, FLOAT=3, BOOL=4,
 *                    NULL=5, BYTES=6, BIGINT=7)
 * BamlTyClass:       head oneof { name = 1 (BAML FQN), live = 3, anonymous_label = 4 },
 *                    type_args = 2 (repeated BamlTy)
 * BamlTyEnum:        head oneof { name = 1, live = 2, anonymous_label = 3 }
 * BamlTyList:        item = 1 (BamlTy)
 * BamlTyMap:         key = 1 (BamlTy), value = 2 (BamlTy)
 * BamlTyOptional:    inner = 1 (BamlTy)     (BAML `T?` — matches Python's `_fill_wire_ty`)
 * BamlTyUnion:       options = 1 (repeated BamlTy)
 * BamlTyLiteral:     oneof literal: string_value=1, int_value=2, bool_value=3,
 *                    bigint_value=4 (decimal string), float_value=5 (decimal string)
 * </pre>
 * {@link #fromWireTy} returns {@code null} for any arm outside this grammar (a
 * wire type the token cannot represent, e.g. a future, a rust_type, a type
 * variable), and for any composite whose nested tokens are themselves outside
 * the grammar — a partial token would misalign positions, so the whole token is
 * poisoned.
 *
 * <h2>Runtime declarations</h2>
 * A declaration the runtime created ({@code reflect.Class.new}) has no name a
 * lookup could resolve. A type value the engine returns names it by a
 * {@code live} head, a reference the token holds (one per occurrence) and
 * releases when it is let go ({@link #adoptWireTy}); stating the token clones
 * each ({@link #toWireTy}). Beside a value such a declaration appears only as an
 * {@code anonymous_label}, display text a token keeps and never states. Either
 * way {@link #fqn} is {@code null}.
 */
public final class BamlType implements Comparable<BamlType> {
    // BamlTy oneof field numbers (baml_type.proto).
    private static final int TY_PRIMITIVE = 1;
    private static final int TY_CLASS = 2;
    private static final int TY_ENUM = 3;
    private static final int TY_LIST = 4;
    private static final int TY_MAP = 5;
    private static final int TY_OPTIONAL = 6;
    private static final int TY_UNION = 7;
    private static final int TY_LITERAL = 8;
    private static final int TY_TYPE_ALIAS = 9;
    private static final int TY_UNKNOWN = 10;
    private static final int TY_MEDIA = 11;
    private static final int TY_INTERFACE = 12;
    private static final int TY_ENUM_VARIANT = 13;
    private static final int TY_FUNCTION = 14;
    private static final int TY_META_TYPE = 17;
    private static final int TY_PROMPT_AST = 19;
    private static final int TY_NEVER = 24;
    private static final int MEDIA_KIND = 1; // BamlTyMedia.kind
    private static final int INTERFACE_NAME = 1; // BamlTyInterface.name
    private static final int INTERFACE_TYPE_ARGS = 2; // BamlTyInterface.type_args (repeated)
    private static final int ENUM_VARIANT_NAME = 1; // BamlTyEnumVariant.name
    private static final int ENUM_VARIANT_VARIANT = 2; // BamlTyEnumVariant.variant
    private static final int TYPE_ALIAS_NAME = 1;
    // The `live` and `anonymous_label` heads of each nominal message.
    private static final int CLASS_LIVE = 3;
    private static final int CLASS_ANONYMOUS = 4;
    private static final int TYPE_ALIAS_LIVE = 3;
    private static final int TYPE_ALIAS_ANONYMOUS = 4;
    private static final int ENUM_LIVE = 2;
    private static final int ENUM_ANONYMOUS = 3;
    private static final int INTERFACE_LIVE = 4;
    private static final int INTERFACE_ANONYMOUS = 5;
    private static final int ENUM_VARIANT_LIVE = 3;
    private static final int ENUM_VARIANT_ANONYMOUS = 4;
    private static final int FUNCTION_PARAMS = 3; // BamlTyFunction.params (repeated)
    private static final int FUNCTION_RET = 4; // BamlTyFunction.ret
    private static final int FUNCTION_THROWS = 5; // BamlTyFunction.throws
    private static final int PARAM_NAME = 1; // BamlTyFunctionParam.name
    private static final int PARAM_TY = 2; // BamlTyFunctionParam.ty
    private static final int PARAM_MODE = 3; // BamlTyFunctionParam.mode
    private static final int PARAM_MODE_REQUIRED = 1;
    private static final int PARAM_MODE_OPTIONAL = 2;

    // Sub-message field numbers.
    private static final int PRIM_KIND = 1; // BamlTyPrimitive.kind
    private static final int CLASS_NAME = 1; // BamlTyClass.name
    private static final int CLASS_TYPE_ARGS = 2; // BamlTyClass.type_args (repeated)
    private static final int ENUM_NAME = 1; // BamlTyEnum.name
    private static final int LIST_ITEM = 1; // BamlTyList.item
    private static final int MAP_KEY = 1; // BamlTyMap.key
    private static final int MAP_VALUE = 2; // BamlTyMap.value
    private static final int OPT_INNER = 1; // BamlTyOptional.inner
    private static final int UNION_OPTIONS = 1; // BamlTyUnion.options (repeated)

    // BamlTyLiteral oneof arms.
    private static final int LIT_STRING = 1;
    private static final int LIT_INT = 2;
    private static final int LIT_BOOL = 3;
    private static final int LIT_BIGINT = 4;
    private static final int LIT_FLOAT = 5;

    // BamlTyPrimitiveKind enum values supported by the token grammar.
    private static final int PRIM_STRING = 1;
    private static final int PRIM_INT = 2;
    private static final int PRIM_FLOAT = 3;
    private static final int PRIM_BOOL = 4;
    private static final int PRIM_NULL = 5;
    private static final int PRIM_BYTES = 6;
    private static final int PRIM_BIGINT = 7;

    /**
     * The token's shape. Every kind but {@link #TYPEVAR} and {@link #OPAQUE},
     * decode-only hints that throw on {@link #toWireTy}, renders as a
     * {@code BamlTy} oneof arm.
     */
    public enum Kind {
        PRIMITIVE,
        CLASS,
        ENUM,
        LIST,
        MAP,
        OPTIONAL,
        UNION,
        LITERAL,
        /** A named type alias (FQN in {@link #fqn}), such as {@code baml.json.json}. */
        TYPE_ALIAS,
        /** A callable's type: its parameters, return type and throws type. */
        FUNCTION,
        /** One variant of an enum as a type: the enum's FQN in {@link #fqn}, the variant in {@link #literalValue}. */
        ENUM_VARIANT,
        /** A media type; its kind ({@code BamlTyMediaKind}) in {@link #primitiveKind}. */
        MEDIA,
        /** An interface existential: the interface's FQN in {@link #fqn}, its type arguments as children. */
        INTERFACE,
        /** The empty type: the element type of an empty container whose slot fixes none. */
        NEVER,
        /** A rendered prompt ({@code ai.Prompt}'s value), the type a prompt value has. */
        PROMPT_AST,
        /** The type of a type value ({@code reflect.Type}). */
        TYPE,
        /** A bound TypeVar's decode placeholder (name in {@link #fqn}); wildcard in matching. */
        TYPEVAR,
        /** Decode-this-value-wire-driven; a wildcard that matches any arm. Also the wire type {@code unknown}. */
        UNKNOWN,
        /**
         * A type this token grammar does not spell (a future, a reflected type, a
         * prompt, a native handle, a resource): decoded wire-driven like
         * {@link #UNKNOWN}, never stated on the wire.
         */
        OPAQUE
    }

    public static final BamlType STRING = primitive(PRIM_STRING);
    public static final BamlType INT = primitive(PRIM_INT);
    public static final BamlType BIGINT = primitive(PRIM_BIGINT);
    public static final BamlType BOOL = primitive(PRIM_BOOL);
    public static final BamlType FLOAT = primitive(PRIM_FLOAT);
    /** BAML {@code never}, the type with no values. */
    public static final BamlType NEVER = new BamlType(Kind.NEVER, 0, null, List.of(), 0, null);

    /**
     * The decode-only "decode wire-driven / match any arm" hint. Never encodes
     * ({@link #toWireTy} throws) — it is a decode descriptor, not a value.
     */
    public static final BamlType UNKNOWN =
            new BamlType(Kind.UNKNOWN, 0, null, List.of(), 0, null);
    /** BAML {@code null}, the unit type. */
    public static final BamlType NULL = primitive(PRIM_NULL);
    /** BAML {@code uint8array}. */
    public static final BamlType BYTES = primitive(PRIM_BYTES);
    public static final BamlType OPAQUE =
            new BamlType(Kind.OPAQUE, 0, null, List.of(), 0, null);
    /** The type of a rendered prompt. */
    public static final BamlType PROMPT_AST =
            new BamlType(Kind.PROMPT_AST, 0, null, List.of(), 0, null);
    /** The type of a type value. */
    public static final BamlType TYPE = new BamlType(Kind.TYPE, 0, null, List.of(), 0, null);

    /**
     * The head of a declaration the runtime created ({@code reflect.Class.new},
     * a runtime-compiled package), which no name resolves. Only a type the
     * engine returned carries one; {@link #fqn} is then {@code null}.
     */
    private sealed interface RuntimeHead permits LiveHead, AnonymousHead {}

    /**
     * The declaration itself, held by one engine reference this occurrence
     * owns and releases when the type is let go. The engine keeps one key per
     * declaration while a reference is alive, so equal keys name one
     * declaration.
     */
    private static final class LiveHead implements RuntimeHead {
        private final BamlHandle row;

        LiveHead(BamlHandle row) {
            this.row = row;
        }

        @Override
        public boolean equals(Object o) {
            return o instanceof LiveHead other && row.key() == other.row.key();
        }

        @Override
        public int hashCode() {
            return Long.hashCode(row.key());
        }
    }

    /**
     * Display text for a declaration the engine could not hand over (one in an
     * annotation): it references nothing, so a type carrying it is held and
     * shown but never stated.
     */
    private record AnonymousHead(String label) implements RuntimeHead {}

    /**
     * One parameter of a {@link Kind#FUNCTION} type. {@code name} is the wire
     * name of an optional (by-name) parameter and may be {@code null} for a
     * positional one.
     */
    public record Param(String name, BamlType type, boolean optional) {
        public Param {
            Objects.requireNonNull(type, "type");
        }
    }

    private final Kind kind;
    private final int primitiveKind; // PRIMITIVE only (a BamlTyPrimitiveKind value)
    private final String fqn; // CLASS / ENUM only
    // CLASS: concrete type args (declaration order). LIST: [item]. MAP:
    // [key, value]. OPTIONAL: [inner]. UNION: the options. FUNCTION:
    // [ret, throws]. Always non-null.
    private final List<BamlType> children;
    private final List<Param> params; // FUNCTION only
    // FUNCTION only: the functional interface a generated position spells the
    // type as, or null for a type no position spells (one read off the wire, or
    // built by hand).
    private final Class<?> hostInterface;
    private final int literalArm; // LITERAL only (a BamlTyLiteral oneof arm)
    private final Object literalValue; // LITERAL only: String / Long / Boolean
    // CLASS / ENUM / TYPE_ALIAS / INTERFACE / ENUM_VARIANT whose fqn is null:
    // the declaration's runtime head. A nominal type has exactly one of the two.
    private final RuntimeHead runtimeHead;

    private BamlType(
            Kind kind,
            int primitiveKind,
            String fqn,
            List<BamlType> children,
            int literalArm,
            Object literalValue) {
        this(kind, primitiveKind, fqn, children, List.of(), null, literalArm, literalValue, null);
    }

    private BamlType(
            Kind kind,
            int primitiveKind,
            String fqn,
            List<BamlType> children,
            List<Param> params,
            Class<?> hostInterface,
            int literalArm,
            Object literalValue,
            RuntimeHead runtimeHead) {
        assert runtimeHead == null || (fqn == null && isNominal(kind)) : kind;
        this.kind = kind;
        this.primitiveKind = primitiveKind;
        this.fqn = fqn;
        this.children = children;
        this.params = params;
        this.hostInterface = hostInterface;
        this.literalArm = literalArm;
        this.literalValue = literalValue;
        this.runtimeHead = runtimeHead;
    }

    /** Whether {@code kind} names a declaration by its head. */
    private static boolean isNominal(Kind kind) {
        return switch (kind) {
            case CLASS, ENUM, TYPE_ALIAS, INTERFACE, ENUM_VARIANT -> true;
            case PRIMITIVE, LIST, MAP, OPTIONAL, UNION, LITERAL, FUNCTION, MEDIA, NEVER,
                    PROMPT_AST, TYPE, TYPEVAR, UNKNOWN, OPAQUE -> false;
        };
    }

    private static BamlType primitive(int primitiveKind) {
        return new BamlType(Kind.PRIMITIVE, primitiveKind, null, List.of(), 0, null);
    }

    /**
     * A type token for a registered generated class or enum. The BAML FQN is
     * resolved via {@link TypeRegistry} (the same reverse lookup the value
     * encoder uses); an unregistered class/enum throws
     * {@link IllegalArgumentException}.
     */
    public static BamlType of(Class<?> type) {
        Objects.requireNonNull(type, "type");
        String classFqn = TypeRegistry.classFqnForJavaClass(type);
        if (classFqn != null) {
            return new BamlType(Kind.CLASS, 0, classFqn, List.of(), 0, null);
        }
        String enumFqn = TypeRegistry.enumFqnForJavaClass(type);
        if (enumFqn != null) {
            return new BamlType(Kind.ENUM, 0, enumFqn, List.of(), 0, null);
        }
        throw new IllegalArgumentException(
                "not a registered BAML class or enum: " + type.getName()
                        + " (the generated type must be registered before a BamlType can name it)");
    }

    /**
     * A type token for a reified generic class (e.g. {@code Box<int>}), carrying
     * its concrete {@code typeArgs} in declaration order. Only classes are
     * generic, so an unregistered class (or an enum) throws
     * {@link IllegalArgumentException}. Passing no args is equivalent to
     * {@link #of(Class)} for a class.
     */
    public static BamlType of(Class<?> type, BamlType... typeArgs) {
        Objects.requireNonNull(type, "type");
        Objects.requireNonNull(typeArgs, "typeArgs");
        String classFqn = TypeRegistry.classFqnForJavaClass(type);
        if (classFqn == null) {
            throw new IllegalArgumentException(
                    "not a registered BAML generic class: " + type.getName()
                            + " (only registered classes can be reified with type arguments)");
        }
        return new BamlType(Kind.CLASS, 0, classFqn, copyChildren("type argument", typeArgs), 0, null);
    }

    /**
     * A named-type token built straight from a BAML FQN (no {@link TypeRegistry}
     * lookup) — used to spell a decode descriptor / union-arm token for a
     * class, enum, or recursive-alias union. Decode resolves the FQN against the
     * registry ({@code isClass} / {@code isUnionKey}), so an FQN that names no
     * registered type is harmless (that value decodes wire-driven). Unlike
     * {@link #of(Class)} this names runtime-owned or not-yet-loaded types too.
     */
    public static BamlType classByFqn(String bamlFqn) {
        Objects.requireNonNull(bamlFqn, "bamlFqn");
        return new BamlType(Kind.CLASS, 0, bamlFqn, List.of(), 0, null);
    }

    /**
     * A named-type token by BAML FQN carrying reified type args (declaration
     * order) — the generic counterpart of {@link #classByFqn(String)}, used for a
     * generic class's union-arm identity so two instantiations do not collide in
     * the registry.
     */
    public static BamlType classByFqn(String bamlFqn, BamlType... typeArgs) {
        Objects.requireNonNull(bamlFqn, "bamlFqn");
        Objects.requireNonNull(typeArgs, "typeArgs");
        return new BamlType(Kind.CLASS, 0, bamlFqn, copyChildren("type argument", typeArgs), 0, null);
    }

    /**
     * A decode-only TypeVar placeholder (BAML {@code T}). A decode hint: it never
     * encodes ({@link #toWireTy} throws) and matches any arm — a value bound to
     * this TypeVar decodes wire-driven. Its {@code name} is retained for fidelity.
     */
    public static BamlType typeVar(String name) {
        Objects.requireNonNull(name, "name");
        return new BamlType(Kind.TYPEVAR, 0, name, List.of(), 0, null);
    }

    /** The enum type with BAML FQN {@code bamlFqn}. */
    public static BamlType enumByFqn(String bamlFqn) {
        Objects.requireNonNull(bamlFqn, "bamlFqn");
        return new BamlType(Kind.ENUM, 0, bamlFqn, List.of(), 0, null);
    }

    /** The type of the one variant {@code variant} of the enum {@code bamlFqn}. */
    public static BamlType enumVariant(String bamlFqn, String variant) {
        Objects.requireNonNull(bamlFqn, "bamlFqn");
        Objects.requireNonNull(variant, "variant");
        return new BamlType(Kind.ENUM_VARIANT, 0, bamlFqn, List.of(), 0, variant);
    }

    /** A media type by its {@code BamlTyMediaKind} (image=1, audio=2, video=3, pdf=4, generic=5). */
    public static BamlType media(int kind) {
        if (kind < 1 || kind > 5) {
            throw new IllegalArgumentException("not a BamlTyMediaKind: " + kind);
        }
        return new BamlType(Kind.MEDIA, kind, null, List.of(), 0, null);
    }

    /** The interface existential of {@code bamlFqn} applied to {@code typeArgs}. */
    public static BamlType interfaceByFqn(String bamlFqn, BamlType... typeArgs) {
        Objects.requireNonNull(bamlFqn, "bamlFqn");
        Objects.requireNonNull(typeArgs, "typeArgs");
        return new BamlType(
                Kind.INTERFACE, 0, bamlFqn, copyChildren("type argument", typeArgs), 0, null);
    }

    /** A type-alias token naming a declared alias by its BAML FQN. */
    public static BamlType typeAlias(String bamlFqn) {
        Objects.requireNonNull(bamlFqn, "bamlFqn");
        return new BamlType(Kind.TYPE_ALIAS, 0, bamlFqn, List.of(), 0, null);
    }

    /**
     * This token with every type variable {@code bindings} names replaced by its
     * binding. A variable not in {@code bindings} stays as it is.
     */
    public BamlType substitute(Map<String, BamlType> bindings) {
        return switch (kind) {
            case TYPEVAR -> bindings.getOrDefault(fqn, this);
            case PRIMITIVE, ENUM, ENUM_VARIANT, MEDIA, LITERAL, NEVER, PROMPT_AST, TYPE, TYPE_ALIAS, UNKNOWN, OPAQUE -> this;
            case CLASS, INTERFACE, LIST, MAP, OPTIONAL, UNION -> {
                List<BamlType> mapped = new ArrayList<>(children.size());
                for (BamlType child : children) {
                    mapped.add(child.substitute(bindings));
                }
                yield new BamlType(
                        kind, primitiveKind, fqn, List.copyOf(mapped), List.of(), null,
                        literalArm, literalValue, runtimeHead);
            }
            case FUNCTION -> {
                List<Param> mappedParams = new ArrayList<>(params.size());
                for (Param param : params) {
                    mappedParams.add(
                            new Param(param.name(), param.type().substitute(bindings), param.optional()));
                }
                yield new BamlType(
                        Kind.FUNCTION,
                        0,
                        null,
                        List.of(children.get(0).substitute(bindings), children.get(1).substitute(bindings)),
                        List.copyOf(mappedParams),
                        hostInterface,
                        0,
                        null,
                        null);
            }
        };
    }

    /**
     * The type of a callable with {@code params}, returning {@code ret} and
     * throwing {@code throwsType}.
     */
    public static BamlType function(List<Param> params, BamlType ret, BamlType throwsType) {
        Objects.requireNonNull(params, "params");
        Objects.requireNonNull(ret, "ret");
        Objects.requireNonNull(throwsType, "throwsType");
        return new BamlType(
                Kind.FUNCTION, 0, null, List.of(ret, throwsType), List.copyOf(params), null, 0, null, null);
    }

    /**
     * The type of a callable with {@code params}, returning {@code ret} and
     * throwing {@code throwsType}, at a position whose Java type is the
     * functional interface {@code hostInterface}: a BAML closure decoded there
     * becomes an instance of it that calls back into BAML.
     */
    public static BamlType function(
            Class<?> hostInterface, List<Param> params, BamlType ret, BamlType throwsType) {
        Objects.requireNonNull(hostInterface, "hostInterface");
        Objects.requireNonNull(params, "params");
        Objects.requireNonNull(ret, "ret");
        Objects.requireNonNull(throwsType, "throwsType");
        if (!hostInterface.isInterface()) {
            throw new IllegalArgumentException(
                    "a callable's host type must be an interface, not " + hostInterface.getName());
        }
        return new BamlType(
                Kind.FUNCTION,
                0,
                null,
                List.of(ret, throwsType),
                List.copyOf(params),
                hostInterface,
                0,
                null,
                null);
    }

    /** A list type token {@code T[]} carrying its element token. */
    public static BamlType list(BamlType item) {
        Objects.requireNonNull(item, "item");
        return new BamlType(Kind.LIST, 0, null, List.of(item), 0, null);
    }

    /** A map type token {@code map<K, V>} carrying its key and value tokens. */
    public static BamlType map(BamlType key, BamlType value) {
        Objects.requireNonNull(key, "key");
        Objects.requireNonNull(value, "value");
        return new BamlType(Kind.MAP, 0, null, List.of(key, value), 0, null);
    }

    /**
     * An optional type token {@code T?} carrying its inner token. Mirrors BAML
     * semantics as Python's {@code _fill_wire_ty} models them: a single-arm
     * optional lowers to {@code BamlTyOptional}, not a union with {@code null}.
     */
    public static BamlType optional(BamlType inner) {
        Objects.requireNonNull(inner, "inner");
        return new BamlType(Kind.OPTIONAL, 0, null, List.of(inner), 0, null);
    }

    /** A union type token {@code A | B | ...} carrying its option tokens in order. */
    public static BamlType union(BamlType... options) {
        Objects.requireNonNull(options, "options");
        return new BamlType(Kind.UNION, 0, null, copyChildren("union option", options), 0, null);
    }

    /** A string-literal type token (BAML {@code "draft"}). */
    public static BamlType literalString(String value) {
        Objects.requireNonNull(value, "value");
        return new BamlType(Kind.LITERAL, 0, null, List.of(), LIT_STRING, value);
    }

    /** An int-literal type token (BAML {@code 3}). */
    public static BamlType literalInt(long value) {
        return new BamlType(Kind.LITERAL, 0, null, List.of(), LIT_INT, value);
    }

    /** A bool-literal type token (BAML {@code true}). */
    public static BamlType literalBool(boolean value) {
        return new BamlType(Kind.LITERAL, 0, null, List.of(), LIT_BOOL, value);
    }

    /**
     * A bigint-literal type token. The value rides the wire as a decimal string
     * (the {@code BamlTy} convention — a bigint has no fixed-width proto scalar).
     */
    public static BamlType literalBigint(BigInteger value) {
        Objects.requireNonNull(value, "value");
        return new BamlType(Kind.LITERAL, 0, null, List.of(), LIT_BIGINT, value.toString());
    }

    /**
     * A float-literal type token. The value rides the wire as its decimal source
     * text (a BAML float type preserves its formatting); this convenience takes
     * the already-formatted decimal string.
     */
    public static BamlType literalFloat(String decimalText) {
        Objects.requireNonNull(decimalText, "decimalText");
        return new BamlType(Kind.LITERAL, 0, null, List.of(), LIT_FLOAT, decimalText);
    }

    private static List<BamlType> copyChildren(String what, BamlType[] tokens) {
        List<BamlType> args = new ArrayList<>(tokens.length);
        for (BamlType arg : tokens) {
            args.add(Objects.requireNonNull(arg, what));
        }
        return List.copyOf(args);
    }

    // -- wire codec (internal; used by the call encoder/decoder) --------------

    /**
     * Render this token as a wire {@code BamlTy} message ({@code baml_type.proto}):
     * the type a request states. Internal: the call encoder serializes each
     * statement and explicit type argument through this. Nested tokens recurse.
     *
     * <p>Each occurrence of a runtime declaration's {@code live} head hands the
     * engine one reference, so it is written as a fresh clone of the row this
     * type keeps, owned by the request being encoded until the engine takes it.
     * A decode-only hint ({@link #typeVar}, {@link #OPAQUE}) throws
     * {@link IllegalStateException}; an anonymous declaration, which names
     * nothing the engine could resolve, throws {@link IllegalArgumentException}
     * (see {@link #isStatable}).
     */
    public byte[] toWireTy() {
        WireWriter ty = new WireWriter();
        switch (kind) {
            case PRIMITIVE -> {
                WireWriter prim = new WireWriter();
                prim.writeInt64(PRIM_KIND, primitiveKind);
                ty.writeMessage(TY_PRIMITIVE, prim.toByteArray());
            }
            case CLASS -> {
                WireWriter cls = new WireWriter();
                writeHead(cls, CLASS_NAME, CLASS_LIVE);
                for (BamlType arg : children) {
                    cls.writeMessage(CLASS_TYPE_ARGS, arg.toWireTy());
                }
                ty.writeMessage(TY_CLASS, cls.toByteArray());
            }
            case ENUM -> {
                WireWriter en = new WireWriter();
                writeHead(en, ENUM_NAME, ENUM_LIVE);
                ty.writeMessage(TY_ENUM, en.toByteArray());
            }
            case LIST -> {
                WireWriter lst = new WireWriter();
                lst.writeMessage(LIST_ITEM, children.get(0).toWireTy());
                ty.writeMessage(TY_LIST, lst.toByteArray());
            }
            case MAP -> {
                WireWriter mp = new WireWriter();
                mp.writeMessage(MAP_KEY, children.get(0).toWireTy());
                mp.writeMessage(MAP_VALUE, children.get(1).toWireTy());
                ty.writeMessage(TY_MAP, mp.toByteArray());
            }
            case OPTIONAL -> {
                WireWriter opt = new WireWriter();
                opt.writeMessage(OPT_INNER, children.get(0).toWireTy());
                ty.writeMessage(TY_OPTIONAL, opt.toByteArray());
            }
            case UNION -> {
                WireWriter un = new WireWriter();
                for (BamlType option : children) {
                    un.writeMessage(UNION_OPTIONS, option.toWireTy());
                }
                ty.writeMessage(TY_UNION, un.toByteArray());
            }
            case LITERAL -> ty.writeMessage(TY_LITERAL, literalToWire());
            case TYPE_ALIAS -> {
                WireWriter alias = new WireWriter();
                writeHead(alias, TYPE_ALIAS_NAME, TYPE_ALIAS_LIVE);
                ty.writeMessage(TY_TYPE_ALIAS, alias.toByteArray());
            }
            case NEVER -> ty.writeMessage(TY_NEVER, new byte[0]);
            case UNKNOWN -> ty.writeMessage(TY_UNKNOWN, new byte[0]);
            case PROMPT_AST -> ty.writeMessage(TY_PROMPT_AST, new byte[0]);
            case TYPE -> ty.writeMessage(TY_META_TYPE, new byte[0]);
            case ENUM_VARIANT -> {
                WireWriter variant = new WireWriter();
                writeHead(variant, ENUM_VARIANT_NAME, ENUM_VARIANT_LIVE);
                variant.writeString(ENUM_VARIANT_VARIANT, (String) literalValue);
                ty.writeMessage(TY_ENUM_VARIANT, variant.toByteArray());
            }
            case MEDIA -> {
                WireWriter media = new WireWriter();
                media.writeInt64(MEDIA_KIND, primitiveKind);
                ty.writeMessage(TY_MEDIA, media.toByteArray());
            }
            case INTERFACE -> {
                WireWriter iface = new WireWriter();
                writeHead(iface, INTERFACE_NAME, INTERFACE_LIVE);
                for (BamlType arg : children) {
                    iface.writeMessage(INTERFACE_TYPE_ARGS, arg.toWireTy());
                }
                ty.writeMessage(TY_INTERFACE, iface.toByteArray());
            }
            case FUNCTION -> {
                WireWriter fn = new WireWriter();
                for (Param param : params) {
                    WireWriter p = new WireWriter();
                    if (param.name() != null) {
                        p.writeString(PARAM_NAME, param.name());
                    }
                    p.writeMessage(PARAM_TY, param.type().toWireTy());
                    p.writeInt64(PARAM_MODE, param.optional() ? PARAM_MODE_OPTIONAL : PARAM_MODE_REQUIRED);
                    fn.writeMessage(FUNCTION_PARAMS, p.toByteArray());
                }
                fn.writeMessage(FUNCTION_RET, children.get(0).toWireTy());
                fn.writeMessage(FUNCTION_THROWS, children.get(1).toWireTy());
                ty.writeMessage(TY_FUNCTION, fn.toByteArray());
            }
            case TYPEVAR, OPAQUE -> throw new IllegalStateException(
                    "cannot encode a decode-only BamlType to the wire: " + this
                            + " (" + kind + " is a host-side decode hint, never an encoded type)");
        }
        return ty.toByteArray();
    }

    /** Write this nominal type's head: its name, or a clone of its runtime declaration's row. */
    private void writeHead(WireWriter head, int nameField, int liveField) {
        if (fqn != null) {
            head.writeString(nameField, fqn);
            return;
        }
        if (runtimeHead instanceof LiveHead live) {
            head.writeInt64(liveField, live.row.cloneKeyForWire());
            return;
        }
        AnonymousHead anonymous = (AnonymousHead) runtimeHead;
        throw new IllegalArgumentException(
                "the declaration " + anonymous.label() + " has no reference a request could"
                        + " state; pass a type the engine returned with it");
    }

    /**
     * Whether {@link #toWireTy} can state this type: it mentions no decode-only
     * hint ({@link #typeVar}, {@link #OPAQUE}) and no anonymous declaration.
     * Asking clones nothing.
     */
    public boolean isStatable() {
        if (kind == Kind.TYPEVAR || kind == Kind.OPAQUE || runtimeHead instanceof AnonymousHead) {
            return false;
        }
        return children.stream().allMatch(BamlType::isStatable)
                && params.stream().allMatch(param -> param.type().isStatable());
    }

    private byte[] literalToWire() {
        WireWriter lit = new WireWriter();
        switch (literalArm) {
            case LIT_STRING -> lit.writeString(LIT_STRING, (String) literalValue);
            case LIT_INT -> lit.writeInt64(LIT_INT, (Long) literalValue);
            case LIT_BOOL -> lit.writeBool(LIT_BOOL, (Boolean) literalValue);
            case LIT_BIGINT -> lit.writeString(LIT_BIGINT, (String) literalValue);
            case LIT_FLOAT -> lit.writeString(LIT_FLOAT, (String) literalValue);
            default -> throw new IllegalStateException("unreachable literal arm: " + literalArm);
        }
        return lit.toByteArray();
    }

    /**
     * Parse a wire {@code BamlTy} annotation into a token, or {@code null} when
     * the type falls outside the token grammar (a future, a native handle, a
     * resource, a type variable; or a composite whose nested tokens are
     * themselves outside the grammar). Internal: the decode side reads the
     * types the engine writes beside a value (a union's arms, a container's
     * element type, a generic instance's type arguments) through this. An
     * annotation references nothing: a declaration the runtime created appears
     * in one only as display text ({@code anonymous_label}), which the token
     * keeps and never states, and a {@code live} head there is outside the
     * grammar.
     */
    public static BamlType fromWireTy(byte[] bamlTyBytes) {
        return read(new WireReader(bamlTyBytes), Position.ANNOTATION);
    }

    /**
     * Adopt a {@code ty_value} the engine returned: a type value a host holds.
     * Each occurrence of a runtime declaration's {@code live} head hands the
     * host one engine reference, which the returned token owns and releases
     * when it is let go; an {@code anonymous_label} head is kept as display
     * text. A type the token grammar cannot hold throws
     * {@link IllegalStateException}: a type value never decodes to anything but
     * itself.
     */
    public static BamlType adoptWireTy(byte[] bamlTyBytes) {
        BamlType adopted = read(new WireReader(bamlTyBytes), Position.HELD);
        if (adopted == null) {
            throw new IllegalStateException(
                    "the engine returned a type value the Java bridge cannot represent");
        }
        return adopted;
    }

    /** Where a wire type was read, which decides the heads it may carry. */
    private enum Position {
        /** Beside a value: references nothing, so a live head is outside the grammar. */
        ANNOTATION,
        /** A type value the engine handed over, with its live rows and display labels. */
        HELD
    }

    private static BamlType read(WireReader r, Position position) {
        BamlType result = null;
        while (r.hasRemaining()) {
            int tag = r.readTag();
            int field = WireReader.fieldOf(tag);
            int wire = WireReader.wireOf(tag);
            switch (field) {
                case TY_PRIMITIVE -> result = primitiveFromWire(r.readMessage());
                case TY_CLASS -> result = withArgsFromWire(
                        Kind.CLASS, r.readMessage(), CLASS_NAME, CLASS_LIVE, CLASS_ANONYMOUS,
                        CLASS_TYPE_ARGS, position);
                case TY_ENUM -> result = withArgsFromWire(
                        Kind.ENUM, r.readMessage(), ENUM_NAME, ENUM_LIVE, ENUM_ANONYMOUS, 0, position);
                case TY_LIST -> result = listFromWire(r.readMessage(), position);
                case TY_MAP -> result = mapFromWire(r.readMessage(), position);
                case TY_OPTIONAL -> result = optionalFromWire(r.readMessage(), position);
                case TY_UNION -> result = unionFromWire(r.readMessage(), position);
                case TY_LITERAL -> result = literalFromWire(r.readMessage());
                case TY_TYPE_ALIAS -> result = withArgsFromWire(
                        Kind.TYPE_ALIAS, r.readMessage(), TYPE_ALIAS_NAME, TYPE_ALIAS_LIVE,
                        TYPE_ALIAS_ANONYMOUS, 0, position);
                case TY_NEVER -> {
                    r.readBytes();
                    result = NEVER;
                }
                case TY_UNKNOWN -> {
                    r.readBytes();
                    result = UNKNOWN;
                }
                case TY_PROMPT_AST -> {
                    r.readBytes();
                    result = PROMPT_AST;
                }
                case TY_META_TYPE -> {
                    r.readBytes();
                    result = TYPE;
                }
                case TY_FUNCTION -> result = functionFromWire(r.readMessage(), position);
                case TY_ENUM_VARIANT -> result = enumVariantFromWire(r.readMessage(), position);
                case TY_MEDIA -> result = mediaFromWire(r.readMessage());
                case TY_INTERFACE -> result = withArgsFromWire(
                        Kind.INTERFACE, r.readMessage(), INTERFACE_NAME, INTERFACE_LIVE,
                        INTERFACE_ANONYMOUS, INTERFACE_TYPE_ARGS, position);
                default -> r.skipField(wire);
            }
        }
        return result;
    }

    /** The head of a nominal message, gathered field by field. */
    private static final class HeadReader {
        String name;
        RuntimeHead runtime;
        boolean outOfGrammar;

        /**
         * Read {@code field} if it is a head field, at {@code position}: a named
         * head is its name, an {@code anonymous_label} its text, and a
         * {@code live} one a row this type now owns (in a held type only: an
         * annotation references nothing). Returns whether it was a head field.
         */
        boolean read(WireReader msg, int field, int nameField, int liveField, int anonymousField, Position position) {
            if (field == nameField) {
                name = msg.readString();
            } else if (field == liveField) {
                long key = msg.readVarint();
                if (position == Position.HELD) {
                    runtime = new LiveHead(new BamlHandle(key, BamlHandle.HANDLE_UNSPECIFIED));
                } else {
                    outOfGrammar = true;
                }
            } else if (field == anonymousField) {
                runtime = new AnonymousHead(msg.readString());
            } else {
                return false;
            }
            return true;
        }

        /** Whether a head was read and is in the grammar. */
        boolean present() {
            return !outOfGrammar && (name != null || runtime != null);
        }
    }

    /**
     * A class, enum, type alias or interface: its head, then (where
     * {@code argsField} is nonzero) its type arguments.
     */
    private static BamlType withArgsFromWire(
            Kind kind,
            WireReader msg,
            int nameField,
            int liveField,
            int anonymousField,
            int argsField,
            Position position) {
        HeadReader head = new HeadReader();
        List<BamlType> args = new ArrayList<>();
        boolean argOutOfGrammar = false;
        while (msg.hasRemaining()) {
            int tag = msg.readTag();
            int field = WireReader.fieldOf(tag);
            int wire = WireReader.wireOf(tag);
            if (head.read(msg, field, nameField, liveField, anonymousField, position)) {
                continue;
            }
            if (argsField != 0 && field == argsField) {
                BamlType arg = read(msg.readMessage(), position);
                if (arg == null) {
                    argOutOfGrammar = true;
                } else {
                    args.add(arg);
                }
            } else {
                msg.skipField(wire);
            }
        }
        if (!head.present() || argOutOfGrammar) {
            return null; // an unrepresentable nested arg poisons the whole token
        }
        return new BamlType(
                kind, 0, head.name, List.copyOf(args), List.of(), null, 0, null, head.runtime);
    }

    private static BamlType enumVariantFromWire(WireReader msg, Position position) {
        HeadReader head = new HeadReader();
        String variant = null;
        while (msg.hasRemaining()) {
            int tag = msg.readTag();
            int field = WireReader.fieldOf(tag);
            int wire = WireReader.wireOf(tag);
            if (head.read(msg, field, ENUM_VARIANT_NAME, ENUM_VARIANT_LIVE, ENUM_VARIANT_ANONYMOUS, position)) {
                continue;
            }
            if (field == ENUM_VARIANT_VARIANT) {
                variant = msg.readString();
            } else {
                msg.skipField(wire);
            }
        }
        if (!head.present() || variant == null) {
            return null;
        }
        return new BamlType(
                Kind.ENUM_VARIANT, 0, head.name, List.of(), List.of(), null, 0, variant, head.runtime);
    }

    private static BamlType mediaFromWire(WireReader msg) {
        int kind = 0;
        while (msg.hasRemaining()) {
            int tag = msg.readTag();
            if (WireReader.fieldOf(tag) == MEDIA_KIND) {
                kind = (int) msg.readVarint();
            } else {
                msg.skipField(WireReader.wireOf(tag));
            }
        }
        return kind < 1 || kind > 5 ? null : media(kind);
    }

    private static BamlType functionFromWire(WireReader msg, Position position) {
        List<Param> params = new ArrayList<>();
        BamlType ret = null;
        BamlType throwsType = null;
        while (msg.hasRemaining()) {
            int tag = msg.readTag();
            int field = WireReader.fieldOf(tag);
            int wire = WireReader.wireOf(tag);
            switch (field) {
                case FUNCTION_PARAMS -> {
                    Param param = paramFromWire(msg.readMessage(), position);
                    if (param == null) {
                        return null;
                    }
                    params.add(param);
                }
                case FUNCTION_RET -> ret = read(msg.readMessage(), position);
                case FUNCTION_THROWS -> throwsType = read(msg.readMessage(), position);
                default -> msg.skipField(wire);
            }
        }
        if (ret == null || throwsType == null) {
            return null;
        }
        return function(params, ret, throwsType);
    }

    private static Param paramFromWire(WireReader msg, Position position) {
        String name = null;
        BamlType type = null;
        boolean optional = false;
        while (msg.hasRemaining()) {
            int tag = msg.readTag();
            int field = WireReader.fieldOf(tag);
            int wire = WireReader.wireOf(tag);
            switch (field) {
                case PARAM_NAME -> name = msg.readString();
                case PARAM_TY -> type = read(msg.readMessage(), position);
                case PARAM_MODE -> optional = msg.readVarint() == PARAM_MODE_OPTIONAL;
                default -> msg.skipField(wire);
            }
        }
        return type == null ? null : new Param(name, type, optional);
    }

    private static BamlType primitiveFromWire(WireReader msg) {
        long kind = 0;
        while (msg.hasRemaining()) {
            int tag = msg.readTag();
            if (WireReader.fieldOf(tag) == PRIM_KIND) {
                kind = msg.readVarint();
            } else {
                msg.skipField(WireReader.wireOf(tag));
            }
        }
        return switch ((int) kind) {
            case PRIM_STRING -> STRING;
            case PRIM_INT -> INT;
            case PRIM_FLOAT -> FLOAT;
            case PRIM_BOOL -> BOOL;
            case PRIM_BIGINT -> BIGINT;
            case PRIM_NULL -> NULL;
            case PRIM_BYTES -> BYTES;
            default -> null; // an unspecified or future kind names no type
        };
    }

    private static BamlType listFromWire(WireReader msg, Position position) {
        BamlType item = readSingleChild(msg, LIST_ITEM, position);
        return item == null ? null : new BamlType(Kind.LIST, 0, null, List.of(item), 0, null);
    }

    private static BamlType mapFromWire(WireReader msg, Position position) {
        BamlType key = null;
        BamlType value = null;
        boolean childOutOfGrammar = false;
        while (msg.hasRemaining()) {
            int tag = msg.readTag();
            int field = WireReader.fieldOf(tag);
            int wire = WireReader.wireOf(tag);
            switch (field) {
                case MAP_KEY -> {
                    key = read(msg.readMessage(), position);
                    childOutOfGrammar |= key == null;
                }
                case MAP_VALUE -> {
                    value = read(msg.readMessage(), position);
                    childOutOfGrammar |= value == null;
                }
                default -> msg.skipField(wire);
            }
        }
        if (key == null || value == null || childOutOfGrammar) {
            return null;
        }
        return new BamlType(Kind.MAP, 0, null, List.of(key, value), 0, null);
    }

    private static BamlType optionalFromWire(WireReader msg, Position position) {
        BamlType inner = readSingleChild(msg, OPT_INNER, position);
        return inner == null ? null : new BamlType(Kind.OPTIONAL, 0, null, List.of(inner), 0, null);
    }

    private static BamlType unionFromWire(WireReader msg, Position position) {
        List<BamlType> options = new ArrayList<>();
        boolean optionOutOfGrammar = false;
        while (msg.hasRemaining()) {
            int tag = msg.readTag();
            int field = WireReader.fieldOf(tag);
            int wire = WireReader.wireOf(tag);
            if (field == UNION_OPTIONS) {
                BamlType option = read(msg.readMessage(), position);
                if (option == null) {
                    optionOutOfGrammar = true;
                } else {
                    options.add(option);
                }
            } else {
                msg.skipField(wire);
            }
        }
        if (optionOutOfGrammar) {
            return null;
        }
        return new BamlType(Kind.UNION, 0, null, List.copyOf(options), 0, null);
    }

    /** Read the sole {@code BamlTy} sub-field {@code fieldNumber} of a wrapper message. */
    private static BamlType readSingleChild(WireReader msg, int fieldNumber, Position position) {
        BamlType child = null;
        boolean outOfGrammar = false;
        while (msg.hasRemaining()) {
            int tag = msg.readTag();
            int field = WireReader.fieldOf(tag);
            int wire = WireReader.wireOf(tag);
            if (field == fieldNumber) {
                child = read(msg.readMessage(), position);
                outOfGrammar = child == null;
            } else {
                msg.skipField(wire);
            }
        }
        return outOfGrammar ? null : child;
    }

    private static BamlType literalFromWire(WireReader msg) {
        int arm = 0;
        Object value = null;
        while (msg.hasRemaining()) {
            int tag = msg.readTag();
            int field = WireReader.fieldOf(tag);
            int wire = WireReader.wireOf(tag);
            switch (field) {
                case LIT_STRING -> {
                    arm = LIT_STRING;
                    value = msg.readString();
                }
                case LIT_INT -> {
                    arm = LIT_INT;
                    value = msg.readVarint();
                }
                case LIT_BOOL -> {
                    arm = LIT_BOOL;
                    value = msg.readVarint() != 0;
                }
                case LIT_BIGINT -> {
                    arm = LIT_BIGINT;
                    value = msg.readString();
                }
                case LIT_FLOAT -> {
                    arm = LIT_FLOAT;
                    value = msg.readString();
                }
                default -> msg.skipField(wire);
            }
        }
        return arm == 0 ? null : new BamlType(Kind.LITERAL, 0, null, List.of(), arm, value);
    }

    // -- value semantics ------------------------------------------------------

    @Override
    public boolean equals(Object o) {
        if (this == o) {
            return true;
        }
        if (!(o instanceof BamlType other)) {
            return false;
        }
        return kind == other.kind
                && primitiveKind == other.primitiveKind
                && literalArm == other.literalArm
                && Objects.equals(fqn, other.fqn)
                && Objects.equals(literalValue, other.literalValue)
                && Objects.equals(runtimeHead, other.runtimeHead)
                && children.equals(other.children)
                && params.equals(other.params);
        // hostInterface is deliberately absent: it is how one host position
        // spells a function type, not part of the type.
    }

    @Override
    public int hashCode() {
        return Objects.hash(kind, primitiveKind, fqn, runtimeHead, children, params, literalArm, literalValue);
    }

    /**
     * A structural total order, consistent with {@link #equals} (returns 0 iff
     * equal): by {@link Kind} ordinal, then primitive kind, then FQN / TypeVar
     * name, then literal arm + value, then children lexicographically. It exists
     * only to normalize a union's arm list (sort + distinct) into a stable
     * registry key — never a rendered string, so a crafted literal value can never
     * alias two distinct arm sets.
     */
    @Override
    public int compareTo(BamlType o) {
        int c = Integer.compare(kind.ordinal(), o.kind.ordinal());
        if (c != 0) {
            return c;
        }
        c = Integer.compare(primitiveKind, o.primitiveKind);
        if (c != 0) {
            return c;
        }
        c = compareNullable(fqn, o.fqn);
        if (c != 0) {
            return c;
        }
        c = compareRuntimeHeads(runtimeHead, o.runtimeHead);
        if (c != 0) {
            return c;
        }
        c = Integer.compare(literalArm, o.literalArm);
        if (c != 0) {
            return c;
        }
        c = compareLiteralValue(literalValue, o.literalValue);
        if (c != 0) {
            return c;
        }
        int n = Math.min(children.size(), o.children.size());
        for (int i = 0; i < n; i++) {
            c = children.get(i).compareTo(o.children.get(i));
            if (c != 0) {
                return c;
            }
        }
        c = Integer.compare(children.size(), o.children.size());
        if (c != 0) {
            return c;
        }
        int m = Math.min(params.size(), o.params.size());
        for (int i = 0; i < m; i++) {
            Param a = params.get(i);
            Param b = o.params.get(i);
            c = compareNullable(a.name(), b.name());
            if (c != 0) {
                return c;
            }
            c = Boolean.compare(a.optional(), b.optional());
            if (c != 0) {
                return c;
            }
            c = a.type().compareTo(b.type());
            if (c != 0) {
                return c;
            }
        }
        return Integer.compare(params.size(), o.params.size());
    }

    /** Named heads first, then live heads by key, then anonymous ones by label. */
    private static int compareRuntimeHeads(RuntimeHead a, RuntimeHead b) {
        int c = Integer.compare(runtimeHeadRank(a), runtimeHeadRank(b));
        if (c != 0) {
            return c;
        }
        if (a instanceof LiveHead la && b instanceof LiveHead lb) {
            return Long.compare(la.row.key(), lb.row.key());
        }
        if (a instanceof AnonymousHead aa && b instanceof AnonymousHead ab) {
            return aa.label().compareTo(ab.label());
        }
        return 0;
    }

    private static int runtimeHeadRank(RuntimeHead head) {
        if (head == null) {
            return 0;
        }
        return head instanceof LiveHead ? 1 : 2;
    }

    private static int compareNullable(String a, String b) {
        if (a == null) {
            return b == null ? 0 : -1;
        }
        return b == null ? 1 : a.compareTo(b);
    }

    private static int compareLiteralValue(Object a, Object b) {
        if (a == null) {
            return b == null ? 0 : -1;
        }
        if (b == null) {
            return 1;
        }
        // A matching literalArm (compared first) implies a matching runtime type
        // (String / Long / Boolean — all Comparable), so this compare is total.
        @SuppressWarnings("unchecked")
        Comparable<Object> ca = (Comparable<Object>) a;
        return ca.compareTo(b);
    }

    /** How this nominal type's head reads: its name, or its runtime declaration's. */
    private String headName() {
        if (fqn != null) {
            return fqn;
        }
        if (runtimeHead instanceof LiveHead live) {
            return "<runtime declaration " + live.row.key() + ">";
        }
        return ((AnonymousHead) runtimeHead).label();
    }

    @Override
    public String toString() {
        return switch (kind) {
            case PRIMITIVE -> switch (primitiveKind) {
                case PRIM_INT -> "int";
                case PRIM_STRING -> "string";
                case PRIM_BOOL -> "bool";
                case PRIM_FLOAT -> "float";
                case PRIM_BIGINT -> "bigint";
                case PRIM_NULL -> "null";
                case PRIM_BYTES -> "uint8array";
                default -> "primitive(" + primitiveKind + ")";
            };
            case ENUM -> headName();
            case CLASS -> children.isEmpty()
                    ? headName()
                    : headName() + children.stream()
                            .map(BamlType::toString)
                            .collect(Collectors.joining(", ", "<", ">"));
            case LIST -> children.get(0) + "[]";
            case MAP -> "map<" + children.get(0) + ", " + children.get(1) + ">";
            case OPTIONAL -> children.get(0) + "?";
            case UNION -> children.stream()
                    .map(BamlType::toString)
                    .collect(Collectors.joining(" | "));
            case LITERAL -> literalArm == LIT_STRING
                    ? "\"" + literalValue + "\""
                    : String.valueOf(literalValue);
            case TYPE_ALIAS -> headName();
            case FUNCTION -> {
                StringBuilder out = new StringBuilder("(");
                for (int i = 0; i < params.size(); i++) {
                    Param param = params.get(i);
                    if (i > 0) {
                        out.append(", ");
                    }
                    if (param.name() != null) {
                        out.append(param.name()).append(": ");
                    }
                    out.append(param.type());
                    if (param.optional()) {
                        out.append(" = ...");
                    }
                }
                yield out.append(") -> ")
                        .append(children.get(0))
                        .append(" throws ")
                        .append(children.get(1))
                        .toString();
            }
            case ENUM_VARIANT -> headName() + "." + literalValue;
            case MEDIA -> switch (primitiveKind) {
                case 1 -> "image";
                case 2 -> "audio";
                case 3 -> "video";
                case 4 -> "pdf";
                default -> "media";
            };
            case INTERFACE -> children.isEmpty()
                    ? headName()
                    : headName() + "<" + children.stream().map(BamlType::toString).collect(java.util.stream.Collectors.joining(", ")) + ">";
            case NEVER -> "never";
            case PROMPT_AST -> "prompt";
            case TYPE -> "type";
            case TYPEVAR -> fqn;
            case UNKNOWN -> "unknown";
            case OPAQUE -> "<opaque>";
        };
    }

    // -- structural matching (decode-side arm selection) ----------------------

    /** This token's shape. */
    public Kind kind() {
        return kind;
    }

    /**
     * The BAML FQN of a named declaration (CLASS / ENUM / TYPE_ALIAS /
     * INTERFACE / ENUM_VARIANT), or the TypeVar name (TYPEVAR); else
     * {@code null}, as for a declaration the runtime created, which no name
     * resolves.
     */
    public String fqn() {
        return fqn;
    }

    /** Whether this is the {@code int} primitive token. */
    public boolean isInt() {
        return kind == Kind.PRIMITIVE && primitiveKind == PRIM_INT;
    }

    /** Whether this is the {@code bigint} primitive token. */
    public boolean isBigint() {
        return kind == Kind.PRIMITIVE && primitiveKind == PRIM_BIGINT;
    }

    /** Whether this is the {@code string} primitive token. */
    public boolean isString() {
        return kind == Kind.PRIMITIVE && primitiveKind == PRIM_STRING;
    }

    /** Whether this is the {@code bool} primitive token. */
    public boolean isBool() {
        return kind == Kind.PRIMITIVE && primitiveKind == PRIM_BOOL;
    }

    /** Whether this is the {@code float} primitive token. */
    public boolean isFloat() {
        return kind == Kind.PRIMITIVE && primitiveKind == PRIM_FLOAT;
    }

    /** Whether this token is a decode-side wildcard (TYPEVAR / UNKNOWN). */
    public boolean isWildcard() {
        return kind == Kind.UNKNOWN || kind == Kind.TYPEVAR || kind == Kind.OPAQUE;
    }

    /** The element token of a LIST. */
    public BamlType listItem() {
        return children.get(0);
    }

    /** The key token of a MAP. */
    public BamlType mapKey() {
        return children.get(0);
    }

    /** The value token of a MAP. */
    public BamlType mapValue() {
        return children.get(1);
    }

    /** The inner token of an OPTIONAL. */
    public BamlType optionalInner() {
        return children.get(0);
    }

    /** The option tokens of a UNION (declaration order). */
    public List<BamlType> unionOptions() {
        return children;
    }

    /** The type arguments of a {@link Kind#CLASS} or {@link Kind#INTERFACE} type, in order. */
    public List<BamlType> classTypeArgs() {
        return children;
    }

    /** The parameters of a {@link Kind#FUNCTION} type, in declaration order. */
    public List<Param> functionParams() {
        return params;
    }

    public BamlType functionReturn() {
        return children.get(0);
    }

    public BamlType functionThrows() {
        return children.get(1);
    }

    /**
     * The functional interface a generated position spells this {@link
     * Kind#FUNCTION} type as, or {@code null} when no position named one.
     */
    public Class<?> functionHostInterface() {
        return hostInterface;
    }

    /** The literal base name ({@code string} / {@code int} / {@code bool} / {@code bigint} / {@code float}). */
    public String literalBase() {
        return switch (literalArm) {
            case LIT_STRING -> "string";
            case LIT_INT -> "int";
            case LIT_BOOL -> "bool";
            case LIT_BIGINT -> "bigint";
            case LIT_FLOAT -> "float";
            default -> "?";
        };
    }

    /**
     * Whether this token (a declared type or union arm) matches a
     * <em>precise</em> wire token {@code wire} — the {@link #fromWireTy} of a
     * container's element / key / value type. Both sides are precise (no bare /
     * imprecise inners — those surface as a {@code null} {@code wire} handled by
     * the caller as a wildcard), so this is a structural compatibility check:
     * primitives by kind, named types by FQN (kind- and type-arg-agnostic — the
     * class-generic-args safe bare-inner fallback), containers recursively, and a
     * wire optional unwrapped to its inner (wire {@code BamlTy} tokenizer parity).
     * A wildcard arm ({@link #isWildcard}) matches anything.
     */
    public boolean matchesStructural(BamlType wire) {
        // A wire optional contributes its inner (mirrors the wire tokenizer,
        // which unwraps an optional element to its inner token).
        if (wire != null && wire.kind == Kind.OPTIONAL) {
            return matchesStructural(wire.children.get(0));
        }
        // A wire union with a null member is the optional of the rest.
        if (wire != null && wire.kind == Kind.UNION && wire.children.contains(NULL)) {
            List<BamlType> rest = wire.children.stream().filter(option -> !option.equals(NULL)).toList();
            if (rest.isEmpty()) {
                return matchesStructural(NULL);
            }
            return matchesStructural(rest.size() == 1 ? rest.get(0) : union(rest.toArray(new BamlType[0])));
        }
        if (isWildcard()) {
            return true;
        }
        if (wire == null) {
            return false;
        }
        return switch (kind) {
            case PRIMITIVE -> wire.kind == Kind.PRIMITIVE && primitiveKind == wire.primitiveKind;
            // A named type matches by its FQN whatever kind the wire spells it
            // as: a recursive alias arrives as a type_alias token, and the arm
            // tokens render every named type as a class.
            case CLASS, ENUM, TYPE_ALIAS -> (wire.kind == Kind.CLASS
                            || wire.kind == Kind.ENUM
                            || wire.kind == Kind.TYPE_ALIAS)
                    && Objects.equals(fqn, wire.fqn);
            case LIST -> wire.kind == Kind.LIST
                    && children.get(0).matchesStructural(wire.children.get(0));
            case MAP -> wire.kind == Kind.MAP
                    && children.get(0).matchesStructural(wire.children.get(0))
                    && children.get(1).matchesStructural(wire.children.get(1));
            case OPTIONAL -> children.get(0).matchesStructural(wire);
            case LITERAL -> wire.kind == Kind.LITERAL
                    && literalArm == wire.literalArm
                    && Objects.equals(literalValue, wire.literalValue);
            case UNION -> children.stream().anyMatch(opt -> opt.matchesStructural(wire));
            default -> false;
        };
    }
}
