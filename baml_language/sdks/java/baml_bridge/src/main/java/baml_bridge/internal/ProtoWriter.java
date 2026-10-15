package baml_bridge.internal;

import baml_bridge.BamlType;
import baml_bridge.BamlTypes;
import baml_bridge.TypeRegistry;

import java.math.BigInteger;
import java.util.List;
import java.util.Map;

/**
 * Encodes host arguments into a {@code CallFunctionArgs} protobuf
 * ({@code baml_inbound.proto}).
 *
 * <p>Every node leaves with its concrete type. A scalar, null, enum constant,
 * media value or handle implies its own and carries nothing. A list, map,
 * class or host callable states its type in {@code InboundValue.value_type}:
 * the slot's type where the generated binding supplies one that fixes it (a
 * {@code List<Long>} under a {@code (int | string)[]} parameter goes out as a
 * {@code (int | string)[]}), and the type the value itself has where no slot
 * fixes it ({@link #naturalType}). A callable has no type of its own, so one
 * that reaches the encoder without a function type is refused. The engine
 * builds exactly the value stated and checks it against the slot.
 *
 * <h2>Field numbers (against {@code baml_inbound.proto})</h2>
 * <pre>
 * CallFunctionArgs: kwargs = 1 (repeated InboundMapEntry), call_id = 2 (uint64),
 *                   type_args = 3 (repeated BamlTyArg — explicit-generics bindings)
 * BamlTyArg:        type_var = 1 (string), type_value = 2 (BamlTy)
 * InboundMapEntry:  string_key = 1, value = 6 (InboundValue)
 * InboundValue:       value_type = 1 (the node's concrete type, where not implied)
 * InboundValue oneof: string_value = 2, int_value = 3, float_value = 4,
 *                     bool_value = 5, list_value = 6, map_value = 7,
 *                     class_value = 8, enum_value = 9, handle = 10,
 *                     uint8array_value = 11, bigint_value = 12,
 *                     media_value = 15, prompt_ast_value = 16
 * BamlValueMedia:    media = 1 (MediaTypeEnum), mime_type = 2, url = 3, base64 = 4,
 *                    file_content = 6 (BamlValueMediaFileContent: name = 1, base64 = 2)
 * InboundListValue:  values = 1 (repeated InboundValue)
 * InboundMapValue:   entries = 1 (repeated InboundMapEntry)
 * InboundClassValue: fields = 2 (repeated InboundMapEntry; field 1 is reserved)
 * BamlTyClass:       name = 1 (BAML FQN), type_args = 2 (repeated BamlTy — a reified
 *                    generic instance's concrete class type args, from the TypeRegistry
 *                    side-table; empty for a non-generic/unbound instance)
 * InboundEnumValue:  name = 1 (BAML FQN), value = 2 (wire variant name)
 * </pre>
 */
public final class ProtoWriter {
    // CallFunctionArgs
    private static final int CALL_ARGS_KWARGS = 1;
    private static final int CALL_ARGS_CALL_ID = 2;
    private static final int CALL_ARGS_TYPE_ARGS = 3;
    private static final int CALL_ARGS_FUNCTION_NAME = 4;
    private static final int CALL_ARGS_FUNCTION_HANDLE = 5;

    // BamlTyArg (one explicit TypeVar binding).
    private static final int TY_ARG_TYPE_VAR = 1;
    private static final int TY_ARG_TYPE_VALUE = 2;

    // InboundMapEntry
    private static final int MAP_ENTRY_STRING_KEY = 1;
    private static final int MAP_ENTRY_VALUE = 6;

    // InboundValue oneof
    private static final int IV_VALUE_TYPE = 1;
    private static final int IV_STRING = 2;
    private static final int IV_INT = 3;
    private static final int IV_FLOAT = 4;
    private static final int IV_BOOL = 5;
    private static final int IV_LIST = 6;
    private static final int IV_MAP = 7;
    private static final int IV_CLASS = 8;
    private static final int IV_ENUM = 9;
    private static final int IV_HANDLE = 10;
    private static final int IV_UINT8ARRAY = 11;
    private static final int IV_BIGINT = 12;
    private static final int IV_MEDIA = 15;
    private static final int IV_PROMPT_AST = 16;

    // BamlHandle (baml_handle.proto): key = 1 (uint64), handle_type = 2 (enum).
    private static final int HANDLE_KEY = 1;
    private static final int HANDLE_TYPE = 2;

    // BamlHandleType discriminants for host-owned values (api.rs / baml_handle.proto).
    // These keys live in the Java-side BamlFfi.HOST_VALUES registry, not
    // HANDLE_TABLE, so their handle keys are written verbatim (no clone-for-wire).
    private static final int HOST_VALUE_CALLABLE = 15;
    private static final int HOST_VALUE_OPAQUE = 16;

    /** The synthetic BAML class a host-thrown native exception is encoded as. */
    private static final String HOST_CALLABLE_FQN = "baml.errors.HostCallable";

    // BamlValueMedia / BamlValueMediaFileContent (baml_outbound.proto)
    private static final int MEDIA_KIND = 1;
    private static final int MEDIA_MIME_TYPE = 2;
    private static final int MEDIA_URL = 3;
    private static final int MEDIA_BASE64 = 4;
    private static final int MEDIA_FILE_CONTENT = 6;
    private static final int FILE_CONTENT_NAME = 1;
    private static final int FILE_CONTENT_BASE64 = 2;

    // InboundListValue / InboundMapValue
    private static final int LIST_VALUES = 1;
    private static final int MAP_ENTRIES = 1;

    // InboundClassValue (field 1 reserved) / BamlTy / BamlTyClass / InboundEnumValue
    private static final int CLASS_FIELDS = 2;
    private static final int TY_CLASS = 2;
    private static final int TY_CLASS_NAME = 1;
    private static final int TY_CLASS_TYPE_ARGS = 2; // BamlTyClass.type_args (repeated BamlTy)
    private static final int ENUM_NAME = 1;
    private static final int ENUM_VALUE = 2;

    private static final BigInteger LONG_MIN = BigInteger.valueOf(Long.MIN_VALUE);
    private static final BigInteger LONG_MAX = BigInteger.valueOf(Long.MAX_VALUE);

    private ProtoWriter() {}

    /**
     * Encode positional args (paired with their declared parameter names) plus a
     * nonzero {@code call_id} into {@code CallFunctionArgs} bytes, with no
     * explicit-generics bindings. Byte-identical to the four-arg overload with a
     * {@code null} bag.
     */
    public static byte[] encodeCallFunctionArgs(String[] names, Object[] args, long callId) {
        return encodeCallFunctionArgs(names, args, callId, null);
    }

    /**
     * As {@link #encodeCallFunctionArgs(String[], Object[], long)}, but also
     * encodes an optional {@code typeArgs} bag as {@code CallFunctionArgs.type_args}
     * — one {@code BamlTyArg{type_var, type_value}} per binding, in the bag's
     * insertion (De Bruijn) order. A {@code null} or empty bag writes no
     * {@code type_args} field, so the output is byte-identical to the pre-generics
     * encoding (the regression that non-generic callers depend on). The engine
     * rejects a zero {@code call_id}, so the caller must mint one via
     * {@code BamlFfi.nativeNewCallId()}.
     */
    public static byte[] encodeCallFunctionArgs(
            String[] names, Object[] args, long callId, BamlTypes typeArgs) {
        WireWriter invocation = new WireWriter();
        invocation.writeInt64(4, InvocationFrames.currentState()); invocation.writeInt64(5, callId);
        return encodeCallFunctionArgs(names, args, callId, typeArgs, invocation.toByteArray());
    }
    public static byte[] encodeCallFunctionArgs(String[] names, Object[] args, long callId, BamlTypes typeArgs, byte[] preparedInvocation) {
        if (names.length != args.length) {
            throw new IllegalArgumentException(
                    "names/args length mismatch: " + names.length + " vs " + args.length);
        }
        WireWriter w = new WireWriter();
        for (int i = 0; i < names.length; i++) {
            try {
                w.writeMessage(CALL_ARGS_KWARGS, encodeMapEntry(names[i], args[i]));
            } catch (UnsupportedInboundTypeException e) {
                // Rewrap the deep rejection so it names the *top-level* argument
                // (the parameter from names[]), not the nested position — an
                // unsupported list element or class field inside argument `x`
                // still reports `x`, mirroring bridge_python's TypeError naming
                // the kwarg (proto.py). The offending Java type carries through
                // in the message (and the original as the cause). The message
                // reads "cannot encode argument '<name>': …" — an encode-time
                // rejection naming the top-level kwarg (Python parity: "Cannot
                // encode …" + naming the kwarg).
                throw new IllegalArgumentException(
                        "cannot encode argument '" + names[i] + "': " + e.getMessage(), e);
            }
        }
        w.writeInt64(CALL_ARGS_CALL_ID, callId);
        w.writeMessage(6, preparedInvocation);
        if (typeArgs != null && !typeArgs.isEmpty()) {
            for (Map.Entry<String, BamlType> binding : typeArgs.bindings()) {
                w.writeMessage(CALL_ARGS_TYPE_ARGS, encodeTypeArg(binding.getKey(), binding.getValue()));
            }
        }
        return w.toByteArray();
    }

    public static byte[] encodeNamedCallFunctionArgs(
            String functionName,
            String[] names,
            Object[] args,
            long callId,
            BamlTypes typeArgs) {
        return encodeNamedCallFunctionArgs(functionName, names, args, callId, typeArgs, null);
    }
    public static byte[] encodeNamedCallFunctionArgs(String functionName, String[] names, Object[] args, long callId, BamlTypes typeArgs, byte[] preparedInvocation) {
        WireWriter w = new WireWriter();
        w.writeRawBytes(preparedInvocation == null ? encodeCallFunctionArgs(names, args, callId, typeArgs) : encodeCallFunctionArgs(names, args, callId, typeArgs, preparedInvocation));
        w.writeString(CALL_ARGS_FUNCTION_NAME, functionName);
        return w.toByteArray();
    }

    public static byte[] encodeHandleCallFunctionArgs(
            long functionHandle, String[] names, Object[] args, long callId) {
        return encodeHandleCallFunctionArgs(functionHandle, names, args, callId, null);
    }
    public static byte[] encodeHandleCallFunctionArgs(long functionHandle, String[] names, Object[] args, long callId, byte[] preparedInvocation) {
        if (functionHandle <= 0) {
            throw new IllegalArgumentException("function handle must be positive");
        }
        WireWriter w = new WireWriter();
        w.writeRawBytes(preparedInvocation == null ? encodeCallFunctionArgs(names, args, callId, null) : encodeCallFunctionArgs(names, args, callId, null, preparedInvocation));
        w.writeInt64(CALL_ARGS_FUNCTION_HANDLE, functionHandle);
        return w.toByteArray();
    }

    /** One {@code BamlTyArg}: TypeVar name on {@code type_var}, the lowered type on {@code type_value}. */
    private static byte[] encodeTypeArg(String typeVar, BamlType type) {
        WireWriter arg = new WireWriter();
        arg.writeString(TY_ARG_TYPE_VAR, typeVar);
        arg.writeMessage(TY_ARG_TYPE_VALUE, type.toWireTy());
        return arg.toByteArray();
    }

    /** One {@code InboundMapEntry} with a string key and (unless null) a value. */
    private static byte[] encodeMapEntry(String key, Object value) {
        return encodeMapEntry(key, value, null);
    }

    private static byte[] encodeMapEntry(String key, Object value, BamlType contextualType) {
        WireWriter entry = new WireWriter();
        entry.writeString(MAP_ENTRY_STRING_KEY, key);
        // A null value leaves the `value` field absent, which the engine reads
        // as null (an unset oneof ≡ null).
        if (value != null) {
            entry.writeMessage(MAP_ENTRY_VALUE, encodeInboundValue(value, contextualType));
        }
        return entry.toByteArray();
    }

    /**
     * Encode a single {@code InboundValue}. A null argument returns an empty
     * message (no oneof arm set) — for list items and map values, an empty
     * {@code InboundValue} decodes to null while still preserving position.
     */
    public static byte[] encodeInboundValue(Object value) {
        if (value instanceof baml_bridge.BamlTypedValue typed) {
            return encodeInboundValue(typed.value(), typed.type());
        }
        return encodeInboundValue(value, null);
    }

    /**
     * Encode {@code value} for a slot of {@code contextualType} ({@code null}
     * when the binding supplies none). The slot's type is what a container
     * states where it fixes one; otherwise the node states its own.
     */
    private static byte[] encodeInboundValue(Object value, BamlType contextualType) {
        if (value instanceof baml_bridge.BamlTypedValue typed) {
            return encodeInboundValue(typed.value(), typed.type());
        }
        WireWriter w = new WireWriter();
        if (value == null) {
            return w.toByteArray(); // absent oneof ≡ null
        }
        while (contextualType != null && contextualType.kind() == BamlType.Kind.OPTIONAL) {
            contextualType = contextualType.optionalInner();
        }
        if (contextualType != null
                && contextualType.kind() == BamlType.Kind.UNION
                && (value instanceof baml_bridge.BamlUnion || TypeRegistry.isUnionRecord(value))) {
            int arm = value instanceof baml_bridge.BamlUnion
                    ? genericUnionArmIndex(value)
                    : TypeRegistry.unionRecordArmIndex(value);
            List<BamlType> options = contextualType.unionOptions();
            if (arm < 0 || arm >= options.size()) {
                throw new IllegalArgumentException(
                        "Java union arm index " + arm + " is out of range for " + contextualType);
            }
            BamlType selected = options.get(arm);
            Object inner = value instanceof baml_bridge.BamlUnion
                    ? unwrapGenericUnion(value)
                    : TypeRegistry.unionRecordInner(value);
            return encodeInboundValue(inner, selected);
        }
        // The type a list or map node states; everything else implies its own
        // or states it where it is written.
        BamlType statedType = null;
        // Only a JDK proxy can be a returned BAML closure. Checking first keeps
        // pure encoding from initializing BamlFfi, which loads the native library.
        baml_bridge.BamlHandle returnedHandle =
                value != null && java.lang.reflect.Proxy.isProxyClass(value.getClass())
                        ? baml_bridge.BamlFfi.returnedClosureHandle(value)
                        : null;
        if (returnedHandle != null) {
            return encodeInboundValue(returnedHandle, contextualType);
        }
        // bool must precede the integer arms (mirrors Python's isinstance order).
        if (value instanceof Boolean b) {
            w.writeBool(IV_BOOL, b);
        } else if (value instanceof Long l) {
            w.writeInt64(IV_INT, l);
        } else if (value instanceof Integer || value instanceof Short || value instanceof Byte) {
            w.writeInt64(IV_INT, ((Number) value).longValue());
        } else if (value instanceof BigInteger bi) {
            // In i64 range → int_value; otherwise the hex-string bigint channel
            // (lowercase, sign-prefixed; matches num-bigint's `{bi:x}`).
            if (bi.compareTo(LONG_MIN) >= 0 && bi.compareTo(LONG_MAX) <= 0) {
                w.writeInt64(IV_INT, bi.longValue());
            } else {
                w.writeString(IV_BIGINT, bi.toString(16));
            }
        } else if (value instanceof Double d) {
            w.writeDouble(IV_FLOAT, d);
        } else if (value instanceof Float f) {
            w.writeDouble(IV_FLOAT, f.doubleValue());
        } else if (value instanceof String s) {
            w.writeString(IV_STRING, s);
        } else if (value instanceof byte[] bytes) {
            w.writeBytes(IV_UINT8ARRAY, bytes);
        } else if (value instanceof List<?> list) {
            boolean slotIsList = contextualType != null && contextualType.kind() == BamlType.Kind.LIST;
            w.writeMessage(IV_LIST, encodeList(list, slotIsList ? contextualType.listItem() : null));
            statedType = slotIsList && isExact(contextualType) ? contextualType : naturalType(list);
        } else if (value instanceof Map<?, ?> map) {
            boolean slotIsMap = contextualType != null && contextualType.kind() == BamlType.Kind.MAP;
            w.writeMessage(IV_MAP, encodeMap(map, slotIsMap ? contextualType.mapValue() : null));
            statedType = slotIsMap && isExact(contextualType) ? contextualType : naturalType(map);
        } else if (value instanceof Enum<?> constant) {
            // Any Java enum: encode it only if its type is a registered generated
            // enum; an unregistered enum is not a BAML value.
            TypeRegistry.EnumWire ew = TypeRegistry.enumWire(constant);
            if (ew == null) {
                throw unsupported(value);
            }
            w.writeMessage(IV_ENUM, encodeEnum(ew));
        } else if (value instanceof baml_bridge.BamlMedia media) {
            // Media crosses as its portable value, read back out of the native
            // handle the Java wrapper holds; the kind implies the type.
            w.writeMessage(IV_MEDIA, encodeMedia(media));
        } else if (value instanceof baml_bridge.BamlPrompt prompt) {
            // ai.Prompt is portable rather than handle-backed. Preserve the
            // canonical prompt AST bytes copied from the outbound boundary so
            // the same host value can be passed into another ordinary BAML
            // call (or another runtime) without consulting the handle table.
            w.writeMessage(IV_PROMPT_AST, prompt.bamlWireCopy());
        } else if (value instanceof baml_bridge.BamlFunctionSpec<?> spec) {
            // FunctionSpec is an engine-owned capability. Its host wrapper is
            // transparent on the inbound boundary, just like BamlStream: clone
            // the owned handle key so the engine may drain the wire copy while
            // the Java object remains reusable.
            return encodeInboundValue(spec.bamlHandle(), contextualType);
        } else if (value instanceof baml_bridge.BamlStream stream) {
            // BamlStream (ai.stream.Stream receiver): lifted to a bare
            // handle_value(ADT_TAGGED_HEAP_HANDLE) on the wire — the engine
            // reconstructs the heap pointer from the receiver's HANDLE_TABLE row
            // (Adt(TaggedHeapHandle)). Mirrors bridge_python's
            // `isinstance(value, BamlStream)` branch, which recurses on the inner
            // handle (proto.py). Delegate to the BamlHandle arm below, which
            // clones the key per the drain contract; the inner handle already
            // carries handle_type = ADT_TAGGED_HEAP_HANDLE.
            return encodeInboundValue(stream.bamlHandle(), contextualType);
        } else if (value instanceof baml_bridge.BamlHandle handle) {
            // A bare engine handle (a $rust_type shell's private field, e.g.
            // baml.fs.File `_handle` / baml.http.Response `_body`): an
            // InboundValue.handle carrying BamlHandle{key, handle_type}. The
            // key is a fresh clone — the engine drains its copy on decode
            // while the Java shell keeps its own row (same contract as the
            // media `_data` handle).
            WireWriter handleMsg = new WireWriter();
            handleMsg.writeInt64(HANDLE_KEY, handle.cloneKeyForWire());
            handleMsg.writeInt64(HANDLE_TYPE, handle.handleType());
            w.writeMessage(IV_HANDLE, handleMsg.toByteArray());
        } else if (value instanceof baml_bridge.BamlUnion) {
            // Generic-family arm record (Union2..Union10): unwrap to the
            // single `value()` component — no union envelope inbound.
            return encodeInboundValue(unwrapGenericUnion(value), contextualType);
        } else if (TypeRegistry.isUnionRecord(value)) {
            // A union wrapper record carries no wrapper on the inbound wire:
            // unwrap to its bare inner value and encode that (inbound has no
            // union arm — the engine re-validates the union at the boundary).
            return encodeInboundValue(TypeRegistry.unionRecordInner(value), contextualType);
        } else if (isHostCallable(value)) {
            // A host callable (a java.util.function.* shape, or a generated
            // @FunctionalInterface marked with baml_bridge.BamlHostCallable) has
            // no type of its own: it takes the function type the generated
            // carrier or the slot gives it, states that type beside its handle,
            // and is registered in the Java-side host-value table. The engine
            // binds it to an Object::HostClosure and dispatches back into the
            // host when BAML invokes it.
            BamlType functionType = value instanceof baml_bridge.BamlTypedCallable typed
                    ? typed.functionType()
                    : contextualType != null && contextualType.kind() == BamlType.Kind.FUNCTION
                            ? contextualType
                            : null;
            if (functionType == null) {
                throw new UnsupportedInboundTypeException(
                        "a callable has no type of its own; declare the slot as a function type");
            }
            if (!isExact(functionType)) {
                throw new UnsupportedInboundTypeException(
                        "a callable's type " + functionType + " is not fully bound; pass the "
                                + "call's type arguments explicitly");
            }
            w.writeMessage(IV_VALUE_TYPE, functionType.toWireTy());
            long key = baml_bridge.BamlFfi.registerHostCallable(value);
            WireWriter handleMsg = new WireWriter();
            handleMsg.writeInt64(HANDLE_KEY, key);
            handleMsg.writeInt64(HANDLE_TYPE, HOST_VALUE_CALLABLE);
            w.writeMessage(IV_HANDLE, handleMsg.toByteArray());
        } else {
            // A registered generated class encodes as a class_value; anything
            // else (handle / media / host-callable / arbitrary object) is not
            // part of this slice.
            TypeRegistry.ClassWire cw = TypeRegistry.classWire(value);
            if (cw == null) {
                throw unsupported(value);
            }
            // A class node states the slot's class type where the slot fixes
            // one, and its fields are encoded under that type's arguments;
            // otherwise it states the instance's own type.
            List<BamlType> typeArgs;
            if (contextualType != null
                    && contextualType.kind() == BamlType.Kind.CLASS
                    && isExact(contextualType)) {
                w.writeMessage(IV_VALUE_TYPE, contextualType.toWireTy());
                typeArgs = contextualType.classTypeArgs();
            } else {
                typeArgs = instanceTypeArgs(value, cw);
                writeClassType(w, cw.fqn, typeArgs);
            }
            w.writeMessage(IV_CLASS, encodeClass(cw, typeArgs));
        }
        // A container's natural type can mention a type variable (from a slot
        // whose binding is open) or be unknown to the token grammar (an element
        // with no BAML type the token can spell): such a node stays unstated.
        if (statedType != null && isExact(statedType)) {
            WireWriter stated = new WireWriter();
            stated.writeMessage(IV_VALUE_TYPE, statedType.toWireTy());
            stated.writeRawBytes(w.toByteArray());
            return stated.toByteArray();
        }
        return w.toByteArray();
    }

    /** No {@code TYPEVAR} anywhere in the token: exactly the tokens the wire can spell. */
    private static boolean isExact(BamlType type) {
        try {
            type.toWireTy();
            return true;
        } catch (IllegalStateException decodeOnlyType) {
            return false;
        }
    }

    /**
     * The BAML type a Java value has on its own, as the engine would see it: the
     * concrete type of a value under a slot that fixes nothing. A container's
     * element type is the union of its elements' types (duplicates collapsed,
     * one element's type as is), and an empty container has element type
     * {@code never}. {@code null} when some part of the value has no BAML type
     * the token grammar can spell (bytes, media, handles); the node is then left
     * unstated.
     */
    private static BamlType naturalType(Object value) {
        if (value instanceof Boolean) {
            return BamlType.BOOL;
        }
        if (value instanceof Long || value instanceof Integer || value instanceof Short || value instanceof Byte) {
            return BamlType.INT;
        }
        if (value instanceof BigInteger bi) {
            return bi.compareTo(LONG_MIN) >= 0 && bi.compareTo(LONG_MAX) <= 0 ? BamlType.INT : BamlType.BIGINT;
        }
        if (value instanceof Double || value instanceof Float) {
            return BamlType.FLOAT;
        }
        if (value instanceof String) {
            return BamlType.STRING;
        }
        if (value instanceof List<?> list) {
            BamlType item = naturalElementType(list);
            return item == null ? null : BamlType.list(item);
        }
        if (value instanceof Map<?, ?> map) {
            BamlType item = naturalElementType(map.values());
            return item == null ? null : BamlType.map(BamlType.STRING, item);
        }
        if (value instanceof Enum<?> constant) {
            TypeRegistry.EnumWire ew = TypeRegistry.enumWire(constant);
            return ew == null ? null : BamlType.of(constant.getDeclaringClass());
        }
        if (value instanceof baml_bridge.BamlUnion) {
            return naturalType(unwrapGenericUnion(value));
        }
        if (TypeRegistry.isUnionRecord(value)) {
            // A record of a declared alias (`baml.json.json`) has the alias as
            // its type; a record of an anonymous union has its inner value's.
            String alias = TypeRegistry.unionAliasFqn(value);
            return alias != null ? BamlType.typeAlias(alias) : naturalType(TypeRegistry.unionRecordInner(value));
        }
        TypeRegistry.ClassWire cw = value == null ? null : TypeRegistry.classWire(value);
        if (cw != null) {
            return BamlType.classByFqn(cw.fqn, instanceTypeArgs(value, cw).toArray(new BamlType[0]));
        }
        return null;
    }

    /**
     * The type arguments of an instance of a generic class: the ones bound when
     * it was decoded, else the ones its field values determine. Java erases a
     * class's type arguments at run time, so an instance built in Java states
     * what its fields make of each parameter ({@code new GenericBox<>(5L)} is a
     * {@code GenericBox<int>}), as the untyped hosts do. A parameter no field
     * determines leaves the instance with no type to state, and the caller
     * names it.
     */
    private static List<BamlType> instanceTypeArgs(Object value, TypeRegistry.ClassWire cw) {
        List<BamlType> bound = TypeRegistry.typeArgsOf(value);
        if (!bound.isEmpty() || cw.typeParams == null || cw.typeParams.length == 0) {
            return bound;
        }
        Map<String, BamlType> bindings = new java.util.HashMap<>();
        if (cw.fieldDescs != null) {
            for (int i = 0; i < cw.fieldNames.length; i++) {
                BamlType desc = cw.fieldDescs[i];
                Object fieldValue = cw.fieldValues[i];
                if (desc == null || fieldValue == null) {
                    continue;
                }
                BamlType natural = naturalType(fieldValue);
                if (natural != null) {
                    unify(desc, natural, bindings);
                }
            }
        }
        List<BamlType> solved = new java.util.ArrayList<>(cw.typeParams.length);
        for (String param : cw.typeParams) {
            BamlType binding = bindings.get(param);
            if (binding == null) {
                throw new IllegalArgumentException(
                        "the fields of this " + cw.fqn + " do not determine its type argument `"
                                + param + "`; pass the call's type arguments");
            }
            solved.add(binding);
        }
        return solved;
    }

    /**
     * Bind the type variables of the field descriptor {@code desc} from the
     * natural type {@code natural} of the field's value, as the shared walker's
     * {@code unify} does: an empty container binds {@code never} (the exact
     * type of {@code []}), two fields that disagree about a variable are an
     * error, and a union descriptor gives no evidence.
     */
    private static void unify(BamlType desc, BamlType natural, Map<String, BamlType> bindings) {
        switch (desc.kind()) {
            case TYPEVAR -> {
                BamlType bound = bindings.putIfAbsent(desc.fqn(), natural);
                if (bound != null && !bound.equals(natural)) {
                    throw new IllegalArgumentException(
                            "the fields of this instance disagree about its type argument `"
                                    + desc.fqn() + "`: " + bound + " and " + natural);
                }
            }
            case OPTIONAL -> unify(
                    desc.optionalInner(),
                    natural.kind() == BamlType.Kind.OPTIONAL ? natural.optionalInner() : natural,
                    bindings);
            case LIST -> {
                if (natural.kind() == BamlType.Kind.LIST) {
                    unify(desc.listItem(), natural.listItem(), bindings);
                }
            }
            case MAP -> {
                if (natural.kind() == BamlType.Kind.MAP) {
                    unify(desc.mapKey(), natural.mapKey(), bindings);
                    unify(desc.mapValue(), natural.mapValue(), bindings);
                }
            }
            case CLASS -> {
                if (natural.kind() == BamlType.Kind.CLASS
                        && java.util.Objects.equals(desc.fqn(), natural.fqn())
                        && desc.classTypeArgs().size() == natural.classTypeArgs().size()) {
                    for (int i = 0; i < desc.classTypeArgs().size(); i++) {
                        unify(desc.classTypeArgs().get(i), natural.classTypeArgs().get(i), bindings);
                    }
                }
            }
            default -> {
                // A union, literal, primitive or named leaf fixes no variable.
            }
        }
    }

    /**
     * The one type every element of a container inhabits: a null element is
     * {@code null}'s type folded in as an optional, several element types form a
     * union in first-seen order, and no elements give {@code never}.
     */
    private static BamlType naturalElementType(java.util.Collection<?> elements) {
        java.util.LinkedHashSet<BamlType> types = new java.util.LinkedHashSet<>();
        boolean nullable = false;
        for (Object element : elements) {
            if (element == null) {
                nullable = true;
                continue;
            }
            BamlType type = naturalType(element);
            if (type == null) {
                return null;
            }
            types.add(type);
        }
        BamlType item;
        if (types.isEmpty()) {
            item = BamlType.NEVER;
        } else if (types.size() == 1) {
            item = types.iterator().next();
        } else {
            item = BamlType.union(types.toArray(new BamlType[0]));
        }
        return nullable ? BamlType.optional(item) : item;
    }

    private static int genericUnionArmIndex(Object value) {
        String simpleName = value.getClass().getSimpleName();
        if (!simpleName.startsWith("Arm")) {
            return -1;
        }
        try {
            return Integer.parseInt(simpleName.substring(3));
        } catch (NumberFormatException e) {
            return -1;
        }
    }

    /**
     * Encode an {@code InboundClassValue}: one {@code fields} entry per registry
     * field (key = field name, value = the accessor-read value, recursed through
     * {@link #encodeInboundValue}). Nominal identity is carried by the enclosing
     * {@code InboundValue.value_type}, never by this payload.
     */
    private static byte[] encodeClass(TypeRegistry.ClassWire cw, List<BamlType> typeArgs) {
        // A field declared in terms of the class's type parameters is encoded
        // under the type the instance's bound arguments make of it: `T[]` is
        // `int[]` for a `Box<int>`, so the field needs no statement of its own.
        Map<String, BamlType> bindings = Map.of();
        if (cw.typeParams != null && typeArgs.size() == cw.typeParams.length) {
            bindings = new java.util.HashMap<>();
            for (int i = 0; i < cw.typeParams.length; i++) {
                bindings.put(cw.typeParams[i], typeArgs.get(i));
            }
        }
        WireWriter w = new WireWriter();
        for (int i = 0; i < cw.fieldNames.length; i++) {
            BamlType fieldDesc = cw.fieldDescs == null ? null : cw.fieldDescs[i];
            if (fieldDesc != null && !bindings.isEmpty()) {
                fieldDesc = fieldDesc.substitute(bindings);
            }
            w.writeMessage(
                    CLASS_FIELDS,
                    encodeMapEntry(cw.fieldNames[i], cw.fieldValues[i], fieldDesc));
        }
        return w.toByteArray();
    }

    /** Write {@code InboundValue.value_type = class<FQN, args...>}. */
    private static void writeClassType(WireWriter value, String fqn, List<BamlType> typeArgs) {
        WireWriter classTy = new WireWriter();
        classTy.writeString(TY_CLASS_NAME, fqn);
        for (BamlType arg : typeArgs) {
            classTy.writeMessage(TY_CLASS_TYPE_ARGS, arg.toWireTy());
        }
        WireWriter ty = new WireWriter();
        ty.writeMessage(TY_CLASS, classTy.toByteArray());
        value.writeMessage(IV_VALUE_TYPE, ty.toByteArray());
    }

    /**
     * Encode a media value as the portable {@code BamlValueMedia} the engine
     * reads for every host: its kind, its MIME type and the one source it has
     * (a URL, a base64 payload, or file content with its name). The payload is
     * read out of the native handle the Java wrapper holds, which is neither
     * cloned nor drained.
     */
    private static byte[] encodeMedia(baml_bridge.BamlMedia media) {
        int kind = switch (media.bamlFqn()) {
            case "baml.media.Image" -> 1;
            case "baml.media.Audio" -> 2;
            case "baml.media.Pdf" -> 3;
            case "baml.media.Video" -> 4;
            default -> throw new IllegalArgumentException("unknown BAML media type " + media.bamlFqn());
        };
        baml_bridge.BamlHandle handle = media.bamlHandle();
        WireWriter w = new WireWriter();
        w.writeInt64(MEDIA_KIND, kind);
        String mimeType = handle.mediaMimeType();
        if (mimeType != null) {
            w.writeString(MEDIA_MIME_TYPE, mimeType);
        }
        String url = handle.mediaUrl();
        String name = handle.mediaName();
        if (url != null) {
            w.writeString(MEDIA_URL, url);
        } else if (name != null) {
            WireWriter content = new WireWriter();
            content.writeString(FILE_CONTENT_NAME, name);
            content.writeString(FILE_CONTENT_BASE64, handle.mediaBase64());
            w.writeMessage(MEDIA_FILE_CONTENT, content.toByteArray());
        } else {
            w.writeString(MEDIA_BASE64, handle.mediaBase64());
        }
        return w.toByteArray();
    }

    /** Encode an {@code InboundEnumValue}: FQN on {@code name}, variant on {@code value}. */
    private static byte[] encodeEnum(TypeRegistry.EnumWire ew) {
        WireWriter w = new WireWriter();
        w.writeString(ENUM_NAME, ew.fqn);
        w.writeString(ENUM_VALUE, ew.wireName);
        return w.toByteArray();
    }

    /**
     * The encoder cannot put a Java value on the wire: no inbound arm takes it,
     * or it is a callable with no function type to take. The top-level kwarg
     * loop ({@link #encodeCallFunctionArgs}) rewraps it into an
     * {@link IllegalArgumentException} that also names the owning argument
     * (mirroring bridge_python's {@code TypeError} naming the kwarg). Extends
     * {@link IllegalArgumentException} so a direct {@link #encodeInboundValue}
     * call — with no owning argument to name — still surfaces the documented
     * exception type, just without the argument prefix.
     */
    static final class UnsupportedInboundTypeException extends IllegalArgumentException {
        private static final long serialVersionUID = 1L;

        UnsupportedInboundTypeException(String reason) {
            super(reason);
        }
    }

    private static UnsupportedInboundTypeException unsupported(Object value) {
        return new UnsupportedInboundTypeException(
                "unsupported Java type " + value.getClass().getName());
    }


    private static Object unwrapGenericUnion(Object value) {
        try {
            return value.getClass().getMethod("value").invoke(value);
        } catch (ReflectiveOperationException e) {
            throw new IllegalStateException(
                    "BamlUnion arm record without a value() accessor: " + value.getClass(), e);
        }
    }

    private static byte[] encodeList(List<?> list, BamlType itemType) {
        WireWriter w = new WireWriter();
        for (Object item : list) {
            // Always emit an entry (even for a null item) to preserve length.
            w.writeMessage(LIST_VALUES, encodeInboundValue(item, itemType));
        }
        return w.toByteArray();
    }

    private static byte[] encodeMap(Map<?, ?> map, BamlType valueType) {
        WireWriter w = new WireWriter();
        for (Map.Entry<?, ?> e : map.entrySet()) {
            // The engine stringifies map keys; generated maps are Map<String, V>.
            w.writeMessage(
                    MAP_ENTRIES,
                    encodeMapEntry(String.valueOf(e.getKey()), e.getValue(), valueType));
        }
        return w.toByteArray();
    }

    /**
     * Whether {@code value} is a host callable the bridge can dispatch back into.
     * Covers the generated {@link baml_bridge.BamlHostCallable} functional
     * interfaces (callables with optionals / arity &gt; 2) plus the
     * {@code java.util.function.*} shapes codegen uses for plain arity-&le;-2
     * callables, and the {@link baml_bridge.BamlTypedCallable} carrier the
     * generated SDK wraps a callable argument in (registered whole, so the
     * dispatch path can thread its descriptors). Mirrors bridge_python's "any
     * callable" test — an object reaching the encoder here is a BAML function
     * argument, so a functional value in a callable-typed slot is a host
     * callable.
     */
    private static boolean isHostCallable(Object value) {
        return value instanceof baml_bridge.BamlTypedCallable
                || value instanceof baml_bridge.BamlHostCallable
                || value instanceof java.util.function.Function
                || value instanceof java.util.function.BiFunction
                || value instanceof java.util.function.Supplier
                || value instanceof java.util.function.Consumer
                || value instanceof java.util.function.BiConsumer
                || value instanceof Runnable;
    }

    /**
     * Encode an {@code InboundValue} carrying a {@code baml.errors.HostCallable}
     * instance for a native host exception thrown inside a callable (the opaque
     * path, design point D). Fields: {@code message}, {@code class_name} (the
     * Java simple name), {@code language} ({@code "java"}), and a hidden
     * {@code _handle} = {@code Handle{key, HOST_VALUE_OPAQUE}} referencing the
     * original {@code Throwable} in the Java-side registry — the outbound decoder
     * rehydrates it by identity on same-runtime round trip
     * ({@link ProtoReader}). Mirrors bridge_python's
     * {@code build_host_callable_inbound} (host_value.rs).
     */
    public static byte[] encodeHostCallableError(
            String className, String message, String traceback, long handleKey) {
        WireWriter classBody = new WireWriter();
        classBody.writeMessage(CLASS_FIELDS, stringFieldEntry("message", message));
        classBody.writeMessage(CLASS_FIELDS, stringFieldEntry("class_name", className));
        classBody.writeMessage(CLASS_FIELDS, stringFieldEntry("language", "java"));
        // `traceback` is `string?` on the class but must be PRESENT on the wire
        // (the engine rejects an Instance with a missing field). Always emit it.
        classBody.writeMessage(CLASS_FIELDS, stringFieldEntry("traceback", traceback));

        // The hidden `_handle` field: an InboundValue.handle{key, HOST_VALUE_OPAQUE}.
        WireWriter handleMsg = new WireWriter();
        handleMsg.writeInt64(HANDLE_KEY, handleKey);
        handleMsg.writeInt64(HANDLE_TYPE, HOST_VALUE_OPAQUE);
        WireWriter handleValue = new WireWriter();
        handleValue.writeMessage(IV_HANDLE, handleMsg.toByteArray());
        WireWriter handleEntry = new WireWriter();
        handleEntry.writeString(MAP_ENTRY_STRING_KEY, "_handle");
        handleEntry.writeMessage(MAP_ENTRY_VALUE, handleValue.toByteArray());
        classBody.writeMessage(CLASS_FIELDS, handleEntry.toByteArray());

        WireWriter iv = new WireWriter();
        writeClassType(iv, HOST_CALLABLE_FQN, List.of());
        iv.writeMessage(IV_CLASS, classBody.toByteArray());
        return iv.toByteArray();
    }

    /** One {@code InboundMapEntry} with a string key and a string value. */
    private static byte[] stringFieldEntry(String key, String value) {
        WireWriter fieldValue = new WireWriter();
        fieldValue.writeString(IV_STRING, value);
        WireWriter entry = new WireWriter();
        entry.writeString(MAP_ENTRY_STRING_KEY, key);
        entry.writeMessage(MAP_ENTRY_VALUE, fieldValue.toByteArray());
        return entry.toByteArray();
    }
}
