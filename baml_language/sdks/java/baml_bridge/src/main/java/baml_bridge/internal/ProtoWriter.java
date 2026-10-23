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
 * {@code (int | string)[]}, and an empty list under {@code json} as the alias's
 * list member {@code json[]}), and the type the value itself has where no slot
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

    /**
     * How many nested protobuf messages the engine decodes before it refuses a
     * message as too deep: prost's recursion limit, {@code WIRE_NESTING_LIMIT}
     * in {@code bridge_ctypes::wire_depth}.
     */
    static final int WIRE_NESTING_LIMIT = 100;

    /**
     * The deepest a value's lists, maps and classes may nest. Each costs the
     * wire at least two messages (its own and each item's {@code InboundValue};
     * a map entry or class field adds an {@code InboundMapEntry}), so a value
     * nested deeper can never cross, as an argument or as a callback's result.
     * Refusing it at the level that exceeds the bound also bounds the encoder's
     * recursion, which otherwise follows the value down however deep it goes,
     * on whatever thread's stack the encode runs.
     */
    static final int MAX_VALUE_NESTING = WIRE_NESTING_LIMIT / 2;

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
            Object arg = bindTypeArgs(args[i], typeArgs);
            try {
                w.writeMessage(CALL_ARGS_KWARGS, encodeMapEntry(names[i], arg));
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

    /**
     * The call's explicit type bag bound into a typed argument's slot type: the
     * generated binding states each argument under its declared type, which
     * mentions the call's type parameters, and the bag names them.
     */
    private static Object bindTypeArgs(Object arg, BamlTypes typeArgs) {
        if (typeArgs == null || typeArgs.isEmpty()) {
            return arg;
        }
        if (arg instanceof baml_bridge.BamlTypedValue typed) {
            return new baml_bridge.BamlTypedValue(typed.value(), typed.type().substitute(typeArgs.asMap()));
        }
        if (arg instanceof baml_bridge.BamlTypedCallable typed) {
            return new baml_bridge.BamlTypedCallable(typed.callable(), typed.functionType().substitute(typeArgs.asMap()));
        }
        return arg;
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
        return encodeMapEntry(key, value, null, 0);
    }

    /**
     * {@code nesting} is how many lists, maps and classes enclose the entry's
     * value (see {@link #encodeInboundValue(Object, BamlType, int)}).
     */
    private static byte[] encodeMapEntry(
            String key, Object value, BamlType contextualType, int nesting) {
        WireWriter entry = new WireWriter();
        entry.writeString(MAP_ENTRY_STRING_KEY, key);
        // A null value leaves the `value` field absent, which the engine reads
        // as null (an unset oneof ≡ null).
        if (value != null) {
            entry.writeMessage(MAP_ENTRY_VALUE, encodeInboundValue(value, contextualType, nesting));
        }
        return entry.toByteArray();
    }

    /**
     * Encode a single {@code InboundValue}. A null argument returns an empty
     * message (no oneof arm set) — for list items and map values, an empty
     * {@code InboundValue} decodes to null while still preserving position.
     */
    public static byte[] encodeInboundValue(Object value) {
        return encodeInboundValue(value, null, 0);
    }

    /**
     * Encode {@code value} for a slot of {@code contextualType} ({@code null}
     * when the binding supplies none). The slot's type is what a container
     * states where it fixes one; otherwise the node states its own.
     *
     * <p>{@code nesting} is how many lists, maps and classes enclose the node.
     * A list, map or class at {@link #MAX_VALUE_NESTING} is refused before the
     * encoder descends into it.
     */
    private static byte[] encodeInboundValue(Object value, BamlType contextualType, int nesting) {
        assert nesting >= 0 && nesting <= MAX_VALUE_NESTING : nesting;
        if (value instanceof baml_bridge.BamlTypedValue typed) {
            return encodeInboundValue(typed.value(), typed.type(), nesting);
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
            return encodeInboundValue(inner, selected, nesting);
        }
        // The type a list or map node states; everything else implies its own
        // or states it where it is written.
        BamlType statedType = null;
        // Only a JDK proxy can be a returned BAML closure. Checking first keeps
        // pure encoding from initializing BamlFfi, which loads the native library.
        baml_bridge.BamlHandle returnedHandle =
                value != null && java.lang.reflect.Proxy.isProxyClass(value.getClass())
                        ? baml_bridge.BamlFfi.bamlClosureHandle(value)
                        : null;
        if (returnedHandle != null) {
            return encodeInboundValue(returnedHandle, contextualType, nesting);
        }
        // bool must precede the integer arms (mirrors Python's isinstance order).
        if (value instanceof Boolean b) {
            w.writeBool(IV_BOOL, b);
        } else if (value instanceof Long l) {
            w.writeInt64(IV_INT, l);
        } else if (value instanceof Integer || value instanceof Short || value instanceof Byte) {
            w.writeInt64(IV_INT, ((Number) value).longValue());
        } else if (value instanceof BigInteger bi) {
            // A BigInteger is a `bigint` whatever its magnitude: `int` and
            // `bigint` are distinct concrete types, so riding int_value would
            // make it an `int` that no `bigint` slot takes. Hex string,
            // lowercase and sign-prefixed (num-bigint's `{bi:x}`).
            w.writeString(IV_BIGINT, bi.toString(16));
        } else if (value instanceof Double d) {
            w.writeDouble(IV_FLOAT, d);
        } else if (value instanceof Float f) {
            w.writeDouble(IV_FLOAT, f.doubleValue());
        } else if (value instanceof String s) {
            w.writeString(IV_STRING, s);
        } else if (value instanceof byte[] bytes) {
            w.writeBytes(IV_UINT8ARRAY, bytes);
        } else if (value instanceof List<?> list) {
            BamlType slot = containerSlot(contextualType, BamlType.Kind.LIST, list.isEmpty());
            w.writeMessage(
                    IV_LIST,
                    encodeList(list, slot == null ? null : slot.listItem(), enterLevel(nesting)));
            statedType = slot != null && isExact(slot)
                    ? slot
                    : stateNatural(naturalType(list, nesting), contextualType);
        } else if (value instanceof Map<?, ?> map) {
            BamlType slot = containerSlot(contextualType, BamlType.Kind.MAP, map.isEmpty());
            w.writeMessage(
                    IV_MAP,
                    encodeMap(map, slot == null ? null : slot.mapValue(), enterLevel(nesting)));
            statedType = slot != null && isExact(slot)
                    ? slot
                    : stateNatural(naturalType(map, nesting), contextualType);
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
            return encodeInboundValue(spec.bamlHandle(), contextualType, nesting);
        } else if (value instanceof baml_bridge.BamlStream stream) {
            // BamlStream (ai.stream.Stream receiver): lifted to a bare
            // handle_value(ADT_TAGGED_HEAP_HANDLE) on the wire — the engine
            // reconstructs the heap pointer from the receiver's HANDLE_TABLE row
            // (Adt(TaggedHeapHandle)). Mirrors bridge_python's
            // `isinstance(value, BamlStream)` branch, which recurses on the inner
            // handle (proto.py). Delegate to the BamlHandle arm below, which
            // clones the key per the drain contract; the inner handle already
            // carries handle_type = ADT_TAGGED_HEAP_HANDLE.
            return encodeInboundValue(stream.bamlHandle(), contextualType, nesting);
        } else if (value instanceof baml_bridge.BamlOpaque opaque) {
            // An opaque instance (reflect.Package, …) goes back as the
            // ADT_RUNTIME_VALUE handle it arrived as, never by fields.
            return encodeInboundValue(opaque.bamlHandle(), contextualType, nesting);
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
            return encodeInboundValue(unwrapGenericUnion(value), contextualType, nesting);
        } else if (TypeRegistry.isUnionRecord(value)) {
            // A union wrapper record carries no wrapper on the inbound wire:
            // unwrap to its bare inner value and encode that (inbound has no
            // union arm — the engine re-validates the union at the boundary).
            return encodeInboundValue(TypeRegistry.unionRecordInner(value), contextualType, nesting);
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
            // Registered with its type, so a call back decodes each argument
            // against its declared parameter (a closure BAML passes becomes the
            // parameter's functional interface) and encodes the result against
            // the declared return, whether the generated carrier or the slot
            // gave the type.
            long key = baml_bridge.BamlFfi.registerHostCallable(
                    value instanceof baml_bridge.BamlTypedCallable
                            ? value
                            : new baml_bridge.BamlTypedCallable(value, functionType));
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
            // An instance states its own class, never the slot's: classes are
            // nominal, so a slot of another class (one an explicit type
            // argument named) cannot take it, and stating that class would
            // have the engine build one out of this instance's fields.
            List<BamlType> typeArgs;
            if (contextualType != null
                    && contextualType.kind() == BamlType.Kind.CLASS
                    && isExact(contextualType)) {
                if (!cw.fqn.equals(contextualType.fqn())) {
                    throw new UnsupportedInboundTypeException(
                            "a " + cw.fqn + " instance is not a " + contextualType);
                }
                // Java erases type arguments: the ones the instance was bound
                // with are its own, and an unbound instance takes the slot's.
                List<BamlType> bound = TypeRegistry.typeArgsOf(value);
                typeArgs = bound.isEmpty() ? contextualType.classTypeArgs() : bound;
            } else {
                typeArgs = instanceTypeArgs(value, cw, nesting);
            }
            writeClassType(w, cw.fqn, typeArgs);
            w.writeMessage(IV_CLASS, encodeClass(cw, typeArgs, enterLevel(nesting)));
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

    /**
     * The slot a list or map node is written under, or {@code null} when none
     * fixes it and the node states its own type. That is the contextual type
     * when it is of the node's {@code shape}, else the one member of that shape
     * of the union it denotes, looking through optionals and recursive aliases,
     * which are transparent ({@code json}'s list member is {@code json[]}).
     *
     * <p>This is the shared walker's member ranking cut down to what the writer
     * can decide without planning the elements: a member of the node's shape
     * outranks an open one (a type variable, {@code unknown}, an interface),
     * which the walker falls back to only when the shaped member refuses the
     * elements. An empty container fits every member of its shape, so it takes
     * the one there is and is refused as ambiguous between several; a
     * non-empty one takes it only when no open member could take the value as
     * its own type instead.
     */
    private static BamlType containerSlot(BamlType contextualType, BamlType.Kind shape, boolean empty) {
        if (contextualType == null) {
            return null;
        }
        java.util.LinkedHashSet<BamlType> shaped = new java.util.LinkedHashSet<>();
        boolean open = collectMembers(contextualType, shape, shaped, new java.util.HashSet<>());
        if (shaped.size() == 1) {
            return empty || !open ? shaped.iterator().next() : null;
        }
        if (shaped.size() > 1 && empty) {
            throw new IllegalArgumentException(
                    "an empty " + (shape == BamlType.Kind.LIST ? "list" : "map")
                            + " is ambiguous under " + contextualType + ": it fits each of "
                            + shaped + "; pass it typed as one of them");
        }
        return null;
    }

    /**
     * Gather the members of {@code type} that have the given {@code shape} into
     * {@code shaped}, flattening optionals, unions and recursive aliases (each
     * alias read once, so a self-reference outside a container ends). Returns
     * whether some member is open: a type variable, {@code unknown} or an
     * interface, any of which could take a container as its own type.
     */
    private static boolean collectMembers(
            BamlType type,
            BamlType.Kind shape,
            java.util.Set<BamlType> shaped,
            java.util.Set<BamlType> seenAliases) {
        if (type.kind() == shape) {
            shaped.add(type);
            return false;
        }
        return switch (type.kind()) {
            case OPTIONAL -> collectMembers(type.optionalInner(), shape, shaped, seenAliases);
            case UNION -> {
                boolean open = false;
                for (BamlType option : type.unionOptions()) {
                    open |= collectMembers(option, shape, shaped, seenAliases);
                }
                yield open;
            }
            case TYPE_ALIAS -> seenAliases.add(type)
                    && collectMembers(TypeRegistry.aliasBody(type), shape, shaped, seenAliases);
            case TYPEVAR, UNKNOWN, INTERFACE -> true;
            case PRIMITIVE, CLASS, ENUM, LIST, MAP, LITERAL, FUNCTION, ENUM_VARIANT, MEDIA, NEVER, OPAQUE -> false;
        };
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
     * The type a container states under a slot that does not fix it. An empty
     * container has no element type of its own ({@code never} in
     * {@code natural}): under a slot that leaves a type variable open nothing
     * binds the variable, and the call is refused until the caller names it,
     * as the dynamic hosts refuse it; anywhere else (no slot, {@code unknown},
     * a slot with no member of its shape) it states {@code unknown}, as the
     * compiler types {@code []} with no annotation, and the engine judges that
     * against the slot. A {@code never?} stays: that is {@code null}.
     */
    private static BamlType stateNatural(BamlType natural, BamlType contextualType) {
        if (natural == null || !mentionsNever(natural)) {
            return natural;
        }
        String var = contextualType == null ? null : firstTypeVar(contextualType);
        if (var == null) {
            return neverToUnknown(natural);
        }
        throw new IllegalArgumentException(
                "BAML could not infer a type for `" + var
                        + "` from the value; pass the call's type arguments");
    }

    /** Whether {@code type} has a {@code never} anywhere but as an optional's inner type. */
    private static boolean mentionsNever(BamlType type) {
        return switch (type.kind()) {
            case NEVER -> true;
            case OPTIONAL -> type.optionalInner().kind() != BamlType.Kind.NEVER && mentionsNever(type.optionalInner());
            case LIST -> mentionsNever(type.listItem());
            case MAP -> mentionsNever(type.mapKey()) || mentionsNever(type.mapValue());
            case UNION -> type.unionOptions().stream().anyMatch(ProtoWriter::mentionsNever);
            case CLASS -> type.classTypeArgs().stream().anyMatch(ProtoWriter::mentionsNever);
            default -> false;
        };
    }

    /** {@code type} with every {@code never} that is not an optional's inner type spelled {@code unknown}. */
    private static BamlType neverToUnknown(BamlType type) {
        return switch (type.kind()) {
            case NEVER -> BamlType.UNKNOWN;
            case OPTIONAL -> type.optionalInner().kind() == BamlType.Kind.NEVER
                    ? type
                    : BamlType.optional(neverToUnknown(type.optionalInner()));
            case LIST -> BamlType.list(neverToUnknown(type.listItem()));
            case MAP -> BamlType.map(neverToUnknown(type.mapKey()), neverToUnknown(type.mapValue()));
            case UNION -> BamlType.union(
                    type.unionOptions().stream().map(ProtoWriter::neverToUnknown).toArray(BamlType[]::new));
            case CLASS -> BamlType.classByFqn(
                    type.fqn(),
                    type.classTypeArgs().stream().map(ProtoWriter::neverToUnknown).toArray(BamlType[]::new));
            default -> type;
        };
    }

    /** The name of the first type variable {@code type} mentions, or {@code null}. */
    private static String firstTypeVar(BamlType type) {
        return switch (type.kind()) {
            case TYPEVAR -> type.fqn();
            case OPTIONAL -> firstTypeVar(type.optionalInner());
            case LIST -> firstTypeVar(type.listItem());
            case MAP -> {
                String key = firstTypeVar(type.mapKey());
                yield key != null ? key : firstTypeVar(type.mapValue());
            }
            case UNION -> type.unionOptions().stream().map(ProtoWriter::firstTypeVar)
                    .filter(java.util.Objects::nonNull).findFirst().orElse(null);
            case CLASS -> type.classTypeArgs().stream().map(ProtoWriter::firstTypeVar)
                    .filter(java.util.Objects::nonNull).findFirst().orElse(null);
            default -> null;
        };
    }

    /**
     * The BAML type a Java value has on its own, as the engine would see it: the
     * concrete type of a value under a slot that fixes nothing. A container's
     * element type is the union of its elements' types (duplicates collapsed,
     * one element's type as is), and an empty container has element type
     * {@code never}. {@code null} when some part of the value has no BAML type
     * the token grammar can spell (bytes, media, handles); the node is then left
     * unstated.
     *
     * <p>{@code nesting} is how many lists, maps and classes enclose the value,
     * counted and bounded as the encoder counts them: this walk follows the
     * value down too, and may run on a value the encoder has not yet refused.
     */
    private static BamlType naturalType(Object value, int nesting) {
        if (value instanceof Boolean) {
            return BamlType.BOOL;
        }
        if (value instanceof Long || value instanceof Integer || value instanceof Short || value instanceof Byte) {
            return BamlType.INT;
        }
        if (value instanceof BigInteger) {
            return BamlType.BIGINT;
        }
        if (value instanceof Double || value instanceof Float) {
            return BamlType.FLOAT;
        }
        if (value instanceof String) {
            return BamlType.STRING;
        }
        if (value instanceof List<?> list) {
            BamlType item = naturalElementType(list, enterLevel(nesting));
            return item == null ? null : BamlType.list(item);
        }
        if (value instanceof Map<?, ?> map) {
            BamlType item = naturalElementType(map.values(), enterLevel(nesting));
            return item == null ? null : BamlType.map(BamlType.STRING, item);
        }
        if (value instanceof Enum<?> constant) {
            TypeRegistry.EnumWire ew = TypeRegistry.enumWire(constant);
            return ew == null ? null : BamlType.of(constant.getDeclaringClass());
        }
        if (value instanceof baml_bridge.BamlUnion) {
            return naturalType(unwrapGenericUnion(value), nesting);
        }
        if (TypeRegistry.isUnionRecord(value)) {
            // A record of a declared alias (`baml.json.json`) has the alias as
            // its type; a record of an anonymous union has its inner value's.
            String alias = TypeRegistry.unionAliasFqn(value);
            return alias != null ? BamlType.typeAlias(alias) : naturalType(TypeRegistry.unionRecordInner(value), nesting);
        }
        TypeRegistry.ClassWire cw = value == null ? null : TypeRegistry.classWire(value);
        if (cw != null) {
            return BamlType.classByFqn(cw.fqn, instanceTypeArgs(value, cw, nesting).toArray(new BamlType[0]));
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
     * names it. {@code nesting} counts the classes, lists and maps enclosing the
     * instance, as {@link #naturalType} does.
     */
    private static List<BamlType> instanceTypeArgs(
            Object value, TypeRegistry.ClassWire cw, int nesting) {
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
                BamlType natural = naturalType(fieldValue, enterLevel(nesting));
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
     * solver does: an empty container has no element type and binds nothing
     * (its {@code never} is the absence of evidence, not a type), two fields
     * that disagree about a variable are an error, and a union descriptor
     * gives no evidence.
     */
    private static void unify(BamlType desc, BamlType natural, Map<String, BamlType> bindings) {
        switch (desc.kind()) {
            case TYPEVAR -> {
                if (mentionsNever(natural)) {
                    return;
                }
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
    private static BamlType naturalElementType(java.util.Collection<?> elements, int nesting) {
        java.util.LinkedHashSet<BamlType> types = new java.util.LinkedHashSet<>();
        boolean nullable = false;
        for (Object element : elements) {
            if (element == null) {
                nullable = true;
                continue;
            }
            BamlType type = naturalType(element, nesting);
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
    private static byte[] encodeClass(
            TypeRegistry.ClassWire cw, List<BamlType> typeArgs, int nesting) {
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
                    encodeMapEntry(cw.fieldNames[i], cw.fieldValues[i], fieldDesc, nesting));
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
     * it is a callable with no function type to take, or it nests deeper than
     * the wire carries ({@link #enterLevel}). The top-level kwarg
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

    /**
     * The nesting of the items of a list, map or class enclosed by
     * {@code nesting} others, or a refusal when it would exceed
     * {@link #MAX_VALUE_NESTING}.
     */
    private static int enterLevel(int nesting) {
        if (nesting >= MAX_VALUE_NESTING) {
            throw new UnsupportedInboundTypeException(
                    "the value nests lists, maps and classes more than " + MAX_VALUE_NESTING
                            + " deep, deeper than the " + WIRE_NESTING_LIMIT
                            + " nested messages the wire carries");
        }
        return nesting + 1;
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

    private static byte[] encodeList(List<?> list, BamlType itemType, int nesting) {
        WireWriter w = new WireWriter();
        for (Object item : list) {
            // Always emit an entry (even for a null item) to preserve length.
            w.writeMessage(LIST_VALUES, encodeInboundValue(item, itemType, nesting));
        }
        return w.toByteArray();
    }

    private static byte[] encodeMap(Map<?, ?> map, BamlType valueType, int nesting) {
        WireWriter w = new WireWriter();
        for (Map.Entry<?, ?> e : map.entrySet()) {
            // The engine stringifies map keys; generated maps are Map<String, V>.
            w.writeMessage(
                    MAP_ENTRIES,
                    encodeMapEntry(String.valueOf(e.getKey()), e.getValue(), valueType, nesting));
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

    // HostCallCompletion fields (baml_inbound.proto).
    private static final int HCC_RETURNED = 1;
    private static final int HCC_THROWN = 2;
    private static final int HCC_RAISED = 3;
    private static final int HCC_CONTRACT_VIOLATION = 4;
    private static final int HCC_BRIDGE_FAILURE = 5;
    // HostException fields.
    private static final int HE_MESSAGE = 1;
    private static final int HE_CLASS_NAME = 2;
    private static final int HE_LANGUAGE = 3;
    private static final int HE_TRACEBACK = 4;
    private static final int HE_HANDLE = 5;

    /** The completion for a callable that returned {@code inboundValue} (an encoded {@code InboundValue}). */
    public static byte[] hostCallReturned(byte[] inboundValue) {
        WireWriter w = new WireWriter();
        w.writeMessage(HCC_RETURNED, inboundValue);
        return w.toByteArray();
    }

    /** The completion for a callable that threw the BAML value {@code inboundValue}. */
    public static byte[] hostCallThrown(byte[] inboundValue) {
        WireWriter w = new WireWriter();
        w.writeMessage(HCC_THROWN, inboundValue);
        return w.toByteArray();
    }

    /**
     * The completion for a callable that raised an exception of the host's own.
     * {@code handleKey} is the Java registry's key for the {@link Throwable}, so the
     * engine-built {@code baml.errors.HostCallable} rehydrates the same object by
     * identity on round trip.
     */
    public static byte[] hostCallRaised(String className, String message, String traceback, long handleKey) {
        WireWriter exception = new WireWriter();
        exception.writeString(HE_MESSAGE, message);
        exception.writeString(HE_CLASS_NAME, className);
        exception.writeString(HE_LANGUAGE, "java");
        if (traceback != null) exception.writeString(HE_TRACEBACK, traceback);
        exception.writeInt64(HE_HANDLE, handleKey);
        WireWriter w = new WireWriter();
        w.writeMessage(HCC_RAISED, exception.toByteArray());
        return w.toByteArray();
    }

    /** The completion for a value the callable produced that does not fit its declared type. */
    public static byte[] hostCallContractViolation(String message) {
        WireWriter w = new WireWriter();
        w.writeString(HCC_CONTRACT_VIOLATION, message);
        return w.toByteArray();
    }

    /** The completion for a failure of the bridge's own. */
    public static byte[] hostCallBridgeFailure(String message) {
        WireWriter w = new WireWriter();
        w.writeString(HCC_BRIDGE_FAILURE, message);
        return w.toByteArray();
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
