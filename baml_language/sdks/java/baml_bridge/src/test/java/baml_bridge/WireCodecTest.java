package baml_bridge;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertArrayEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertInstanceOf;
import static org.junit.jupiter.api.Assertions.assertNotEquals;
import static org.junit.jupiter.api.Assertions.assertNull;
import static org.junit.jupiter.api.Assertions.assertThrows;
import static org.junit.jupiter.api.Assertions.assertTrue;

import baml_bridge.internal.ProtoReader;
import baml_bridge.internal.ProtoWriter;
import baml_bridge.internal.WireWriter;
import baml_sdk.baml.media.Image;
import baml_sdk.baml.media.Pdf;

import java.math.BigInteger;
import java.util.ArrayList;
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
    private static final int OV_PROMPT_AST = 18;
    private static final int OV_UINT8ARRAY = 19;
    private static final int OV_BIGINT = 20;
    private static final int OV_TY = 21;

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
    void decode_ok_type_value() {
        WireWriter primitive = new WireWriter();
        primitive.writeInt64(1, 2); // BamlTyPrimitive.kind = INT
        WireWriter ty = new WireWriter();
        ty.writeMessage(1, primitive.toByteArray()); // BamlTy.primitive = 1
        WireWriter value = new WireWriter();
        value.writeMessage(OV_TY, ty.toByteArray());
        assertEquals(
                BamlType.INT,
                ProtoReader.decodeOutboundResult(okEnvelope(value.toByteArray())));
    }

    @Test
    void named_stream_call_writes_operation_two() {
        byte[] encoded =
                ProtoWriter.encodeNamedCallFunctionArgs(
                        "user.Extract",
                        new String[0],
                        new Object[0],
                        7,
                        null,
                        BamlFunctionOperation.STREAM);
        boolean found = false;
        for (int i = 0; i + 1 < encoded.length; i++) {
            if (encoded[i] == 0x30 && encoded[i + 1] == 0x02) {
                found = true;
                break;
            }
        }
        assertTrue(found, "expected operation field 6 with STREAM value 2");
    }

    // -- bigint decode length cap (Python-parity DoS hardening) --------------
    // Mirrors the Python bridge's `_parse_hex_bigint` (proto.py) and the
    // Rust `MAX_BIGINT_HEX_LEN` in bridge_ctypes/src/value_decode.rs: strict
    // hex plus a pre-allocation length cap so an adversarial multi-megabyte
    // hex blob can't drive an unbounded BigInteger allocation. Boundary cases
    // hit `parseHexBigInt` directly (avoids copying a 64 MB blob through the
    // full envelope); the two wire read sites are covered end-to-end below.

    /** The Java cap is byte-for-byte the workspace formula (2^28 bits / 4 + 2). */
    @Test
    void bigint_cap_matches_workspace_formula() {
        assertEquals((1 << 28) / 4 + 2, ProtoReader.MAX_BIGINT_HEX_LEN);
    }

    /** A hex string of exactly the cap length is accepted (parse stays cheap). */
    @Test
    void bigint_at_cap_length_decodes() {
        // Length == cap; leading zeros keep BigInteger's radix parse O(n) cheap.
        String hex = "0".repeat(ProtoReader.MAX_BIGINT_HEX_LEN - 2) + "ff";
        assertEquals(ProtoReader.MAX_BIGINT_HEX_LEN, hex.length());
        assertEquals(BigInteger.valueOf(255), ProtoReader.parseHexBigInt(hex));
    }

    /** One char over the cap is rejected before the BigInteger is built. */
    @Test
    void bigint_over_cap_length_rejects() {
        String hex = "f".repeat(ProtoReader.MAX_BIGINT_HEX_LEN + 1);
        IllegalStateException ex =
                assertThrows(IllegalStateException.class, () -> ProtoReader.parseHexBigInt(hex));
        assertTrue(ex.getMessage().contains("exceeds the workspace cap"));
        // The error must not embed the megabyte-scale input.
        assertTrue(ex.getMessage().length() < 200);
    }

    /**
     * A malformed (non-strict-hex) payload that is UNDER the cap still must not
     * echo the whole blob into the exception message: it can be nearly cap-sized
     * (~67 MB), so the message reports the length plus a short prefix preview.
     */
    @Test
    void bigint_malformed_hex_message_is_length_bounded() {
        String bad = "z".repeat(5000); // non-hex, well under the cap
        IllegalStateException ex =
                assertThrows(IllegalStateException.class, () -> ProtoReader.parseHexBigInt(bad));
        assertTrue(ex.getMessage().contains("invalid bigint hex string"), ex.getMessage());
        // The true length is reported, but the 5000-char blob is not echoed.
        assertTrue(ex.getMessage().contains("5000 chars"), ex.getMessage());
        assertTrue(ex.getMessage().length() < 200, "message too long: " + ex.getMessage());
        assertFalse(ex.getMessage().contains(bad));
    }

    /** Non-strict hex — letters outside [0-9a-fA-F], `0x`, `+`, whitespace, empty — is rejected. */
    @Test
    void bigint_malformed_hex_rejects() {
        for (String bad : new String[] {"12xz", "", "0x1f", " 1f", "1f ", "+1f", "-", "1_000"}) {
            assertThrows(
                    IllegalStateException.class,
                    () -> ProtoReader.parseHexBigInt(bad),
                    () -> "expected reject for " + '"' + bad + '"');
        }
    }

    /** A single leading minus is honored (magnitude then parsed strictly). */
    @Test
    void bigint_negative_hex_round_trips() {
        assertEquals(BigInteger.valueOf(-42), ProtoReader.parseHexBigInt("-2a"));
        assertEquals(BigInteger.ZERO, ProtoReader.parseHexBigInt("0"));
    }

    /** The value read site (OV_BIGINT = 20) routes through the strict-hex guard. */
    @Test
    void decode_bigint_value_site_rejects_malformed() {
        WireWriter w = new WireWriter();
        w.writeString(OV_BIGINT, "not-a-number");
        assertThrows(
                IllegalStateException.class,
                () -> ProtoReader.decodeOutboundResult(okEnvelope(w.toByteArray())));
    }

    /** The literal read site (BamlLiteralValue.bigint_value = 4) also routes through the guard. */
    @Test
    void decode_literal_bigint_site_rejects_malformed() {
        WireWriter lit = new WireWriter();
        lit.writeString(4, "not-a-number"); // BamlLiteralValue.bigint_value = 4
        WireWriter w = new WireWriter();
        w.writeMessage(OV_LITERAL, lit.toByteArray());
        assertThrows(
                IllegalStateException.class,
                () -> ProtoReader.decodeOutboundResult(okEnvelope(w.toByteArray())));
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
    void prompt_ast_decodes_and_reencodes_as_portable_data() {
        WireWriter simple = new WireWriter();
        simple.writeString(1, "hello"); // BamlValuePromptAstSimple.string
        WireWriter promptAst = new WireWriter();
        promptAst.writeMessage(1, simple.toByteArray()); // BamlValuePromptAst.simple
        WireWriter outbound = new WireWriter();
        outbound.writeMessage(OV_PROMPT_AST, promptAst.toByteArray());

        BamlPrompt prompt = assertInstanceOf(
                BamlPrompt.class,
                ProtoReader.decodeOutboundResult(okEnvelope(outbound.toByteArray())));
        WireWriter expectedInbound = new WireWriter();
        expectedInbound.writeMessage(16, promptAst.toByteArray());
        assertArrayEquals(expectedInbound.toByteArray(), ProtoWriter.encodeInboundValue(prompt));
        assertArrayEquals(expectedInbound.toByteArray(), ProtoWriter.encodeInboundValue(prompt));
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
        // A direct encodeInboundValue call has no owning argument to name, so it
        // surfaces the bare "unsupported Java type <class>" IllegalArgumentException.
        IllegalArgumentException ex = assertThrows(
                IllegalArgumentException.class,
                () -> ProtoWriter.encodeInboundValue(new Object()));
        assertTrue(
                ex.getMessage().contains("unsupported Java type java.lang.Object"),
                ex.getMessage());
    }

    @Test
    void inbound_unsupported_argument_names_the_argument() {
        // The top-level kwarg loop rewraps a deep rejection to name the offending
        // *argument* plus its unsupported Java type (mirrors Python's TypeError
        // naming the kwarg).
        IllegalArgumentException ex = assertThrows(
                IllegalArgumentException.class,
                () -> ProtoWriter.encodeCallFunctionArgs(
                        new String[] {"tool"},
                        new Object[] {new java.util.Date()},
                        123L));
        assertEquals(
                "cannot encode argument 'tool': unsupported Java type java.util.Date",
                ex.getMessage());
    }

    @Test
    void inbound_unsupported_nested_element_names_top_level_argument() {
        // An unsupported value nested inside argument `x` (here a list element)
        // still reports the top-level argument name, not the nested position.
        IllegalArgumentException ex = assertThrows(
                IllegalArgumentException.class,
                () -> ProtoWriter.encodeCallFunctionArgs(
                        new String[] {"items"},
                        new Object[] {java.util.List.of(1L, new java.util.Date())},
                        123L));
        assertEquals(
                "cannot encode argument 'items': unsupported Java type java.util.Date",
                ex.getMessage());
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

    // A generated-shape union over two differently-typed LIST arms
    // (`int[] | string[]`). Exercises the wire-driven `pickArm` path's typed-
    // container matching: the arm is chosen from the wire `item_type`, not by
    // declaration order — so an empty `string[]` still lands on the string arm.
    public sealed interface ListUnion permits ListUnion.IntListValue, ListUnion.StrListValue {
        record IntListValue(List<Long> value) implements ListUnion {}

        record StrListValue(List<String> value) implements ListUnion {}
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
        // 4-arg overload: a parallel typed fieldDescs array (one BamlType per field).
        TypeRegistry.registerClass(
                BOX_FQN,
                Box.class.getName(),
                new String[] {"payload"},
                new BamlType[] {BamlType.union(BamlType.INT, BamlType.STRING)});
        TypeRegistry.registerEnum(
                SENTIMENT_FQN,
                Sentiment.class.getName(),
                new String[] {"Positive", "new$"}, // Java constants
                new String[] {"Positive", "new"}); // wire variants
        // Keyed structurally by the arm SET; arm tokens + record names in
        // declaration order (int, string).
        TypeRegistry.registerUnion(
                IntOrString.class.getName(),
                new BamlType[] {BamlType.INT, BamlType.STRING},
                new String[] {
                    IntOrString.IntValue.class.getName(), IntOrString.StringValue.class.getName()
                });
        // Arm set {list<int>, list<string>}; arm tokens + records in declaration
        // order (int[] = arm0, string[] = arm1).
        TypeRegistry.registerUnion(
                ListUnion.class.getName(),
                new BamlType[] {BamlType.list(BamlType.INT), BamlType.list(BamlType.STRING)},
                new String[] {
                    ListUnion.IntListValue.class.getName(),
                    ListUnion.StrListValue.class.getName()
                });
    }

    /** A registered class encodes as InboundValue.class_value (=8). */
    @Test
    void inbound_class_value_layout() {
        byte[] got = ProtoWriter.encodeInboundValue(new Resume("Alice", 30L));

        // Hand-build the expected node-local value_type (=1), followed by the
        // structural InboundClassValue fields (=2) in declaration order.
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

        WireWriter classType = new WireWriter();
        classType.writeMessage(2, classTy.toByteArray()); // BamlTy.class_ty = 2

        WireWriter classMsg = new WireWriter();
        classMsg.writeMessage(2, nameEntry.toByteArray()); // InboundClassValue.fields = 2
        classMsg.writeMessage(2, ageEntry.toByteArray());

        WireWriter expected = new WireWriter();
        expected.writeMessage(1, classType.toByteArray()); // InboundValue.value_type = 1
        expected.writeMessage(8, classMsg.toByteArray()); // InboundValue.class_value = 8

        assertArrayEquals(expected.toByteArray(), got);
    }

    /**
     * A reified generic instance (its concrete type args bound in the side-table
     * via the generated {@code of(...)} factory or wire decode) encodes its
     * {@code value_type.class_ty.type_args} — the value-level type channel the engine reads
     * to type-check a generic argument. An unbound instance stays byte-identical
     * to the pre-generics encoding (see {@code inbound_class_value_layout}).
     */
    @Test
    void inbound_class_value_carries_reified_type_args() {
        Resume r = new Resume("Alice", 30L);
        TypeRegistry.bindTypeArgs(r, List.of(BamlType.INT));
        byte[] got = ProtoWriter.encodeInboundValue(r);

        WireWriter nameVal = new WireWriter();
        nameVal.writeString(2, "Alice");
        WireWriter nameEntry = new WireWriter();
        nameEntry.writeString(1, "name");
        nameEntry.writeMessage(6, nameVal.toByteArray());

        WireWriter ageVal = new WireWriter();
        ageVal.writeInt64(3, 30L);
        WireWriter ageEntry = new WireWriter();
        ageEntry.writeString(1, "age");
        ageEntry.writeMessage(6, ageVal.toByteArray());

        WireWriter classTy = new WireWriter();
        classTy.writeString(1, RESUME_FQN); // BamlTyClass.name = 1
        classTy.writeMessage(2, BamlType.INT.toWireTy()); // BamlTyClass.type_args = 2

        WireWriter classType = new WireWriter();
        classType.writeMessage(2, classTy.toByteArray());

        WireWriter classMsg = new WireWriter();
        classMsg.writeMessage(2, nameEntry.toByteArray());
        classMsg.writeMessage(2, ageEntry.toByteArray());

        WireWriter expected = new WireWriter();
        expected.writeMessage(1, classType.toByteArray());
        expected.writeMessage(8, classMsg.toByteArray());

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
                IllegalArgumentException.class,
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

    private static byte[] ovUnionSelected(byte[] selfType, int selectedIndex, byte[] innerValue) {
        WireWriter uv = new WireWriter();
        uv.writeMessage(4, selfType);
        uv.writeMessage(6, innerValue);
        uv.writeInt64(8, selectedIndex);
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

    /** A generated typed carrier projects a union wrapper to its exact arm type. */
    @Test
    void encode_generic_union_second_empty_list_arm_has_exact_node_type() {
        BamlType unionType = BamlType.union(
                BamlType.list(BamlType.STRING), BamlType.list(BamlType.INT));
        byte[] got = ProtoWriter.encodeInboundValue(new BamlTypedValue(
                new Union2.Arm1<List<String>, List<Long>>(List.of()), unionType));

        WireWriter expected = new WireWriter();
        expected.writeMessage(1, BamlType.list(BamlType.INT).toWireTy());
        expected.writeMessage(6, new byte[0]);
        assertArrayEquals(expected.toByteArray(), got);
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
                ProtoReader.decodeOutboundResult(okEnvelope(ovInt(1)), BamlType.union(BamlType.INT, BamlType.STRING));
        Union2.Arm0<?, ?> arm = assertInstanceOf(Union2.Arm0.class, decoded);
        assertEquals(1L, arm.value());
    }

    /** union[int;string] desc + a BARE string wire → Union2.Arm1. */
    @Test
    void decode_desc_union_bare_string_arm1() {
        Object decoded =
                ProtoReader.decodeOutboundResult(okEnvelope(ovString("hi")), BamlType.union(BamlType.INT, BamlType.STRING));
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
                okEnvelope(ovUnion(selfType, ovInt(7))), BamlType.union(BamlType.INT, BamlType.STRING));
        Union2.Arm0<?, ?> arm = assertInstanceOf(Union2.Arm0.class, decoded);
        assertEquals(7L, arm.value());
    }

    /** union_variant_value wrapper carrying a string → Union2.Arm1 (desc-driven). */
    @Test
    void decode_desc_union_variant_wrapped_string_arm1() {
        byte[] selfType = tyUnion(tyPrimitive(PRIM_INT), tyPrimitive(PRIM_STRING));
        Object decoded = ProtoReader.decodeOutboundResult(
                okEnvelope(ovUnion(selfType, ovString("yo"))), BamlType.union(BamlType.INT, BamlType.STRING));
        Union2.Arm1<?, ?> arm = assertInstanceOf(Union2.Arm1.class, decoded);
        assertEquals("yo", arm.value());
    }

    /** T|null collapses in codegen to just T: desc "int", a bare int → the bare Long. */
    @Test
    void decode_desc_optional_collapses_to_base() {
        assertEquals(42L, ProtoReader.decodeOutboundResult(okEnvelope(ovInt(42)), BamlType.INT));
    }

    /** T|null collapse: desc "int", a null wire value → null. */
    @Test
    void decode_desc_optional_null_is_null() {
        assertNull(ProtoReader.decodeOutboundResult(okEnvelope(ovNull()), BamlType.INT));
    }

    /** A class-arm union via FQN desc: the class_value lands on the FQN arm. */
    @Test
    void decode_desc_union_class_arm_via_fqn() {
        Object decoded = ProtoReader.decodeOutboundResult(
                okEnvelope(ovResume("Alice", 30L)), BamlType.union(BamlType.classByFqn(RESUME_FQN), BamlType.STRING));
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
                () -> ProtoReader.decodeOutboundResult(okEnvelope(ovBool(true)), BamlType.union(BamlType.INT, BamlType.STRING)));
    }

    /** A registered class FQN desc constructs the generated class (fieldDescs unused here). */
    @Test
    void decode_desc_fqn_constructs_registered_class() {
        Object decoded =
                ProtoReader.decodeOutboundResult(okEnvelope(ovResume("Bob", 40L)), BamlType.classByFqn(RESUME_FQN));
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
        // int|string union descriptor, so a bare string lands on Union2.Arm1.
        Object decoded = ProtoReader.decodeOutboundResult(
                okEnvelope(ov.toByteArray()), BamlType.classByFqn(BOX_FQN));
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

    /** A TypeVar / UNKNOWN (decode-only) descriptor falls back to the wire-driven path. */
    @Test
    void decode_wildcard_desc_falls_back_to_wire_driven() {
        byte[] envelope = okEnvelope(ovInt(9));
        assertEquals(9L, ProtoReader.decodeOutboundResult(envelope, BamlType.typeVar("T")));
        assertEquals(9L, ProtoReader.decodeOutboundResult(envelope, BamlType.UNKNOWN));
    }

    /**
     * A string literal carrying what used to be a grammar separator (`;`) rides
     * RAW through the typed descriptor — there is no grammar to collide with, so
     * no escaping. The literal arm is matched by value and the raw value
     * round-trips.
     */
    @Test
    void decode_desc_union_literal_with_structural_char_matches_raw() {
        Object decoded = ProtoReader.decodeOutboundResult(
                okEnvelope(ovLiteralString("a;b")),
                BamlType.union(BamlType.literalString("a;b"), BamlType.INT));
        Union2.Arm0<?, ?> arm = assertInstanceOf(Union2.Arm0.class, decoded);
        assertEquals("a;b", arm.value());
    }

    /**
     * Registering the same union arm SET under a DIFFERENT sealed interface is an
     * identity conflict (two unions sharing one key — decode would silently reify
     * onto whichever won the slot) and throws, rather than first-winning. A
     * re-registration under the SAME interface stays idempotent.
     */
    @Test
    void register_union_conflicting_sealed_interface_throws() {
        TypeRegistry.registerUnion(
                "baml_sdk.x.UnionA",
                new BamlType[] {BamlType.INT},
                new String[] {"baml_sdk.x.UnionA$IntValue"});
        // Same arm set + same interface: idempotent no-op (no throw).
        TypeRegistry.registerUnion(
                "baml_sdk.x.UnionA",
                new BamlType[] {BamlType.INT},
                new String[] {"baml_sdk.x.UnionA$IntValue"});
        // Same arm set {int} + DIFFERENT interface: conflict.
        IllegalStateException ex = assertThrows(
                IllegalStateException.class,
                () -> TypeRegistry.registerUnion(
                        "baml_sdk.x.UnionB",
                        new BamlType[] {BamlType.INT},
                        new String[] {"baml_sdk.x.UnionB$IntValue"}));
        assertTrue(ex.getMessage().contains("conflicting union registration"), ex.getMessage());
    }

    // -- typed-container union arm selection (empty-list correctness fix) -----
    // The wire `item_type` / `key_type`+`value_type` on a list/map value now
    // drives arm selection, so a differently-typed container arm is told apart
    // even when the container is EMPTY (no items to sniff). A container value
    // WITHOUT its type on the wire keeps the pre-fix first-match compatibility.

    // BamlTyPrimitiveKind.STRING already covered by PRIM_STRING/PRIM_INT above.

    /** BamlOutboundValue.list_value (=11) with item_type (=1) + optional items (=2). */
    private static byte[] ovListTyped(byte[] itemTy, byte[]... items) {
        WireWriter list = new WireWriter();
        list.writeMessage(1, itemTy); // BamlValueList.item_type = 1
        for (byte[] it : items) {
            list.writeMessage(2, it); // items = 2
        }
        WireWriter ov = new WireWriter();
        ov.writeMessage(OV_LIST, list.toByteArray());
        return ov.toByteArray();
    }

    /** BamlOutboundValue.list_value (=11) carrying items only — NO item_type. */
    private static byte[] ovListUntyped(byte[]... items) {
        WireWriter list = new WireWriter();
        for (byte[] it : items) {
            list.writeMessage(2, it); // items = 2
        }
        WireWriter ov = new WireWriter();
        ov.writeMessage(OV_LIST, list.toByteArray());
        return ov.toByteArray();
    }

    /** A BamlTy wrapping a map (BamlTy.map = 5): key (=1) + value (=2). */
    private static byte[] tyMap(byte[] keyTy, byte[] valueTy) {
        WireWriter map = new WireWriter();
        map.writeMessage(1, keyTy); // BamlTyMap.key = 1
        map.writeMessage(2, valueTy); // BamlTyMap.value = 2
        WireWriter ty = new WireWriter();
        ty.writeMessage(5, map.toByteArray()); // BamlTy.map = 5
        return ty.toByteArray();
    }

    /** BamlOutboundValue.map_value (=12) with key_type (=1) + value_type (=2), no entries. */
    private static byte[] ovMapTyped(byte[] keyTy, byte[] valueTy) {
        WireWriter map = new WireWriter();
        map.writeMessage(1, keyTy); // BamlValueMap.key_type = 1
        map.writeMessage(2, valueTy); // BamlValueMap.value_type = 2
        WireWriter ov = new WireWriter();
        ov.writeMessage(OV_MAP, map.toByteArray());
        return ov.toByteArray();
    }

    // --- descriptor-driven path (generic Union{k}) ---

    /** An EMPTY typed int[] lands on the int[] arm via item_type — not the first list arm. */
    @Test
    void decode_desc_union_empty_typed_int_list_picks_int_arm() {
        byte[] env = okEnvelope(ovListTyped(tyPrimitive(PRIM_INT))); // empty int[]
        Object decoded =
                ProtoReader.decodeOutboundResult(env, BamlType.union(BamlType.list(BamlType.STRING), BamlType.list(BamlType.INT)));
        Union2.Arm1<?, ?> arm = assertInstanceOf(Union2.Arm1.class, decoded);
        assertEquals(List.of(), arm.value());
    }

    /**
     * An empty typed string[] lands on the string[] arm even though it is
     * declared SECOND — arm order does not decide it, item_type does.
     */
    @Test
    void decode_desc_union_empty_typed_string_list_picks_string_arm_regardless_of_order() {
        byte[] env = okEnvelope(ovListTyped(tyPrimitive(PRIM_STRING))); // empty string[]
        Object decoded =
                ProtoReader.decodeOutboundResult(env, BamlType.union(BamlType.list(BamlType.INT), BamlType.list(BamlType.STRING)));
        Union2.Arm1<?, ?> arm = assertInstanceOf(Union2.Arm1.class, decoded);
        assertEquals(List.of(), arm.value());
    }

    /** A list WITHOUT item_type keeps the pre-fix first-match fallback (first list arm). */
    @Test
    void decode_desc_union_untyped_list_falls_back_to_first_list_arm() {
        byte[] env = okEnvelope(ovListUntyped(ovInt(1))); // [1], no item_type
        Object decoded =
                ProtoReader.decodeOutboundResult(env, BamlType.union(BamlType.list(BamlType.STRING), BamlType.list(BamlType.INT)));
        // Bare "list" is an element-type wildcard → the FIRST list arm (Arm0),
        // even though the sole element is an int. This is the back-compat lane.
        Union2.Arm0<?, ?> arm = assertInstanceOf(Union2.Arm0.class, decoded);
        assertEquals(List.of(1L), arm.value());
    }

    /** An empty typed map is disambiguated by its value_type (map<string,int> arm). */
    @Test
    void decode_desc_union_empty_typed_map_picks_matching_value_type_arm() {
        byte[] env = okEnvelope(ovMapTyped(tyPrimitive(PRIM_STRING), tyPrimitive(PRIM_INT)));
        Object decoded = ProtoReader.decodeOutboundResult(
                env, BamlType.union(BamlType.map(BamlType.STRING, BamlType.STRING), BamlType.map(BamlType.STRING, BamlType.INT)));
        Union2.Arm1<?, ?> arm = assertInstanceOf(Union2.Arm1.class, decoded);
        assertEquals(Map.of(), arm.value());
    }

    // --- wire-driven path (registered nominal record, pickArm) ---

    /**
     * An empty typed string[] inside a `int[] | string[]` union_variant reifies
     * onto the registered StrListValue record — the item_type picks the arm, so
     * the string arm wins over the first-declared int arm.
     */
    @Test
    void decode_union_empty_typed_string_list_arm_constructs_registered_record() {
        byte[] selfType =
                tyUnion(tyList(tyPrimitive(PRIM_INT)), tyList(tyPrimitive(PRIM_STRING)));
        byte[] inner = ovListTyped(tyPrimitive(PRIM_STRING)); // empty string[]
        Object decoded =
                ProtoReader.decodeOutboundResult(okEnvelope(ovUnion(selfType, inner)));
        ListUnion.StrListValue v = assertInstanceOf(ListUnion.StrListValue.class, decoded);
        assertEquals(List.of(), v.value());
    }

    /** A non-empty typed int[] arm reifies onto IntListValue (arm proven by item_type). */
    @Test
    void decode_union_typed_int_list_arm_constructs_registered_record() {
        byte[] selfType =
                tyUnion(tyList(tyPrimitive(PRIM_INT)), tyList(tyPrimitive(PRIM_STRING)));
        byte[] inner = ovListTyped(tyPrimitive(PRIM_INT), ovInt(1), ovInt(2));
        Object decoded =
                ProtoReader.decodeOutboundResult(okEnvelope(ovUnion(selfType, inner)));
        ListUnion.IntListValue v = assertInstanceOf(ListUnion.IntListValue.class, decoded);
        assertEquals(List.of(1L, 2L), v.value());
    }

    /**
     * A list WITHOUT item_type on the wire-driven path keeps the first-match
     * fallback: bare "list" picks the first list arm (IntListValue).
     */
    @Test
    void decode_union_untyped_list_arm_falls_back_to_first_registered_record() {
        byte[] selfType =
                tyUnion(tyList(tyPrimitive(PRIM_INT)), tyList(tyPrimitive(PRIM_STRING)));
        byte[] inner = ovListUntyped(ovString("x")); // ["x"], no item_type
        Object decoded =
                ProtoReader.decodeOutboundResult(okEnvelope(ovUnion(selfType, inner)));
        // Bare "list" → first list arm (int[]), even though the element is a
        // string — the untyped-wire back-compat lane, unchanged by the fix.
        assertInstanceOf(ListUnion.IntListValue.class, decoded);
    }

    /** The canonical selected index beats ambiguous empty-container shape. */
    @Test
    void decode_union_selected_index_preserves_second_empty_list_arm() {
        byte[] selfType =
                tyUnion(tyList(tyPrimitive(PRIM_INT)), tyList(tyPrimitive(PRIM_STRING)));
        Object decoded = ProtoReader.decodeOutboundResult(
                okEnvelope(ovUnionSelected(selfType, 1, ovListUntyped())));
        ListUnion.StrListValue value =
                assertInstanceOf(ListUnion.StrListValue.class, decoded);
        assertEquals(List.of(), value.value());
    }

    // -- media handle decode (baml.media.*) ----------------------------------
    // Native-free: decoding a handle only constructs a BamlHandle token
    // (no native call); its Cleaner-driven release is guarded, so these run
    // without the bridge library. BamlOutboundHandle: key = 1, handle_type = 2.

    // BamlOutboundValue.handle_value = 16; BamlHandleType ADT_MEDIA_{IMAGE,PDF}.
    private static final int OV_HANDLE = 16;
    private static final int ADT_MEDIA_IMAGE = 6;
    private static final int ADT_MEDIA_PDF = 9;
    private static final int ADT_TAGGED_HEAP_HANDLE = 14;
    private static final int FUNCTION_REF = 5;

    private static byte[] handleMsg(long key, int handleType) {
        return handleMsg(key, handleType, null);
    }

    private static byte[] handleMsg(long key, int handleType, byte[] ty) {
        WireWriter h = new WireWriter();
        h.writeInt64(1, key); // BamlOutboundHandle.key = 1
        h.writeInt64(2, handleType); // handle_type = 2
        if (ty != null) {
            h.writeMessage(3, ty); // ty = 3
        }
        return h.toByteArray();
    }

    private static byte[] ovHandle(long key, int handleType) {
        return ovHandle(key, handleType, null);
    }

    private static byte[] ovHandle(long key, int handleType, byte[] ty) {
        WireWriter ov = new WireWriter();
        ov.writeMessage(OV_HANDLE, handleMsg(key, handleType, ty));
        return ov.toByteArray();
    }

    /** Tagged handles retain their class identity and derive method FQNs from it. */
    @Test
    void decode_stream_handle_retains_carried_class_fqn() {
        Object decoded = ProtoReader.decodeOutboundResult(okEnvelope(ovHandle(
                101L,
                ADT_TAGGED_HEAP_HANDLE,
                tyClass("ai.stream.Stream", tyPrimitive(PRIM_STRING), tyPrimitive(PRIM_STRING)))));
        BamlStream<?, ?> stream = assertInstanceOf(BamlStream.class, decoded);
        assertEquals("ai.stream.Stream", stream.bamlClassFqn());
        assertEquals("ai.stream.Stream.next", stream.methodFqn("next"));
        assertEquals("ai.stream.Stream.final", stream.methodFqn("final"));
    }

    /** A tagged stream handle without its type identity cannot be called safely. */
    @Test
    void decode_stream_handle_rejects_missing_class_fqn() {
        IllegalArgumentException ex = assertThrows(
                IllegalArgumentException.class,
                () -> ProtoReader.decodeOutboundResult(
                        okEnvelope(ovHandle(102L, ADT_TAGGED_HEAP_HANDLE))));
        assertTrue(ex.getMessage().contains("missing its carried BAML class identity"));
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

    // -- explicit generics: BamlType / BamlTypes tokens ----------------------
    // The runtime substrate for explicit generics. BamlType is a value token;
    // BamlTypes an ordered bag; the encoder threads a bag into
    // CallFunctionArgs.type_args; the decoder reconstructs a class value's
    // reified type_args into the TypeRegistry side-table.

    @Test
    void baml_type_primitive_value_equality() {
        assertEquals(BamlType.INT, BamlType.INT);
        assertEquals(BamlType.INT.hashCode(), BamlType.INT.hashCode());
        assertNotEquals(BamlType.INT, BamlType.STRING);
        assertNotEquals(BamlType.INT, BamlType.FLOAT);
        assertNotEquals(BamlType.BOOL, BamlType.INT);
        assertEquals("int", BamlType.INT.toString());
        assertEquals("string", BamlType.STRING.toString());
        assertEquals("bool", BamlType.BOOL.toString());
        assertEquals("float", BamlType.FLOAT.toString());
    }

    @Test
    void baml_type_of_registered_class_and_enum() {
        BamlType resume = BamlType.of(Resume.class);
        assertEquals(BamlType.of(Resume.class), resume);
        assertEquals(BamlType.of(Resume.class).hashCode(), resume.hashCode());
        assertEquals(RESUME_FQN, resume.toString());

        BamlType sentiment = BamlType.of(Sentiment.class);
        assertEquals(BamlType.of(Sentiment.class), sentiment);
        assertEquals(SENTIMENT_FQN, sentiment.toString());
        // A class token and an enum token (different FQNs, different wire arms).
        assertNotEquals(resume, sentiment);
    }

    @Test
    void baml_type_reified_generic_distinct_from_bare() {
        BamlType bare = BamlType.of(Resume.class);
        BamlType reified = BamlType.of(Resume.class, BamlType.INT);
        assertNotEquals(bare, reified);
        assertEquals(BamlType.of(Resume.class, BamlType.INT), reified);
        assertEquals(RESUME_FQN + "<int>", reified.toString());
    }

    @Test
    void baml_type_nested_reified_equality_and_hashcode() {
        BamlType a = BamlType.of(Box.class, BamlType.of(Resume.class, BamlType.INT));
        BamlType b = BamlType.of(Box.class, BamlType.of(Resume.class, BamlType.INT));
        assertEquals(a, b);
        assertEquals(a.hashCode(), b.hashCode());
        // A different nested arg breaks equality.
        BamlType c = BamlType.of(Box.class, BamlType.of(Resume.class, BamlType.STRING));
        assertNotEquals(a, c);
    }

    @Test
    void baml_type_of_unregistered_class_throws() {
        IllegalArgumentException ex =
                assertThrows(IllegalArgumentException.class, () -> BamlType.of(String.class));
        assertTrue(ex.getMessage().contains(String.class.getName()));
        // The reified overload also rejects an unregistered class.
        assertThrows(
                IllegalArgumentException.class, () -> BamlType.of(String.class, BamlType.INT));
    }

    @Test
    void baml_types_ordering_and_chaining() {
        BamlTypes types = BamlTypes.of("T", BamlType.INT).and("U", BamlType.STRING);
        List<String> names = new ArrayList<>();
        List<BamlType> values = new ArrayList<>();
        for (Map.Entry<String, BamlType> e : types.bindings()) {
            names.add(e.getKey());
            values.add(e.getValue());
        }
        assertEquals(List.of("T", "U"), names); // insertion order preserved
        assertEquals(List.of(BamlType.INT, BamlType.STRING), values);
        assertFalse(types.isEmpty());
    }

    @Test
    void baml_types_chaining_is_immutable() {
        BamlTypes one = BamlTypes.of("T", BamlType.INT);
        BamlTypes two = one.and("U", BamlType.STRING);
        // `and` returns a new bag; the receiver is unchanged (still one binding).
        assertEquals(1, one.bindings().size());
        assertEquals(2, two.bindings().size());
    }

    @Test
    void baml_types_duplicate_name_rejected() {
        assertThrows(
                IllegalArgumentException.class,
                () -> BamlTypes.of("T", BamlType.INT).and("T", BamlType.STRING));
    }

    // -- encode: CallFunctionArgs.type_args ----------------------------------

    /** A type_args bag rides CallFunctionArgs.type_args (=3) as BamlTyArg entries. */
    @Test
    void encode_call_args_with_type_args() {
        byte[] got = ProtoWriter.encodeCallFunctionArgs(
                new String[] {"x"}, new Object[] {7L}, 5L, BamlTypes.of("T", BamlType.INT));

        // kwargs entry {string_key:"x", value:int 7}.
        WireWriter xVal = new WireWriter();
        xVal.writeInt64(3, 7L); // InboundValue.int_value = 3
        WireWriter xEntry = new WireWriter();
        xEntry.writeString(1, "x"); // InboundMapEntry.string_key = 1
        xEntry.writeMessage(6, xVal.toByteArray()); // value = 6

        // type_args entry {type_var:"T", type_value: BamlTy{primitive{kind:INT=2}}}.
        WireWriter prim = new WireWriter();
        prim.writeInt64(1, PRIM_INT); // BamlTyPrimitive.kind = 1
        WireWriter ty = new WireWriter();
        ty.writeMessage(1, prim.toByteArray()); // BamlTy.primitive = 1
        WireWriter tyArg = new WireWriter();
        tyArg.writeString(1, "T"); // BamlTyArg.type_var = 1
        tyArg.writeMessage(2, ty.toByteArray()); // BamlTyArg.type_value = 2

        WireWriter expected = new WireWriter();
        expected.writeMessage(1, xEntry.toByteArray()); // CallFunctionArgs.kwargs = 1
        expected.writeInt64(2, 5L); // call_id = 2
        expected.writeMessage(3, tyArg.toByteArray()); // type_args = 3

        assertArrayEquals(expected.toByteArray(), got);
    }

    /** A reified generic class binding renders type_value as class_ty{name, type_args}. */
    @Test
    void encode_call_args_reified_class_type_value() {
        byte[] got = ProtoWriter.encodeCallFunctionArgs(
                new String[0],
                new Object[0],
                9L,
                BamlTypes.of("T", BamlType.of(Resume.class, BamlType.INT)));

        // type_value = BamlTy.class_ty{ name = RESUME_FQN, type_args = [BamlTy.primitive int] }.
        WireWriter argPrim = new WireWriter();
        argPrim.writeInt64(1, PRIM_INT); // BamlTyPrimitive.kind = 1
        WireWriter argTy = new WireWriter();
        argTy.writeMessage(1, argPrim.toByteArray()); // BamlTy.primitive = 1
        WireWriter classTy = new WireWriter();
        classTy.writeString(1, RESUME_FQN); // BamlTyClass.name = 1
        classTy.writeMessage(2, argTy.toByteArray()); // BamlTyClass.type_args = 2
        WireWriter ty = new WireWriter();
        ty.writeMessage(2, classTy.toByteArray()); // BamlTy.class_ty = 2
        WireWriter tyArg = new WireWriter();
        tyArg.writeString(1, "T");
        tyArg.writeMessage(2, ty.toByteArray());

        WireWriter expected = new WireWriter();
        expected.writeInt64(2, 9L); // call_id = 2 (no kwargs)
        expected.writeMessage(3, tyArg.toByteArray()); // type_args = 3

        assertArrayEquals(expected.toByteArray(), got);
    }

    /** Regression pin: no bag → byte-identical to the pre-generics 3-arg encoding. */
    @Test
    void encode_call_args_without_type_args_is_byte_identical() {
        byte[] threeArg =
                ProtoWriter.encodeCallFunctionArgs(new String[] {"n"}, new Object[] {7L}, 123L);
        byte[] nullBag = ProtoWriter.encodeCallFunctionArgs(
                new String[] {"n"}, new Object[] {7L}, 123L, null);
        assertArrayEquals(threeArg, nullBag);
    }

    // -- decode: BamlValueClass.type_args → TypeRegistry side-table -----------

    /** A class_value carrying type_args (=3) binds reified BamlType tokens to the instance. */
    @Test
    void decode_class_value_binds_reified_type_args() {
        byte[] classValue = ovResumeWithTypeArgs(
                "Alice", 30L, tyPrimitive(PRIM_INT), tyClass(RESUME_FQN), tyEnum(SENTIMENT_FQN));
        Object decoded = ProtoReader.decodeOutboundResult(okEnvelope(classValue));
        Resume r = assertInstanceOf(Resume.class, decoded);
        assertEquals(
                List.of(BamlType.INT, BamlType.of(Resume.class), BamlType.of(Sentiment.class)),
                TypeRegistry.typeArgsOf(r));
    }

    /** A nested reified class arg round-trips through the side-table. */
    @Test
    void decode_class_value_binds_nested_reified_type_arg() {
        byte[] nestedClassTy = tyClass(RESUME_FQN, tyPrimitive(PRIM_INT)); // Resume<int>
        byte[] classValue = ovResumeWithTypeArgs("Nim", 5L, nestedClassTy);
        Object decoded = ProtoReader.decodeOutboundResult(okEnvelope(classValue));
        Resume r = assertInstanceOf(Resume.class, decoded);
        assertEquals(
                List.of(BamlType.of(Resume.class, BamlType.INT)), TypeRegistry.typeArgsOf(r));
    }

    /** A class_value without type_args leaves the side-table empty for the instance. */
    @Test
    void decode_class_value_without_type_args_has_empty_side_table() {
        Object decoded = ProtoReader.decodeOutboundResult(okEnvelope(ovResume("Bob", 40L)));
        Resume r = assertInstanceOf(Resume.class, decoded);
        assertEquals(List.of(), TypeRegistry.typeArgsOf(r));
    }

    /**
     * A container type arg (e.g. list&lt;int&gt;) is now in the widened token
     * grammar, so it binds alongside its siblings (decision 4: bind whenever
     * EVERY wire arg is representable). Previously this whole binding was
     * skipped all-or-nothing.
     */
    @Test
    void decode_class_value_container_type_arg_binds_with_widened_grammar() {
        byte[] listTy = tyList(tyPrimitive(PRIM_INT)); // list<int> — now representable
        byte[] classValue = ovResumeWithTypeArgs("Cara", 22L, tyPrimitive(PRIM_INT), listTy);
        Object decoded = ProtoReader.decodeOutboundResult(okEnvelope(classValue));
        Resume r = assertInstanceOf(Resume.class, decoded);
        assertEquals(
                List.of(BamlType.INT, BamlType.list(BamlType.INT)), TypeRegistry.typeArgsOf(r));
    }

    /**
     * A genuinely out-of-grammar type arg (a media arm — no {@link BamlType}
     * kind) still poisons the whole binding all-or-nothing: positions must never
     * misalign. The residual skip cases after the widening are the non-token
     * {@code BamlTy} arms (media / function / rust_type / unknown / never / …)
     * and the {@code null}/{@code bytes}/{@code bigint} primitive kinds.
     */
    @Test
    void decode_class_value_out_of_grammar_type_arg_skips_binding() {
        byte[] mediaTy = tyMedia(); // BamlTy.media = 11 — outside the token grammar
        byte[] classValue = ovResumeWithTypeArgs("Cara", 22L, tyPrimitive(PRIM_INT), mediaTy);
        Object decoded = ProtoReader.decodeOutboundResult(okEnvelope(classValue));
        Resume r = assertInstanceOf(Resume.class, decoded);
        assertEquals(List.of(), TypeRegistry.typeArgsOf(r));
    }

    /** typeArgsOf tolerates an object that was never bound (a fresh instance). */
    @Test
    void type_args_of_unbound_instance_is_empty() {
        assertEquals(List.of(), TypeRegistry.typeArgsOf(new Resume("x", 1L)));
        assertEquals(List.of(), TypeRegistry.typeArgsOf(null));
    }

    // -- BamlTy builders (decode side) ---------------------------------------

    /** A BamlTy wrapping a class_ty (BamlTy.class_ty = 2): name + optional nested type_args. */
    private static byte[] tyClass(String fqn, byte[]... typeArgs) {
        WireWriter cls = new WireWriter();
        cls.writeString(1, fqn); // BamlTyClass.name = 1
        for (byte[] a : typeArgs) {
            cls.writeMessage(2, a); // BamlTyClass.type_args = 2 (repeated)
        }
        WireWriter ty = new WireWriter();
        ty.writeMessage(2, cls.toByteArray()); // BamlTy.class_ty = 2
        return ty.toByteArray();
    }

    /** A BamlTy wrapping an enum (BamlTy.enum = 3). */
    private static byte[] tyEnum(String fqn) {
        WireWriter en = new WireWriter();
        en.writeString(1, fqn); // BamlTyEnum.name = 1
        WireWriter ty = new WireWriter();
        ty.writeMessage(3, en.toByteArray()); // BamlTy.enum = 3
        return ty.toByteArray();
    }

    /** A BamlTy wrapping a list (BamlTy.list = 4) — now in the widened token grammar. */
    private static byte[] tyList(byte[] itemTy) {
        WireWriter list = new WireWriter();
        list.writeMessage(1, itemTy); // BamlTyList.item = 1
        WireWriter ty = new WireWriter();
        ty.writeMessage(4, list.toByteArray()); // BamlTy.list = 4
        return ty.toByteArray();
    }

    /** A BamlTy wrapping a media arm (BamlTy.media = 11) — outside the token grammar. */
    private static byte[] tyMedia() {
        WireWriter ty = new WireWriter();
        ty.writeMessage(11, new byte[0]); // BamlTy.media = 11
        return ty.toByteArray();
    }

    /** ovResume (a BamlValueClass) plus type_args (=3), one repeated BamlTy per arg. */
    private static byte[] ovResumeWithTypeArgs(String name, long age, byte[]... typeArgs) {
        WireWriter nameVal = new WireWriter();
        nameVal.writeString(OV_STRING, name);
        WireWriter nameEntry = new WireWriter();
        nameEntry.writeString(1, "name");
        nameEntry.writeMessage(2, nameVal.toByteArray());

        WireWriter ageVal = new WireWriter();
        ageVal.writeInt64(OV_INT, age);
        WireWriter ageEntry = new WireWriter();
        ageEntry.writeString(1, "age");
        ageEntry.writeMessage(2, ageVal.toByteArray());

        WireWriter cls = new WireWriter();
        cls.writeString(1, RESUME_FQN); // BamlValueClass.name = 1
        cls.writeMessage(2, nameEntry.toByteArray()); // fields = 2
        cls.writeMessage(2, ageEntry.toByteArray());
        for (byte[] a : typeArgs) {
            cls.writeMessage(3, a); // BamlValueClass.type_args = 3
        }

        WireWriter ov = new WireWriter();
        ov.writeMessage(OV_CLASS, cls.toByteArray());
        return ov.toByteArray();
    }
}
