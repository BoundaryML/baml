package baml_bridge;

import static org.junit.jupiter.api.Assertions.assertArrayEquals;
import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertNotEquals;
import static org.junit.jupiter.api.Assertions.assertNotNull;
import static org.junit.jupiter.api.Assertions.assertNull;
import static org.junit.jupiter.api.Assertions.assertThrows;
import static org.junit.jupiter.api.Assertions.assertTrue;

import baml_bridge.internal.WireWriter;

import java.math.BigInteger;

import org.junit.jupiter.api.Test;

/**
 * Offline codec tests for the {@link BamlType} token grammar — no native
 * library required. They validate {@link BamlType#toWireTy} /
 * {@link BamlType#fromWireTy} against {@code baml_type.proto} field numbers by
 * round-tripping every token kind (primitives, containers, optional, union,
 * literals, and nested class type-args) and by pinning a few exact wire-byte
 * encodings, in the style of {@link WireCodecTest}.
 */
class BamlTypeTest {
    // baml_type.proto oneof / sub-message field numbers (kept in lockstep with
    // BamlType's private constants so a drift is caught here).
    private static final int TY_CLASS = 2;
    private static final int TY_MEDIA = 11; // an out-of-grammar arm
    private static final int CLASS_NAME = 1;
    private static final int CLASS_TYPE_ARGS = 2;
    private static final int PRIM_KIND = 1;
    private static final int TY_PRIMITIVE = 1;
    private static final int PRIM_NULL = 5; // an out-of-grammar primitive kind

    /** A wire {@code BamlTy.class_ty{name, type_args}} carrying already-lowered args. */
    private static byte[] classTyWire(String fqn, byte[]... typeArgs) {
        WireWriter cls = new WireWriter();
        cls.writeString(CLASS_NAME, fqn);
        for (byte[] arg : typeArgs) {
            cls.writeMessage(CLASS_TYPE_ARGS, arg);
        }
        WireWriter ty = new WireWriter();
        ty.writeMessage(TY_CLASS, cls.toByteArray());
        return ty.toByteArray();
    }

    private static void assertRoundTrips(BamlType token) {
        byte[] wire = token.toWireTy();
        BamlType back = BamlType.fromWireTy(wire);
        assertNotNull(back, () -> "fromWireTy returned null for " + token);
        assertEquals(token, back);
        // Re-encoding the decoded token reproduces the exact bytes.
        assertArrayEquals(wire, back.toWireTy());
    }

    // -- primitives ----------------------------------------------------------

    @Test
    void primitives_round_trip() {
        assertRoundTrips(BamlType.INT);
        assertRoundTrips(BamlType.STRING);
        assertRoundTrips(BamlType.BOOL);
        assertRoundTrips(BamlType.FLOAT);
    }

    @Test
    void int_wire_bytes_are_exact() {
        // BamlTy{ primitive(1) = BamlTyPrimitive{ kind(1) = INT(2) } }
        assertArrayEquals(new byte[] {0x0A, 0x02, 0x08, 0x02}, BamlType.INT.toWireTy());
    }

    // -- containers ----------------------------------------------------------

    @Test
    void list_round_trips_and_wire_bytes_are_exact() {
        assertRoundTrips(BamlType.list(BamlType.INT));
        // BamlTy{ list(4) = BamlTyList{ item(1) = <INT wire = 0A 02 08 02> } }
        assertArrayEquals(
                new byte[] {0x22, 0x06, 0x0A, 0x04, 0x0A, 0x02, 0x08, 0x02},
                BamlType.list(BamlType.INT).toWireTy());
    }

    @Test
    void map_round_trips() {
        assertRoundTrips(BamlType.map(BamlType.STRING, BamlType.INT));
        // Distinct key/value must decode to distinct positions.
        BamlType m = BamlType.fromWireTy(BamlType.map(BamlType.STRING, BamlType.BOOL).toWireTy());
        assertEquals(BamlType.map(BamlType.STRING, BamlType.BOOL), m);
        assertNotEquals(BamlType.map(BamlType.BOOL, BamlType.STRING), m);
    }

    @Test
    void optional_round_trips_and_models_T_question() {
        assertRoundTrips(BamlType.optional(BamlType.INT));
        // BamlTy{ optional(6) = BamlTyOptional{ inner(1) = <INT wire> } } — tag
        // (6<<3)|2 = 0x32, NOT a union-with-null (Python `_fill_wire_ty` parity).
        byte[] wire = BamlType.optional(BamlType.INT).toWireTy();
        assertEquals(0x32, wire[0] & 0xFF);
    }

    @Test
    void union_round_trips_in_order() {
        BamlType u = BamlType.union(BamlType.INT, BamlType.STRING, BamlType.BOOL);
        assertRoundTrips(u);
        // Order-sensitive.
        assertNotEquals(u, BamlType.union(BamlType.STRING, BamlType.INT, BamlType.BOOL));
    }

    @Test
    void nested_containers_round_trip() {
        assertRoundTrips(BamlType.list(BamlType.list(BamlType.STRING)));
        assertRoundTrips(
                BamlType.map(BamlType.STRING, BamlType.list(BamlType.optional(BamlType.INT))));
        assertRoundTrips(
                BamlType.union(
                        BamlType.list(BamlType.INT), BamlType.map(BamlType.STRING, BamlType.BOOL)));
    }

    // -- literals ------------------------------------------------------------

    @Test
    void literals_round_trip() {
        assertRoundTrips(BamlType.literalString("draft"));
        assertRoundTrips(BamlType.literalInt(3));
        assertRoundTrips(BamlType.literalBool(true));
        assertRoundTrips(BamlType.literalBool(false));
        assertRoundTrips(BamlType.literalBigint(new BigInteger("170141183460469231731687303715884105727")));
        assertRoundTrips(BamlType.literalFloat("1.5"));
    }

    @Test
    void literal_int_wire_bytes_are_exact() {
        // BamlTy{ literal(8) = BamlTyLiteral{ int_value(2) = 3 } }
        assertArrayEquals(new byte[] {0x42, 0x02, 0x10, 0x03}, BamlType.literalInt(3).toWireTy());
    }

    @Test
    void distinct_literals_are_unequal() {
        assertNotEquals(BamlType.literalInt(3), BamlType.literalInt(4));
        assertNotEquals(BamlType.literalString("a"), BamlType.literalString("b"));
        // A string literal "3" is not the int literal 3.
        assertNotEquals(BamlType.literalString("3"), BamlType.literalInt(3));
    }

    // -- class type-args now admit the widened grammar (decision 4) ----------

    @Test
    void class_type_arg_can_be_a_container() {
        // Previously a list/map/optional/union type-arg poisoned the whole class
        // token (fromWireTy -> null); the widened grammar reconstructs it.
        byte[] wire = classTyWire("foo.Bar", BamlType.list(BamlType.INT).toWireTy());
        BamlType token = BamlType.fromWireTy(wire);
        assertNotNull(token);
        assertArrayEquals(wire, token.toWireTy());

        byte[] wire2 =
                classTyWire(
                        "foo.Baz",
                        BamlType.optional(BamlType.STRING).toWireTy(),
                        BamlType.map(BamlType.STRING, BamlType.INT).toWireTy());
        BamlType token2 = BamlType.fromWireTy(wire2);
        assertNotNull(token2);
        assertArrayEquals(wire2, token2.toWireTy());
    }

    // -- residual out-of-grammar skip cases (documented) ---------------------

    @Test
    void out_of_grammar_arms_decode_to_null() {
        // A media arm (field 11) is not in the token grammar.
        WireWriter media = new WireWriter();
        media.writeMessage(TY_MEDIA, new byte[0]);
        assertNull(BamlType.fromWireTy(media.toByteArray()));

        // A primitive of kind NULL (5) is out of grammar.
        WireWriter prim = new WireWriter();
        prim.writeInt64(PRIM_KIND, PRIM_NULL);
        WireWriter nullPrim = new WireWriter();
        nullPrim.writeMessage(TY_PRIMITIVE, prim.toByteArray());
        assertNull(BamlType.fromWireTy(nullPrim.toByteArray()));
    }

    @Test
    void class_with_out_of_grammar_type_arg_poisons_the_token() {
        // A class type-arg that is itself out of grammar (a media arm) still
        // poisons the whole token — positions must never misalign.
        WireWriter media = new WireWriter();
        media.writeMessage(TY_MEDIA, new byte[0]);
        byte[] wire = classTyWire("foo.Bar", media.toByteArray());
        assertNull(BamlType.fromWireTy(wire));
    }

    // -- decode-only hints (typeVar / UNKNOWN / classByFqn) ------------------

    @Test
    void decode_only_hints_never_encode() {
        // TypeVar and UNKNOWN are host-side decode descriptors, not values — they
        // must never ride the encode wire.
        IllegalStateException tv = assertThrows(
                IllegalStateException.class, () -> BamlType.typeVar("T").toWireTy());
        assertTrue(tv.getMessage().contains("decode-only"), tv.getMessage());
        IllegalStateException uk = assertThrows(
                IllegalStateException.class, BamlType.UNKNOWN::toWireTy);
        assertTrue(uk.getMessage().contains("decode-only"), uk.getMessage());
        // They are wildcards in structural matching (match any wire token).
        assertTrue(BamlType.UNKNOWN.matchesStructural(BamlType.INT));
        assertTrue(BamlType.typeVar("T").matchesStructural(BamlType.list(BamlType.STRING)));
    }

    @Test
    void class_by_fqn_needs_no_registration_and_has_value_semantics() {
        // classByFqn names a type by BAML FQN without a TypeRegistry lookup (so it
        // spells runtime-owned / not-yet-loaded types too).
        BamlType a = BamlType.classByFqn("ai.stream.Stream");
        assertEquals(BamlType.classByFqn("ai.stream.Stream"), a);
        assertEquals("ai.stream.Stream", a.fqn());
        // A generic-args form is distinct from the bare form (distinct registry keys).
        BamlType g = BamlType.classByFqn("user.g.Wrapper", BamlType.INT);
        assertNotEquals(BamlType.classByFqn("user.g.Wrapper"), g);
        assertNotEquals(g, BamlType.classByFqn("user.g.Wrapper", BamlType.STRING));
    }

    @Test
    void compare_to_is_structural_and_consistent_with_equals() {
        // A total order over the same fields equals() rides — 0 iff equal.
        assertEquals(0, BamlType.INT.compareTo(BamlType.INT));
        assertNotEquals(0, BamlType.INT.compareTo(BamlType.STRING));
        assertEquals(
                -BamlType.STRING.compareTo(BamlType.INT),
                BamlType.INT.compareTo(BamlType.STRING));
        // Distinct literal values order deterministically (no rendered-string key).
        assertNotEquals(0, BamlType.literalInt(1).compareTo(BamlType.literalInt(2)));
        // A sorted+distinct arm list is a stable structural key.
        java.util.List<BamlType> a = new java.util.ArrayList<>(
                java.util.List.of(BamlType.STRING, BamlType.INT, BamlType.INT));
        java.util.List<BamlType> b = new java.util.ArrayList<>(
                java.util.List.of(BamlType.INT, BamlType.STRING));
        a = a.stream().distinct().sorted().toList();
        b = b.stream().distinct().sorted().toList();
        assertEquals(a, b);
        assertEquals(a.hashCode(), b.hashCode());
    }
}
