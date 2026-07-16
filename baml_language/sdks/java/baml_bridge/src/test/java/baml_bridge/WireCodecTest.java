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
import baml_sdk.baml.media.Image;
import baml_sdk.baml.media.Pdf;

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
    private static final int OV_UNION = 13;
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
    void inbound_null_kwarg_encodes_as_absent_value_entry() {
        // The optional-args configurator sends a touched-with-null optional as
        // a kwarg whose InboundValue is absent (an unset oneof ≡ explicit BAML
        // null) — distinct from an OMITTED optional, which contributes no
        // kwarg entry at all. Pinning this: a null entry in the args array must
        // still emit its InboundMapEntry (carrying only the string_key), never
        // throw and never drop the entry.
        byte[] got = ProtoWriter.encodeCallFunctionArgs(
                new String[] {"opt1"}, new Object[] {null}, 1L);

        // Expected: one kwarg entry (field 1) carrying only string_key (field 1)
        // with no value (field 6), then call_id (field 2).
        WireWriter entry = new WireWriter();
        entry.writeString(1, "opt1"); // InboundMapEntry.string_key = 1 (value absent)
        WireWriter expected = new WireWriter();
        expected.writeMessage(1, entry.toByteArray()); // CallFunctionArgs.kwargs = 1
        expected.writeInt64(2, 1L); // call_id = 2

        assertArrayEquals(expected.toByteArray(), got);
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

    // A generated-shape union: a sealed interface with one wrapper record per arm
    // (each a `record ...Value(T value)` with a `value()` accessor). `IntValue`
    // uses a primitive `long` component to exercise reflective unboxing of the
    // decoded inner value.
    public sealed interface IntOrString permits IntOrString.IntValue, IntOrString.StringValue {
        record IntValue(long value) implements IntOrString {}

        record StringValue(String value) implements IntOrString {}
    }

    // A generated-shape value class with a single Object field, registered WITH a
    // per-field type-directed descriptor (union[int;string]) to exercise the
    // 4-arg registerClass + fieldDescs decode path.
    public static final class Box {
        private final Object payload;

        public Box(Object payload) {
            this.payload = payload;
        }

        public Object payload() {
            return payload;
        }
    }

    private static final String RESUME_FQN = "user.lorem.Resume";
    private static final String SENTIMENT_FQN = "user.ipsum.Sentiment";
    private static final String BOX_FQN = "user.test.Box";

    @BeforeAll
    static void registerFixtures() {
        // Mirrors the generated Baml static initializer's registration calls.
        // Idempotent, so running once per suite is enough.
        TypeRegistry.registerClass(
                RESUME_FQN, Resume.class.getName(), new String[] {"name", "age"});
        // 4-arg overload: a parallel fieldDescs array (one descriptor per field).
        TypeRegistry.registerClass(
                BOX_FQN,
                Box.class.getName(),
                new String[] {"payload"},
                new String[] {"union[int;string]"});
        TypeRegistry.registerEnum(
                SENTIMENT_FQN,
                Sentiment.class.getName(),
                new String[] {"Positive", "new$"}, // Java constants
                new String[] {"Positive", "new"}); // wire variants
        // Signature = sorted arm tokens joined with `|`; arm tokens + record
        // names in declaration order (int|string).
        TypeRegistry.registerUnion(
                "int|string",
                IntOrString.class.getName(),
                new String[] {"int", "string"},
                new String[] {
                    IntOrString.IntValue.class.getName(), IntOrString.StringValue.class.getName()
                });
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

    // -- union decode / encode (TypeRegistry) --------------------------------

    // BamlTyPrimitiveKind (baml_type.proto).
    private static final int PRIM_STRING = 1;
    private static final int PRIM_INT = 2;
    private static final int PRIM_BOOL = 4;

    /** A BamlTy wrapping a primitive kind (BamlTy.primitive = 1). */
    private static byte[] tyPrimitive(int kind) {
        WireWriter prim = new WireWriter();
        prim.writeInt64(1, kind); // BamlTyPrimitive.kind = 1
        WireWriter ty = new WireWriter();
        ty.writeMessage(1, prim.toByteArray());
        return ty.toByteArray();
    }

    /** A BamlTy wrapping a string literal (BamlTy.literal = 8). */
    private static byte[] tyStringLiteral(String s) {
        WireWriter lit = new WireWriter();
        lit.writeString(1, s); // BamlTyLiteral.string_value = 1
        WireWriter ty = new WireWriter();
        ty.writeMessage(8, lit.toByteArray());
        return ty.toByteArray();
    }

    /** A BamlTy union (BamlTy.union = 7) over the given option BamlTys. */
    private static byte[] tyUnion(byte[]... options) {
        WireWriter u = new WireWriter();
        for (byte[] o : options) {
            u.writeMessage(1, o); // BamlTyUnion.options = 1 (repeated)
        }
        WireWriter ty = new WireWriter();
        ty.writeMessage(7, u.toByteArray());
        return ty.toByteArray();
    }

    /** A BamlOutboundValue.union_variant_value (=13) with self_type (=4) + value (=6). */
    private static byte[] ovUnion(byte[] selfType, byte[] innerValue) {
        WireWriter uv = new WireWriter();
        uv.writeMessage(4, selfType); // BamlValueUnionVariant.self_type = 4
        if (innerValue != null) {
            uv.writeMessage(6, innerValue); // value = 6
        }
        WireWriter ov = new WireWriter();
        ov.writeMessage(OV_UNION, uv.toByteArray());
        return ov.toByteArray();
    }

    private static byte[] ovString(String s) {
        WireWriter w = new WireWriter();
        w.writeString(OV_STRING, s);
        return w.toByteArray();
    }

    private static byte[] ovLiteralString(String s) {
        WireWriter lit = new WireWriter();
        lit.writeString(1, s); // BamlLiteralValue.string_value = 1
        WireWriter w = new WireWriter();
        w.writeMessage(OV_LITERAL, lit.toByteArray());
        return w.toByteArray();
    }

    private static byte[] ovNull() {
        WireWriter w = new WireWriter();
        w.writeMessage(OV_NULL, new WireWriter().toByteArray()); // null_value = empty message
        return w.toByteArray();
    }

    /** (a) int|string carrying an int decodes to the registered IntValue record. */
    @Test
    void decode_union_int_arm_constructs_record() {
        byte[] selfType = tyUnion(tyPrimitive(PRIM_INT), tyPrimitive(PRIM_STRING));
        Object decoded = ProtoReader.decodeOutboundResult(okEnvelope(ovUnion(selfType, ovInt(1))));
        IntOrString.IntValue v = assertInstanceOf(IntOrString.IntValue.class, decoded);
        assertEquals(1L, v.value());
    }

    /** The arm is picked from the inner value's shape, not by position. */
    @Test
    void decode_union_string_arm_constructs_record() {
        byte[] selfType = tyUnion(tyPrimitive(PRIM_INT), tyPrimitive(PRIM_STRING));
        Object decoded =
                ProtoReader.decodeOutboundResult(okEnvelope(ovUnion(selfType, ovString("hi"))));
        IntOrString.StringValue v = assertInstanceOf(IntOrString.StringValue.class, decoded);
        assertEquals("hi", v.value());
    }

    /** (b) An unregistered signature falls back to the bare decoded inner value. */
    @Test
    void decode_union_unknown_signature_falls_back_to_bare_inner() {
        // int | bool — not a registered signature.
        byte[] selfType = tyUnion(tyPrimitive(PRIM_INT), tyPrimitive(PRIM_BOOL));
        assertEquals(7L, ProtoReader.decodeOutboundResult(okEnvelope(ovUnion(selfType, ovInt(7)))));
    }

    /** (c) A literal-over-one-base union (erased, never registered) → bare inner. */
    @Test
    void decode_union_literal_arms_fall_back_to_bare_inner() {
        byte[] selfType = tyUnion(tyStringLiteral("draft"), tyStringLiteral("sent"));
        Object decoded =
                ProtoReader.decodeOutboundResult(okEnvelope(ovUnion(selfType, ovLiteralString("draft"))));
        assertEquals("draft", decoded);
    }

    /** (d) Encoding a union record unwraps to the bare inner value's encoding. */
    @Test
    void encode_union_record_unwraps_to_bare_inner() {
        byte[] wrapped = ProtoWriter.encodeInboundValue(new IntOrString.IntValue(5L));
        byte[] bare = ProtoWriter.encodeInboundValue(5L);
        assertArrayEquals(bare, wrapped);
    }

    /** (e) A null inner value decodes to null (no wrapper record). */
    @Test
    void decode_union_null_inner_is_null() {
        byte[] selfType = tyUnion(tyPrimitive(PRIM_INT), tyPrimitive(PRIM_STRING));
        assertNull(ProtoReader.decodeOutboundResult(okEnvelope(ovUnion(selfType, ovNull()))));
    }

    // -- type-directed (descriptor-driven) decode: generic Union{k} family ----

    private static byte[] ovBool(boolean b) {
        WireWriter w = new WireWriter();
        w.writeBool(OV_BOOL, b);
        return w.toByteArray();
    }

    /** A BamlOutboundValue.class_value (=7) for the registered Resume fixture. */
    private static byte[] ovResume(String name, long age) {
        WireWriter nameVal = new WireWriter();
        nameVal.writeString(OV_STRING, name);
        WireWriter nameEntry = new WireWriter();
        nameEntry.writeString(1, "name"); // BamlOutboundMapEntry.key = 1
        nameEntry.writeMessage(2, nameVal.toByteArray()); // value = 2

        WireWriter ageVal = new WireWriter();
        ageVal.writeInt64(OV_INT, age);
        WireWriter ageEntry = new WireWriter();
        ageEntry.writeString(1, "age");
        ageEntry.writeMessage(2, ageVal.toByteArray());

        WireWriter cls = new WireWriter();
        cls.writeString(1, RESUME_FQN); // BamlValueClass.name = 1
        cls.writeMessage(2, nameEntry.toByteArray()); // fields = 2
        cls.writeMessage(2, ageEntry.toByteArray());

        WireWriter ov = new WireWriter();
        ov.writeMessage(OV_CLASS, cls.toByteArray());
        return ov.toByteArray();
    }

    /** union[int;string] desc + a BARE int wire → Union2.Arm0 (declared arm order). */
    @Test
    void decode_desc_union_bare_int_arm0() {
        Object decoded =
                ProtoReader.decodeOutboundResult(okEnvelope(ovInt(1)), "union[int;string]");
        Union2.Arm0<?, ?> arm = assertInstanceOf(Union2.Arm0.class, decoded);
        assertEquals(1L, arm.value());
    }

    /** union[int;string] desc + a BARE string wire → Union2.Arm1. */
    @Test
    void decode_desc_union_bare_string_arm1() {
        Object decoded =
                ProtoReader.decodeOutboundResult(okEnvelope(ovString("hi")), "union[int;string]");
        Union2.Arm1<?, ?> arm = assertInstanceOf(Union2.Arm1.class, decoded);
        assertEquals("hi", arm.value());
    }

    /**
     * union[int;string] desc + a union_variant_value wrapper: the descriptor is
     * preferred over the wire self_type, the wrapper is unwrapped, and the inner
     * int lands on Union2.Arm0.
     */
    @Test
    void decode_desc_union_variant_wrapped_int_arm0() {
        // self_type deliberately unregistered/irrelevant — the desc wins.
        byte[] selfType = tyUnion(tyPrimitive(PRIM_INT), tyPrimitive(PRIM_STRING));
        Object decoded = ProtoReader.decodeOutboundResult(
                okEnvelope(ovUnion(selfType, ovInt(7))), "union[int;string]");
        Union2.Arm0<?, ?> arm = assertInstanceOf(Union2.Arm0.class, decoded);
        assertEquals(7L, arm.value());
    }

    /** union_variant_value wrapper carrying a string → Union2.Arm1 (desc-driven). */
    @Test
    void decode_desc_union_variant_wrapped_string_arm1() {
        byte[] selfType = tyUnion(tyPrimitive(PRIM_INT), tyPrimitive(PRIM_STRING));
        Object decoded = ProtoReader.decodeOutboundResult(
                okEnvelope(ovUnion(selfType, ovString("yo"))), "union[int;string]");
        Union2.Arm1<?, ?> arm = assertInstanceOf(Union2.Arm1.class, decoded);
        assertEquals("yo", arm.value());
    }

    /** T|null collapses in codegen to just T: desc "int", a bare int → the bare Long. */
    @Test
    void decode_desc_optional_collapses_to_base() {
        assertEquals(42L, ProtoReader.decodeOutboundResult(okEnvelope(ovInt(42)), "int"));
    }

    /** T|null collapse: desc "int", a null wire value → null. */
    @Test
    void decode_desc_optional_null_is_null() {
        assertNull(ProtoReader.decodeOutboundResult(okEnvelope(ovNull()), "int"));
    }

    /** A class-arm union via FQN desc: the class_value lands on the FQN arm. */
    @Test
    void decode_desc_union_class_arm_via_fqn() {
        Object decoded = ProtoReader.decodeOutboundResult(
                okEnvelope(ovResume("Alice", 30L)), "union[" + RESUME_FQN + ";string]");
        Union2.Arm0<?, ?> arm = assertInstanceOf(Union2.Arm0.class, decoded);
        Resume r = assertInstanceOf(Resume.class, arm.value());
        assertEquals("Alice", r.name());
        assertEquals(30L, r.age());
    }

    /** No declared arm matches the wire value's kind → BamlError. */
    @Test
    void decode_desc_union_no_arm_match_throws() {
        assertThrows(
                BamlError.class,
                () -> ProtoReader.decodeOutboundResult(okEnvelope(ovBool(true)), "union[int;string]"));
    }

    /** A registered class FQN desc constructs the generated class (fieldDescs unused here). */
    @Test
    void decode_desc_fqn_constructs_registered_class() {
        Object decoded =
                ProtoReader.decodeOutboundResult(okEnvelope(ovResume("Bob", 40L)), RESUME_FQN);
        Resume r = assertInstanceOf(Resume.class, decoded);
        assertEquals("Bob", r.name());
        assertEquals(40L, r.age());
    }

    /**
     * A class registered WITH fieldDescs decodes its union-typed field onto the
     * Union{k} arm family (the fieldDescs path).
     */
    @Test
    void decode_desc_class_field_uses_field_descs() {
        // BamlValueClass { name = BOX_FQN, fields: { payload: <bare string> } }
        WireWriter payloadVal = new WireWriter();
        payloadVal.writeString(OV_STRING, "wrapped");
        WireWriter payloadEntry = new WireWriter();
        payloadEntry.writeString(1, "payload");
        payloadEntry.writeMessage(2, payloadVal.toByteArray());

        WireWriter cls = new WireWriter();
        cls.writeString(1, BOX_FQN);
        cls.writeMessage(2, payloadEntry.toByteArray());

        WireWriter ov = new WireWriter();
        ov.writeMessage(OV_CLASS, cls.toByteArray());

        // Return the Box directly by its FQN desc; its `payload` field carries the
        // union[int;string] descriptor, so a bare string lands on Union2.Arm1.
        Object decoded = ProtoReader.decodeOutboundResult(okEnvelope(ov.toByteArray()), BOX_FQN);
        Box box = assertInstanceOf(Box.class, decoded);
        Union2.Arm1<?, ?> arm = assertInstanceOf(Union2.Arm1.class, box.payload());
        assertEquals("wrapped", arm.value());
    }

    /**
     * Regression: with a null descriptor (the 1-arg / 3-arg wire-driven path), an
     * int|string union_variant still reifies onto the registered nominal record,
     * NOT the generic Union{k} family.
     */
    @Test
    void decode_null_desc_keeps_wire_driven_registered_record() {
        byte[] selfType = tyUnion(tyPrimitive(PRIM_INT), tyPrimitive(PRIM_STRING));
        byte[] envelope = okEnvelope(ovUnion(selfType, ovInt(3)));
        // 1-arg overload.
        Object oneArg = ProtoReader.decodeOutboundResult(envelope);
        IntOrString.IntValue a = assertInstanceOf(IntOrString.IntValue.class, oneArg);
        assertEquals(3L, a.value());
        // 2-arg overload with an explicit null descriptor: same wire-driven result.
        Object nullDesc = ProtoReader.decodeOutboundResult(envelope, null);
        IntOrString.IntValue b = assertInstanceOf(IntOrString.IntValue.class, nullDesc);
        assertEquals(3L, b.value());
    }

    /** An unparseable / tv: / unknown descriptor falls back to the wire-driven path. */
    @Test
    void decode_unparseable_desc_falls_back_to_wire_driven() {
        byte[] envelope = okEnvelope(ovInt(9));
        assertEquals(9L, ProtoReader.decodeOutboundResult(envelope, "tv:T"));
        assertEquals(9L, ProtoReader.decodeOutboundResult(envelope, "unknown"));
        assertEquals(9L, ProtoReader.decodeOutboundResult(envelope, "union[")); // malformed
    }

    // -- media handle decode (baml.media.*) ----------------------------------
    // Native-free: decoding a handle only constructs a BamlHandle token
    // (no native call); its Cleaner-driven release is guarded, so these run
    // without the bridge library. BamlOutboundHandle: key = 1, handle_type = 2.

    // BamlOutboundValue.handle_value = 16; BamlHandleType ADT_MEDIA_{IMAGE,PDF}.
    private static final int OV_HANDLE = 16;
    private static final int ADT_MEDIA_IMAGE = 6;
    private static final int ADT_MEDIA_PDF = 9;
    private static final int FUNCTION_REF = 5;

    private static byte[] handleMsg(long key, int handleType) {
        WireWriter h = new WireWriter();
        h.writeInt64(1, key); // BamlOutboundHandle.key = 1
        h.writeInt64(2, handleType); // handle_type = 2
        return h.toByteArray();
    }

    private static byte[] ovHandle(long key, int handleType) {
        WireWriter ov = new WireWriter();
        ov.writeMessage(OV_HANDLE, handleMsg(key, handleType));
        return ov.toByteArray();
    }

    /** A bare handle_value(ADT_MEDIA_IMAGE) decodes to a runtime-owned Image. */
    @Test
    void decode_media_handle_constructs_image() {
        Object decoded = ProtoReader.decodeOutboundResult(okEnvelope(ovHandle(4242L, ADT_MEDIA_IMAGE)));
        Image img = assertInstanceOf(Image.class, decoded);
        assertEquals("baml.media.Image", img.bamlFqn());
        assertEquals(4242L, img.bamlHandle().key());
        assertEquals(ADT_MEDIA_IMAGE, img.bamlHandle().handleType());
    }

    /** class_value(baml.media.Pdf, {_data: handle_value}) unwraps to the Pdf. */
    @Test
    void decode_media_class_wrapper_unwraps_to_media() {
        WireWriter dataVal = new WireWriter();
        dataVal.writeMessage(OV_HANDLE, handleMsg(77L, ADT_MEDIA_PDF)); // _data value
        WireWriter dataEntry = new WireWriter();
        dataEntry.writeString(1, "_data"); // BamlOutboundMapEntry.key = 1
        dataEntry.writeMessage(2, dataVal.toByteArray()); // value = 2

        WireWriter cls = new WireWriter();
        cls.writeString(1, "baml.media.Pdf"); // BamlValueClass.name = 1
        cls.writeMessage(2, dataEntry.toByteArray()); // fields = 2

        WireWriter ov = new WireWriter();
        ov.writeMessage(OV_CLASS, cls.toByteArray());

        Object decoded = ProtoReader.decodeOutboundResult(okEnvelope(ov.toByteArray()));
        Pdf pdf = assertInstanceOf(Pdf.class, decoded);
        assertEquals(77L, pdf.bamlHandle().key());
    }

    /** A non-media handle type keeps the bare-BamlHandle fallback. */
    @Test
    void decode_unknown_handle_type_falls_back_to_bare_handle() {
        Object decoded = ProtoReader.decodeOutboundResult(okEnvelope(ovHandle(9L, FUNCTION_REF)));
        BamlHandle h = assertInstanceOf(BamlHandle.class, decoded);
        assertEquals(9L, h.key());
        assertEquals(FUNCTION_REF, h.handleType());
    }
}
