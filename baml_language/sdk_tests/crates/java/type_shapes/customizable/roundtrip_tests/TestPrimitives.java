// Roundtrip coverage for `baml_sdk.primitives` (35-series goal).
//
// Calls each emitted BAML function for the primitive Ty variants from
// Java and asserts the value survives the encode → FFI → decode round
// trip. `return_*` functions exercise decode-only; `round_trip_*`
// exercise the full encode/decode pair.
//
// Port of python_pydantic2/type_shapes/customizable/roundtrip_tests/
// test_primitives.py — same test names, cases, inputs, assertions.
package roundtrip_tests;

import static org.junit.jupiter.api.Assertions.assertArrayEquals;
import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertNull;
import static org.junit.jupiter.api.Assertions.assertTrue;

import baml_sdk.primitives.Fns;
import baml_sdk.primitives.Primitives;
import java.math.BigInteger;
import org.junit.jupiter.api.Test;

class TestPrimitives {

    @Test
    void test_primitives_return_int() {
        assertEquals(42L, Fns.return_int());
    }

    @Test
    void test_primitives_return_float() {
        assertEquals(3.14, Fns.return_float());
    }

    @Test
    void test_primitives_return_string() {
        assertEquals("hello", Fns.return_string());
    }

    @Test
    void test_primitives_return_bool() {
        assertTrue(Fns.return_bool());
    }

    @Test
    void test_primitives_return_bigint() {
        // 12345678901234567890 > i64 max (9223372036854775807), so it decodes
        // through the hex bigint wire channel rather than int_value.
        assertEquals(new BigInteger("12345678901234567890"), Fns.return_bigint());
    }

    @Test
    void test_primitives_return_null() {
        assertNull(Fns.return_null());
    }

    @Test
    void test_primitives_round_trip_int() {
        assertEquals(7L, Fns.round_trip_int(7L));
    }

    @Test
    void test_primitives_round_trip_float() {
        assertEquals(2.5, Fns.round_trip_float(2.5));
    }

    @Test
    void test_primitives_round_trip_float_accepts_int() {
        // java-port note: Python pins that an int handed to a float param
        // widens at the FFI boundary. In Java the widening happens at the
        // call site (int literal → double), so the wire already carries
        // 7.0 — the assertion pins the same contract from the caller's
        // point of view: an integral input to a `-> float` function comes
        // back as a genuine double.
        double result = Fns.round_trip_float(7);
        assertEquals(7.0, result);
    }

    @Test
    void test_primitives_round_trip_string() {
        assertEquals("hi", Fns.round_trip_string("hi"));
    }

    @Test
    void test_primitives_round_trip_bool() {
        assertFalse(Fns.round_trip_bool(false));
    }

    @Test
    void test_primitives_round_trip_bigint() {
        // Encode uses the hex bigint channel only for values outside i64 range
        // (an in-range int rides int_value), so every case here is beyond i64:
        // a large positive, its negation, and a power-of-two hex boundary
        // (2^64 — the first magnitude needing a 17th hex digit).
        BigInteger big = BigInteger.TWO.pow(80);
        assertEquals(big, Fns.round_trip_bigint(big));
        assertEquals(big.negate(), Fns.round_trip_bigint(big.negate()));
        BigInteger hexBoundary = BigInteger.TWO.pow(64);
        assertEquals(hexBoundary, Fns.round_trip_bigint(hexBoundary));
    }

    @Test
    void test_primitives_round_trip_null() {
        assertNull(Fns.round_trip_null(null));
    }

    @Test
    void test_primitives_round_trip_uint8_array() {
        assertArrayEquals(new byte[] {0, 1, 2}, Fns.round_trip_uint8_array(new byte[] {0, 1, 2}));
    }

    @Test
    void test_primitives_round_trip_primitives() {
        Primitives p =
                new Primitives(1L, 1.5, "s", true, null, new byte[] {'a', 'b'});
        assertEquals(p, Fns.round_trip_primitives(p));
    }

    @Test
    void test_primitives_round_trip_primitives_float_field_accepts_int() {
        // java-port note: Python pins pydantic's construction-time int →
        // float coercion. Java has no construction-time coercion — the int
        // literal widens to double at the constructor call site — so this
        // pins the same observable contract: an integral value stored in a
        // float field round-trips as a genuine double.
        Primitives p =
                new Primitives(1L, 2, "s", true, null, new byte[] {'a', 'b'});
        assertEquals(2.0, p.float_field());
        Primitives result = Fns.round_trip_primitives(p);
        assertEquals(2.0, result.float_field());
    }
}
