package baml_bridge.internal;

import baml_bridge.BamlError;
import baml_bridge.BamlHandle;
import baml_bridge.BamlOpaque;
import baml_bridge.BamlPanic;
import baml_bridge.BamlType;
import baml_bridge.TypeRegistry;
import baml_sdk.baml.media.Audio;
import baml_sdk.baml.media.Image;
import baml_sdk.baml.media.Pdf;
import baml_sdk.baml.media.Video;

import java.lang.reflect.Constructor;
import java.math.BigInteger;
import java.util.ArrayList;
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
    private static final int LIST_ITEM_TYPE = 1; // BamlValueList.item_type (BamlTy)
    private static final int LIST_ITEMS = 2;
    private static final int MAP_KEY_TYPE = 1; // BamlValueMap.key_type (BamlTy)
    private static final int MAP_VALUE_TYPE = 2; // BamlValueMap.value_type (BamlTy)
    private static final int MAP_ENTRIES = 3;
    private static final int MAP_ENTRY_KEY = 1;
    private static final int MAP_ENTRY_VALUE = 2;

    // BamlValueClass / BamlValueEnum
    private static final int CLASS_NAME = 1;
    private static final int CLASS_FIELDS = 2;
    private static final int CLASS_TYPE_ARGS = 3; // repeated BamlTy (concrete generic args)
    private static final int ENUM_NAME = 1;
    private static final int ENUM_VALUE = 2;

    // BamlValueMedia
    private static final int MEDIA_KIND = 1;
    private static final int MEDIA_MIME_TYPE = 2;
    private static final int MEDIA_URL = 3;
    private static final int MEDIA_BASE64 = 4;
    // Field 5 once named a file and is reserved.
    private static final int MEDIA_FILE_CONTENT = 6;
    // MediaTypeEnum (baml_outbound.proto), the kind a media value carries. Its
    // numbering differs from BamlTyMediaKind, the kind a media type carries.
    private static final int MEDIA_VALUE_IMAGE = 1;
    private static final int MEDIA_VALUE_AUDIO = 2;
    private static final int MEDIA_VALUE_PDF = 3;
    private static final int MEDIA_VALUE_VIDEO = 4;

    // BamlValueMediaFileContent
    private static final int FILE_CONTENT_NAME = 1;
    private static final int FILE_CONTENT_BASE64 = 2;

    // BamlValueUnionVariant
    private static final int UNION_SELF_TYPE = 4;
    private static final int UNION_VALUE = 6;
    private static final int UNION_SELECTED_OPTION_INDEX = 8;

    // BamlOutboundHandle (key = 1, handle_type = 2, ty = 3).
    private static final int HANDLE_KEY = 1;
    private static final int HANDLE_TYPE = 2;
    private static final int HANDLE_TY = 3;

    // BamlToHostCall (engine→host callable dispatch): args = 1.
    // BamlToHostArg: value = 1 (BamlOutboundValue), arg_name = 2, is_optional_arg = 3.
    private static final int TO_HOST_ARGS = 1;
    private static final int TO_HOST_ARG_VALUE = 1;
    private static final int TO_HOST_ARG_NAME = 2;
    private static final int TO_HOST_ARG_IS_OPTIONAL = 3;

    /** The synthetic BAML class a host-thrown native exception surfaces as. */
    private static final String HOST_CALLABLE_CLASS = "baml.errors.HostCallable";

    /** The stdlib class whose values cross as a {@code prompt_ast_value}. */
    private static final String PROMPT_CLASS = "ai.Prompt";

    /** The stdlib class whose values cross as a {@code ty_value}. */
    private static final String TYPE_CLASS = "reflect.Type";

    // BamlTyFunction.params (repeated BamlTyFunctionParam); BamlTyFunctionParam.name.
    private static final int TY_FUNCTION_PARAMS = 3;
    private static final int TY_FUNCTION_PARAM_NAME = 1;

    // BamlHandleType (baml_handle.proto): the ADT_MEDIA_* discriminants the media
    // decode dispatches on. Every other handle type decodes to a bare BamlHandle.
    private static final int ADT_MEDIA_IMAGE = 6;
    private static final int ADT_MEDIA_AUDIO = 7;
    private static final int ADT_MEDIA_VIDEO = 8;
    private static final int ADT_MEDIA_PDF = 9;
    private static final int ADT_MEDIA_GENERIC = 10;
    // Adt(TaggedHeapHandle{ty, heap_handle}) — a streaming call's result. Reifies
    // the runtime-owned BamlStream wrapper (the sole tagged-heap-handle capability
    // today; the typed generics are erased, exactly as in bridge_python).
    private static final int ADT_TAGGED_HEAP_HANDLE = 14;
    private static final int ADT_FUNCTION_SPEC = 17;

    // The single field name a handle-backed media class carries on the wire.
    private static final String MEDIA_DATA_FIELD = "_data";

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

    // Sub-message field numbers on the BamlTy variants.
    private static final int TY_NAME = 1; // BamlTyClass/BamlTyEnum.name
    private static final int TY_CLASS_TYPE_ARGS = 2; // BamlTyClass.type_args (repeated)
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
     * {@link #decodeOutboundResult(byte[], BamlType)} with a {@code null} descriptor.
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
     * <p>{@code returnDesc} is the type-directed decode descriptor ({@link BamlType})
     * for the declared return type (see {@code ref-java-codegen-conventions.md});
     * when non-null it drives the {@code ok}-arm decode against the declared shape
     * (unions land on the {@code Union{k}} arm family, chosen from the declared
     * arm order). {@code error}/{@code panic} values always stay wire-driven — a
     * thrown BAML value is self-describing and carries no host return type.
     */
    public static Object decodeOutboundResult(byte[] data, BamlType returnDesc) {
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
            case RESULT_OK -> decodeWithDesc(okBytes, returnDesc, false);
            // Absent oneof: an all-default envelope is a null `ok`.
            default -> null;
        };
    }

    private static RuntimeException decodeError(byte[] errBytes) {
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
        // A host-thrown native exception surfaces as a `baml.errors.HostCallable`
        // instance carrying a hidden `_handle` into the Java-side host-value
        // registry. On the SAME runtime, rehydrate the ORIGINAL Throwable by
        // identity (assertSame holds — the object never left the JVM) and rethrow
        // it unwrapped. A foreign/released key falls through to a metadata
        // BamlError. Mirrors bridge_python's `_try_rehydrate_host_value` (proto.py).
        if (HOST_CALLABLE_CLASS.equals(className)) {
            Throwable original = rehydrateHostThrowable(value);
            if (original != null) {
                throw sneakyThrow(original);
            }
        }
        // A value/type mismatch at the call boundary (`baml.errors.TypeMismatch`,
        // synthesized host-side from `EngineError::TypeMismatch`) is a *caller*
        // type error — surface it as Java's native IllegalArgumentException (the
        // analog of Python's TypeError remap, proto.py) rather than a BamlError
        // wrapper. Covers inbound-generics inference failures (a TypeVar that
        // can't be inferred / has no consistent binding) and ordinary
        // argument-type mismatches. The message is the value's `message` field
        // so the host-side type-error assertions (TestGenericInference /
        // TestGenericCalls) can match on it; the value itself rides along, so a
        // callback forwarding the error forwards it as the BAML error it is.
        if (TYPE_MISMATCH_CLASS.equals(className)) {
            return BamlTraceback.splice(
                    new baml_bridge.BamlTypeMismatchError(typeMismatchMessage(value), value, trace, className),
                    trace);
        }
        return BamlTraceback.splice(new BamlError(value, trace, className), trace);
    }

    /** The BAML FQN whose thrown value is remapped to {@link IllegalArgumentException}. */
    private static final String TYPE_MISMATCH_CLASS = "baml.errors.TypeMismatch";

    /**
     * The reshaped arguments of a BAML→host callable dispatch: required args in
     * declared order ({@link #positional}) and supplied optionals keyed by BAML
     * parameter name ({@link #optional}). The engine already resolved the call
     * against the callee's declared params and dropped omitted optionals, so an
     * omitted optional is simply absent from {@link #optional} (the host's own
     * default then applies). Mirrors bridge_python's {@code decode_args} split.
     */
    public static final class HostCallArgs {
        public final List<Object> positional;
        public final Map<String, Object> optional;

        HostCallArgs(List<Object> positional, Map<String, Object> optional) {
            this.positional = positional;
            this.optional = optional;
        }
    }

    /**
     * Decode a {@code BamlToHostCall} (engine→host callable dispatch) into its
     * positional + optional argument buckets: {@code is_optional_arg} routes an
     * arg into the optional bucket keyed by {@code arg_name}, everything else is
     * positional. Each positional arg decodes against its declared parameter
     * descriptor ({@code positionalDescs}, declared order) and each supplied
     * optional against the descriptor keyed by its BAML wire name
     * ({@code optionalNames} / {@code optionalDescs}, parallel arrays), through
     * the same {@link #decodeWithDesc} the outbound result path uses — so a
     * callable parameter declared as e.g. {@code baml.json.json} materializes
     * as the generated sealed-union type, exactly like a declared return type.
     * An argument the declared type has no parameter for decodes wire-driven;
     * the caller refuses the call.
     */
    public static HostCallArgs decodeBamlToHostCall(
            byte[] bytes,
            BamlType[] positionalDescs,
            String[] optionalNames,
            BamlType[] optionalDescs) {
        WireReader r = new WireReader(bytes);
        List<Object> positional = new ArrayList<>();
        Map<String, Object> optional = new LinkedHashMap<>();
        while (r.hasRemaining()) {
            int tag = r.readTag();
            int field = WireReader.fieldOf(tag);
            int wire = WireReader.wireOf(tag);
            if (field == TO_HOST_ARGS) {
                decodeToHostArg(
                        r.readMessage(),
                        positional,
                        optional,
                        positionalDescs,
                        optionalNames,
                        optionalDescs);
            } else {
                r.skipField(wire);
            }
        }
        return new HostCallArgs(positional, optional);
    }

    private static void decodeToHostArg(
            WireReader r,
            List<Object> positional,
            Map<String, Object> optional,
            BamlType[] positionalDescs,
            String[] optionalNames,
            BamlType[] optionalDescs) {
        byte[] valueBytes = null;
        String argName = "";
        boolean isOptional = false;
        while (r.hasRemaining()) {
            int tag = r.readTag();
            int field = WireReader.fieldOf(tag);
            int wire = WireReader.wireOf(tag);
            switch (field) {
                case TO_HOST_ARG_VALUE -> valueBytes = r.readBytes();
                case TO_HOST_ARG_NAME -> argName = r.readString();
                case TO_HOST_ARG_IS_OPTIONAL -> isOptional = r.readVarint() != 0;
                default -> r.skipField(wire);
            }
        }
        // The engine emits required args in declared order, so the next
        // positional slot's descriptor is at the bucket's current size.
        BamlType desc = isOptional
                ? optionalDescByName(argName, optionalNames, optionalDescs)
                : positional.size() < positionalDescs.length
                        ? positionalDescs[positional.size()]
                        : null;
        Object value = valueBytes == null ? null : decodeWithDesc(valueBytes, desc, false);
        if (isOptional) {
            optional.put(argName, value);
        } else {
            positional.add(value);
        }
    }

    /** The descriptor registered for optional arg {@code argName}, or {@code null}. */
    private static BamlType optionalDescByName(
            String argName, String[] optionalNames, BamlType[] optionalDescs) {
        assert optionalNames.length == optionalDescs.length;
        for (int i = 0; i < optionalNames.length; i++) {
            if (optionalNames[i].equals(argName)) {
                return optionalDescs[i];
            }
        }
        return null;
    }

    /**
     * Rehydrate the original {@link Throwable} a {@code baml.errors.HostCallable}
     * decoded value references via its hidden {@code _handle}, or null when the
     * handle is absent, the wrong kind, or the key is foreign/released (the
     * caller then builds a metadata {@link BamlError}). The decoded HostCallable
     * is a field {@code Map} (it is not a registered class), so the {@code
     * _handle} decodes to a bare {@link BamlHandle}.
     */
    private static Throwable rehydrateHostThrowable(Object value) {
        BamlHandle handle = hostOpaqueHandle(value);
        if (handle == null || handle.handleType() != BamlHandle.HOST_VALUE_OPAQUE) {
            return null;
        }
        Object original = baml_bridge.HostCalls.lookup(handle.key());
        return original instanceof Throwable t ? t : null;
    }

    /**
     * The {@code _handle} {@link BamlHandle} of a decoded {@code baml.errors.HostCallable}.
     * The class is registered, so it usually reifies to a generated instance whose
     * {@code _handle()} accessor is read reflectively (the runtime library cannot
     * statically reference the generated class); a field {@code Map} is the
     * unregistered-FQN fallback. Null when neither carries a {@link BamlHandle}.
     */
    private static BamlHandle hostOpaqueHandle(Object value) {
        if (value instanceof Map<?, ?> map) {
            return map.get("_handle") instanceof BamlHandle handle ? handle : null;
        }
        if (value == null) {
            return null;
        }
        try {
            Object h = value.getClass().getMethod("_handle").invoke(value);
            return h instanceof BamlHandle handle ? handle : null;
        } catch (ReflectiveOperationException | RuntimeException ignored) {
            return null;
        }
    }

    /**
     * Rethrow {@code t} with its exact runtime type, unwrapped, without a checked
     * declaration (the generic-erasure "sneaky throw" idiom). The declared
     * {@link RuntimeException} return lets callers write {@code throw
     * sneakyThrow(t)}; the method never returns — it always throws {@code t}. Used
     * to re-raise a rehydrated host exception so {@code assertSame} holds for any
     * {@link Throwable} kind (checked, unchecked, or {@link Error}).
     */
    @SuppressWarnings("unchecked")
    private static <T extends Throwable> RuntimeException sneakyThrow(Throwable t) throws T {
        throw (T) t;
    }

    /**
     * The message for the {@link IllegalArgumentException} a {@code
     * baml.errors.TypeMismatch} is remapped to: the value's {@code message} field
     * (a registered {@code TypeMismatch} instance's {@code message()} accessor, or
     * a {@code "message"} map entry when the FQN is unregistered), falling back to
     * {@code value.toString()} — mirroring Python's {@code getattr(decoded,
     * "message", None)} → dict lookup → {@code str(decoded)} ladder.
     */
    private static String typeMismatchMessage(Object value) {
        Object message = extractMessageField(value);
        if (message != null) {
            return message.toString();
        }
        return value == null ? "null" : value.toString();
    }

    /**
     * The {@code message} field of a decoded thrown value, or null. A map (the
     * unregistered-FQN fallback) yields its {@code "message"} entry; any other
     * object is probed reflectively for a zero-arg {@code message()} accessor (the
     * runtime library cannot statically reference the generated {@code
     * baml.errors.TypeMismatch} class).
     */
    private static Object extractMessageField(Object value) {
        if (value instanceof Map<?, ?> map) {
            return map.get("message");
        }
        if (value == null) {
            return null;
        }
        try {
            return value.getClass().getMethod("message").invoke(value);
        } catch (ReflectiveOperationException | RuntimeException ignored) {
            return null;
        }
    }

    private static Error decodePanic(byte[] panicBytes) {
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
            // Clean baml.sys.exit: hard-terminate, bypassing JVM shutdown hooks.
            Runtime.getRuntime().halt((int) exitCode);
            // Unreachable: halt() never returns. Satisfy the compiler with an
            // Error (BamlPanic is now an Error, so this method returns Error).
            return new AssertionError("halt returned");
        }
        String className = valueBytes == null ? null : outboundClassFqn(valueBytes);
        Object value = valueBytes == null ? null : decodeValue(new WireReader(valueBytes), true);
        return BamlTraceback.splice(new BamlPanic(value, trace, className), trace);
    }

    /**
     * Decode a {@code BamlOutboundValue}. A {@code class_value} / {@code enum_value}
     * is reified through the {@link TypeRegistry}: a registered FQN yields the
     * generated class instance / enum constant, and an unregistered FQN degrades
     * to a field {@code Map<String,Object>} (like Python's no-typemap fallback) /
     * the raw variant string — so a thrown {@code baml.errors.*} value still
     * surfaces on the error/panic path. Runtime-owned capabilities decode into
     * their Java wrappers, including portable media and prompt AST values.
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
                case OV_BIGINT -> result = parseHexBigInt(r.readString());
                case OV_CLASS -> result = decodeClass(r.readMessage(), lenient);
                case OV_ENUM -> result = decodeEnum(r.readMessage());
                case OV_HANDLE -> result = decodeHandle(r.readMessage());
                case OV_PROMPT_AST -> result = baml_bridge.BamlPrompt.fromWire(r.readBytes());
                case OV_TY -> result = decodeTypeValue(r.readBytes(), lenient);
                case OV_MEDIA -> result = decodeMedia(r.readMessage());
                default -> r.skipField(wire);
            }
        }
        return result;
    }

    /**
     * A type value the engine returned, holding the engine references it names
     * ({@link BamlType#adoptWireTy}). One the Java grammar cannot hold is a
     * refusal, except inside a thrown value ({@code lenient}), where the error
     * itself must still surface and the type reads as {@code null}.
     */
    private static Object decodeTypeValue(byte[] bamlTy, boolean lenient) {
        try {
            return BamlType.adoptWireTy(bamlTy);
        } catch (IllegalStateException unrepresentable) {
            if (lenient) {
                return null;
            }
            throw new BamlError(unrepresentable.getMessage(), List.of(), null);
        }
    }

    /** Decode canonical portable media data into the matching native-backed Java wrapper. */
    private static Object decodeMedia(WireReader r) {
        int kind = 0;
        String mimeType = null;
        int source = 0;
        String value = null;
        String name = null;
        while (r.hasRemaining()) {
            int tag = r.readTag();
            int field = WireReader.fieldOf(tag);
            int wire = WireReader.wireOf(tag);
            switch (field) {
                case MEDIA_KIND -> kind = (int) r.readVarint();
                case MEDIA_MIME_TYPE -> mimeType = r.readString();
                case MEDIA_URL, MEDIA_BASE64 -> {
                    source = field;
                    value = r.readString();
                }
                case MEDIA_FILE_CONTENT -> {
                    source = field;
                    // proto3 omits an empty string: absent means empty.
                    name = "";
                    value = "";
                    WireReader content = r.readMessage();
                    while (content.hasRemaining()) {
                        int contentTag = content.readTag();
                        switch (WireReader.fieldOf(contentTag)) {
                            case FILE_CONTENT_NAME -> name = content.readString();
                            case FILE_CONTENT_BASE64 -> value = content.readString();
                            default -> content.skipField(WireReader.wireOf(contentTag));
                        }
                    }
                }
                default -> r.skipField(wire);
            }
        }
        if (source == 0) {
            throw new BamlError(
                    "BEX emitted a portable media value with no content", List.of(), null);
        }
        return switch (kind) {
            case MEDIA_VALUE_IMAGE -> constructMedia(source, value, name, mimeType,
                    Image::from_url, Image::from_base64, Image::from_file_content);
            case MEDIA_VALUE_AUDIO -> constructMedia(source, value, name, mimeType,
                    Audio::from_url, Audio::from_base64, Audio::from_file_content);
            case MEDIA_VALUE_PDF -> constructMedia(source, value, name, mimeType,
                    Pdf::from_url, Pdf::from_base64, Pdf::from_file_content);
            case MEDIA_VALUE_VIDEO -> constructMedia(source, value, name, mimeType,
                    Video::from_url, Video::from_base64, Video::from_file_content);
            default -> throw new BamlError(
                    "BEX emitted unsupported portable media kind " + kind, List.of(), null);
        };
    }

    @FunctionalInterface
    private interface MediaConstructor {
        Object construct(String value, String mimeType);
    }

    @FunctionalInterface
    private interface NamedMediaConstructor {
        Object construct(String file, String base64, String mimeType);
    }

    private static Object constructMedia(
            int source,
            String value,
            String name,
            String mimeType,
            MediaConstructor fromUrl,
            MediaConstructor fromBase64,
            NamedMediaConstructor fromFileContent) {
        return switch (source) {
            case MEDIA_URL -> fromUrl.construct(value, mimeType);
            case MEDIA_BASE64 -> fromBase64.construct(value, mimeType);
            // Content read from a file keeps the file's name.
            case MEDIA_FILE_CONTENT -> fromFileContent.construct(name, value, mimeType);
            default -> throw new AssertionError("unknown portable media source " + source);
        };
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
                case LIT_BIGINT -> result = parseHexBigInt(r.readString());
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
     * ({@code BamlTy}) is read into its arm {@link BamlType}s — a resolved union
     * keys the registry by its arm set, a recursive-alias node by its FQN — and the
     * arm within it is picked from the inner value's own shape
     * ({@link TypeRegistry#unionArmTokenForFqn} / the arm-set entry's pick). A
     * registered union whose arm matches yields the generated wrapper record; an
     * unregistered union or an unmatched arm falls back to the bare decoded inner
     * value — literal-over-one-base unions are erased to that base in codegen and
     * never registered.
     */
    private static Object decodeUnionVariant(WireReader r, boolean lenient) {
        byte[] selfTypeBytes = null;
        byte[] innerValueBytes = null;
        Integer selectedOptionIndex = null;
        while (r.hasRemaining()) {
            int tag = r.readTag();
            int field = WireReader.fieldOf(tag);
            int wire = WireReader.wireOf(tag);
            switch (field) {
                case UNION_SELF_TYPE -> selfTypeBytes = r.readBytes();
                case UNION_VALUE -> innerValueBytes = r.readBytes();
                case UNION_SELECTED_OPTION_INDEX -> selectedOptionIndex = (int) r.readVarint();
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
            List<BamlType> arms = selfTypeArms(new WireReader(selfTypeBytes));
            Object record;
            if (arms != null) {
                if (selectedOptionIndex != null) {
                    BamlType rawSelected = selfTypeOptionAt(selfTypeBytes, selectedOptionIndex);
                    if (rawSelected == null) {
                        // Error values are decoded leniently so a host that cannot
                        // represent one arm's type metadata still surfaces the
                        // original thrown value instead of masking it with a
                        // secondary bridge-decoder failure.
                        if (lenient) {
                            return inner;
                        }
                        throw new BamlError(
                                "union selected option index " + selectedOptionIndex
                                        + " does not name a representable non-null arm",
                                List.of(), null);
                    }
                    BamlType selectedType = hostSelectedType(rawSelected);
                    inner = decodeWithDesc(innerValueBytes, selectedType, lenient);
                    record = TypeRegistry.constructUnionForArmsSelected(arms, selectedType, inner);
                } else {
                    record = TypeRegistry.constructUnionForArms(arms, innerValueBytes, inner);
                }
            } else {
                String fqn = selfTypeFqn(new WireReader(selfTypeBytes));
                record = fqn == null
                        ? null
                        : selectedOptionIndex == null
                                ? TypeRegistry.constructUnionForFqn(fqn, innerValueBytes, inner)
                                : TypeRegistry.constructUnionForFqnAtIndex(
                                        fqn, selectedOptionIndex, inner);
            }
            if (record != null) {
                return record;
            }
        }
        // Fallback (load-bearing): the bare decoded inner value.
        return inner;
    }

    /**
     * The arm {@link BamlType}s of a resolved-union {@code self_type} (declaration
     * order; {@code null} / unrepresentable arms dropped, exactly as the emitter
     * strips the null arm before registration), or {@code null} when the
     * {@code self_type} is not a union — the caller then tries the alias-FQN path.
     */
    private static List<BamlType> selfTypeArms(WireReader ty) {
        while (ty.hasRemaining()) {
            int tag = ty.readTag();
            int field = WireReader.fieldOf(tag);
            int wire = WireReader.wireOf(tag);
            if (field == TY_UNION) {
                List<BamlType> arms = new ArrayList<>();
                WireReader u = ty.readMessage();
                while (u.hasRemaining()) {
                    int t = u.readTag();
                    if (WireReader.fieldOf(t) == TY_UNION_OPTIONS) {
                        BamlType arm = wireArmType(u.readMessage());
                        if (arm != null) {
                            arms.add(arm);
                        }
                    } else {
                        u.skipField(WireReader.wireOf(t));
                    }
                }
                return arms;
            }
            ty.skipField(wire);
        }
        return null;
    }

    /**
     * The FQN of a {@code self_type} that is a single named node
     * (class / enum / recursive-alias), or {@code null} otherwise.
     */
    private static String selfTypeFqn(WireReader ty) {
        while (ty.hasRemaining()) {
            int tag = ty.readTag();
            int field = WireReader.fieldOf(tag);
            int wire = WireReader.wireOf(tag);
            if (field == TY_CLASS || field == TY_ENUM || field == TY_TYPE_ALIAS) {
                return nameField(ty.readMessage());
            }
            ty.skipField(wire);
        }
        return null;
    }

    /**
     * Read one {@code BamlTy} into its {@link BamlType}, or {@code null} when it is
     * outside the token grammar (media / function / rust_type / unknown, and
     * the {@code null} / {@code bytes} / {@code bigint} primitive kinds). Named
     * types (class / enum / type_alias) become a bare {@link BamlType#classByFqn}
     * (matching the emitter's arm rendering — a wire optional unwraps to its inner,
     * type args are dropped); this is the wire counterpart of the emitter's arm
     * builder, so a wire union's arm set equals the registered one.
     */
    private static BamlType wireArmType(WireReader ty) {
        if (ty == null) {
            return null;
        }
        BamlType result = null;
        while (ty.hasRemaining()) {
            int tag = ty.readTag();
            int field = WireReader.fieldOf(tag);
            int wire = WireReader.wireOf(tag);
            switch (field) {
                case TY_PRIMITIVE -> result = wirePrimitiveType(ty.readMessage());
                case TY_CLASS, TY_ENUM, TY_TYPE_ALIAS -> result = BamlType.classByFqn(nameField(ty.readMessage()));
                case TY_LIST -> {
                    BamlType item = wireArmType(subTy(ty.readMessage(), TY_LIST_ITEM));
                    result = item == null ? null : BamlType.list(item);
                }
                case TY_MAP -> result = wireMapType(ty.readMessage());
                case TY_OPTIONAL -> result = wireArmType(subTy(ty.readMessage(), TY_OPTIONAL_INNER));
                case TY_LITERAL -> result = wireLiteralType(ty.readMessage());
                default -> ty.skipField(wire);
            }
        }
        return result;
    }

    private static BamlType wirePrimitiveType(WireReader msg) {
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
            case PRIM_STRING -> BamlType.STRING;
            case PRIM_INT -> BamlType.INT;
            case PRIM_FLOAT -> BamlType.FLOAT;
            case PRIM_BOOL -> BamlType.BOOL;
            case PRIM_BIGINT -> BamlType.BIGINT;
            // BUG: null and uint8array are types (BamlType.NULL / BYTES), but the
            // generator renders a registered union's uint8array arm as UNKNOWN
            // (registry_arm_expr), so both sides drop it to keep the arm sets
            // equal: a recursive alias with a uint8array member never reifies.
            default -> null;
        };
    }

    private static BamlType wireMapType(WireReader map) {
        BamlType key = null;
        BamlType value = null;
        while (map.hasRemaining()) {
            int tag = map.readTag();
            int field = WireReader.fieldOf(tag);
            int wire = WireReader.wireOf(tag);
            if (field == TY_MAP_KEY) {
                key = wireArmType(map.readMessage());
            } else if (field == TY_MAP_VALUE) {
                value = wireArmType(map.readMessage());
            } else {
                map.skipField(wire);
            }
        }
        return (key == null || value == null) ? null : BamlType.map(key, value);
    }

    private static BamlType wireLiteralType(WireReader msg) {
        BamlType result = null;
        while (msg.hasRemaining()) {
            int tag = msg.readTag();
            int field = WireReader.fieldOf(tag);
            int wire = WireReader.wireOf(tag);
            switch (field) {
                case LIT_STRING -> result = BamlType.literalString(msg.readString());
                case LIT_INT -> result = BamlType.literalInt(msg.readVarint());
                case LIT_BOOL -> result = BamlType.literalBool(msg.readVarint() != 0);
                // A BamlTyLiteral bigint rides as a decimal string.
                case LIT_BIGINT -> result = BamlType.literalBigint(new BigInteger(msg.readString()));
                case LIT_FLOAT -> result = BamlType.literalFloat(msg.readString());
                default -> msg.skipField(wire);
            }
        }
        return result;
    }

    /** Read a {@code name} (field 1) string from a class/enum {@code BamlTy} sub-message. */
    private static String nameField(WireReader msg) {
        // BUG: a runtime declaration's head is `live` or `anonymous_label`, not
        // `name`; this reads it as the made-up FQN "?" (a runtime instance's
        // handle then claims class "?"). Callers should take the head through
        // BamlType instead of a bare name.
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
        List<byte[]> typeArgBytes = null;
        while (r.hasRemaining()) {
            int tag = r.readTag();
            int field = WireReader.fieldOf(tag);
            int wire = WireReader.wireOf(tag);
            switch (field) {
                case CLASS_NAME -> fqn = r.readString();
                case CLASS_FIELDS -> decodeMapEntry(r.readMessage(), fields, lenient);
                case CLASS_TYPE_ARGS -> {
                    if (typeArgBytes == null) {
                        typeArgBytes = new ArrayList<>();
                    }
                    typeArgBytes.add(r.readBytes());
                }
                default -> r.skipField(wire);
            }
        }
        // A media stdlib class wraps its engine handle in a `_data` field; the
        // inner decode already reified the media wrapper, so unwrap and return it.
        if (isMediaFqn(fqn) && fields.containsKey(MEDIA_DATA_FIELD)) {
            return fields.get(MEDIA_DATA_FIELD);
        }
        Object instance = TypeRegistry.constructClass(fqn, fields);
        if (instance == null) {
            return fields;
        }
        bindReifiedTypeArgs(instance, typeArgBytes);
        return instance;
    }

    /**
     * Convert a generic class value's wire {@code type_args} (each a
     * {@code BamlTy}) into {@link BamlType} tokens and retain them for
     * {@code instance} in the {@link TypeRegistry} side-table, so the (future)
     * emitted {@code bamlTypeArgs()} accessor can surface them. Binding is
     * all-or-nothing: if any arg falls outside {@code BamlType}'s minimal grammar
     * (so {@link BamlType#fromWireTy} yields {@code null}), nothing is stored —
     * a partial list would misalign the De Bruijn positions.
     */
    private static void bindReifiedTypeArgs(Object instance, List<byte[]> typeArgBytes) {
        if (typeArgBytes == null || typeArgBytes.isEmpty()) {
            return;
        }
        List<BamlType> tokens = new ArrayList<>(typeArgBytes.size());
        for (byte[] tyBytes : typeArgBytes) {
            BamlType token = BamlType.fromWireTy(tyBytes);
            if (token == null) {
                return; // an unrepresentable arg poisons the whole binding
            }
            tokens.add(token);
        }
        TypeRegistry.bindTypeArgs(instance, tokens);
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

    /**
     * Decode a {@code BamlOutboundHandle} ({@code key}, {@code handle_type}; the
     * {@code ty} field is unused for the media/bare cases). A media handle type
     * ({@code ADT_MEDIA_*}) reifies the matching runtime-owned media class over a
     * fresh {@link BamlHandle}; an {@code ADT_RUNTIME_VALUE} handle whose type
     * names a registered opaque class reifies that class's generated wrapper,
     * at the class type arguments the type states; any other handle type
     * (including {@code ADT_MEDIA_GENERIC} and the not-yet-modeled
     * handle-backed capabilities) decodes to a bare {@link BamlHandle}. Mirrors
     * {@code bridge_python}'s {@code _decode_handle}.
     */
    private static Object decodeHandle(WireReader r) {
        long key = 0;
        int handleType = 0;
        byte[] tyBytes = null;
        while (r.hasRemaining()) {
            int tag = r.readTag();
            int field = WireReader.fieldOf(tag);
            int wire = WireReader.wireOf(tag);
            switch (field) {
                case HANDLE_KEY -> key = r.readVarint();
                case HANDLE_TYPE -> handleType = (int) r.readVarint();
                case HANDLE_TY -> tyBytes = r.readBytes();
                default -> r.skipField(wire);
            }
        }
        String classFqn = tyBytes == null ? null : selfTypeFqn(new WireReader(tyBytes));
        BamlHandle handle = new BamlHandle(key, handleType, classFqn);
        if (handleType == BamlHandle.ADT_RUNTIME_VALUE
                && classFqn != null
                && TypeRegistry.isOpaqueClass(classFqn)) {
            BamlOpaque instance = TypeRegistry.constructOpaque(classFqn, handle);
            bindReifiedTypeArgs(instance, classTypeArgs(tyBytes));
            return instance;
        }
        return switch (handleType) {
            case ADT_MEDIA_IMAGE -> Image.fromHandle(handle);
            case ADT_MEDIA_AUDIO -> Audio.fromHandle(handle);
            case ADT_MEDIA_VIDEO -> Video.fromHandle(handle);
            case ADT_MEDIA_PDF -> Pdf.fromHandle(handle);
            // A tagged heap handle reifies the runtime-owned BamlStream wrapper.
            // The wrapper retains handle.ty.class_ty.name and derives `.next`
            // and `.final` from that identity. Java erases the generic args but
            // must not erase the receiver class FQN.
            case ADT_TAGGED_HEAP_HANDLE -> baml_bridge.BamlStream.fromHandle(handle);
            case ADT_FUNCTION_SPEC -> baml_bridge.BamlFunctionSpec.fromHandle(handle);
            default -> handle;
        };
    }

    /**
     * Whether {@code fqn} names one of the handle-backed media stdlib classes.
     * The engine wraps a media value as {@code class_value(baml.media.X, {_data})}
     * whose {@code _data} field is a media {@code handle_value}; the wrapper is
     * unwrapped to that decoded field (see {@link #decodeClass}), mirroring
     * {@code bridge_python}'s {@code _decode_class} media special-case. These
     * FQNs are runtime-owned and never registered in the {@link TypeRegistry}.
     */
    private static boolean isMediaFqn(String fqn) {
        return Image.FQN.equals(fqn)
                || Audio.FQN.equals(fqn)
                || Video.FQN.equals(fqn)
                || Pdf.FQN.equals(fqn);
    }

    // ========================================================================
    // Type-directed decode (descriptor-driven).
    //
    // A generated binding passes a typed decode descriptor ({@link BamlType}) for
    // its declared return type (and per-field descriptors on registerClass). The
    // descriptor drives decode against the *declared* shape — most importantly it
    // lands a union result on the generic `baml_bridge.Union{k}.Arm{i}` family,
    // picking the arm from the DECLARED arm order (the wire carries no trustworthy
    // arm order). A null descriptor (or a TypeVar / UNKNOWN / bare primitive /
    // literal one) falls back to the wire-driven `decodeValue` path, intact.
    // ========================================================================

    /**
     * Decode a {@code BamlOutboundValue} ({@code valueBytes}) against a descriptor
     * {@link BamlType}. A null descriptor (or a TypeVar / UNKNOWN / bare primitive
     * / literal one) decodes wire-driven; a list / map recurses element/value
     * decode through the descriptor; a named type (CLASS / ENUM) reifies the
     * class / enum / recursive-alias; a union matches the wire value against the
     * declared arms in order and wraps the arm.
     */
    public static Object decodeWithDesc(byte[] valueBytes, BamlType desc, boolean lenient) {
        if (valueBytes == null) {
            return null;
        }
        if (desc == null) {
            return decodeValue(new WireReader(valueBytes), lenient);
        }
        switch (desc.kind()) {
            case LIST:
                return decodeListWithDesc(valueBytes, desc.listItem(), lenient);
            case MAP:
                return decodeMapWithDesc(valueBytes, desc.mapValue(), lenient);
            case CLASS:
            case ENUM:
            case TYPE_ALIAS:
                // A declaration the runtime created has no generated type: its
                // values are the engine's own and decode wire-driven.
                if (desc.fqn() == null) {
                    return decodeValue(new WireReader(valueBytes), lenient);
                }
                // A recursive alias is registered under its FQN like a class.
                return decodeFqnWithDesc(valueBytes, desc.fqn(), lenient);
            case UNION:
                // The descriptor is the declared type; the Java shape the generated
                // code expects follows the generator's rules for it: a union of
                // literals of one base is that base's scalar, and a union
                // mentioning a type variable is the bare wire value (its Java type
                // is Object). Anything else is the arity family's arm record.
                if (literalBaseOf(desc) != null || mentionsTypeVar(desc)) {
                    return decodeValue(new WireReader(valueBytes), lenient);
                }
                return decodeUnionWithDesc(valueBytes, desc, lenient);
            case OPTIONAL:
                return decodeWithDesc(valueBytes, desc.optionalInner(), lenient);
            case FUNCTION:
                return decodeCallable(valueBytes, desc, lenient);
            case PRIMITIVE:
            case LITERAL:
            case ENUM_VARIANT:
            case MEDIA:
            case INTERFACE:
            case NEVER:
            case TYPEVAR:
            case UNKNOWN:
            case OPAQUE:
            default:
                // A scalar/literal descriptor carries no structural recursion; the
                // self-describing wire form already yields the right value. TypeVar /
                // UNKNOWN / OPAQUE are the explicit wire-driven fallbacks.
                return decodeValue(new WireReader(valueBytes), lenient);
        }
    }

    /** Decode a {@code list_value}, reifying each element through {@code elemDesc}. */
    private static Object decodeListWithDesc(byte[] valueBytes, BamlType elemDesc, boolean lenient) {
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
    private static Object decodeMapWithDesc(byte[] valueBytes, BamlType valueDesc, boolean lenient) {
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
     * Decode against a named-type (FQN) descriptor. The registry decides the kind:
     * a class reifies with per-field descriptors; a named recursive-alias union
     * routes to the wire-driven {@link TypeRegistry#constructUnionForFqn} (its
     * minted nominal records); an enum (or an unresolved FQN) decodes wire-driven.
     */
    private static Object decodeFqnWithDesc(byte[] valueBytes, String fqn, boolean lenient) {
        if (TypeRegistry.isOpaqueClass(fqn)) {
            return decodeOpaque(valueBytes, fqn, lenient);
        }
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
            BamlType armType = TypeRegistry.unionArmTokenForFqn(fqn, effective);
            // Decode the arm's inner value type-directed via the arm token
            // (nested recursive-alias contents must reify, not decode bare).
            Object inner = armType != null
                    ? decodeWithDesc(effective, armType, lenient)
                    : decodeValue(new WireReader(effective), lenient);
            if (inner == null) {
                return null;
            }
            Object record = TypeRegistry.constructUnionForFqn(fqn, effective, inner);
            return record != null ? record : inner;
        }
        // Enum or unresolved FQN: the wire form is self-describing.
        return decodeValue(new WireReader(valueBytes), lenient);
    }

    /**
     * Decode a value at a position typed as {@code fqn}, a class that crosses as
     * an opaque reference: the engine sends its instance as an
     * {@code ADT_RUNTIME_VALUE} handle whose {@code ty} names the class, and the
     * handle decodes into that class's generated wrapper. Anything else at such
     * a position is the engine breaking its contract.
     */
    private static Object decodeOpaque(byte[] valueBytes, String fqn, boolean lenient) {
        Object decoded = decodeValue(new WireReader(valueBytes), lenient);
        if (decoded == null) {
            return null;
        }
        // The handle decoder reifies a registered opaque class's wrapper.
        if (decoded instanceof BamlOpaque instance && fqn.equals(instance.bamlHandle().classFqn())) {
            return instance;
        }
        if (lenient) {
            return decoded;
        }
        throw new BamlError(
                "expected a " + fqn + " instance as a runtime-value handle, received "
                        + (decoded instanceof BamlHandle handle
                                ? "a handle of type " + handle.handleType() + " to " + handle.classFqn()
                                : "a " + decoded.getClass().getName()),
                List.of(),
                null);
    }

    /**
     * The class type arguments ({@code BamlTy} bytes, in declaration order) a
     * type states, or an empty list when it is not a class type or states none.
     */
    private static List<byte[]> classTypeArgs(byte[] tyBytes) {
        WireReader classTy = subTy(new WireReader(tyBytes), TY_CLASS);
        if (classTy == null) {
            return List.of();
        }
        List<byte[]> typeArgs = new ArrayList<>();
        while (classTy.hasRemaining()) {
            int tag = classTy.readTag();
            if (WireReader.fieldOf(tag) == TY_CLASS_TYPE_ARGS) {
                typeArgs.add(classTy.readBytes());
            } else {
                classTy.skipField(WireReader.wireOf(tag));
            }
        }
        return typeArgs;
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
        List<byte[]> typeArgBytes = null;
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
                case CLASS_TYPE_ARGS -> {
                    if (typeArgBytes == null) {
                        typeArgBytes = new ArrayList<>();
                    }
                    typeArgBytes.add(r.readBytes());
                }
                default -> r.skipField(wire);
            }
        }
        String[] order = TypeRegistry.classFieldOrder(fqn);
        BamlType[] descs = TypeRegistry.classFieldDescs(fqn);
        Map<String, Object> fields = new LinkedHashMap<>();
        for (int i = 0; i < keys.size(); i++) {
            BamlType fieldDesc = null;
            if (order != null && descs != null) {
                int idx = indexOf(order, keys.get(i));
                if (idx >= 0 && idx < descs.length) {
                    fieldDesc = descs[idx];
                }
            }
            byte[] vb = vals.get(i);
            fields.put(keys.get(i), vb == null ? null : decodeWithDesc(vb, fieldDesc, lenient));
        }
        // Defensive parity with the wire-driven path: a media stdlib class
        // (never registered, so normally unreachable here) unwraps its `_data`.
        if (isMediaFqn(fqn) && fields.containsKey(MEDIA_DATA_FIELD)) {
            return fields.get(MEDIA_DATA_FIELD);
        }
        Object instance = TypeRegistry.constructClass(fqn, fields);
        if (instance == null) {
            return fields;
        }
        bindReifiedTypeArgs(instance, typeArgBytes);
        return instance;
    }

    /**
     * Decode against a union descriptor onto the generic {@code Union{k}} arm
     * family. A {@code union_variant_value} wrapper is unwrapped first (the
     * descriptor is preferred over the wire {@code self_type}), then the
     * (unwrapped) wire value is matched against the declared arms IN ORDER by its
     * shape ({@link #armMatchesValue}); the first matching arm's inner value is
     * decoded with that arm and wrapped in {@code baml_bridge.Union{k}.Arm{i}}. A
     * null wire value decodes to null; no arm match throws {@link BamlError}.
     */
    private static Object decodeUnionWithDesc(byte[] valueBytes, BamlType unionDesc, boolean lenient) {
        byte[] effective = valueBytes;
        BamlType selectedType = null;
        if (outboundArm(valueBytes) == OV_UNION) {
            selectedType = extractUnionSelectedType(valueBytes);
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
        List<BamlType> arms = unionDesc.unionOptions();
        int k = arms.size();
        if (selectedType != null) {
            int selectedArm = arms.indexOf(selectedType);
            if (selectedArm >= 0) {
                // The declared arm, not the wire's spelling of it: the declared
                // one also names the Java type its values take (a callable's
                // functional interface).
                Object inner = decodeWithDesc(effective, arms.get(selectedArm), lenient);
                return wrapArm(k, selectedArm, inner);
            }
            // The wire names a type no arm spells the same way (a literal arm
            // for a scalar, an alias for its body): the value's shape decides.
        }
        for (int i = 0; i < k; i++) {
            if (armMatchesValue(arms.get(i), effective)) {
                Object inner = decodeWithDesc(effective, arms.get(i), lenient);
                return wrapArm(k, i, inner);
            }
        }
        throw new BamlError(
                "type-directed union decode: no declared arm matched the wire value"
                        + " for descriptor " + unionDesc,
                List.of(),
                null);
    }

    /**
     * The base of a union whose every arm is a literal of one base ({@code "a" |
     * "b"} is a {@code string}), or {@code null}. The generator gives such a
     * union the base's Java type, so its values decode as that scalar.
     */
    private static String literalBaseOf(BamlType union) {
        String base = null;
        for (BamlType arm : union.unionOptions()) {
            if (arm.kind() != BamlType.Kind.LITERAL) {
                return null;
            }
            if (base == null) {
                base = arm.literalBase();
            } else if (!base.equals(arm.literalBase())) {
                return null;
            }
        }
        return base;
    }

    /** Whether a type variable occurs anywhere in {@code type}. */
    private static boolean mentionsTypeVar(BamlType type) {
        if (type.kind() == BamlType.Kind.TYPEVAR) {
            return true;
        }
        switch (type.kind()) {
            case LIST -> {
                return mentionsTypeVar(type.listItem());
            }
            case MAP -> {
                return mentionsTypeVar(type.mapKey()) || mentionsTypeVar(type.mapValue());
            }
            case OPTIONAL -> {
                return mentionsTypeVar(type.optionalInner());
            }
            case UNION -> {
                return type.unionOptions().stream().anyMatch(ProtoReader::mentionsTypeVar);
            }
            case FUNCTION -> {
                return type.functionParams().stream().anyMatch(p -> mentionsTypeVar(p.type()))
                        || mentionsTypeVar(type.functionReturn())
                        || mentionsTypeVar(type.functionThrows());
            }
            default -> {
                return false;
            }
        }
    }

    // -- structural arm matching (shared by both arm-picking paths) -----------

    /**
     * Whether a union arm {@link BamlType} matches the shape of a wire
     * {@code BamlOutboundValue} ({@code valueBytes}). Shared by the descriptor
     * path ({@link #decodeUnionWithDesc}) and the wire-driven registry path
     * ({@code TypeRegistry.UnionEntry.pickArm}).
     *
     * <h3>Rules (structural — the compat lattice)</h3>
     * <ul>
     *   <li>a wildcard arm (TypeVar / UNKNOWN) matches anything;</li>
     *   <li>a nested union matches if any of its arms match;</li>
     *   <li>a primitive matches the same wire scalar kind;</li>
     *   <li>a media type matches a media value of its kind (any media for the
     *       generic {@code media}), portable or handle-backed;</li>
     *   <li>a function type matches a callable handle (a BAML closure, or a host
     *       callable coming back);</li>
     *   <li>a named type (CLASS / ENUM) matches a class/enum value of the same
     *       FQN, a handle whose type names that FQN (a stream, a function spec, an
     *       opaque instance), the value form of {@code ai.Prompt} /
     *       {@code reflect.Type}, or — when it names a registered recursive-alias
     *       union — a value that union recognizes (the FQN + recursive-alias
     *       registry fallback);</li>
     *   <li>a literal matches a wire literal of the same base (a same-base literal
     *       union is erased in codegen, so at most one literal arm per base
     *       survives — base match cannot be ambiguous), or a bare scalar of that
     *       base;</li>
     *   <li>a list / map matches a wire container of the same base whose element /
     *       key+value type is <em>structurally</em> the same (via
     *       {@link BamlType#matchesStructural}), or whose type is absent / imprecise
     *       — a bare wire container is an element-type wildcard, so an empty
     *       typed {@code int[]} lands on the {@code int[]} arm, not a
     *       {@code string[]} one declared first.</li>
     * </ul>
     */
    public static boolean armMatchesValue(BamlType arm, byte[] valueBytes) {
        if (arm.isWildcard()) {
            return true;
        }
        if (arm.kind() == BamlType.Kind.UNION) {
            for (BamlType opt : arm.unionOptions()) {
                if (armMatchesValue(opt, valueBytes)) {
                    return true;
                }
            }
            return false;
        }
        int ov = outboundArm(valueBytes);
        return switch (arm.kind()) {
            case PRIMITIVE -> (arm.isInt() && ov == OV_INT)
                    || (arm.isBigint() && ov == OV_BIGINT)
                    || (arm.isString() && ov == OV_STRING)
                    || (arm.isBool() && ov == OV_BOOL)
                    || (arm.isFloat() && ov == OV_FLOAT)
                    || (arm.equals(BamlType.BYTES) && ov == OV_UINT8ARRAY);
            case MEDIA -> mediaArmMatches(arm, ov, valueBytes);
            case FUNCTION -> {
                HandleWire handle = ov == OV_HANDLE ? handleWire(valueBytes) : null;
                yield handle != null && handle.isCallable();
            }
            case CLASS, ENUM -> namedArmMatches(arm, ov, valueBytes);
            case LITERAL -> literalArmMatches(arm, ov, valueBytes);
            case TYPE -> ov == OV_TY;
            case PROMPT_AST -> ov == OV_PROMPT_AST;
            case LIST -> ov == OV_LIST && containerItemMatches(arm.listItem(), valueBytes, OV_LIST, LIST_ITEM_TYPE);
            case MAP -> ov == OV_MAP && mapArmMatches(arm, valueBytes);
            case OPTIONAL -> ov == OV_NULL || armMatchesValue(arm.optionalInner(), valueBytes);
            default -> false;
        };
    }

    /**
     * Whether a CLASS / ENUM arm matches a class/enum value FQN, a handle typed
     * as that FQN, the value form of a class that crosses as one, or a
     * recursive-alias union it names.
     */
    private static boolean namedArmMatches(BamlType arm, int ov, byte[] valueBytes) {
        if (ov == OV_CLASS || ov == OV_ENUM) {
            String name = namedValueFqn(valueBytes);
            if (arm.fqn().equals(name)) {
                return true;
            }
        }
        if (ov == OV_HANDLE) {
            HandleWire handle = handleWire(valueBytes);
            if (handle != null && arm.fqn().equals(handle.classFqn())) {
                return true;
            }
        }
        if ((ov == OV_PROMPT_AST && PROMPT_CLASS.equals(arm.fqn()))
                || (ov == OV_TY && TYPE_CLASS.equals(arm.fqn()))) {
            return true;
        }
        // Recursive-alias union arm: the arm names a registered union; match when
        // that union recognizes the wire value's shape as one of its arms.
        return TypeRegistry.isUnionKey(arm.fqn())
                && TypeRegistry.unionArmTokenForFqn(arm.fqn(), valueBytes) != null;
    }

    /** Whether a MEDIA arm matches a media value of its kind (the generic arm: of any kind). */
    private static boolean mediaArmMatches(BamlType arm, int ov, byte[] valueBytes) {
        int kind = mediaTypeKindOf(ov, valueBytes);
        return kind != 0
                && (arm.equals(BamlType.media(MEDIA_GENERIC)) || arm.equals(BamlType.media(kind)));
    }

    /**
     * The {@code BamlTyMediaKind} of a media value, portable ({@code media_value})
     * or handle-backed ({@code handle_value} of an {@code ADT_MEDIA_*} type), or
     * 0 when the value is not media.
     */
    private static int mediaTypeKindOf(int ov, byte[] valueBytes) {
        if (ov == OV_MEDIA) {
            WireReader r = new WireReader(outboundSub(valueBytes, OV_MEDIA));
            int kind = 0;
            while (r.hasRemaining()) {
                int tag = r.readTag();
                if (WireReader.fieldOf(tag) == MEDIA_KIND) {
                    kind = (int) r.readVarint();
                } else {
                    r.skipField(WireReader.wireOf(tag));
                }
            }
            return switch (kind) {
                case MEDIA_VALUE_IMAGE -> MEDIA_IMAGE;
                case MEDIA_VALUE_AUDIO -> MEDIA_AUDIO;
                case MEDIA_VALUE_PDF -> MEDIA_PDF;
                case MEDIA_VALUE_VIDEO -> MEDIA_VIDEO;
                default -> 0;
            };
        }
        if (ov == OV_HANDLE) {
            HandleWire handle = handleWire(valueBytes);
            return handle == null ? 0 : switch (handle.type()) {
                case ADT_MEDIA_IMAGE -> MEDIA_IMAGE;
                case ADT_MEDIA_AUDIO -> MEDIA_AUDIO;
                case ADT_MEDIA_VIDEO -> MEDIA_VIDEO;
                case ADT_MEDIA_PDF -> MEDIA_PDF;
                case ADT_MEDIA_GENERIC -> MEDIA_GENERIC;
                default -> 0;
            };
        }
        return 0;
    }

    /**
     * A {@code handle_value}'s fields, read without taking the handle: matching
     * an arm must not mint a {@link BamlHandle}, whose release would free the
     * row the decode that follows takes.
     */
    private record HandleWire(long key, int type, byte[] ty) {
        boolean isCallable() {
            return type == BamlHandle.FUNCTION_REF || type == BamlHandle.HOST_VALUE_CALLABLE;
        }

        /** The class the handle's type names, or null when it names none. */
        String classFqn() {
            return ty == null ? null : selfTypeFqn(new WireReader(ty));
        }
    }

    /** The {@code handle_value} of an outbound value, or null when it carries none. */
    private static HandleWire handleWire(byte[] valueBytes) {
        byte[] sub = outboundSub(valueBytes, OV_HANDLE);
        if (sub == null) {
            return null;
        }
        WireReader r = new WireReader(sub);
        long key = 0;
        int type = 0;
        byte[] ty = null;
        while (r.hasRemaining()) {
            int tag = r.readTag();
            switch (WireReader.fieldOf(tag)) {
                case HANDLE_KEY -> key = r.readVarint();
                case HANDLE_TYPE -> type = (int) r.readVarint();
                case HANDLE_TY -> ty = r.readBytes();
                default -> r.skipField(WireReader.wireOf(tag));
            }
        }
        return new HandleWire(key, type, ty);
    }

    /**
     * Decode a value at a function-typed position into the functional
     * interface the position names ({@link BamlType#functionHostInterface}). A
     * BAML closure arrives as a {@code FUNCTION_REF} handle and becomes an
     * instance of that interface which calls back into BAML through the
     * handle; a host callable coming back is the host's own object again. A
     * position no generated type names decodes wire-driven, to the bare handle.
     */
    private static Object decodeCallable(byte[] valueBytes, BamlType functionType, boolean lenient) {
        Class<?> host = functionType.functionHostInterface();
        if (host == null) {
            return decodeValue(new WireReader(valueBytes), lenient);
        }
        int ov = outboundArm(valueBytes);
        if (ov == 0 || ov == OV_NULL) {
            return null;
        }
        HandleWire handle = ov == OV_HANDLE ? handleWire(valueBytes) : null;
        if (handle != null && handle.type() == BamlHandle.FUNCTION_REF) {
            // The engine handed over one reference to the closure: own it
            // first, so a closure refused below is released with its handle.
            BamlHandle owned = new BamlHandle(handle.key(), handle.type());
            try {
                if (functionType.functionParams().stream().anyMatch(BamlType.Param::optional)) {
                    // BUG: a BAML closure with optional parameters cannot be called
                    // through a generated interface yet: its SAM folds them into an
                    // `Opts` bag the closure proxy does not unfold. Refused here, as
                    // the generator refuses such a returned callable.
                    throw new BamlError(
                            "a BAML closure of type " + functionType + " has optional parameters, which "
                                    + host.getName() + " cannot pass yet",
                            List.of(),
                            null);
                }
                String[] names = callableParamNames(handle.ty());
                if (names.length != functionType.functionParams().size()) {
                    throw new BamlError(
                            "a BAML closure taking " + names.length + " arguments arrived where "
                                    + functionType + " was declared",
                            List.of(),
                            null);
                }
                return baml_bridge.BamlFfi.bamlClosure(host, owned, names, functionType.functionReturn());
            } catch (RuntimeException refused) {
                owned.close();
                throw refused;
            }
        }
        if (handle != null && handle.type() == BamlHandle.HOST_VALUE_CALLABLE
                && baml_bridge.HostCalls.lookup(handle.key())
                        instanceof baml_bridge.BamlTypedCallable typed
                && host.isInstance(typed.callable())) {
            return typed.callable();
        }
        if (lenient) {
            return decodeValue(new WireReader(valueBytes), lenient);
        }
        throw new BamlError(
                "expected a callable " + host.getName() + " for " + functionType + ", received "
                        + (handle == null
                                ? "wire value " + ov
                                : "a handle of type " + handle.type()),
                List.of(),
                null);
    }

    /**
     * The argument names a BAML closure binds by: its own function type's
     * parameter names, a nameless parameter being {@code arg<index>}, as the
     * engine names it when it binds a call through the handle.
     */
    private static String[] callableParamNames(byte[] ty) {
        if (ty == null) {
            throw new BamlError("a BAML closure arrived without its type", List.of(), null);
        }
        byte[] function = outboundSub(ty, TY_FUNCTION);
        if (function == null) {
            throw new BamlError("a BAML closure's type is not a function type", List.of(), null);
        }
        List<String> names = new ArrayList<>();
        WireReader r = new WireReader(function);
        while (r.hasRemaining()) {
            int tag = r.readTag();
            if (WireReader.fieldOf(tag) != TY_FUNCTION_PARAMS) {
                r.skipField(WireReader.wireOf(tag));
                continue;
            }
            String name = null;
            WireReader param = r.readMessage();
            while (param.hasRemaining()) {
                int paramTag = param.readTag();
                if (WireReader.fieldOf(paramTag) == TY_FUNCTION_PARAM_NAME) {
                    name = param.readString();
                } else {
                    param.skipField(WireReader.wireOf(paramTag));
                }
            }
            names.add(name != null ? name : "arg" + names.size());
        }
        return names.toArray(String[]::new);
    }

    /** The FQN of a class/enum {@code BamlOutboundValue} (name field), or null. */
    private static String namedValueFqn(byte[] valueBytes) {
        byte[] sub = outboundSub(valueBytes, OV_CLASS);
        if (sub == null) {
            sub = outboundSub(valueBytes, OV_ENUM);
        }
        return sub == null ? null : classNameOf(new WireReader(sub));
    }

    /** Whether a LITERAL arm matches a wire literal of the same base, or a bare scalar of that base. */
    private static boolean literalArmMatches(BamlType arm, int ov, byte[] valueBytes) {
        String base = arm.literalBase();
        if (ov == OV_LITERAL) {
            return base.equals(literalValueBase(valueBytes));
        }
        return switch (base) {
            case "string" -> ov == OV_STRING;
            case "int" -> ov == OV_INT;
            case "bool" -> ov == OV_BOOL;
            case "float" -> ov == OV_FLOAT;
            case "bigint" -> ov == OV_BIGINT;
            default -> false;
        };
    }

    /** The base name of a wire {@code literal_value} ({@code string} / {@code int} / …), or {@code "?"}. */
    private static String literalValueBase(byte[] valueBytes) {
        byte[] lit = outboundSub(valueBytes, OV_LITERAL);
        if (lit == null) {
            return "?";
        }
        WireReader r = new WireReader(lit);
        String base = "?";
        while (r.hasRemaining()) {
            int tag = r.readTag();
            int field = WireReader.fieldOf(tag);
            int wire = WireReader.wireOf(tag);
            switch (field) {
                case LIT_STRING -> { r.skipField(wire); base = "string"; }
                case LIT_INT -> { r.skipField(wire); base = "int"; }
                case LIT_BOOL -> { r.skipField(wire); base = "bool"; }
                case LIT_BIGINT -> { r.skipField(wire); base = "bigint"; }
                case LIT_FLOAT -> { r.skipField(wire); base = "float"; }
                default -> r.skipField(wire);
            }
        }
        return base;
    }

    /**
     * Whether a container arm's inner type {@code armInner} matches a wire
     * container's element type — the {@code typeField} ({@code item_type} /
     * {@code key_type} / {@code value_type}) {@code BamlTy} of the {@code ovField}
     * sub-message, via {@link BamlType#matchesStructural}. An absent or imprecise
     * (unrepresentable / alias) wire type is an element-type wildcard (matches).
     */
    private static boolean containerItemMatches(
            BamlType armInner, byte[] valueBytes, int ovField, int typeField) {
        byte[] container = outboundSub(valueBytes, ovField);
        if (container == null) {
            return false;
        }
        byte[] itemTy = outboundSub(container, typeField);
        if (itemTy == null) {
            return true; // bare / pre-typed wire → element-type wildcard
        }
        BamlType wireItem = BamlType.fromWireTy(itemTy);
        return wireItem == null || armInner.matchesStructural(wireItem);
    }

    /** Whether a MAP arm matches a wire map by both key and value type (bare/imprecise → wildcard). */
    private static boolean mapArmMatches(BamlType arm, byte[] valueBytes) {
        byte[] mapBytes = outboundSub(valueBytes, OV_MAP);
        if (mapBytes == null) {
            return false;
        }
        byte[] keyTy = outboundSub(mapBytes, MAP_KEY_TYPE);
        byte[] valTy = outboundSub(mapBytes, MAP_VALUE_TYPE);
        if (keyTy == null || valTy == null) {
            return true; // bare → wildcard
        }
        BamlType wireKey = BamlType.fromWireTy(keyTy);
        BamlType wireVal = BamlType.fromWireTy(valTy);
        if (wireKey == null || wireVal == null) {
            return true; // imprecise → wildcard
        }
        return arm.mapKey().matchesStructural(wireKey) && arm.mapValue().matchesStructural(wireVal);
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

    /**
     * The set {@code BamlOutboundValue} oneof arm field number (last-wins), or 0
     * when none is set (proto3's encoding of a {@code null} value).
     */
    private static int outboundArm(byte[] valueBytes) {
        WireReader r = new WireReader(valueBytes);
        int arm = 0;
        while (r.hasRemaining()) {
            int tag = r.readTag();
            int field = WireReader.fieldOf(tag);
            int wire = WireReader.wireOf(tag);
            switch (field) {
                case OV_NULL, OV_STRING, OV_INT, OV_FLOAT, OV_BOOL, OV_CLASS, OV_ENUM,
                        OV_LITERAL, OV_LIST, OV_MAP, OV_UNION, OV_HANDLE, OV_MEDIA,
                        OV_PROMPT_AST, OV_UINT8ARRAY, OV_BIGINT, OV_TY -> {
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

    private static BamlType extractUnionSelectedType(byte[] valueBytes) {
        byte[] uvBytes = outboundSub(valueBytes, OV_UNION);
        if (uvBytes == null) {
            return null;
        }
        WireReader r = new WireReader(uvBytes);
        Integer selected = null;
        byte[] selfType = null;
        while (r.hasRemaining()) {
            int tag = r.readTag();
            int field = WireReader.fieldOf(tag);
            int wire = WireReader.wireOf(tag);
            if (field == UNION_SELECTED_OPTION_INDEX) {
                selected = (int) r.readVarint();
            } else if (field == UNION_SELF_TYPE) {
                selfType = r.readBytes();
            } else {
                r.skipField(wire);
            }
        }
        if (selected == null || selfType == null) {
            return null;
        }
        BamlType selectedType = selfTypeOptionAt(selfType, selected);
        if (selectedType == null) {
            throw new BamlError(
                    "union selected option index " + selected + " is invalid for self_type",
                    List.of(), null);
        }
        return hostSelectedType(selectedType);
    }

    /** Resolve an index against the raw union options without dropping null holes. */
    private static BamlType selfTypeOptionAt(byte[] selfTypeBytes, int selectedIndex) {
        WireReader ty = new WireReader(selfTypeBytes);
        while (ty.hasRemaining()) {
            int tag = ty.readTag();
            int field = WireReader.fieldOf(tag);
            int wire = WireReader.wireOf(tag);
            if (field != TY_UNION) {
                ty.skipField(wire);
                continue;
            }
            WireReader union = ty.readMessage();
            int index = 0;
            while (union.hasRemaining()) {
                int optionTag = union.readTag();
                int optionField = WireReader.fieldOf(optionTag);
                int optionWire = WireReader.wireOf(optionTag);
                if (optionField == TY_UNION_OPTIONS) {
                    byte[] option = union.readBytes();
                    if (index == selectedIndex) {
                        return BamlType.fromWireTy(option);
                    }
                    index++;
                } else {
                    union.skipField(optionWire);
                }
            }
            return null;
        }
        return null;
    }

    private static BamlType hostSelectedType(BamlType selected) {
        while (selected.kind() == BamlType.Kind.OPTIONAL) {
            selected = selected.optionalInner();
        }
        return selected;
    }

    private static int indexOf(String[] arr, String want) {
        for (int i = 0; i < arr.length; i++) {
            if (arr[i].equals(want)) {
                return i;
            }
        }
        return -1;
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

    /**
     * Workspace bigint cap = 2^28 bits ⇒ at most {@code (2^28)/4} hex digits
     * (plus a small slack for the sign), mirroring the Rust-side
     * {@code MAX_BIGINT_HEX_LEN} in {@code bridge_ctypes/src/value_decode.rs}
     * and the Python ({@code _MAX_BIGINT_HEX_LEN}) / TypeScript bridges. A wire
     * payload longer than this is rejected before the {@link BigInteger} is
     * built, so an adversarial multi-megabyte hex blob can't drive an unbounded
     * allocation ahead of the VM's own {@code MAX_BIGINT_BITS} guard.
     */
    public static final int MAX_BIGINT_HEX_LEN = (1 << 28) / 4 + 2;

    /**
     * Decode a wire {@code bigint_value} (base-16, with an optional single
     * leading {@code -}) with a pre-allocation length cap and strict-hex
     * validation, mirroring the Python bridge's {@code _parse_hex_bigint} and
     * the Rust/TypeScript bridges. Rejects an over-cap or non-strict-hex payload
     * with {@link IllegalStateException} — the malformed-wire failure mode the
     * rest of this codec uses (see {@link WireReader}) — rather than letting
     * {@code new BigInteger(hex, 16)} allocate unbounded or surface a bare
     * {@link NumberFormatException}.
     */
    public static BigInteger parseHexBigInt(String hex) {
        // Strip exactly one leading minus (matching the encoders and the other
        // bridges); anything else — `+`, `0x`, underscores, whitespace — is
        // rejected by the strict-hex check below.
        boolean negative = !hex.isEmpty() && hex.charAt(0) == '-';
        String magnitude = negative ? hex.substring(1) : hex;
        if (magnitude.length() > MAX_BIGINT_HEX_LEN) {
            throw new IllegalStateException(
                    "bigint hex exceeds the workspace cap ("
                            + magnitude.length() + " chars, limit " + MAX_BIGINT_HEX_LEN + ")");
        }
        if (magnitude.isEmpty() || !isStrictHex(magnitude)) {
            // A malformed payload can be nearly cap-sized (up to MAX_BIGINT_HEX_LEN
            // ~67 MB): report its length plus a short prefix preview rather than
            // echoing the whole adversarial blob into the exception message / logs.
            throw new IllegalStateException(
                    "invalid bigint hex string (" + hex.length() + " chars): " + preview(hex));
        }
        BigInteger value = new BigInteger(magnitude, 16);
        return negative ? value.negate() : value;
    }

    /** Max characters of a wire string echoed into an error message. */
    private static final int PREVIEW_CAP = 32;

    /**
     * A length-bounded preview of a possibly-huge wire string for error messages:
     * the first {@link #PREVIEW_CAP} characters, then an elision marker noting how
     * many were dropped, so an adversarial multi-megabyte payload can't bloat the
     * message.
     */
    private static String preview(String s) {
        if (s.length() <= PREVIEW_CAP) {
            return s;
        }
        return s.substring(0, PREVIEW_CAP) + "… (+" + (s.length() - PREVIEW_CAP) + " more chars)";
    }

    /** True iff every char is an ASCII hex digit (no sign, `0x` prefix, or whitespace). */
    private static boolean isStrictHex(String s) {
        for (int i = 0; i < s.length(); i++) {
            char c = s.charAt(i);
            boolean hex =
                    (c >= '0' && c <= '9') || (c >= 'a' && c <= 'f') || (c >= 'A' && c <= 'F');
            if (!hex) {
                return false;
            }
        }
        return true;
    }

}
