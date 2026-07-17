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
 * (per kind) primitive kind / FQN / nested tokens / literal arm+value match.
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
public final class BamlType {
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

    /** How this token renders on the wire (which {@code BamlTy} oneof arm). */
    private enum Kind {
        PRIMITIVE,
        CLASS,
        ENUM,
        LIST,
        MAP,
        OPTIONAL,
        UNION,
        LITERAL
    }

    public static final BamlType STRING = primitive(PRIM_STRING);
    public static final BamlType INT = primitive(PRIM_INT);
    public static final BamlType BOOL = primitive(PRIM_BOOL);
    public static final BamlType FLOAT = primitive(PRIM_FLOAT);

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
        };
    }
}
