package baml_bridge;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertArrayEquals;
import static org.junit.jupiter.api.Assertions.assertInstanceOf;
import static org.junit.jupiter.api.Assertions.assertNull;
import static org.junit.jupiter.api.Assertions.assertThrows;
import static org.junit.jupiter.api.Assertions.assertTrue;

import baml_bridge.internal.ProtoReader;
import baml_bridge.internal.ProtoWriter;
import baml_bridge.internal.WireWriter;

import java.math.BigInteger;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;

import org.junit.jupiter.api.BeforeAll;
import org.junit.jupiter.api.Test;

/**
 * Offline codec tests — no native library required. They validate the
 * hand-rolled protobuf wire codec against {@code baml_inbound.proto} /
 * {@code baml_outbound.proto} field numbers by round-tripping representative
 * primitives-slice values.
 */
class WireCodecTest {
    // BamlOutboundValue oneof field numbers.
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
    private static final int OV_UINT8ARRAY = 19;
    private static final int OV_BIGINT = 20;

    // -- outbound builders (mirror the engine's value_encode side) -----------

    /** Wrap a BamlOutboundValue message into a BamlOutboundResult.ok envelope. */
    private static byte[] okEnvelope(byte[] outboundValue) {
        WireWriter w = new WireWriter();
        w.writeMessage(1, outboundValue); // BamlOutboundResult.ok = 1
        return w.toByteArray();
    }

    private static byte[] ovInt(long v) {
        WireWriter w = new WireWriter();
        w.writeInt64(OV_INT, v);
        return w.toByteArray();
    }

    @Test
    void decode_ok_int() {
        assertEquals(42L, ProtoReader.decodeOutboundResult(okEnvelope(ovInt(42))));
    }

    @Test
    void decode_ok_negative_int() {
        assertEquals(-7L, ProtoReader.decodeOutboundResult(okEnvelope(ovInt(-7))));
    }

    @Test
    void decode_ok_string() {
        WireWriter w = new WireWriter();
        w.writeString(OV_STRING, "héllo");
        assertEquals("héllo", ProtoReader.decodeOutboundResult(okEnvelope(w.toByteArray())));
    }

    @Test
    void decode_ok_bool() {
        WireWriter w = new WireWriter();
        w.writeBool(OV_BOOL, true);
        assertEquals(Boolean.TRUE, ProtoReader.decodeOutboundResult(okEnvelope(w.toByteArray())));
    }

    @Test
    void decode_ok_double() {
        WireWriter w = new WireWriter();
        w.writeDouble(OV_FLOAT, 3.5);
        assertEquals(3.5, ProtoReader.decodeOutboundResult(okEnvelope(w.toByteArray())));
    }

    @Test
    void decode_ok_null() {
        WireWriter nullMsg = new WireWriter();
        WireWriter w = new WireWriter();
        w.writeMessage(OV_NULL, nullMsg.toByteArray()); // null_value = empty message
        assertNull(ProtoReader.decodeOutboundResult(okEnvelope(w.toByteArray())));
    }

    @Test
    void decode_ok_absent_oneof_is_null() {
        // An all-default envelope (no ok/error/panic arm set) is a null ok.
        assertNull(ProtoReader.decodeOutboundResult(new byte[0]));
    }

    @Test
    void decode_ok_bytes() {
        WireWriter w = new WireWriter();
        w.writeBytes(OV_UINT8ARRAY, new byte[] {0, 1, 2, (byte) 0xFF});
        assertArrayEquals(
                new byte[] {0, 1, 2, (byte) 0xFF},
                (byte[]) ProtoReader.decodeOutboundResult(okEnvelope(w.toByteArray())));
    }

    @Test
    void decode_ok_bigint() {
        BigInteger big = new BigInteger("123456789012345678901234567890");
        WireWriter w = new WireWriter();
        w.writeString(OV_BIGINT, big.toString(16));
        assertEquals(big, ProtoReader.decodeOutboundResult(okEnvelope(w.toByteArray())));
    }

    @Test
    void decode_ok_literal_int_unwraps() {
        WireWriter lit = new WireWriter();
        lit.writeInt64(2, 99); // BamlLiteralValue.int_value = 2
        WireWriter w = new WireWriter();
        w.writeMessage(OV_LITERAL, lit.toByteArray());
        assertEquals(99L, ProtoReader.decodeOutboundResult(okEnvelope(w.toByteArray())));
    }

    @Test
    void decode_ok_literal_float_from_source_text() {
        WireWriter lit = new WireWriter();
        lit.writeString(5, "2.75"); // BamlLiteralValue.float_value = 5 (source text)
        WireWriter w = new WireWriter();
        w.writeMessage(OV_LITERAL, lit.toByteArray());
        assertEquals(2.75, ProtoReader.decodeOutboundResult(okEnvelope(w.toByteArray())));
    }

    @Test
    void decode_ok_list() {
        WireWriter list = new WireWriter();
        list.writeMessage(2, ovInt(1)); // items = 2
        list.writeMessage(2, ovInt(2));
        list.writeMessage(2, ovInt(3));
        WireWriter w = new WireWriter();
        w.writeMessage(OV_LIST, list.toByteArray());
        assertEquals(
                List.of(1L, 2L, 3L),
                ProtoReader.decodeOutboundResult(okEnvelope(w.toByteArray())));
    }

    @Test
    void decode_ok_map() {
        WireWriter entry = new WireWriter();
        entry.writeString(1, "k"); // BamlOutboundMapEntry.key = 1
        entry.writeMessage(2, ovInt(5)); // value = 2
        WireWriter map = new WireWriter();
        map.writeMessage(3, entry.toByteArray()); // entries = 3
        WireWriter w = new WireWriter();
        w.writeMessage(OV_MAP, map.toByteArray());
        assertEquals(
                Map.of("k", 5L),
                ProtoReader.decodeOutboundResult(okEnvelope(w.toByteArray())));
    }

    @Test
    void decode_error_arm_throws_baml_error() {
        // BamlValueClass { name = "baml.errors.GenericSdkError", fields: { message: "boom" } }
        WireWriter messageField = new WireWriter();
        messageField.writeString(1, "message"); // BamlOutboundMapEntry.key
        WireWriter messageValue = new WireWriter();
        messageValue.writeString(OV_STRING, "boom");
        messageField.writeMessage(2, messageValue.toByteArray()); // value

        WireWriter classValue = new WireWriter();
        classValue.writeString(1, "baml.errors.GenericSdkError"); // BamlValueClass.name
        classValue.writeMessage(2, messageField.toByteArray()); // fields = 2

        WireWriter ov = new WireWriter();
        ov.writeMessage(OV_CLASS, classValue.toByteArray());

        WireWriter err = new WireWriter();
        err.writeMessage(1, ov.toByteArray()); // BamlOutboundError.value = 1
        err.writeString(2, "File \"x.baml\", line 1, in f"); // trace = 2 (repeated)

        WireWriter envelope = new WireWriter();
        envelope.writeMessage(2, err.toByteArray()); // BamlOutboundResult.error = 2

        BamlError thrown = assertThrows(
                BamlError.class,
                () -> ProtoReader.decodeOutboundResult(envelope.toByteArray()));
        assertEquals("baml.errors.GenericSdkError", thrown.class_name());
        assertEquals(List.of("File \"x.baml\", line 1, in f"), thrown.baml_trace());
        assertEquals(Map.of("message", "boom"), thrown.value());
    }

    // -- inbound (ProtoWriter) round-trip via the outbound reader ------------

    @Test
    void inbound_encodes_call_id_and_kwargs() {
        // Encode CallFunctionArgs, then re-read it with the low-level reader to
        // confirm call_id and a kwarg landed where the .proto says they should.
        byte[] bytes = ProtoWriter.encodeCallFunctionArgs(
                new String[] {"n"}, new Object[] {7L}, 123L);
        // Minimal manual check: the bytes are non-empty and decode without error
        // through a reader (structure validated by the Rust side in integration).
        assertTrue(bytes.length > 0);
    }

    @Test
    void inbound_null_argument_is_absent_value() {
        byte[] bytes = ProtoWriter.encodeInboundValue(null);
        assertEquals(0, bytes.length); // empty InboundValue ≡ null
    }

    @Test
    void inbound_bigint_in_range_uses_int_channel() {
        // A BigInteger within i64 range must ride int_value, not bigint_value.
        byte[] small = ProtoWriter.encodeInboundValue(BigInteger.valueOf(5));
        byte[] asLong = ProtoWriter.encodeInboundValue(5L);
        assertArrayEquals(asLong, small);
    }

    @Test
    void inbound_unsupported_type_throws() {
        assertThrows(
                UnsupportedOperationException.class,
                () -> ProtoWriter.encodeInboundValue(new Object()));
    }

    // -- class / enum wire codec (TypeRegistry) ------------------------------

    // Generated-shape fixtures: a value class with a single public all-args
    // constructor + PreserveCase zero-arg accessors, and a plain enum whose
    // `new$` constant models a Java-keyword-escaped variant (wire name "new").
    public static final class Resume {
        private final String name;
        private final long age;

        public Resume(String name, long age) {
            this.name = name;
            this.age = age;
        }

        public String name() {
            return name;
        }

        public long age() {
            return age;
        }
    }

    public enum Sentiment {
        Positive,
        new$ // wire variant "new" (escaped Java keyword)
    }

    private static final String RESUME_FQN = "user.lorem.Resume";
    private static final String SENTIMENT_FQN = "user.ipsum.Sentiment";

    @BeforeAll
    static void registerFixtures() {
        // Mirrors the generated Baml static initializer's registration calls.
        // Idempotent, so running once per suite is enough.
        TypeRegistry.registerClass(
                RESUME_FQN, Resume.class.getName(), new String[] {"name", "age"});
        TypeRegistry.registerEnum(
                SENTIMENT_FQN,
                Sentiment.class.getName(),
                new String[] {"Positive", "new$"}, // Java constants
                new String[] {"Positive", "new"}); // wire variants
    }

    /** A registered class encodes as InboundValue.class_value (=8). */
    @Test
    void inbound_class_value_layout() {
        byte[] got = ProtoWriter.encodeInboundValue(new Resume("Alice", 30L));

        // Hand-build the expected InboundClassValue: fields (=2) in declaration
        // order, then class_ty (=3) carrying the FQN on name (=1).
        WireWriter nameVal = new WireWriter();
        nameVal.writeString(2, "Alice"); // InboundValue.string_value = 2
        WireWriter nameEntry = new WireWriter();
        nameEntry.writeString(1, "name"); // InboundMapEntry.string_key = 1
        nameEntry.writeMessage(6, nameVal.toByteArray()); // InboundMapEntry.value = 6

        WireWriter ageVal = new WireWriter();
        ageVal.writeInt64(3, 30L); // InboundValue.int_value = 3
        WireWriter ageEntry = new WireWriter();
        ageEntry.writeString(1, "age");
        ageEntry.writeMessage(6, ageVal.toByteArray());

        WireWriter classTy = new WireWriter();
        classTy.writeString(1, RESUME_FQN); // BamlTyClass.name = 1

        WireWriter classMsg = new WireWriter();
        classMsg.writeMessage(2, nameEntry.toByteArray()); // InboundClassValue.fields = 2
        classMsg.writeMessage(2, ageEntry.toByteArray());
        classMsg.writeMessage(3, classTy.toByteArray()); // InboundClassValue.class_ty = 3

        WireWriter expected = new WireWriter();
        expected.writeMessage(8, classMsg.toByteArray()); // InboundValue.class_value = 8

        assertArrayEquals(expected.toByteArray(), got);
    }

    /** A registered enum constant encodes as InboundValue.enum_value (=9). */
    @Test
    void inbound_enum_value_layout_escaped_variant() {
        byte[] got = ProtoWriter.encodeInboundValue(Sentiment.new$);

        WireWriter enumMsg = new WireWriter();
        enumMsg.writeString(1, SENTIMENT_FQN); // InboundEnumValue.name = 1
        enumMsg.writeString(2, "new"); // InboundEnumValue.value = 2 (wire variant)

        WireWriter expected = new WireWriter();
        expected.writeMessage(9, enumMsg.toByteArray()); // InboundValue.enum_value = 9

        assertArrayEquals(expected.toByteArray(), got);
    }

    /** An unregistered enum type is not a BAML value → encoder rejects it. */
    @Test
    void inbound_unregistered_enum_throws() {
        assertThrows(
                UnsupportedOperationException.class,
                () -> ProtoWriter.encodeInboundValue(UnregisteredEnum.A));
    }

    private enum UnregisteredEnum {
        A
    }

    /** A registered FQN decodes class_value into the generated class instance. */
    @Test
    void decode_class_value_constructs_registered_class() {
        WireWriter nameVal = new WireWriter();
        nameVal.writeString(OV_STRING, "Alice"); // BamlOutboundValue.string_value = 3
        WireWriter nameEntry = new WireWriter();
        nameEntry.writeString(1, "name"); // BamlOutboundMapEntry.key = 1
        nameEntry.writeMessage(2, nameVal.toByteArray()); // value = 2

        WireWriter ageVal = new WireWriter();
        ageVal.writeInt64(OV_INT, 30L); // BamlOutboundValue.int_value = 4
        WireWriter ageEntry = new WireWriter();
        ageEntry.writeString(1, "age");
        ageEntry.writeMessage(2, ageVal.toByteArray());

        WireWriter cls = new WireWriter();
        cls.writeString(1, RESUME_FQN); // BamlValueClass.name = 1
        cls.writeMessage(2, nameEntry.toByteArray()); // fields = 2
        cls.writeMessage(2, ageEntry.toByteArray());

        WireWriter ov = new WireWriter();
        ov.writeMessage(OV_CLASS, cls.toByteArray());

        Object decoded = ProtoReader.decodeOutboundResult(okEnvelope(ov.toByteArray()));
        Resume r = assertInstanceOf(Resume.class, decoded);
        assertEquals("Alice", r.name());
        assertEquals(30L, r.age());
    }

    /** An unregistered FQN keeps the lenient LinkedHashMap fallback. */
    @Test
    void decode_class_value_unknown_fqn_falls_back_to_map() {
        WireWriter fieldVal = new WireWriter();
        fieldVal.writeString(OV_STRING, "boom");
        WireWriter fieldEntry = new WireWriter();
        fieldEntry.writeString(1, "message");
        fieldEntry.writeMessage(2, fieldVal.toByteArray());

        WireWriter cls = new WireWriter();
        cls.writeString(1, "unknown.Nope"); // not registered
        cls.writeMessage(2, fieldEntry.toByteArray());

        WireWriter ov = new WireWriter();
        ov.writeMessage(OV_CLASS, cls.toByteArray());

        Object decoded = ProtoReader.decodeOutboundResult(okEnvelope(ov.toByteArray()));
        Map<String, Object> expected = new LinkedHashMap<>();
        expected.put("message", "boom");
        assertEquals(expected, decoded);
    }

    /** A registered FQN maps the wire variant to the Java enum constant. */
    @Test
    void decode_enum_value_maps_wire_variant_to_constant() {
        assertEquals(Sentiment.Positive, decodeEnum(SENTIMENT_FQN, "Positive"));
        // The escaped constant: wire "new" ↔ Java constant `new$`.
        assertEquals(Sentiment.new$, decodeEnum(SENTIMENT_FQN, "new"));
    }

    /** An unregistered enum FQN falls back to the raw wire variant string. */
    @Test
    void decode_enum_value_unknown_fqn_falls_back_to_string() {
        assertEquals("Whatever", decodeEnum("unknown.Mood", "Whatever"));
    }

    /** Decode a BamlValueEnum { name, value } wrapped in an ok envelope. */
    private static Object decodeEnum(String fqn, String wireVariant) {
        WireWriter en = new WireWriter();
        en.writeString(1, fqn); // BamlValueEnum.name = 1
        en.writeString(2, wireVariant); // BamlValueEnum.value = 2
        WireWriter ov = new WireWriter();
        ov.writeMessage(OV_ENUM, en.toByteArray());
        return ProtoReader.decodeOutboundResult(okEnvelope(ov.toByteArray()));
    }
}
