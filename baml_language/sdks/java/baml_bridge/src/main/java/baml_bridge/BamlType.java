package baml_bridge;

import baml_bridge.internal.WireReader;
import baml_bridge.internal.WireWriter;

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
 * <h2>Minimal grammar</h2>
 * <ul>
 *   <li>the primitive constants {@link #INT}, {@link #STRING}, {@link #BOOL},
 *       {@link #FLOAT};</li>
 *   <li>{@link #of(Class)} for a registered generated class or enum (the BAML
 *       FQN is resolved via {@link TypeRegistry} — the same lookup the value
 *       encoder uses); and</li>
 *   <li>{@link #of(Class, BamlType...)} for a reified generic class (e.g.
 *       {@code Box<int>}), carrying its concrete args in declaration order.</li>
 * </ul>
 *
 * <p>Value semantics: two tokens are {@link #equals equal} when their kind, and
 * (per kind) primitive kind / FQN / nested type-arg tokens match.
 *
 * <h2>Wire {@code BamlTy} ({@code baml_type.proto})</h2>
 * A token renders to the shared {@code BamlTy} oneof — {@code primitive}
 * (field 1), {@code class_ty} (field 2), or {@code enum} (field 3). Only these
 * three arms are in the minimal grammar; {@link #fromWireTy} returns {@code null}
 * for any other arm (a wire type outside the grammar).
 *
 * <pre>
 * BamlTy oneof:      primitive = 1 (BamlTyPrimitive), class_ty = 2 (BamlTyClass),
 *                    enum = 3 (BamlTyEnum)
 * BamlTyPrimitive:   kind = 1 (BamlTyPrimitiveKind: STRING=1, INT=2, FLOAT=3, BOOL=4)
 * BamlTyClass:       name = 1 (BAML FQN), type_args = 2 (repeated BamlTy)
 * BamlTyEnum:        name = 1 (BAML FQN — enums are never generic)
 * </pre>
 */
public final class BamlType {
    // BamlTy oneof field numbers (baml_type.proto).
    private static final int TY_PRIMITIVE = 1;
    private static final int TY_CLASS = 2;
    private static final int TY_ENUM = 3;

    // Sub-message field numbers.
    private static final int PRIM_KIND = 1; // BamlTyPrimitive.kind
    private static final int CLASS_NAME = 1; // BamlTyClass.name
    private static final int CLASS_TYPE_ARGS = 2; // BamlTyClass.type_args (repeated)
    private static final int ENUM_NAME = 1; // BamlTyEnum.name

    // BamlTyPrimitiveKind enum values (the four in the minimal grammar).
    private static final int PRIM_STRING = 1;
    private static final int PRIM_INT = 2;
    private static final int PRIM_FLOAT = 3;
    private static final int PRIM_BOOL = 4;

    /** How this token renders on the wire (which {@code BamlTy} oneof arm). */
    private enum Kind {
        PRIMITIVE,
        CLASS,
        ENUM
    }

    public static final BamlType STRING = new BamlType(Kind.PRIMITIVE, PRIM_STRING, null, List.of());
    public static final BamlType INT = new BamlType(Kind.PRIMITIVE, PRIM_INT, null, List.of());
    public static final BamlType BOOL = new BamlType(Kind.PRIMITIVE, PRIM_BOOL, null, List.of());
    public static final BamlType FLOAT = new BamlType(Kind.PRIMITIVE, PRIM_FLOAT, null, List.of());

    private final Kind kind;
    private final int primitiveKind; // PRIMITIVE only (a BamlTyPrimitiveKind value)
    private final String fqn; // CLASS / ENUM only
    private final List<BamlType> typeArgs; // CLASS only; always non-null (possibly empty)

    private BamlType(Kind kind, int primitiveKind, String fqn, List<BamlType> typeArgs) {
        this.kind = kind;
        this.primitiveKind = primitiveKind;
        this.fqn = fqn;
        this.typeArgs = typeArgs;
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
            return new BamlType(Kind.CLASS, 0, classFqn, List.of());
        }
        String enumFqn = TypeRegistry.enumFqnForJavaClass(type);
        if (enumFqn != null) {
            return new BamlType(Kind.ENUM, 0, enumFqn, List.of());
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
        List<BamlType> args = new ArrayList<>(typeArgs.length);
        for (BamlType arg : typeArgs) {
            args.add(Objects.requireNonNull(arg, "type argument"));
        }
        return new BamlType(Kind.CLASS, 0, classFqn, List.copyOf(args));
    }

    // -- wire codec (internal; used by the call encoder/decoder) --------------

    /**
     * Render this token as a wire {@code BamlTy} message ({@code baml_type.proto}).
     * Internal: the call encoder serializes each {@code _types=} binding's value
     * through this. Nested class type-args recurse.
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
                for (BamlType arg : typeArgs) {
                    cls.writeMessage(CLASS_TYPE_ARGS, arg.toWireTy());
                }
                ty.writeMessage(TY_CLASS, cls.toByteArray());
            }
            case ENUM -> {
                WireWriter en = new WireWriter();
                en.writeString(ENUM_NAME, fqn);
                ty.writeMessage(TY_ENUM, en.toByteArray());
            }
            default -> throw new IllegalStateException("unreachable BamlType kind: " + kind);
        }
        return ty.toByteArray();
    }

    /**
     * Parse a wire {@code BamlTy} message into a token, or {@code null} when the
     * type falls outside the minimal grammar (any arm other than a
     * {@code primitive} of kind int/string/bool/float, a {@code class_ty}, or an
     * {@code enum}; or a {@code class_ty} whose own type-args are themselves
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
        return new BamlType(Kind.CLASS, 0, name, List.copyOf(args));
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
        return name == null ? null : new BamlType(Kind.ENUM, 0, name, List.of());
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
                && Objects.equals(fqn, other.fqn)
                && typeArgs.equals(other.typeArgs);
    }

    @Override
    public int hashCode() {
        return Objects.hash(kind, primitiveKind, fqn, typeArgs);
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
            case CLASS -> typeArgs.isEmpty()
                    ? fqn
                    : fqn + typeArgs.stream()
                            .map(BamlType::toString)
                            .collect(Collectors.joining(", ", "<", ">"));
        };
    }
}
