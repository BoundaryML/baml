package baml_bridge;

import baml_bridge.internal.WireReader;
import baml_bridge.internal.WireWriter;

import java.math.BigInteger;
import java.util.ArrayList;
import java.util.List;
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
 *   <li>the primitive constants {@link #INT}, {@link #STRING}, {@link #BOOL},
 *       {@link #FLOAT};</li>
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
 * <h2>Decode-only hints</h2>
 * Two kinds exist purely to drive host-side decode and never ride the encode
 * wire: {@link #UNKNOWN} (decode this value wire-driven, matches any arm) and
 * {@link #typeVar} (a bound TypeVar's decode placeholder). {@link #toWireTy} on
 * either throws {@link IllegalStateException} — they are decode descriptors, not
 * values. {@link #classByFqn} builds a named-type token straight from a BAML FQN
 * (no {@link TypeRegistry} lookup, so it names runtime-owned / not-yet-loaded
 * types too); decode resolves the FQN against the registry.
 *
 * <h2>Wire {@code BamlTy} ({@code baml_type.proto})</h2>
 * A token renders to the shared {@code BamlTy} oneof:
 * <pre>
 * BamlTy oneof:      primitive = 1, class_ty = 2, enum = 3, list = 4, map = 5,
 *                    optional = 6, union = 7, literal = 8
 * BamlTyPrimitive:   kind = 1 (BamlTyPrimitiveKind: STRING=1, INT=2, FLOAT=3, BOOL=4)
 * BamlTyClass:       name = 1 (BAML FQN), type_args = 2 (repeated BamlTy)
 * BamlTyEnum:        name = 1 (BAML FQN — enums are never generic)
 * BamlTyList:        item = 1 (BamlTy)
 * BamlTyMap:         key = 1 (BamlTy), value = 2 (BamlTy)
 * BamlTyOptional:    inner = 1 (BamlTy)     (BAML `T?` — matches Python's `_fill_wire_ty`)
 * BamlTyUnion:       options = 1 (repeated BamlTy)
 * BamlTyLiteral:     oneof literal: string_value=1, int_value=2, bool_value=3,
 *                    bigint_value=4 (decimal string), float_value=5 (decimal string)
 * </pre>
 * {@link #fromWireTy} returns {@code null} for any arm outside this grammar (a
 * wire type the token cannot represent, e.g. media / function / rust_type /
 * unknown / never / the {@code null}/{@code bytes}/{@code bigint} primitive
 * kinds), and for any composite whose nested tokens are themselves outside the
 * grammar — a partial token would misalign positions, so the whole token is
 * poisoned.
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

    // BamlTyPrimitiveKind enum values (the four in the token grammar).
    private static final int PRIM_STRING = 1;
    private static final int PRIM_INT = 2;
    private static final int PRIM_FLOAT = 3;
    private static final int PRIM_BOOL = 4;

    /**
     * The token's shape. The first eight render on the wire (a {@code BamlTy}
     * oneof arm); {@link #TYPEVAR} and {@link #UNKNOWN} are decode-only hints
     * (they throw on {@link #toWireTy}).
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
        /** A bound TypeVar's decode placeholder (name in {@link #fqn}); wildcard in matching. */
        TYPEVAR,
        /** Decode-this-value-wire-driven; a wildcard that matches any arm. */
        UNKNOWN
    }

    public static final BamlType STRING = primitive(PRIM_STRING);
    public static final BamlType INT = primitive(PRIM_INT);
    public static final BamlType BOOL = primitive(PRIM_BOOL);
    public static final BamlType FLOAT = primitive(PRIM_FLOAT);

    /**
     * The decode-only "decode wire-driven / match any arm" hint. Never encodes
     * ({@link #toWireTy} throws) — it is a decode descriptor, not a value.
     */
    public static final BamlType UNKNOWN =
            new BamlType(Kind.UNKNOWN, 0, null, List.of(), 0, null);

    private final Kind kind;
    private final int primitiveKind; // PRIMITIVE only (a BamlTyPrimitiveKind value)
    private final String fqn; // CLASS / ENUM only
    // CLASS: concrete type args (declaration order). LIST: [item]. MAP:
    // [key, value]. OPTIONAL: [inner]. UNION: the options. Always non-null.
    private final List<BamlType> children;
    private final int literalArm; // LITERAL only (a BamlTyLiteral oneof arm)
    private final Object literalValue; // LITERAL only: String / Long / Boolean

    private BamlType(
            Kind kind,
            int primitiveKind,
            String fqn,
            List<BamlType> children,
            int literalArm,
            Object literalValue) {
        this.kind = kind;
        this.primitiveKind = primitiveKind;
        this.fqn = fqn;
        this.children = children;
        this.literalArm = literalArm;
        this.literalValue = literalValue;
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
     * Render this token as a wire {@code BamlTy} message ({@code baml_type.proto}).
     * Internal: the call encoder serializes each {@code _types=} binding's value
     * through this. Nested tokens recurse.
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
                cls.writeString(CLASS_NAME, fqn);
                for (BamlType arg : children) {
                    cls.writeMessage(CLASS_TYPE_ARGS, arg.toWireTy());
                }
                ty.writeMessage(TY_CLASS, cls.toByteArray());
            }
            case ENUM -> {
                WireWriter en = new WireWriter();
                en.writeString(ENUM_NAME, fqn);
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
            case TYPEVAR, UNKNOWN -> throw new IllegalStateException(
                    "cannot encode a decode-only BamlType to the wire: " + this
                            + " (" + kind + " is a host-side decode hint, never an encoded value)");
            default -> throw new IllegalStateException("unreachable BamlType kind: " + kind);
        }
        return ty.toByteArray();
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
     * Parse a wire {@code BamlTy} message into a token, or {@code null} when the
     * type falls outside the token grammar (any arm other than a
     * {@code primitive} of kind int/string/bool/float, {@code class_ty},
     * {@code enum}, {@code list}, {@code map}, {@code optional}, {@code union},
     * or {@code literal}; or a composite whose nested tokens are themselves
     * outside the grammar). Internal: used on the decode side to reconstruct the
     * reified type-args of a generic class value.
     */
    public static BamlType fromWireTy(byte[] bamlTyBytes) {
        WireReader r = new WireReader(bamlTyBytes);
        BamlType result = null;
        while (r.hasRemaining()) {
            int tag = r.readTag();
            int field = WireReader.fieldOf(tag);
            int wire = WireReader.wireOf(tag);
            switch (field) {
                case TY_PRIMITIVE -> result = primitiveFromWire(r.readMessage());
                case TY_CLASS -> result = classFromWire(r.readMessage());
                case TY_ENUM -> result = enumFromWire(r.readMessage());
                case TY_LIST -> result = listFromWire(r.readMessage());
                case TY_MAP -> result = mapFromWire(r.readMessage());
                case TY_OPTIONAL -> result = optionalFromWire(r.readMessage());
                case TY_UNION -> result = unionFromWire(r.readMessage());
                case TY_LITERAL -> result = literalFromWire(r.readMessage());
                default -> r.skipField(wire);
            }
        }
        return result;
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
            default -> null; // null/bytes/bigint and any future kind are out of grammar
        };
    }

    private static BamlType classFromWire(WireReader msg) {
        String name = null;
        List<BamlType> args = new ArrayList<>();
        boolean argOutOfGrammar = false;
        while (msg.hasRemaining()) {
            int tag = msg.readTag();
            int field = WireReader.fieldOf(tag);
            int wire = WireReader.wireOf(tag);
            switch (field) {
                case CLASS_NAME -> name = msg.readString();
                case CLASS_TYPE_ARGS -> {
                    BamlType arg = fromWireTy(msg.readBytes());
                    if (arg == null) {
                        argOutOfGrammar = true;
                    } else {
                        args.add(arg);
                    }
                }
                default -> msg.skipField(wire);
            }
        }
        if (name == null || argOutOfGrammar) {
            return null; // an unrepresentable nested arg poisons the whole token
        }
        return new BamlType(Kind.CLASS, 0, name, List.copyOf(args), 0, null);
    }

    private static BamlType enumFromWire(WireReader msg) {
        String name = null;
        while (msg.hasRemaining()) {
            int tag = msg.readTag();
            if (WireReader.fieldOf(tag) == ENUM_NAME) {
                name = msg.readString();
            } else {
                msg.skipField(WireReader.wireOf(tag));
            }
        }
        return name == null ? null : new BamlType(Kind.ENUM, 0, name, List.of(), 0, null);
    }

    private static BamlType listFromWire(WireReader msg) {
        BamlType item = readSingleChild(msg, LIST_ITEM);
        return item == null ? null : new BamlType(Kind.LIST, 0, null, List.of(item), 0, null);
    }

    private static BamlType mapFromWire(WireReader msg) {
        BamlType key = null;
        BamlType value = null;
        boolean childOutOfGrammar = false;
        while (msg.hasRemaining()) {
            int tag = msg.readTag();
            int field = WireReader.fieldOf(tag);
            int wire = WireReader.wireOf(tag);
            switch (field) {
                case MAP_KEY -> {
                    key = fromWireTy(msg.readBytes());
                    childOutOfGrammar |= key == null;
                }
                case MAP_VALUE -> {
                    value = fromWireTy(msg.readBytes());
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

    private static BamlType optionalFromWire(WireReader msg) {
        BamlType inner = readSingleChild(msg, OPT_INNER);
        return inner == null ? null : new BamlType(Kind.OPTIONAL, 0, null, List.of(inner), 0, null);
    }

    private static BamlType unionFromWire(WireReader msg) {
        List<BamlType> options = new ArrayList<>();
        boolean optionOutOfGrammar = false;
        while (msg.hasRemaining()) {
            int tag = msg.readTag();
            int field = WireReader.fieldOf(tag);
            int wire = WireReader.wireOf(tag);
            if (field == UNION_OPTIONS) {
                BamlType option = fromWireTy(msg.readBytes());
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
    private static BamlType readSingleChild(WireReader msg, int fieldNumber) {
        BamlType child = null;
        boolean outOfGrammar = false;
        while (msg.hasRemaining()) {
            int tag = msg.readTag();
            int field = WireReader.fieldOf(tag);
            int wire = WireReader.wireOf(tag);
            if (field == fieldNumber) {
                child = fromWireTy(msg.readBytes());
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
                && children.equals(other.children);
    }

    @Override
    public int hashCode() {
        return Objects.hash(kind, primitiveKind, fqn, children, literalArm, literalValue);
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
        return Integer.compare(children.size(), o.children.size());
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

    @Override
    public String toString() {
        return switch (kind) {
            case PRIMITIVE -> switch (primitiveKind) {
                case PRIM_INT -> "int";
                case PRIM_STRING -> "string";
                case PRIM_BOOL -> "bool";
                case PRIM_FLOAT -> "float";
                default -> "primitive(" + primitiveKind + ")";
            };
            case ENUM -> fqn;
            case CLASS -> children.isEmpty()
                    ? fqn
                    : fqn + children.stream()
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
            case TYPEVAR -> fqn;
            case UNKNOWN -> "unknown";
        };
    }

    // -- structural matching (decode-side arm selection) ----------------------

    /** This token's shape. */
    public Kind kind() {
        return kind;
    }

    /** The BAML FQN (CLASS / ENUM), or the TypeVar name (TYPEVAR); else {@code null}. */
    public String fqn() {
        return fqn;
    }

    /** Whether this is the {@code int} primitive token. */
    public boolean isInt() {
        return kind == Kind.PRIMITIVE && primitiveKind == PRIM_INT;
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
        return kind == Kind.TYPEVAR || kind == Kind.UNKNOWN;
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
        if (isWildcard()) {
            return true;
        }
        if (wire == null) {
            return false;
        }
        return switch (kind) {
            case PRIMITIVE -> wire.kind == Kind.PRIMITIVE && primitiveKind == wire.primitiveKind;
            case CLASS, ENUM -> (wire.kind == Kind.CLASS || wire.kind == Kind.ENUM)
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
