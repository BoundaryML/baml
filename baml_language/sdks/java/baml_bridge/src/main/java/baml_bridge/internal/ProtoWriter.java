package baml_bridge.internal;

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
 *                   type_args = 3 (unused here — empty)
 * InboundMapEntry:  string_key = 1, value = 6 (InboundValue)
 * InboundValue oneof: string_value = 2, int_value = 3, float_value = 4,
 *                     bool_value = 5, list_value = 6, map_value = 7,
 *                     class_value = 8, enum_value = 9,
 *                     uint8array_value = 11, bigint_value = 12
 *                     (handle = 10 / ty_value = 13 are not implemented in this slice)
 * InboundListValue:  values = 1 (repeated InboundValue)
 * InboundMapValue:   entries = 1 (repeated InboundMapEntry)
 * InboundClassValue: fields = 2 (repeated InboundMapEntry), class_ty = 3 (BamlTyClass)
 *                    (field 1, formerly `name`, is reserved — the FQN lives on class_ty)
 * BamlTyClass:       name = 1 (BAML FQN), type_args = 2 (unused here — generics reify later)
 * InboundEnumValue:  name = 1 (BAML FQN), value = 2 (wire variant name)
 * </pre>
 */
public final class ProtoWriter {
    // CallFunctionArgs
    private static final int CALL_ARGS_KWARGS = 1;
    private static final int CALL_ARGS_CALL_ID = 2;

    // InboundMapEntry
    private static final int MAP_ENTRY_STRING_KEY = 1;
    private static final int MAP_ENTRY_VALUE = 6;

    // InboundValue oneof
    private static final int IV_STRING = 2;
    private static final int IV_INT = 3;
    private static final int IV_FLOAT = 4;
    private static final int IV_BOOL = 5;
    private static final int IV_LIST = 6;
    private static final int IV_MAP = 7;
    private static final int IV_CLASS = 8;
    private static final int IV_ENUM = 9;
    private static final int IV_UINT8ARRAY = 11;
    private static final int IV_BIGINT = 12;

    // InboundListValue / InboundMapValue
    private static final int LIST_VALUES = 1;
    private static final int MAP_ENTRIES = 1;

    // InboundClassValue (field 1 reserved) / BamlTyClass / InboundEnumValue
    private static final int CLASS_FIELDS = 2;
    private static final int CLASS_TY = 3;
    private static final int TY_CLASS_NAME = 1;
    private static final int ENUM_NAME = 1;
    private static final int ENUM_VALUE = 2;

    private static final BigInteger LONG_MIN = BigInteger.valueOf(Long.MIN_VALUE);
    private static final BigInteger LONG_MAX = BigInteger.valueOf(Long.MAX_VALUE);

    private ProtoWriter() {}

    /**
     * Encode positional args (paired with their declared parameter names) plus a
     * nonzero {@code call_id} into {@code CallFunctionArgs} bytes. The engine
     * rejects a zero {@code call_id}, so the caller must mint one via
     * {@code BamlFfi.nativeNewCallId()}.
     */
    public static byte[] encodeCallFunctionArgs(String[] names, Object[] args, long callId) {
        if (names.length != args.length) {
            throw new IllegalArgumentException(
                    "names/args length mismatch: " + names.length + " vs " + args.length);
        }
        WireWriter w = new WireWriter();
        for (int i = 0; i < names.length; i++) {
            w.writeMessage(CALL_ARGS_KWARGS, encodeMapEntry(names[i], args[i]));
        }
        w.writeInt64(CALL_ARGS_CALL_ID, callId);
        return w.toByteArray();
    }

    /** One {@code InboundMapEntry} with a string key and (unless null) a value. */
    private static byte[] encodeMapEntry(String key, Object value) {
        WireWriter entry = new WireWriter();
        entry.writeString(MAP_ENTRY_STRING_KEY, key);
        // A null value leaves the `value` field absent, which the engine reads
        // as null (an unset oneof ≡ null).
        if (value != null) {
            entry.writeMessage(MAP_ENTRY_VALUE, encodeInboundValue(value));
        }
        return entry.toByteArray();
    }

    /**
     * Encode a single {@code InboundValue}. A null argument returns an empty
     * message (no oneof arm set) — for list items and map values, an empty
     * {@code InboundValue} decodes to null while still preserving position.
     */
    public static byte[] encodeInboundValue(Object value) {
        WireWriter w = new WireWriter();
        if (value == null) {
            return w.toByteArray(); // absent oneof ≡ null
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
            w.writeMessage(IV_LIST, encodeList(list));
        } else if (value instanceof Map<?, ?> map) {
            w.writeMessage(IV_MAP, encodeMap(map));
        } else if (value instanceof Enum<?> constant) {
            // Any Java enum: encode it only if its type is a registered generated
            // enum; an unregistered enum is not a BAML value.
            TypeRegistry.EnumWire ew = TypeRegistry.enumWire(constant);
            if (ew == null) {
                throw unsupported(value);
            }
            w.writeMessage(IV_ENUM, encodeEnum(ew));
        } else if (TypeRegistry.isUnionRecord(value)) {
            // A union wrapper record carries no wrapper on the inbound wire:
            // unwrap to its bare inner value and encode that (inbound has no
            // union arm — the engine re-validates the union at the boundary).
            return encodeInboundValue(TypeRegistry.unionRecordInner(value));
        } else {
            // A registered generated class encodes as a class_value; anything
            // else (handle / media / host-callable / arbitrary object) is not
            // part of this slice.
            TypeRegistry.ClassWire cw = TypeRegistry.classWire(value);
            if (cw == null) {
                throw unsupported(value);
            }
            w.writeMessage(IV_CLASS, encodeClass(cw));
        }
        return w.toByteArray();
    }

    /**
     * Encode an {@code InboundClassValue}: one {@code fields} entry per registry
     * field (key = field name, value = the accessor-read value, recursed through
     * {@link #encodeInboundValue}) plus the FQN on {@code class_ty.name}. Generic
     * {@code type_args} are omitted — they reify later.
     */
    private static byte[] encodeClass(TypeRegistry.ClassWire cw) {
        WireWriter w = new WireWriter();
        for (int i = 0; i < cw.fieldNames.length; i++) {
            w.writeMessage(CLASS_FIELDS, encodeMapEntry(cw.fieldNames[i], cw.fieldValues[i]));
        }
        WireWriter classTy = new WireWriter();
        classTy.writeString(TY_CLASS_NAME, cw.fqn);
        w.writeMessage(CLASS_TY, classTy.toByteArray());
        return w.toByteArray();
    }

    /** Encode an {@code InboundEnumValue}: FQN on {@code name}, variant on {@code value}. */
    private static byte[] encodeEnum(TypeRegistry.EnumWire ew) {
        WireWriter w = new WireWriter();
        w.writeString(ENUM_NAME, ew.fqn);
        w.writeString(ENUM_VALUE, ew.wireName);
        return w.toByteArray();
    }

    private static UnsupportedOperationException unsupported(Object value) {
        return new UnsupportedOperationException(
                "capability not yet implemented: cannot encode argument of type "
                        + value.getClass().getName());
    }

    private static byte[] encodeList(List<?> list) {
        WireWriter w = new WireWriter();
        for (Object item : list) {
            // Always emit an entry (even for a null item) to preserve length.
            w.writeMessage(LIST_VALUES, encodeInboundValue(item));
        }
        return w.toByteArray();
    }

    private static byte[] encodeMap(Map<?, ?> map) {
        WireWriter w = new WireWriter();
        for (Map.Entry<?, ?> e : map.entrySet()) {
            // The engine stringifies map keys; generated maps are Map<String, V>.
            w.writeMessage(MAP_ENTRIES, encodeMapEntry(String.valueOf(e.getKey()), e.getValue()));
        }
        return w.toByteArray();
    }
}
