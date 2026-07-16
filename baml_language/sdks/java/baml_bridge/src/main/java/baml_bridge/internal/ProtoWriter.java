package baml_bridge.internal;

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
 *                     uint8array_value = 11, bigint_value = 12
 *                     (class_value = 8 / enum_value = 9 / handle = 10 / ty_value = 13
 *                      are not implemented in this slice)
 * InboundListValue: values = 1 (repeated InboundValue)
 * InboundMapValue:  entries = 1 (repeated InboundMapEntry)
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
    private static final int IV_UINT8ARRAY = 11;
    private static final int IV_BIGINT = 12;

    // InboundListValue / InboundMapValue
    private static final int LIST_VALUES = 1;
    private static final int MAP_ENTRIES = 1;

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
        } else {
            // class_value / enum_value / handle / media / host-callable are not
            // part of the primitives slice yet.
            throw new UnsupportedOperationException(
                    "capability not yet implemented: cannot encode argument of type "
                            + value.getClass().getName());
        }
        return w.toByteArray();
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
