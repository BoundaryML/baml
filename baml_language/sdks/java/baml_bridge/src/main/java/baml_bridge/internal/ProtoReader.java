package baml_bridge.internal;

import baml_bridge.BamlError;
import baml_bridge.BamlPanic;
import baml_bridge.TypeRegistry;

import java.lang.reflect.Constructor;
import java.math.BigInteger;
import java.util.ArrayList;
import java.util.Collections;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;

/**
 * Decodes a {@code BamlOutboundResult} envelope ({@code baml_outbound.proto})
 * for the primitives slice, dispatching its {@code ok} / {@code error} /
 * {@code panic} arm. Mirrors the Python bridge's {@code decode_call_result} /
 * {@code decode_value} (proto.py §09e).
 *
 * <h2>Field numbers (against {@code baml_outbound.proto})</h2>
 * <pre>
 * BamlOutboundResult oneof: ok = 1, error = 2, panic = 3
 * BamlOutboundError:  value = 1 (BamlOutboundValue), trace = 2 (repeated string)
 * BamlOutboundPanic:  value = 1, trace = 2, is_exit_panic = 3, exit_code = 4
 * BamlOutboundValue oneof: null_value = 2, string_value = 3, int_value = 4,
 *   float_value = 5, bool_value = 6, class_value = 7, enum_value = 8,
 *   literal_value = 9, list_value = 11, map_value = 12,
 *   union_variant_value = 13, handle_value = 16, media_value = 17,
 *   prompt_ast_value = 18, uint8array_value = 19, bigint_value = 20, ty_value = 21
 * BamlLiteralValue oneof: string_value = 1, int_value = 2, bool_value = 3,
 *   bigint_value = 4 (hex), float_value = 5 (source text)
 * BamlValueList: item_type = 1, items = 2 (repeated BamlOutboundValue)
 * BamlValueMap:  key_type = 1, value_type = 2, entries = 3 (BamlOutboundMapEntry)
 * BamlOutboundMapEntry: key = 1 (string), value = 2 (BamlOutboundValue)
 * BamlValueClass: name = 1, fields = 2 (BamlOutboundMapEntry), type_args = 3
 * BamlValueEnum:  name = 1, value = 2, is_dynamic = 3
 * BamlValueUnionVariant: name = 1, is_optional = 2, is_single_pattern = 3,
 *   self_type = 4, value_option_name = 5, value = 6 (BamlOutboundValue)
 * </pre>
 */
public final class ProtoReader {
    // BamlOutboundResult
    private static final int RESULT_OK = 1;
    private static final int RESULT_ERROR = 2;
    private static final int RESULT_PANIC = 3;

    // BamlOutboundError / BamlOutboundPanic
    private static final int ERR_VALUE = 1;
    private static final int ERR_TRACE = 2;
    private static final int PANIC_VALUE = 1;
    private static final int PANIC_TRACE = 2;
    private static final int PANIC_IS_EXIT = 3;
    private static final int PANIC_EXIT_CODE = 4;

    // BamlOutboundValue oneof
    private static final int OV_NULL = 2;
    private static final int OV_STRING = 3;
    private static final int OV_INT = 4;
    private static final int OV_FLOAT = 5;
    private static final int OV_BOOL = 6;
    private static final int OV_CLASS = 7;
    private static final int OV_ENUM = 8;
    private static final int OV_LITERAL = 9;
    private static final int OV_LIST = 11;
    private static final int OV_MAP = 12;
    private static final int OV_UNION = 13;
    private static final int OV_HANDLE = 16;
    private static final int OV_MEDIA = 17;
    private static final int OV_PROMPT_AST = 18;
    private static final int OV_UINT8ARRAY = 19;
    private static final int OV_BIGINT = 20;
    private static final int OV_TY = 21;

    // BamlLiteralValue oneof
    private static final int LIT_STRING = 1;
    private static final int LIT_INT = 2;
    private static final int LIT_BOOL = 3;
    private static final int LIT_BIGINT = 4;
    private static final int LIT_FLOAT = 5;

    // BamlValueList / BamlValueMap / BamlOutboundMapEntry
    private static final int LIST_ITEMS = 2;
    private static final int MAP_ENTRIES = 3;
    private static final int MAP_ENTRY_KEY = 1;
    private static final int MAP_ENTRY_VALUE = 2;

    // BamlValueClass / BamlValueEnum
    private static final int CLASS_NAME = 1;
    private static final int CLASS_FIELDS = 2;
    private static final int ENUM_NAME = 1;
    private static final int ENUM_VALUE = 2;

    // BamlValueUnionVariant
    private static final int UNION_SELF_TYPE = 4;
    private static final int UNION_VALUE = 6;

    // BamlTy oneof (baml_type.proto) — used to tokenize a union's self_type into
    // the arm-token signature the TypeRegistry keys on. Unlisted variants fall to
    // an unmatchable "?" token, which forces the bare-inner fallback.
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
    private static final int TY_FUNCTION = 14;
    private static final int TY_RUST_TYPE = 16;
    private static final int TY_VOID = 20;

    // Sub-message field numbers on the BamlTy variants.
    private static final int TY_NAME = 1; // BamlTyClass/BamlTyEnum.name
    private static final int TY_PRIMITIVE_KIND = 1; // BamlTyPrimitive.kind (enum varint)
    private static final int TY_MEDIA_KIND = 1; // BamlTyMedia.kind (enum varint)
    private static final int TY_LIST_ITEM = 1; // BamlTyList.item
    private static final int TY_MAP_KEY = 1; // BamlTyMap.key
    private static final int TY_MAP_VALUE = 2; // BamlTyMap.value
    private static final int TY_OPTIONAL_INNER = 1; // BamlTyOptional.inner
    private static final int TY_UNION_OPTIONS = 1; // BamlTyUnion.options (repeated)

    // BamlTyPrimitiveKind enum values.
    private static final int PRIM_STRING = 1;
    private static final int PRIM_INT = 2;
    private static final int PRIM_FLOAT = 3;
    private static final int PRIM_BOOL = 4;
    private static final int PRIM_NULL = 5;
    private static final int PRIM_BYTES = 6;
    private static final int PRIM_BIGINT = 7;

    // BamlTyMediaKind enum values.
    private static final int MEDIA_IMAGE = 1;
    private static final int MEDIA_AUDIO = 2;
    private static final int MEDIA_VIDEO = 3;
    private static final int MEDIA_PDF = 4;
    private static final int MEDIA_GENERIC = 5;

    private ProtoReader() {}

    /**
     * Decode a {@code BamlOutboundResult} envelope with no return-type descriptor
     * (wire-driven decode — the pre-descriptor behavior). Equivalent to
     * {@link #decodeOutboundResult(byte[], String)} with a {@code null} descriptor.
     */
    public static Object decodeOutboundResult(byte[] data) {
        return decodeOutboundResult(data, null);
    }

    /**
     * Decode a {@code BamlOutboundResult} envelope. Returns the decoded value on
     * the {@code ok} arm; throws {@link BamlError} on {@code error}; throws
     * {@link BamlPanic} on a non-exit {@code panic}; and for an exit panic
     * ({@code is_exit_panic}) terminates the process via
     * {@code Runtime.getRuntime().halt(exit_code)}.
     *
     * <p>{@code returnDesc} is the type-directed decode descriptor for the
     * declared return type (see {@code ref-java-codegen-conventions.md}); when
     * non-null it drives the {@code ok}-arm decode against the declared shape
     * (unions land on the {@code Union{k}} arm family, chosen from the declared
     * arm order). {@code error}/{@code panic} values always stay wire-driven — a
     * thrown BAML value is self-describing and carries no host return type.
     */
    public static Object decodeOutboundResult(byte[] data, String returnDesc) {
        WireReader r = new WireReader(data);
        int arm = 0;
        byte[] okBytes = null;
        byte[] errBytes = null;
        byte[] panicBytes = null;
        while (r.hasRemaining()) {
            int tag = r.readTag();
            int field = WireReader.fieldOf(tag);
            int wire = WireReader.wireOf(tag);
            switch (field) {
                case RESULT_OK -> {
                    okBytes = r.readBytes();
                    arm = RESULT_OK;
                }
                case RESULT_ERROR -> {
                    errBytes = r.readBytes();
                    arm = RESULT_ERROR;
                }
                case RESULT_PANIC -> {
                    panicBytes = r.readBytes();
                    arm = RESULT_PANIC;
                }
                default -> r.skipField(wire);
            }
        }
        return switch (arm) {
            case RESULT_ERROR -> throw decodeError(errBytes);
            case RESULT_PANIC -> throw decodePanic(panicBytes);
            case RESULT_OK -> decodeWithDesc(okBytes, parseDesc(returnDesc), false);
            // Absent oneof: an all-default envelope is a null `ok`.
            default -> null;
        };
    }

    private static BamlError decodeError(byte[] errBytes) {
        WireReader r = new WireReader(errBytes);
        byte[] valueBytes = null;
        List<String> trace = new ArrayList<>();
        while (r.hasRemaining()) {
            int tag = r.readTag();
            int field = WireReader.fieldOf(tag);
            int wire = WireReader.wireOf(tag);
            switch (field) {
                case ERR_VALUE -> valueBytes = r.readBytes();
                case ERR_TRACE -> trace.add(r.readString());
                default -> r.skipField(wire);
            }
        }
        String className = valueBytes == null ? null : outboundClassFqn(valueBytes);
        Object value = valueBytes == null ? null : decodeValue(new WireReader(valueBytes), true);
        return new BamlError(value, trace, className);
    }

    private static RuntimeException decodePanic(byte[] panicBytes) {
        WireReader r = new WireReader(panicBytes);
        byte[] valueBytes = null;
        List<String> trace = new ArrayList<>();
        boolean isExit = false;
        long exitCode = 0;
        while (r.hasRemaining()) {
            int tag = r.readTag();
            int field = WireReader.fieldOf(tag);
            int wire = WireReader.wireOf(tag);
            switch (field) {
                case PANIC_VALUE -> valueBytes = r.readBytes();
                case PANIC_TRACE -> trace.add(r.readString());
                case PANIC_IS_EXIT -> isExit = r.readVarint() != 0;
                case PANIC_EXIT_CODE -> exitCode = r.readVarint();
                default -> r.skipField(wire);
            }
        }
        if (isExit) {
            // Clean baml.sys.exit: hard-terminate the process, bypassing JVM
            // shutdown hooks (the analog of Python's os._exit).
            Runtime.getRuntime().halt((int) exitCode);
            // Unreachable: halt() never returns. Satisfy the compiler.
            return new IllegalStateException("halt returned");
        }
        String className = valueBytes == null ? null : outboundClassFqn(valueBytes);
        Object value = valueBytes == null ? null : decodeValue(new WireReader(valueBytes), true);
        return new BamlPanic(value, trace, className);
    }

    /**
     * Decode a {@code BamlOutboundValue}. A {@code class_value} / {@code enum_value}
     * is reified through the {@link TypeRegistry}: a registered FQN yields the
     * generated class instance / enum constant, and an unregistered FQN degrades
     * to a field {@code Map<String,Object>} (like Python's no-typemap fallback) /
     * the raw variant string — so a thrown {@code baml.errors.*} value still
     * surfaces on the error/panic path. When {@code lenient}, the remaining
     * undecodable capabilities (opaque handles/media/prompt-ast/ty) degrade to
     * null; on the {@code ok} path ({@code lenient == false}) they throw
     * {@link UnsupportedOperationException}.
     */
    public static Object decodeValue(WireReader r, boolean lenient) {
        Object result = null;
        while (r.hasRemaining()) {
            int tag = r.readTag();
            int field = WireReader.fieldOf(tag);
            int wire = WireReader.wireOf(tag);
            switch (field) {
                case OV_NULL -> {
                    r.skipField(wire);
                    result = null;
                }
                case OV_STRING -> result = r.readString();
                case OV_INT -> result = r.readVarint();
                case OV_FLOAT -> result = r.readDouble();
                case OV_BOOL -> result = r.readVarint() != 0;
                case OV_LITERAL -> result = decodeLiteral(r.readMessage());
                case OV_LIST -> result = decodeList(r.readMessage(), lenient);
                case OV_MAP -> result = decodeMap(r.readMessage(), lenient);
                case OV_UNION -> result = decodeUnionVariant(r.readMessage(), lenient);
                case OV_UINT8ARRAY -> result = r.readBytes();
                case OV_BIGINT -> result = new BigInteger(r.readString(), 16);
                case OV_CLASS -> result = decodeClass(r.readMessage(), lenient);
                case OV_ENUM -> result = decodeEnum(r.readMessage());
                case OV_HANDLE, OV_MEDIA, OV_PROMPT_AST, OV_TY -> {
                    if (lenient) {
                        r.skipField(wire);
                        result = null;
                    } else {
                        r.skipField(wire);
                        throw unsupported(kindName(field));
                    }
                }
                default -> r.skipField(wire);
            }
        }
        return result;
    }

    private static Object decodeLiteral(WireReader r) {
        Object result = null;
        while (r.hasRemaining()) {
            int tag = r.readTag();
            int field = WireReader.fieldOf(tag);
            int wire = WireReader.wireOf(tag);
            switch (field) {
                case LIT_STRING -> result = r.readString();
                case LIT_INT -> result = r.readVarint();
                case LIT_BOOL -> result = r.readVarint() != 0;
                case LIT_BIGINT -> result = new BigInteger(r.readString(), 16);
                case LIT_FLOAT -> result = Double.parseDouble(r.readString());
                default -> r.skipField(wire);
            }
        }
        return result;
    }

    private static List<Object> decodeList(WireReader r, boolean lenient) {
        List<Object> items = new ArrayList<>();
        while (r.hasRemaining()) {
            int tag = r.readTag();
            int field = WireReader.fieldOf(tag);
            int wire = WireReader.wireOf(tag);
            if (field == LIST_ITEMS) {
                items.add(decodeValue(r.readMessage(), lenient));
            } else {
                r.skipField(wire); // item_type and any unknown fields
            }
        }
        return items;
    }

    private static Map<String, Object> decodeMap(WireReader r, boolean lenient) {
        Map<String, Object> map = new LinkedHashMap<>();
        while (r.hasRemaining()) {
            int tag = r.readTag();
            int field = WireReader.fieldOf(tag);
            int wire = WireReader.wireOf(tag);
            if (field == MAP_ENTRIES) {
                decodeMapEntry(r.readMessage(), map, lenient);
            } else {
                r.skipField(wire); // key_type / value_type / unknown
            }
        }
        return map;
    }

    private static void decodeMapEntry(WireReader r, Map<String, Object> into, boolean lenient) {
        String key = null;
        Object value = null;
        while (r.hasRemaining()) {
            int tag = r.readTag();
            int field = WireReader.fieldOf(tag);
            int wire = WireReader.wireOf(tag);
            switch (field) {
                case MAP_ENTRY_KEY -> key = r.readString();
                case MAP_ENTRY_VALUE -> value = decodeValue(r.readMessage(), lenient);
                default -> r.skipField(wire);
            }
        }
        into.put(key, value);
    }

    /**
     * Decode a {@code BamlValueUnionVariant}. The engine only wraps a value in a
     * union variant for a union of &ge;2 non-null arms (a {@code T|null} optional
     * arrives as the bare value/null), and its {@code value_option_name} is
     * unreliable, so the arm is resolved structurally: the {@code self_type}
     * ({@code BamlTy}) is tokenized (dropping {@code null}) into the sorted
     * signature the {@link TypeRegistry} keys on, and the arm within it is picked
     * from the inner value's own shape. A registered signature whose arm matches
     * yields the generated wrapper record; an unregistered signature or an
     * unmatched arm falls back to the bare decoded inner value — literal-over-one-
     * base unions are erased to that base in codegen and never registered.
     */
    private static Object decodeUnionVariant(WireReader r, boolean lenient) {
        byte[] selfTypeBytes = null;
        byte[] innerValueBytes = null;
        while (r.hasRemaining()) {
            int tag = r.readTag();
            int field = WireReader.fieldOf(tag);
            int wire = WireReader.wireOf(tag);
            switch (field) {
                case UNION_SELF_TYPE -> selfTypeBytes = r.readBytes();
                case UNION_VALUE -> innerValueBytes = r.readBytes();
                default -> r.skipField(wire);
            }
        }
        // Inner null (absent value oneof, or a null_value arm) → null.
        if (innerValueBytes == null) {
            return null;
        }
        Object inner = decodeValue(new WireReader(innerValueBytes), lenient);
        if (inner == null) {
            return null;
        }
        if (selfTypeBytes != null) {
            String signature = unionSignature(selfTypeBytes);
            String discriminator = innerDiscriminator(new WireReader(innerValueBytes));
            Object record = TypeRegistry.constructUnion(signature, discriminator, inner);
            if (record != null) {
                return record;
            }
        }
        // Fallback (load-bearing): the bare decoded inner value.
        return inner;
    }

    /** The sorted {@code |}-joined arm-token signature of a union's {@code self_type}. */
    private static String unionSignature(byte[] selfTypeBytes) {
        List<String> arms = tyArms(new WireReader(selfTypeBytes));
        arms.removeIf("null"::equals);
        Collections.sort(arms);
        // Duplicate-insensitive: the engine's runtime union type keeps
        // duplicate arms (e.g. `int | int | string`) that the emitter-side
        // registration (TIR, dedup'd) never sees.
        java.util.List<String> distinct = arms.stream().distinct().toList();
        return String.join("|", distinct);
    }

    /**
     * The arm tokens of a {@code self_type} {@code BamlTy}: a union expands to one
     * token per option; an optional to {@code token(inner)} plus an implicit
     * {@code "null"} (dropped later); anything else is a single-arm token.
     */
    private static List<String> tyArms(WireReader ty) {
        List<String> arms = new ArrayList<>();
        while (ty.hasRemaining()) {
            int tag = ty.readTag();
            int field = WireReader.fieldOf(tag);
            int wire = WireReader.wireOf(tag);
            if (field == TY_UNION) {
                WireReader u = ty.readMessage();
                while (u.hasRemaining()) {
                    int t = u.readTag();
                    if (WireReader.fieldOf(t) == TY_UNION_OPTIONS) {
                        arms.add(tyToken(u.readMessage()));
                    } else {
                        u.skipField(WireReader.wireOf(t));
                    }
                }
            } else if (field == TY_OPTIONAL) {
                arms.add(tyToken(subTy(ty.readMessage(), TY_OPTIONAL_INNER)));
                arms.add("null");
            } else {
                String t = tyTokenForArm(field, wire, ty);
                if (t != null) {
                    arms.add(t);
                }
            }
        }
        return arms;
    }

    /** The single arm token for a {@code BamlTy} message (reads its oneof arm). */
    private static String tyToken(WireReader ty) {
        if (ty == null) {
            return "?";
        }
        String token = "?";
        while (ty.hasRemaining()) {
            int tag = ty.readTag();
            int field = WireReader.fieldOf(tag);
            int wire = WireReader.wireOf(tag);
            String t = tyTokenForArm(field, wire, ty);
            if (t != null) {
                token = t;
            }
        }
        return token;
    }

    /**
     * The token for a single {@code BamlTy} oneof arm whose tag was already read.
     * Consumes the arm's value; returns null for an unrecognized field (skipped),
     * leaving the caller's running token untouched.
     */
    private static String tyTokenForArm(int field, int wire, WireReader r) {
        switch (field) {
            case TY_PRIMITIVE:
                return primitiveToken(r.readMessage());
            case TY_CLASS, TY_ENUM, TY_TYPE_ALIAS:
                // Class/enum arms token as their canonical FQN; a type_alias
                // node does too (recursive aliases keep a nominal identity —
                // the emitter registers them under the same FQN token).
                return nameField(r.readMessage());
            case TY_LIST:
                return "list<" + tyToken(subTy(r.readMessage(), TY_LIST_ITEM)) + ">";
            case TY_MAP:
                return mapToken(r.readMessage());
            case TY_OPTIONAL:
                // A nested optional (inside a list/map arm) contributes its inner
                // token; a mismatch here only forces the safe bare-inner fallback.
                return tyToken(subTy(r.readMessage(), TY_OPTIONAL_INNER));
            case TY_LITERAL:
                return literalTyToken(r.readMessage());
            case TY_MEDIA:
                return mediaToken(r.readMessage());
            case TY_UNKNOWN:
                r.skipField(wire);
                return "unknown";
            case TY_FUNCTION:
                r.skipField(wire);
                return "callable";
            case TY_RUST_TYPE:
                r.skipField(wire);
                return "handle";
            case TY_VOID:
                r.skipField(wire);
                return "void";
            default:
                r.skipField(wire);
                return null;
        }
    }

    private static String mapToken(WireReader map) {
        String key = "?";
        String value = "?";
        while (map.hasRemaining()) {
            int tag = map.readTag();
            int field = WireReader.fieldOf(tag);
            int wire = WireReader.wireOf(tag);
            if (field == TY_MAP_KEY) {
                key = tyToken(map.readMessage());
            } else if (field == TY_MAP_VALUE) {
                value = tyToken(map.readMessage());
            } else {
                map.skipField(wire);
            }
        }
        return "map<" + key + "," + value + ">";
    }

    private static String primitiveToken(WireReader msg) {
        long kind = 0;
        while (msg.hasRemaining()) {
            int tag = msg.readTag();
            if (WireReader.fieldOf(tag) == TY_PRIMITIVE_KIND) {
                kind = msg.readVarint();
            } else {
                msg.skipField(WireReader.wireOf(tag));
            }
        }
        return switch ((int) kind) {
            case PRIM_STRING -> "string";
            case PRIM_INT -> "int";
            case PRIM_FLOAT -> "float";
            case PRIM_BOOL -> "bool";
            case PRIM_NULL -> "null";
            case PRIM_BYTES -> "uint8array";
            case PRIM_BIGINT -> "bigint";
            default -> "?";
        };
    }

    private static String mediaToken(WireReader msg) {
        long kind = 0;
        while (msg.hasRemaining()) {
            int tag = msg.readTag();
            if (WireReader.fieldOf(tag) == TY_MEDIA_KIND) {
                kind = msg.readVarint();
            } else {
                msg.skipField(WireReader.wireOf(tag));
            }
        }
        return switch ((int) kind) {
            case MEDIA_IMAGE -> "media:image";
            case MEDIA_AUDIO -> "media:audio";
            case MEDIA_VIDEO -> "media:video";
            case MEDIA_PDF -> "media:pdf";
            case MEDIA_GENERIC -> "media:generic";
            default -> "?";
        };
    }

    /** Read a {@code name} (field 1) string from a class/enum {@code BamlTy} sub-message. */
    private static String nameField(WireReader msg) {
        String name = null;
        while (msg.hasRemaining()) {
            int tag = msg.readTag();
            if (WireReader.fieldOf(tag) == TY_NAME) {
                name = msg.readString();
            } else {
                msg.skipField(WireReader.wireOf(tag));
            }
        }
        return name == null ? "?" : name;
    }

    /** Tokenize a {@code BamlTyLiteral} (its bigint/float are decimal/source strings). */
    private static String literalTyToken(WireReader msg) {
        String token = "?";
        while (msg.hasRemaining()) {
            int tag = msg.readTag();
            int field = WireReader.fieldOf(tag);
            int wire = WireReader.wireOf(tag);
            switch (field) {
                case LIT_STRING -> token = "lit:string:" + msg.readString();
                case LIT_INT -> token = "lit:int:" + msg.readVarint();
                case LIT_BOOL -> token = "lit:bool:" + (msg.readVarint() != 0);
                case LIT_BIGINT -> token = "lit:bigint:" + msg.readString(); // decimal
                case LIT_FLOAT -> token = "lit:float:" + msg.readString(); // source text
                default -> msg.skipField(wire);
            }
        }
        return token;
    }

    /**
     * A discriminator token for the inner {@code BamlOutboundValue}: a primitive
     * kind, a class/enum FQN, {@code "list"} / {@code "map"} sentinels (prefix-
     * matched against {@code list<...>} / {@code map<...>} arms), or a
     * {@code "lit:<base>:<value>"} token. The inner {@code bigint} literal channel
     * is hex on the wire but normalized to decimal here so it matches the arm
     * token's decimal spelling.
     */
    private static String innerDiscriminator(WireReader r) {
        String token = "?";
        while (r.hasRemaining()) {
            int tag = r.readTag();
            int field = WireReader.fieldOf(tag);
            int wire = WireReader.wireOf(tag);
            switch (field) {
                case OV_STRING -> {
                    r.skipField(wire);
                    token = "string";
                }
                case OV_INT -> {
                    r.skipField(wire);
                    token = "int";
                }
                case OV_FLOAT -> {
                    r.skipField(wire);
                    token = "float";
                }
                case OV_BOOL -> {
                    r.skipField(wire);
                    token = "bool";
                }
                case OV_BIGINT -> {
                    r.skipField(wire);
                    token = "bigint";
                }
                case OV_UINT8ARRAY -> {
                    r.skipField(wire);
                    token = "uint8array";
                }
                case OV_NULL -> {
                    r.skipField(wire);
                    token = "null";
                }
                case OV_CLASS, OV_ENUM -> token = nameField(r.readMessage());
                case OV_LITERAL -> token = literalValueToken(r.readMessage());
                case OV_LIST -> {
                    r.skipField(wire);
                    token = "list";
                }
                case OV_MAP -> {
                    r.skipField(wire);
                    token = "map";
                }
                default -> r.skipField(wire);
            }
        }
        return token;
    }

    /** Tokenize a {@code BamlLiteralValue} (outbound); its bigint channel is hex. */
    private static String literalValueToken(WireReader msg) {
        String token = "?";
        while (msg.hasRemaining()) {
            int tag = msg.readTag();
            int field = WireReader.fieldOf(tag);
            int wire = WireReader.wireOf(tag);
            switch (field) {
                case LIT_STRING -> token = "lit:string:" + msg.readString();
                case LIT_INT -> token = "lit:int:" + msg.readVarint();
                case LIT_BOOL -> token = "lit:bool:" + (msg.readVarint() != 0);
                // Hex on the wire → decimal, matching the arm token's spelling.
                case LIT_BIGINT -> token = "lit:bigint:" + new BigInteger(msg.readString(), 16);
                case LIT_FLOAT -> token = "lit:float:" + msg.readString();
                default -> msg.skipField(wire);
            }
        }
        return token;
    }

    /** The first {@code wantField} sub-message of {@code msg} as a reader, or null. */
    private static WireReader subTy(WireReader msg, int wantField) {
        WireReader found = null;
        while (msg.hasRemaining()) {
            int tag = msg.readTag();
            int field = WireReader.fieldOf(tag);
            int wire = WireReader.wireOf(tag);
            if (field == wantField && found == null) {
                found = msg.readMessage();
            } else {
                msg.skipField(wire);
            }
        }
        return found;
    }

    /**
     * Decode a {@code BamlValueClass}: gather its FQN ({@code name}) and fields,
     * then reify via the {@link TypeRegistry}. A registered FQN yields the
     * generated class instance (fields marshalled positionally in the registry's
     * declaration order); an unregistered FQN degrades to the field
     * {@code Map<String,Object>}. {@code type_args} (generics) are skipped — the
     * host reifies generics later.
     */
    private static Object decodeClass(WireReader r, boolean lenient) {
        String fqn = null;
        Map<String, Object> fields = new LinkedHashMap<>();
        while (r.hasRemaining()) {
            int tag = r.readTag();
            int field = WireReader.fieldOf(tag);
            int wire = WireReader.wireOf(tag);
            switch (field) {
                case CLASS_NAME -> fqn = r.readString();
                case CLASS_FIELDS -> decodeMapEntry(r.readMessage(), fields, lenient);
                default -> r.skipField(wire); // type_args
            }
        }
        Object instance = TypeRegistry.constructClass(fqn, fields);
        return instance != null ? instance : fields;
    }

    /**
     * Decode a {@code BamlValueEnum}: gather its FQN ({@code name}) and wire
     * variant ({@code value}), then map to the generated Java enum constant via
     * the {@link TypeRegistry}. An unregistered FQN (or unknown variant) degrades
     * to the raw wire variant string.
     */
    private static Object decodeEnum(WireReader r) {
        String fqn = null;
        String variant = null;
        while (r.hasRemaining()) {
            int tag = r.readTag();
            int field = WireReader.fieldOf(tag);
            int wire = WireReader.wireOf(tag);
            switch (field) {
                case ENUM_NAME -> fqn = r.readString();
                case ENUM_VALUE -> variant = r.readString();
                default -> r.skipField(wire); // is_dynamic
            }
        }
        Object constant = TypeRegistry.resolveEnum(fqn, variant);
        return constant != null ? constant : variant;
    }

    // ========================================================================
    // Type-directed decode (descriptor-driven).
    //
    // A generated binding passes a descriptor string for its declared return
    // type (and per-field descriptors on registerClass). The descriptor drives
    // decode against the *declared* shape — most importantly it lands a union
    // result on the generic `baml_bridge.Union{k}.Arm{i}` family, picking the arm
    // from the DECLARED arm order (the wire carries no trustworthy arm order).
    // A null / `tv:` / `unknown` / unparseable descriptor falls back to the
    // wire-driven `decodeValue` path, which stays fully intact.
    // ========================================================================

    /**
     * Decode a {@code BamlOutboundValue} ({@code valueBytes}) against a parsed
     * descriptor. A null descriptor (or a {@code tv:} / {@code unknown} / bare
     * primitive / literal one) decodes wire-driven; a {@code list<>} / {@code map<>}
     * recurses element/value decode through the descriptor; an FQN reifies the
     * named class/enum/recursive-alias; a {@code union[...]} matches the wire
     * value against the declared arms in order and wraps the arm.
     */
    static Object decodeWithDesc(byte[] valueBytes, Desc desc, boolean lenient) {
        if (valueBytes == null) {
            return null;
        }
        if (desc == null) {
            return decodeValue(new WireReader(valueBytes), lenient);
        }
        switch (desc.kind) {
            case LIST:
                return decodeListWithDesc(valueBytes, desc.children.get(0), lenient);
            case MAP:
                return decodeMapWithDesc(valueBytes, desc.children.get(1), lenient);
            case FQN:
                return decodeFqnWithDesc(valueBytes, desc.text, lenient);
            case UNION:
                return decodeUnionWithDesc(valueBytes, desc, lenient);
            case PRIM:
            case LIT:
            case TV:
            case UNKNOWN:
            default:
                // A scalar/literal descriptor carries no structural recursion; the
                // self-describing wire form already yields the right value. `tv:` /
                // `unknown` are the explicit wire-driven fallbacks.
                return decodeValue(new WireReader(valueBytes), lenient);
        }
    }

    /** Decode a {@code list_value}, reifying each element through {@code elemDesc}. */
    private static Object decodeListWithDesc(byte[] valueBytes, Desc elemDesc, boolean lenient) {
        byte[] listBytes = outboundSub(valueBytes, OV_LIST);
        if (listBytes == null) {
            // Descriptor says list but the wire is not one (e.g. null) → wire-driven.
            return decodeValue(new WireReader(valueBytes), lenient);
        }
        WireReader r = new WireReader(listBytes);
        List<Object> items = new ArrayList<>();
        while (r.hasRemaining()) {
            int tag = r.readTag();
            int field = WireReader.fieldOf(tag);
            int wire = WireReader.wireOf(tag);
            if (field == LIST_ITEMS) {
                items.add(decodeWithDesc(r.readBytes(), elemDesc, lenient));
            } else {
                r.skipField(wire);
            }
        }
        return items;
    }

    /** Decode a {@code map_value}, reifying each entry value through {@code valueDesc}. */
    private static Object decodeMapWithDesc(byte[] valueBytes, Desc valueDesc, boolean lenient) {
        byte[] mapBytes = outboundSub(valueBytes, OV_MAP);
        if (mapBytes == null) {
            return decodeValue(new WireReader(valueBytes), lenient);
        }
        WireReader r = new WireReader(mapBytes);
        Map<String, Object> map = new LinkedHashMap<>();
        while (r.hasRemaining()) {
            int tag = r.readTag();
            int field = WireReader.fieldOf(tag);
            int wire = WireReader.wireOf(tag);
            if (field == MAP_ENTRIES) {
                WireReader e = r.readMessage();
                String key = null;
                byte[] vb = null;
                while (e.hasRemaining()) {
                    int t = e.readTag();
                    int ef = WireReader.fieldOf(t);
                    int ew = WireReader.wireOf(t);
                    switch (ef) {
                        case MAP_ENTRY_KEY -> key = e.readString();
                        case MAP_ENTRY_VALUE -> vb = e.readBytes();
                        default -> e.skipField(ew);
                    }
                }
                map.put(key, vb == null ? null : decodeWithDesc(vb, valueDesc, lenient));
            } else {
                r.skipField(wire);
            }
        }
        return map;
    }

    /**
     * Decode against an FQN descriptor. The registry decides the kind: a class
     * reifies with per-field descriptors; a named recursive-alias union routes to
     * the wire-driven {@link TypeRegistry#constructUnion} (its minted nominal
     * records); an enum (or an unresolved FQN) decodes wire-driven.
     */
    private static Object decodeFqnWithDesc(byte[] valueBytes, String fqn, boolean lenient) {
        if (TypeRegistry.isClass(fqn)) {
            byte[] classBytes = outboundSub(valueBytes, OV_CLASS);
            if (classBytes == null) {
                return decodeValue(new WireReader(valueBytes), lenient);
            }
            return decodeClassWithDesc(new WireReader(classBytes), lenient);
        }
        if (TypeRegistry.isUnionKey(fqn)) {
            // Named recursive alias: unwrap a union_variant wrapper if present,
            // then reify onto the registered nominal record via the wire shape.
            byte[] effective = valueBytes;
            if (outboundArm(valueBytes) == OV_UNION) {
                byte[] inner = extractUnionInner(valueBytes);
                if (inner == null) {
                    return null;
                }
                effective = inner;
            }
            String discriminator = innerDiscriminator(new WireReader(effective));
            String armToken = TypeRegistry.unionArmToken(fqn, discriminator);
            // Decode the arm's inner value type-directed via the arm token
            // (nested recursive-alias contents must reify, not decode bare).
            Object inner = armToken != null
                    ? decodeWithDesc(effective, parseDesc(armToken), lenient)
                    : decodeValue(new WireReader(effective), lenient);
            if (inner == null) {
                return null;
            }
            Object record = TypeRegistry.constructUnion(fqn, discriminator, inner);
            return record != null ? record : inner;
        }
        // Enum or unresolved FQN: the wire form is self-describing.
        return decodeValue(new WireReader(valueBytes), lenient);
    }

    /**
     * Decode a {@code BamlValueClass} reifying each field through its registered
     * descriptor (when present). Mirrors {@link #decodeClass} but reads each field
     * value's raw bytes so it can drive them with a descriptor; an unregistered
     * FQN or a field without a descriptor decodes wire-driven.
     */
    private static Object decodeClassWithDesc(WireReader r, boolean lenient) {
        String fqn = null;
        List<String> keys = new ArrayList<>();
        List<byte[]> vals = new ArrayList<>();
        while (r.hasRemaining()) {
            int tag = r.readTag();
            int field = WireReader.fieldOf(tag);
            int wire = WireReader.wireOf(tag);
            switch (field) {
                case CLASS_NAME -> fqn = r.readString();
                case CLASS_FIELDS -> {
                    WireReader e = r.readMessage();
                    String key = null;
                    byte[] vb = null;
                    while (e.hasRemaining()) {
                        int t = e.readTag();
                        int ef = WireReader.fieldOf(t);
                        int ew = WireReader.wireOf(t);
                        switch (ef) {
                            case MAP_ENTRY_KEY -> key = e.readString();
                            case MAP_ENTRY_VALUE -> vb = e.readBytes();
                            default -> e.skipField(ew);
                        }
                    }
                    keys.add(key);
                    vals.add(vb);
                }
                default -> r.skipField(wire); // type_args
            }
        }
        String[] order = TypeRegistry.classFieldOrder(fqn);
        String[] descs = TypeRegistry.classFieldDescs(fqn);
        Map<String, Object> fields = new LinkedHashMap<>();
        for (int i = 0; i < keys.size(); i++) {
            Desc fieldDesc = null;
            if (order != null && descs != null) {
                int idx = indexOf(order, keys.get(i));
                if (idx >= 0 && idx < descs.length) {
                    fieldDesc = parseDesc(descs[idx]);
                }
            }
            byte[] vb = vals.get(i);
            fields.put(keys.get(i), vb == null ? null : decodeWithDesc(vb, fieldDesc, lenient));
        }
        Object instance = TypeRegistry.constructClass(fqn, fields);
        return instance != null ? instance : fields;
    }

    /**
     * Decode against a {@code union[...]} descriptor onto the generic
     * {@code Union{k}} arm family. A {@code union_variant_value} wrapper is
     * unwrapped first (the descriptor is preferred over the wire {@code self_type}),
     * then the (unwrapped) wire value is matched against the declared arms IN
     * ORDER by its wire kind; the first matching arm's inner value is decoded with
     * that arm's descriptor and wrapped in {@code baml_bridge.Union{k}.Arm{i}}. A
     * null wire value decodes to null; no arm match throws {@link BamlError}.
     */
    private static Object decodeUnionWithDesc(byte[] valueBytes, Desc unionDesc, boolean lenient) {
        byte[] effective = valueBytes;
        if (outboundArm(valueBytes) == OV_UNION) {
            byte[] inner = extractUnionInner(valueBytes);
            if (inner == null) {
                return null; // null inner ≡ null
            }
            effective = inner;
        }
        int effArm = outboundArm(effective);
        if (effArm == 0 || effArm == OV_NULL) {
            return null;
        }
        String token = innerDiscriminator(new WireReader(effective));
        if (token.equals("null")) {
            return null;
        }
        int k = unionDesc.children.size();
        for (int i = 0; i < k; i++) {
            if (armMatchesToken(unionDesc.children.get(i), token)) {
                Object inner = decodeWithDesc(effective, unionDesc.children.get(i), lenient);
                return wrapArm(k, i, inner);
            }
        }
        throw new BamlError(
                "type-directed union decode: no declared arm matched wire value (kind '"
                        + token + "') for descriptor " + unionDesc,
                List.of(),
                null);
    }

    /**
     * Whether a union arm descriptor matches a wire discriminator token (from
     * {@link #innerDiscriminator}). Primitives/FQNs match their token exactly;
     * {@code list<>} / {@code map<>} match the {@code "list"} / {@code "map"}
     * sentinels; a literal arm matches an exact {@code lit:base:value} token, else
     * its base (a same-base literal union is erased in codegen, so at most one
     * literal arm per base survives — base match cannot be ambiguous); a nested
     * union matches if any of its arms match; {@code tv:} / {@code unknown} are
     * wildcards (wire-driven).
     */
    private static boolean armMatchesToken(Desc arm, String token) {
        switch (arm.kind) {
            case PRIM:
            case FQN:
                return arm.text.equals(token);
            case LIST:
                return token.equals("list");
            case MAP:
                return token.equals("map");
            case LIT: {
                if (token.startsWith("lit:")) {
                    String exact = "lit:" + arm.text + ":" + arm.litValue;
                    return token.equals(exact) || token.startsWith("lit:" + arm.text + ":");
                }
                // A bare scalar of the literal's base type also matches the base.
                return token.equals(arm.text);
            }
            case UNION:
                for (Desc child : arm.children) {
                    if (armMatchesToken(child, token)) {
                        return true;
                    }
                }
                return false;
            case TV:
            case UNKNOWN:
                return true;
            default:
                return false;
        }
    }

    /** Reflectively build {@code baml_bridge.Union{k}.Arm{idx}} around {@code inner}. */
    private static Object wrapArm(int k, int idx, Object inner) {
        String className = "baml_bridge.Union" + k + "$Arm" + idx;
        Class<?> cls;
        try {
            cls = Class.forName(className);
        } catch (ClassNotFoundException e) {
            throw new IllegalStateException(
                    "generic union arm class not found on the classpath: " + className, e);
        }
        Constructor<?>[] all = cls.getConstructors(); // single canonical record ctor
        if (all.length == 0) {
            throw new IllegalStateException("no public constructor on union arm " + className);
        }
        try {
            return all[0].newInstance(inner);
        } catch (ReflectiveOperationException e) {
            throw new IllegalStateException("failed to construct union arm " + className, e);
        }
    }

    /** The set {@code BamlOutboundValue} oneof arm field number (last-wins), or 0. */
    private static int outboundArm(byte[] valueBytes) {
        WireReader r = new WireReader(valueBytes);
        int arm = 0;
        while (r.hasRemaining()) {
            int tag = r.readTag();
            int field = WireReader.fieldOf(tag);
            int wire = WireReader.wireOf(tag);
            switch (field) {
                case OV_NULL, OV_STRING, OV_INT, OV_FLOAT, OV_BOOL, OV_CLASS, OV_ENUM,
                        OV_LITERAL, OV_LIST, OV_MAP, OV_UNION, OV_UINT8ARRAY, OV_BIGINT -> {
                    arm = field;
                    r.skipField(wire);
                }
                default -> r.skipField(wire);
            }
        }
        return arm;
    }

    /** The last length-delimited sub-message payload for {@code wantField}, or null. */
    private static byte[] outboundSub(byte[] valueBytes, int wantField) {
        WireReader r = new WireReader(valueBytes);
        byte[] found = null;
        while (r.hasRemaining()) {
            int tag = r.readTag();
            int field = WireReader.fieldOf(tag);
            int wire = WireReader.wireOf(tag);
            if (field == wantField && wire == WireWriter.WIRE_LEN) {
                found = r.readBytes();
            } else {
                r.skipField(wire);
            }
        }
        return found;
    }

    /** The inner {@code value} bytes of a {@code union_variant_value} outbound value. */
    private static byte[] extractUnionInner(byte[] valueBytes) {
        byte[] uvBytes = outboundSub(valueBytes, OV_UNION);
        return uvBytes == null ? null : unionInnerValue(new WireReader(uvBytes));
    }

    private static int indexOf(String[] arr, String want) {
        for (int i = 0; i < arr.length; i++) {
            if (arr[i].equals(want)) {
                return i;
            }
        }
        return -1;
    }

    // -- descriptor grammar (one tokenizer, shared with the emitter) ----------

    /**
     * A parsed type-directed decode descriptor. Grammar (per
     * {@code ref-java-codegen-conventions.md}): primitives
     * {@code int|bigint|float|string|bool|null|uint8array|void|unknown}; an FQN
     * (class/enum/named recursive alias); {@code list<D>}; {@code map<D,D>};
     * ordered {@code union[D;D;...]}; {@code lit:<base>:<value>}; and {@code tv:<name>}.
     */
    static final class Desc {
        enum Kind { PRIM, FQN, LIST, MAP, UNION, LIT, TV, UNKNOWN }

        final Kind kind;
        final String text; // PRIM: name; FQN: fqn; TV: var name; LIT: base
        final String litValue; // LIT only
        final List<Desc> children; // LIST: [elem]; MAP: [key, value]; UNION: arms

        private Desc(Kind kind, String text, String litValue, List<Desc> children) {
            this.kind = kind;
            this.text = text;
            this.litValue = litValue;
            this.children = children;
        }

        static Desc prim(String name) {
            return new Desc(Kind.PRIM, name, null, null);
        }

        static Desc fqn(String name) {
            return new Desc(Kind.FQN, name, null, null);
        }

        static Desc tv(String name) {
            return new Desc(Kind.TV, name, null, null);
        }

        static Desc unknown() {
            return new Desc(Kind.UNKNOWN, "unknown", null, null);
        }

        static Desc lit(String base, String value) {
            return new Desc(Kind.LIT, base, value, null);
        }

        static Desc list(Desc elem) {
            return new Desc(Kind.LIST, null, null, List.of(elem));
        }

        static Desc map(Desc key, Desc value) {
            return new Desc(Kind.MAP, null, null, List.of(key, value));
        }

        static Desc union(List<Desc> arms) {
            return new Desc(Kind.UNION, null, null, arms);
        }

        @Override
        public String toString() {
            return switch (kind) {
                case PRIM, TV, UNKNOWN -> text;
                case FQN -> text;
                case LIT -> "lit:" + text + ":" + litValue;
                case LIST -> "list<" + children.get(0) + ">";
                case MAP -> "map<" + children.get(0) + "," + children.get(1) + ">";
                case UNION -> {
                    StringBuilder sb = new StringBuilder("union[");
                    for (int i = 0; i < children.size(); i++) {
                        if (i > 0) {
                            sb.append(';');
                        }
                        sb.append(children.get(i));
                    }
                    yield sb.append(']').toString();
                }
            };
        }
    }

    /**
     * Parse a descriptor string, or return null for a null / empty / unparseable
     * descriptor (the caller then decodes wire-driven).
     */
    static Desc parseDesc(String s) {
        if (s == null || s.isEmpty()) {
            return null;
        }
        int[] p = {0};
        Desc d;
        try {
            d = parseNode(s, p);
        } catch (RuntimeException e) {
            return null; // unparseable → wire-driven fallback
        }
        skipWs(s, p);
        return p[0] == s.length() ? d : null; // trailing garbage → fallback
    }

    private static Desc parseNode(String s, int[] p) {
        skipWs(s, p);
        if (consume(s, p, "list<")) {
            Desc elem = parseNode(s, p);
            expect(s, p, '>');
            return Desc.list(elem);
        }
        if (consume(s, p, "map<")) {
            Desc key = parseNode(s, p);
            expect(s, p, ',');
            Desc value = parseNode(s, p);
            expect(s, p, '>');
            return Desc.map(key, value);
        }
        if (consume(s, p, "union[")) {
            List<Desc> arms = new ArrayList<>();
            arms.add(parseNode(s, p));
            skipWs(s, p);
            while (p[0] < s.length() && s.charAt(p[0]) == ';') {
                p[0]++;
                arms.add(parseNode(s, p));
                skipWs(s, p);
            }
            expect(s, p, ']');
            return Desc.union(arms);
        }
        if (consume(s, p, "lit:")) {
            int bs = p[0];
            while (p[0] < s.length() && s.charAt(p[0]) != ':') {
                p[0]++;
            }
            String base = s.substring(bs, p[0]);
            expect(s, p, ':');
            // The literal value is a leaf: read to the next structural terminator
            // (so `lit:string:hello world` keeps its space; a value containing
            // `;]>,` is an accepted parser limitation, noted in the conventions).
            int vs = p[0];
            while (p[0] < s.length()) {
                char c = s.charAt(p[0]);
                if (c == ';' || c == ']' || c == '>' || c == ',') {
                    break;
                }
                p[0]++;
            }
            return Desc.lit(base, s.substring(vs, p[0]));
        }
        String tok = readLeaf(s, p);
        if (tok.isEmpty()) {
            throw new IllegalArgumentException("empty descriptor token at " + p[0]);
        }
        return classifyLeaf(tok);
    }

    private static Desc classifyLeaf(String tok) {
        switch (tok) {
            case "int":
            case "bigint":
            case "float":
            case "string":
            case "bool":
            case "null":
            case "uint8array":
            case "void":
                return Desc.prim(tok);
            case "unknown":
                return Desc.unknown();
            default:
                if (tok.startsWith("tv:")) {
                    return Desc.tv(tok.substring(3));
                }
                return Desc.fqn(tok);
        }
    }

    /** Read a leaf bareword (primitive / FQN / tv) up to a delimiter or whitespace. */
    private static String readLeaf(String s, int[] p) {
        int start = p[0];
        while (p[0] < s.length()) {
            char c = s.charAt(p[0]);
            if (c == '<' || c == '>' || c == '[' || c == ']' || c == ';' || c == ','
                    || Character.isWhitespace(c)) {
                break;
            }
            p[0]++;
        }
        return s.substring(start, p[0]);
    }

    /** Consume {@code lit} if it is the exact prefix at {@code p}; else leave {@code p}. */
    private static boolean consume(String s, int[] p, String lit) {
        if (s.regionMatches(p[0], lit, 0, lit.length())) {
            p[0] += lit.length();
            return true;
        }
        return false;
    }

    private static void expect(String s, int[] p, char c) {
        skipWs(s, p);
        if (p[0] >= s.length() || s.charAt(p[0]) != c) {
            throw new IllegalArgumentException("expected '" + c + "' at " + p[0] + " in " + s);
        }
        p[0]++;
    }

    private static void skipWs(String s, int[] p) {
        while (p[0] < s.length() && Character.isWhitespace(s.charAt(p[0]))) {
            p[0]++;
        }
    }

    /**
     * The BAML FQN of a {@code BamlOutboundValue} that is a class instance (e.g.
     * {@code baml.errors.GenericSdkError}), unwrapping any union wrapper — used
     * only to build a readable {@code class_name()} on {@code BamlError} /
     * {@code BamlPanic}. Returns null when the value is not a class.
     */
    private static String outboundClassFqn(byte[] valueBytes) {
        WireReader r = new WireReader(valueBytes);
        while (r.hasRemaining()) {
            int tag = r.readTag();
            int field = WireReader.fieldOf(tag);
            int wire = WireReader.wireOf(tag);
            if (field == OV_CLASS) {
                return classNameOf(r.readMessage());
            } else if (field == OV_UNION) {
                byte[] inner = unionInnerValue(r.readMessage());
                return inner == null ? null : outboundClassFqn(inner);
            } else {
                r.skipField(wire);
            }
        }
        return null;
    }

    private static String classNameOf(WireReader r) {
        String name = null;
        while (r.hasRemaining()) {
            int tag = r.readTag();
            int field = WireReader.fieldOf(tag);
            int wire = WireReader.wireOf(tag);
            if (field == CLASS_NAME) {
                name = r.readString();
            } else {
                r.skipField(wire);
            }
        }
        return name;
    }

    private static byte[] unionInnerValue(WireReader r) {
        byte[] inner = null;
        while (r.hasRemaining()) {
            int tag = r.readTag();
            int field = WireReader.fieldOf(tag);
            int wire = WireReader.wireOf(tag);
            if (field == UNION_VALUE) {
                inner = r.readBytes();
            } else {
                r.skipField(wire);
            }
        }
        return inner;
    }

    private static UnsupportedOperationException unsupported(String kind) {
        return new UnsupportedOperationException("capability not yet implemented: " + kind);
    }

    private static String kindName(int field) {
        return switch (field) {
            case OV_HANDLE -> "handle_value";
            case OV_MEDIA -> "media_value";
            case OV_PROMPT_AST -> "prompt_ast_value";
            case OV_TY -> "ty_value";
            default -> "field " + field;
        };
    }
}
