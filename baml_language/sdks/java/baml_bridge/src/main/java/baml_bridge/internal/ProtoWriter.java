package baml_bridge.internal;

import baml_bridge.BamlType;
import baml_bridge.BamlTypes;
import baml_bridge.TypeRegistry;

import java.math.BigInteger;
import java.util.List;
import java.util.Map;

/**
 * Encodes host arguments into a {@code CallFunctionArgs} protobuf
 * ({@code baml_inbound.proto}) for the primitives slice. The encoder
 * dispatches on the Java runtime shape of each argument — never on the
 * declared BAML parameter type — exactly like the Python bridge's
 * {@code _set_inbound_value} (proto.py §09d). Rust re-runs BAML type checking
 * after decoding, so structural mismatches surface as a {@code BamlError}.
 *
 * <h2>Field numbers (against {@code baml_inbound.proto})</h2>
 * <pre>
 * CallFunctionArgs: kwargs = 1 (repeated InboundMapEntry), call_id = 2 (uint64),
 *                   type_args = 3 (repeated BamlTyArg — explicit-generics bindings)
 * BamlTyArg:        type_var = 1 (string), type_value = 2 (BamlTy)
 * InboundMapEntry:  string_key = 1, value = 6 (InboundValue)
 * InboundValue:       value_type = 1 (sparse exact BamlTy annotation)
 * InboundValue oneof: string_value = 2, int_value = 3, float_value = 4,
 *                     bool_value = 5, list_value = 6, map_value = 7,
 *                     class_value = 8, enum_value = 9,
 *                     uint8array_value = 11, bigint_value = 12
 *                     (handle = 10 / ty_value = 13 are not implemented in this slice)
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
    private static final int CALL_ARGS_OPERATION = 6;

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

    // The single field name a handle-backed media class carries on the wire.
    private static final String MEDIA_DATA_FIELD = "_data";

    // InboundListValue / InboundMapValue
    private static final int LIST_VALUES = 1;
    private static final int MAP_ENTRIES = 1;

    // InboundClassValue (field 1 reserved) / BamlTy / BamlTyClass / InboundEnumValue
    private static final int CLASS_FIELDS = 2;
    private static final int TY_CLASS = 2;
    private static final int TY_MEDIA = 11;
    private static final int TY_CLASS_NAME = 1;
    private static final int TY_CLASS_TYPE_ARGS = 2; // BamlTyClass.type_args (repeated BamlTy)
    private static final int ENUM_NAME = 1;
    private static final int ENUM_VALUE = 2;
    private static final int TY_MEDIA_KIND = 1;

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
                        "cannot encode argument '" + names[i] + "': unsupported Java type "
                                + e.typeName(),
                        e);
            }
        }
        w.writeInt64(CALL_ARGS_CALL_ID, callId);
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
        return encodeNamedCallFunctionArgs(
                functionName,
                names,
                args,
                callId,
                typeArgs,
                baml_bridge.BamlFunctionOperation.DIRECT);
    }

    public static byte[] encodeNamedCallFunctionArgs(
            String functionName,
            String[] names,
            Object[] args,
            long callId,
            BamlTypes typeArgs,
            baml_bridge.BamlFunctionOperation operation) {
        WireWriter w = new WireWriter();
        w.writeRawBytes(encodeCallFunctionArgs(names, args, callId, typeArgs));
        w.writeString(CALL_ARGS_FUNCTION_NAME, functionName);
        if (operation != baml_bridge.BamlFunctionOperation.DIRECT) {
            w.writeInt64(CALL_ARGS_OPERATION, operation.wireValue());
        }
        return w.toByteArray();
    }

    public static byte[] encodeHandleCallFunctionArgs(
            long functionHandle, String[] names, Object[] args, long callId) {
        if (functionHandle <= 0) {
            throw new IllegalArgumentException("function handle must be positive");
        }
        WireWriter w = new WireWriter();
        w.writeRawBytes(encodeCallFunctionArgs(names, args, callId, null));
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
        return encodeMapEntry(key, value, contextualType, false);
    }

    private static byte[] encodeMapEntry(
            String key, Object value, BamlType contextualType, boolean exactContext) {
        WireWriter entry = new WireWriter();
        entry.writeString(MAP_ENTRY_STRING_KEY, key);
        // A null value leaves the `value` field absent, which the engine reads
        // as null (an unset oneof ≡ null).
        if (value != null) {
            entry.writeMessage(
                    MAP_ENTRY_VALUE, encodeInboundValue(value, contextualType, exactContext));
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
            return encodeInboundValue(typed.value(), typed.type(), false);
        }
        return encodeInboundValue(value, null, false);
    }

    private static byte[] encodeInboundValue(
            Object value, BamlType contextualType, boolean selectedArm) {
        if (value instanceof baml_bridge.BamlTypedValue typed) {
            return encodeInboundValue(typed.value(), typed.type(), false);
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
            return encodeInboundValue(inner, selected, true);
        }
        BamlType exactNodeType = selectedArm ? contextualType : null;
        boolean alreadyTyped = false;
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
            BamlType itemType = contextualType != null && contextualType.kind() == BamlType.Kind.LIST
                    ? contextualType.listItem()
                    : null;
            w.writeMessage(IV_LIST, encodeList(list, itemType, selectedArm));
            if (selectedArm
                    && contextualType != null
                    && contextualType.kind() == BamlType.Kind.LIST) {
                exactNodeType = contextualType;
            }
        } else if (value instanceof Map<?, ?> map) {
            BamlType valueType = contextualType != null && contextualType.kind() == BamlType.Kind.MAP
                    ? contextualType.mapValue()
                    : null;
            w.writeMessage(IV_MAP, encodeMap(map, valueType, selectedArm));
            if (selectedArm
                    && contextualType != null
                    && contextualType.kind() == BamlType.Kind.MAP) {
                exactNodeType = contextualType;
            }
        } else if (value instanceof Enum<?> constant) {
            // Any Java enum: encode it only if its type is a registered generated
            // enum; an unregistered enum is not a BAML value.
            TypeRegistry.EnumWire ew = TypeRegistry.enumWire(constant);
            if (ew == null) {
                throw unsupported(value);
            }
            w.writeMessage(IV_ENUM, encodeEnum(ew));
        } else if (value instanceof baml_bridge.BamlMedia media) {
            // Handle-backed media (baml.media.{Image,Audio,Video,Pdf}): a
            // class_value whose only field `_data` carries the engine handle,
            // with the exact media kind on InboundValue.value_type. The
            // class-shaped payload is only the transport for the `_data` handle.
            writeMediaType(w, media.bamlFqn());
            w.writeMessage(IV_CLASS, encodeMediaClass(media));
            alreadyTyped = true;
        } else if (value instanceof baml_bridge.BamlStream stream) {
            // BamlStream (ai.stream.Stream receiver): lifted to a bare
            // handle_value(ADT_TAGGED_HEAP_HANDLE) on the wire — the engine
            // reconstructs the heap pointer from the receiver's HANDLE_TABLE row
            // (Adt(TaggedHeapHandle)). Mirrors bridge_python's
            // `isinstance(value, BamlStream)` branch, which recurses on the inner
            // handle (proto.py). Delegate to the BamlHandle arm below, which
            // clones the key per the drain contract; the inner handle already
            // carries handle_type = ADT_TAGGED_HEAP_HANDLE.
            return encodeInboundValue(stream.bamlHandle(), contextualType, selectedArm);
        } else if (value instanceof baml_bridge.BamlFunctionSpec spec) {
            return encodeInboundValue(spec.bamlHandle(), contextualType, selectedArm);
        } else if (value instanceof baml_bridge.BamlPrompt prompt) {
            w.writeMessage(IV_PROMPT_AST, prompt.bamlWireCopy());
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
            return encodeInboundValue(unwrapGenericUnion(value), contextualType, selectedArm);
        } else if (TypeRegistry.isUnionRecord(value)) {
            // A union wrapper record carries no wrapper on the inbound wire:
            // unwrap to its bare inner value and encode that (inbound has no
            // union arm — the engine re-validates the union at the boundary).
            return encodeInboundValue(TypeRegistry.unionRecordInner(value), contextualType, selectedArm);
        } else if (isHostCallable(value)) {
            // A host callable (a java.util.function.* shape, or a generated
            // @FunctionalInterface marked with baml_bridge.BamlHostCallable):
            // register it in the Java-side host-value table and emit
            // InboundValue.handle{key, HOST_VALUE_CALLABLE}. The engine binds it
            // to an Object::HostClosure and dispatches back into the host when
            // BAML invokes it. Mirrors bridge_python's register_host_callable +
            // Handle{HOST_VALUE_CALLABLE} branch (proto.py).
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
            if (contextualType != null && contextualType.kind() == BamlType.Kind.CLASS) {
                w.writeMessage(IV_VALUE_TYPE, contextualType.toWireTy());
            } else {
                writeClassType(w, cw.fqn, TypeRegistry.typeArgsOf(value));
            }
            w.writeMessage(IV_CLASS, encodeClass(cw));
            alreadyTyped = true;
        }
        if (!alreadyTyped
                && exactNodeType != null
                && exactNodeType.kind() != BamlType.Kind.UNION
                && exactNodeType.kind() != BamlType.Kind.OPTIONAL
                && exactNodeType.kind() != BamlType.Kind.TYPEVAR
                && exactNodeType.kind() != BamlType.Kind.UNKNOWN) {
            byte[] wireType;
            try {
                wireType = exactNodeType.toWireTy();
            } catch (IllegalStateException decodeOnlyType) {
                // A composite containing TypeVar/UNKNOWN is a host-side decode
                // hint, not an exact encodable node type. Context still threads
                // to its children; this node simply stays unannotated.
                return w.toByteArray();
            }
            WireWriter annotated = new WireWriter();
            annotated.writeMessage(IV_VALUE_TYPE, wireType);
            annotated.writeRawBytes(w.toByteArray());
            return annotated.toByteArray();
        }
        return w.toByteArray();
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
    private static byte[] encodeClass(TypeRegistry.ClassWire cw) {
        WireWriter w = new WireWriter();
        for (int i = 0; i < cw.fieldNames.length; i++) {
            BamlType fieldDesc = cw.fieldDescs == null ? null : cw.fieldDescs[i];
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

    private static void writeMediaType(WireWriter value, String fqn) {
        int kind = switch (fqn) {
            case "baml.media.Image" -> 1;
            case "baml.media.Audio" -> 2;
            case "baml.media.Video" -> 3;
            case "baml.media.Pdf" -> 4;
            default -> throw new IllegalArgumentException("unknown BAML media type " + fqn);
        };
        WireWriter media = new WireWriter();
        media.writeInt64(TY_MEDIA_KIND, kind);
        WireWriter ty = new WireWriter();
        ty.writeMessage(TY_MEDIA, media.toByteArray());
        value.writeMessage(IV_VALUE_TYPE, ty.toByteArray());
    }

    /**
     * Encode a handle-backed media value as an {@code InboundClassValue}: a single
     * {@code _data} field whose value is an {@code InboundValue.handle}
     * ({@code BamlHandle{key, handle_type}}). The enclosing
     * {@code InboundValue.value_type} carries the exact media kind. The key is a
     * <em>fresh clone</em> so the engine can
     * {@code drain} its copy on decode while the Java media object keeps its own
     * row (mirrors {@code bridge_python}'s {@code _clone_key_for_wire}).
     */
    private static byte[] encodeMediaClass(baml_bridge.BamlMedia media) {
        baml_bridge.BamlHandle handle = media.bamlHandle();
        long wireKey = handle.cloneKeyForWire();

        WireWriter handleMsg = new WireWriter();
        handleMsg.writeInt64(HANDLE_KEY, wireKey);
        handleMsg.writeInt64(HANDLE_TYPE, handle.handleType());

        WireWriter dataValue = new WireWriter();
        dataValue.writeMessage(IV_HANDLE, handleMsg.toByteArray());

        WireWriter dataEntry = new WireWriter();
        dataEntry.writeString(MAP_ENTRY_STRING_KEY, MEDIA_DATA_FIELD);
        dataEntry.writeMessage(MAP_ENTRY_VALUE, dataValue.toByteArray());

        WireWriter w = new WireWriter();
        w.writeMessage(CLASS_FIELDS, dataEntry.toByteArray());
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
     * The encoder cannot map a Java value to any inbound arm. Carries the
     * offending Java type name so the top-level kwarg loop
     * ({@link #encodeCallFunctionArgs}) can rewrap it into an
     * {@link IllegalArgumentException} that also names the owning argument
     * (mirroring bridge_python's {@code TypeError} naming the kwarg). Extends
     * {@link IllegalArgumentException} so a direct {@link #encodeInboundValue}
     * call — with no owning argument to name — still surfaces the documented
     * "unsupported argument" exception type, just without the argument prefix.
     */
    static final class UnsupportedInboundTypeException extends IllegalArgumentException {
        private static final long serialVersionUID = 1L;

        private final String typeName;

        UnsupportedInboundTypeException(String typeName) {
            super("unsupported Java type " + typeName);
            this.typeName = typeName;
        }

        String typeName() {
            return typeName;
        }
    }

    private static UnsupportedInboundTypeException unsupported(Object value) {
        return new UnsupportedInboundTypeException(value.getClass().getName());
    }


    private static Object unwrapGenericUnion(Object value) {
        try {
            return value.getClass().getMethod("value").invoke(value);
        } catch (ReflectiveOperationException e) {
            throw new IllegalStateException(
                    "BamlUnion arm record without a value() accessor: " + value.getClass(), e);
        }
    }

    private static byte[] encodeList(
            List<?> list, BamlType itemType, boolean exactContext) {
        WireWriter w = new WireWriter();
        for (Object item : list) {
            // Always emit an entry (even for a null item) to preserve length.
            w.writeMessage(LIST_VALUES, encodeInboundValue(item, itemType, exactContext));
        }
        return w.toByteArray();
    }

    private static byte[] encodeMap(
            Map<?, ?> map, BamlType valueType, boolean exactContext) {
        WireWriter w = new WireWriter();
        for (Map.Entry<?, ?> e : map.entrySet()) {
            // The engine stringifies map keys; generated maps are Map<String, V>.
            w.writeMessage(
                    MAP_ENTRIES,
                    encodeMapEntry(
                            String.valueOf(e.getKey()), e.getValue(), valueType, exactContext));
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
