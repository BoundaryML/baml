package baml_bridge;

import static org.junit.jupiter.api.Assertions.assertArrayEquals;
import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertInstanceOf;
import static org.junit.jupiter.api.Assertions.assertNotEquals;
import static org.junit.jupiter.api.Assertions.assertThrows;
import static org.junit.jupiter.api.Assertions.assertTrue;

import baml_bridge.internal.ProtoReader;
import baml_bridge.internal.ProtoWriter;
import baml_bridge.internal.WireReader;
import baml_bridge.internal.WireWriter;

import java.util.ArrayList;
import java.util.List;

import org.junit.jupiter.api.BeforeAll;
import org.junit.jupiter.api.Test;

/**
 * Offline tuple codec tests — no native library required. A tuple crosses
 * outbound as {@code BamlOutboundValue.tuple_value} and inbound as a list
 * stating its tuple type; the Java host type is the generated record
 * registered for the tuple identity, then a list of the join of its slot
 * types, then a plain {@code List<Object>}.
 */
class TupleCodecTest {
    // BamlOutboundValue oneof field numbers.
    private static final int OV_STRING = 3;
    private static final int OV_INT = 4;
    private static final int OV_BOOL = 6;
    private static final int OV_LIST = 11;
    private static final int OV_UNION = 13;
    private static final int OV_TUPLE = 23;
    // InboundValue: value_type = 1, list_value = 6; InboundListValue.values = 1.
    private static final int IV_VALUE_TYPE = 1;
    private static final int IV_LIST = 6;
    private static final int LIST_VALUES = 1;

    /** A generated tuple record for {@code (int, string)}, as the generator emits it. */
    public record Tuple2_IntString(long item0, String item1) {}

    /** {@code (string, int)}: the same slot types in another order, another identity. */
    public record Tuple2_StringInt(String item0, long item1) {}

    /** {@code ((int, string), bool)}: a tuple slot holds its own record. */
    public record Tuple2_Tuple2_IntStringBoolean(Tuple2_IntString item0, boolean item1) {}

    /** {@code (int, string?)}: an optional slot still occupies its position. */
    public record Tuple2_IntOptionalString(long item0, String item1) {}

    private static final BamlType INT_STRING = BamlType.tuple(BamlType.INT, BamlType.STRING);
    private static final BamlType STRING_INT = BamlType.tuple(BamlType.STRING, BamlType.INT);
    private static final BamlType NESTED = BamlType.tuple(INT_STRING, BamlType.BOOL);
    private static final BamlType INT_OPT_STRING =
            BamlType.tuple(BamlType.INT, BamlType.optional(BamlType.STRING));

    @BeforeAll
    static void registerFixtures() {
        // Mirrors the generated Baml static initializer's registerTuple calls.
        TypeRegistry.registerTuple(Tuple2_IntString.class.getName(), INT_STRING);
        TypeRegistry.registerTuple(Tuple2_StringInt.class.getName(), STRING_INT);
        TypeRegistry.registerTuple(Tuple2_Tuple2_IntStringBoolean.class.getName(), NESTED);
        TypeRegistry.registerTuple(Tuple2_IntOptionalString.class.getName(), INT_OPT_STRING);
    }

    // -- outbound builders (mirror the engine's value_encode side) -----------

    private static byte[] ovInt(long v) {
        WireWriter w = new WireWriter();
        w.writeInt64(OV_INT, v);
        return w.toByteArray();
    }

    private static byte[] ovString(String s) {
        WireWriter w = new WireWriter();
        w.writeString(OV_STRING, s);
        return w.toByteArray();
    }

    private static byte[] ovBool(boolean b) {
        WireWriter w = new WireWriter();
        w.writeBool(OV_BOOL, b);
        return w.toByteArray();
    }

    private static byte[] ovNull() {
        WireWriter w = new WireWriter();
        w.writeMessage(2, new byte[0]); // null_value = 2
        return w.toByteArray();
    }

    /** A {@code tuple_value} (=23): {@code BamlValueTuple.items} (=1), one per slot. */
    private static byte[] ovTuple(byte[]... items) {
        WireWriter tuple = new WireWriter();
        for (byte[] item : items) {
            tuple.writeMessage(1, item);
        }
        WireWriter ov = new WireWriter();
        ov.writeMessage(OV_TUPLE, tuple.toByteArray());
        return ov.toByteArray();
    }

    /** An untyped {@code list_value} (=11): {@code BamlValueList.items} (=2). */
    private static byte[] ovList(byte[]... items) {
        WireWriter list = new WireWriter();
        for (byte[] item : items) {
            list.writeMessage(2, item);
        }
        WireWriter ov = new WireWriter();
        ov.writeMessage(OV_LIST, list.toByteArray());
        return ov.toByteArray();
    }

    /** A {@code union_variant_value} (=13) with self_type (=4) and value (=6). */
    private static byte[] ovUnion(BamlType selfType, byte[] inner) {
        WireWriter uv = new WireWriter();
        uv.writeMessage(4, selfType.toWireTy());
        uv.writeMessage(6, inner);
        WireWriter ov = new WireWriter();
        ov.writeMessage(OV_UNION, uv.toByteArray());
        return ov.toByteArray();
    }

    private static Object decode(byte[] value, BamlType desc) {
        return ProtoReader.decodeWithDesc(value, desc, false);
    }

    /** The bytes of the first occurrence of length-delimited {@code field}, or null. */
    private static byte[] fieldOf(byte[] message, int field) {
        WireReader r = new WireReader(message);
        while (r.hasRemaining()) {
            int tag = r.readTag();
            if (WireReader.fieldOf(tag) == field) {
                return r.readBytes();
            }
            r.skipField(WireReader.wireOf(tag));
        }
        return null;
    }

    /** Every occurrence of length-delimited {@code field}. */
    private static List<byte[]> fieldsOf(byte[] message, int field) {
        List<byte[]> found = new ArrayList<>();
        WireReader r = new WireReader(message);
        while (r.hasRemaining()) {
            int tag = r.readTag();
            if (WireReader.fieldOf(tag) == field) {
                found.add(r.readBytes());
            } else {
                r.skipField(WireReader.wireOf(tag));
            }
        }
        return found;
    }

    // -- the type token --------------------------------------------------------

    @Test
    void tuple_tokens_are_ordered_slot_identities() {
        assertEquals("(int, string)", INT_STRING.toString());
        assertEquals("(int,)", BamlType.tuple(BamlType.INT).toString());
        assertNotEquals(INT_STRING, STRING_INT);
        // A one-slot tuple is not its slot, and a tuple is not a list.
        assertNotEquals(BamlType.INT, BamlType.tuple(BamlType.INT));
        assertNotEquals(BamlType.list(BamlType.INT), BamlType.tuple(BamlType.INT));
        // A product of unions is not the union of products it expands to.
        BamlType product = BamlType.tuple(BamlType.union(BamlType.INT, BamlType.STRING), BamlType.BOOL);
        BamlType sum = BamlType.union(
                BamlType.tuple(BamlType.INT, BamlType.BOOL), BamlType.tuple(BamlType.STRING, BamlType.BOOL));
        assertNotEquals(product, sum);
        assertThrows(IllegalArgumentException.class, BamlType::tuple);
    }

    @Test
    void tuple_tokens_round_trip_the_wire() {
        for (BamlType type : List.of(INT_STRING, NESTED, INT_OPT_STRING, BamlType.tuple(BamlType.INT))) {
            assertEquals(type, BamlType.fromWireTy(type.toWireTy()));
        }
        // BamlTy.tuple = 25, BamlTyTuple.items = 1.
        byte[] items = fieldOf(INT_STRING.toWireTy(), 25);
        assertEquals(2, fieldsOf(items, 1).size());
        // A slot the grammar cannot spell poisons the whole token.
        WireWriter future = new WireWriter();
        future.writeMessage(16, new byte[0]); // rust_type
        WireWriter tuple = new WireWriter();
        tuple.writeMessage(1, BamlType.INT.toWireTy());
        tuple.writeMessage(1, future.toByteArray());
        WireWriter ty = new WireWriter();
        ty.writeMessage(25, tuple.toByteArray());
        assertEquals(null, BamlType.fromWireTy(ty.toByteArray()));
    }

    @Test
    void a_tuple_record_names_its_registered_identity() {
        assertEquals(INT_STRING, BamlType.of(Tuple2_IntString.class));
        assertEquals(NESTED, BamlType.of(Tuple2_Tuple2_IntStringBoolean.class));
    }

    @Test
    void conflicting_tuple_registrations_are_refused() {
        // Re-registering the same pair is idempotent.
        TypeRegistry.registerTuple(Tuple2_IntString.class.getName(), INT_STRING);
        assertThrows(
                IllegalStateException.class,
                () -> TypeRegistry.registerTuple("some.Other", INT_STRING));
        assertThrows(
                IllegalStateException.class,
                () -> TypeRegistry.registerTuple(
                        Tuple2_IntString.class.getName(), BamlType.tuple(BamlType.INT, BamlType.INT)));
        assertThrows(
                IllegalArgumentException.class,
                () -> TypeRegistry.registerTuple("some.List", BamlType.list(BamlType.INT)));
    }

    // -- outbound: BAML -> Java ------------------------------------------------

    @Test
    void decode_registered_tuple_constructs_its_record() {
        assertEquals(new Tuple2_IntString(1L, "a"), decode(ovTuple(ovInt(1), ovString("a")), INT_STRING));
        assertEquals(new Tuple2_StringInt("a", 1L), decode(ovTuple(ovString("a"), ovInt(1)), STRING_INT));
        assertEquals(
                new Tuple2_Tuple2_IntStringBoolean(new Tuple2_IntString(2L, "b"), true),
                decode(ovTuple(ovTuple(ovInt(2), ovString("b")), ovBool(true)), NESTED));
        assertEquals(
                new Tuple2_IntOptionalString(3L, null),
                decode(ovTuple(ovInt(3), ovNull()), INT_OPT_STRING));
        // Inside containers, through the element descriptor.
        assertEquals(
                List.of(new Tuple2_IntString(1L, "a"), new Tuple2_IntString(2L, "b")),
                decode(
                        ovList(ovTuple(ovInt(1), ovString("a")), ovTuple(ovInt(2), ovString("b"))),
                        BamlType.list(INT_STRING)));
    }

    @Test
    void decode_tuple_of_another_arity_is_refused() {
        BamlError error = assertThrows(
                BamlError.class,
                () -> decode(ovTuple(ovInt(1), ovString("a"), ovInt(2)), INT_STRING));
        assertTrue(error.getMessage().contains("has 2 slots"), error.getMessage());
        assertThrows(BamlError.class, () -> decode(ovTuple(ovInt(1)), INT_STRING));
        // A thrown value still surfaces: lenient decode keeps the slots.
        assertEquals(
                List.of(1L, "a", 2L),
                ProtoReader.decodeWithDesc(ovTuple(ovInt(1), ovString("a"), ovInt(2)), INT_STRING, true));
    }

    @Test
    void unregistered_tuple_falls_back_to_a_list_of_the_slot_join() {
        BamlType intBool = BamlType.tuple(BamlType.INT, BamlType.BOOL);
        Object got = decode(ovTuple(ovInt(1), ovBool(true)), intBool);
        assertEquals(
                List.of(new Union2.Arm0<Long, Boolean>(1L), new Union2.Arm1<Long, Boolean>(true)), got);
        // Exactly the slots, in order, and immutable like the tuple it stands for.
        assertThrows(UnsupportedOperationException.class, () -> ((List<?>) got).add(null));

        // The join is of the declared slots, not of the values: a union slot
        // contributes its members, and the item keeps the member it decoded to.
        BamlType product = BamlType.tuple(BamlType.union(BamlType.INT, BamlType.STRING), BamlType.BOOL);
        assertEquals(
                List.of(
                        new Union3.Arm1<Long, String, Boolean>("x"),
                        new Union3.Arm2<Long, String, Boolean>(false)),
                decode(ovTuple(ovString("x"), ovBool(false)), product));
        // An optional slot admits a null element.
        BamlType optional = BamlType.tuple(BamlType.optional(BamlType.STRING), BamlType.INT);
        List<Object> withNull = new ArrayList<>();
        withNull.add(null);
        withNull.add(new Union2.Arm1<String, Long>(4L));
        assertEquals(withNull, decode(ovTuple(ovNull(), ovInt(4)), optional));
        // One distinct slot type is that type's own list.
        assertEquals(List.of(1L, 2L), decode(ovTuple(ovInt(1), ovInt(2)), BamlType.tuple(BamlType.INT, BamlType.INT)));
    }

    @Test
    void tuple_without_a_union_representation_is_a_plain_list() {
        // A type variable slot has no generated union.
        BamlType open = BamlType.tuple(BamlType.typeVar("T"), BamlType.INT);
        Object got = decode(ovTuple(ovString("v"), ovInt(1)), open);
        assertEquals(List.of("v", 1L), got);
        assertThrows(UnsupportedOperationException.class, () -> ((List<?>) got).set(0, null));
        // Nor does a join of more than ten members.
        BamlType[] slots = new BamlType[11];
        byte[][] items = new byte[11][];
        List<Object> expected = new ArrayList<>();
        for (int i = 0; i < 10; i++) {
            slots[i] = BamlType.literalInt(i);
            items[i] = ovInt(i);
            expected.add((long) i);
        }
        slots[10] = BamlType.STRING;
        items[10] = ovString("s");
        expected.add("s");
        assertEquals(expected, decode(ovTuple(items), BamlType.tuple(slots)));
    }

    @Test
    void wire_driven_tuple_decodes_to_a_plain_list() {
        Object got = ProtoReader.decodeWithDesc(ovTuple(ovInt(1), ovString("a")), null, false);
        assertEquals(List.of(1L, "a"), got);
        assertThrows(UnsupportedOperationException.class, () -> ((List<?>) got).add(null));
    }

    @Test
    void tuple_arms_match_by_arity_and_slot() {
        byte[] intString = ovTuple(ovInt(1), ovString("a"));
        assertTrue(ProtoReader.armMatchesValue(INT_STRING, intString));
        assertFalse(ProtoReader.armMatchesValue(STRING_INT, intString));
        assertFalse(ProtoReader.armMatchesValue(BamlType.tuple(BamlType.INT), intString));
        // A tuple is not a list, nor a list a tuple.
        assertFalse(ProtoReader.armMatchesValue(BamlType.list(BamlType.INT), ovTuple(ovInt(1))));
        assertFalse(ProtoReader.armMatchesValue(BamlType.tuple(BamlType.INT), ovList(ovInt(1))));
        // An optional slot takes a null item; a union slot's item comes in its envelope.
        assertTrue(ProtoReader.armMatchesValue(INT_OPT_STRING, ovTuple(ovInt(1), ovNull())));
        BamlType product = BamlType.tuple(BamlType.union(BamlType.INT, BamlType.STRING), BamlType.BOOL);
        assertTrue(ProtoReader.armMatchesValue(
                product,
                ovTuple(ovUnion(BamlType.union(BamlType.INT, BamlType.STRING), ovString("s")), ovBool(true))));
    }

    @Test
    void union_of_tuples_selects_the_arm_by_slot() {
        BamlType union = BamlType.union(INT_STRING, STRING_INT);
        assertEquals(
                new Union2.Arm1<Tuple2_IntString, Tuple2_StringInt>(new Tuple2_StringInt("a", 1L)),
                decode(ovTuple(ovString("a"), ovInt(1)), union));
        assertEquals(
                new Union2.Arm0<Tuple2_IntString, Tuple2_StringInt>(new Tuple2_IntString(1L, "a")),
                decode(ovUnion(union, ovTuple(ovInt(1), ovString("a"))), union));
    }

    // -- inbound: Java -> BAML -------------------------------------------------

    @Test
    void encode_record_states_its_registered_identity() {
        byte[] got = ProtoWriter.encodeInboundValue(new Tuple2_IntString(1L, "a"));
        assertArrayEquals(INT_STRING.toWireTy(), fieldOf(got, IV_VALUE_TYPE));
        List<byte[]> items = fieldsOf(fieldOf(got, IV_LIST), LIST_VALUES);
        assertEquals(2, items.size());
        assertArrayEquals(ProtoWriter.encodeInboundValue(1L), items.get(0));
        assertArrayEquals(ProtoWriter.encodeInboundValue("a"), items.get(1));
        // A record states its own identity, under any slot: the engine judges it.
        assertArrayEquals(
                STRING_INT.toWireTy(),
                fieldOf(ProtoWriter.encodeInboundValue(new BamlTypedValue(new Tuple2_StringInt("a", 1L), INT_STRING)),
                        IV_VALUE_TYPE));
        // Nested records state their own identities too.
        byte[] nested = ProtoWriter.encodeInboundValue(
                new Tuple2_Tuple2_IntStringBoolean(new Tuple2_IntString(1L, "a"), false));
        assertArrayEquals(NESTED.toWireTy(), fieldOf(nested, IV_VALUE_TYPE));
        assertArrayEquals(
                INT_STRING.toWireTy(),
                fieldOf(fieldsOf(fieldOf(nested, IV_LIST), LIST_VALUES).get(0), IV_VALUE_TYPE));
        // An optional slot's null keeps its position.
        byte[] optional = ProtoWriter.encodeInboundValue(new Tuple2_IntOptionalString(1L, null));
        assertEquals(2, fieldsOf(fieldOf(optional, IV_LIST), LIST_VALUES).size());
    }

    @Test
    void plain_list_stands_in_for_a_tuple_only_under_a_tuple_slot() {
        byte[] got = ProtoWriter.encodeInboundValue(new BamlTypedValue(List.of(1L, "a"), INT_STRING));
        assertArrayEquals(INT_STRING.toWireTy(), fieldOf(got, IV_VALUE_TYPE));
        assertEquals(2, fieldsOf(fieldOf(got, IV_LIST), LIST_VALUES).size());
        // Under an optional tuple, and as a list of tuples.
        assertArrayEquals(
                INT_STRING.toWireTy(),
                fieldOf(ProtoWriter.encodeInboundValue(
                        new BamlTypedValue(List.of(1L, "a"), BamlType.optional(INT_STRING))), IV_VALUE_TYPE));
        byte[] listOfTuples = ProtoWriter.encodeInboundValue(
                new BamlTypedValue(List.of(List.of(1L, "a")), BamlType.list(INT_STRING)));
        assertArrayEquals(
                INT_STRING.toWireTy(),
                fieldOf(fieldsOf(fieldOf(listOfTuples, IV_LIST), LIST_VALUES).get(0), IV_VALUE_TYPE));
        // With no tuple slot a list is the list it is.
        assertArrayEquals(
                BamlType.list(BamlType.union(BamlType.INT, BamlType.STRING)).toWireTy(),
                fieldOf(ProtoWriter.encodeInboundValue(List.of(1L, "a")), IV_VALUE_TYPE));
        // A list member keeps a list a list.
        BamlType listOrPair = BamlType.union(BamlType.list(BamlType.INT), BamlType.tuple(BamlType.INT, BamlType.INT));
        assertArrayEquals(
                BamlType.list(BamlType.INT).toWireTy(),
                fieldOf(ProtoWriter.encodeInboundValue(new BamlTypedValue(List.of(1L, 2L), listOrPair)), IV_VALUE_TYPE));
    }

    @Test
    void plain_list_of_the_wrong_arity_or_ambiguous_shape_is_refused() {
        IllegalArgumentException arity = assertThrows(
                IllegalArgumentException.class,
                () -> ProtoWriter.encodeInboundValue(new BamlTypedValue(List.of(1L), INT_STRING)));
        assertTrue(arity.getMessage().contains("exactly one item per slot"), arity.getMessage());
        assertThrows(
                IllegalArgumentException.class,
                () -> ProtoWriter.encodeInboundValue(new BamlTypedValue(List.of(1L, "a", 2L), INT_STRING)));
        // Two same-arity tuple members: a list states no identity to choose by.
        IllegalArgumentException ambiguous = assertThrows(
                IllegalArgumentException.class,
                () -> ProtoWriter.encodeInboundValue(
                        new BamlTypedValue(List.of(1L, "a"), BamlType.union(INT_STRING, STRING_INT))));
        assertTrue(ambiguous.getMessage().contains("ambiguous"), ambiguous.getMessage());
        // One of them by arity is no ambiguity.
        BamlType byArity = BamlType.union(INT_STRING, BamlType.tuple(BamlType.INT));
        assertArrayEquals(
                BamlType.tuple(BamlType.INT).toWireTy(),
                fieldOf(ProtoWriter.encodeInboundValue(new BamlTypedValue(List.of(1L), byArity)), IV_VALUE_TYPE));
    }

    @Test
    void plain_list_under_an_open_tuple_slot_states_the_items_types() {
        BamlType open = BamlType.tuple(BamlType.typeVar("T"), BamlType.INT);
        assertArrayEquals(
                BamlType.tuple(BamlType.STRING, BamlType.INT).toWireTy(),
                fieldOf(ProtoWriter.encodeInboundValue(new BamlTypedValue(List.of("v", 1L), open)), IV_VALUE_TYPE));
    }

    @Test
    void callback_result_encodes_against_its_declared_tuple() {
        // A host callable's result is encoded against the declared return.
        byte[] got = ProtoWriter.encodeResult(List.of(1L, "a"), INT_STRING);
        assertArrayEquals(INT_STRING.toWireTy(), fieldOf(got, IV_VALUE_TYPE));
        assertThrows(IllegalArgumentException.class, () -> ProtoWriter.encodeResult(List.of(1L), INT_STRING));
        assertInstanceOf(byte[].class, ProtoWriter.encodeResult(new Tuple2_IntString(1L, "a"), INT_STRING));
    }
}
