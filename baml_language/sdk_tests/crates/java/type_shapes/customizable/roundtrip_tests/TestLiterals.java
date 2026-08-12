// Roundtrip coverage for `baml_sdk.literals` — literal Ty variants.
//
// Float literals are intentionally absent (BAML's parser rejects a bare
// negative-literal-as-field, and Java has no distinct float-literal type
// anyway). The negative-literal-as-field case is still parser-blocked, but
// the function-return form `return_literal_neg_one() -> -1` emits and is
// exercised here.
//
// Port of python_pydantic2/type_shapes/customizable/roundtrip_tests/
// test_literals.py — same test names, cases, inputs, assertions.
//
// java-port note: BAML literal types (`42`, `"draft"`, `true`) have no
// runtime representation distinct from their base primitive in Java — Java
// has no literal/refinement type system, unlike Python's `Literal[...]`
// (which pyright still statically narrows). Every literal-typed value below
// is therefore just a plain `long` / `String` / `boolean`.
package roundtrip_tests;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertTrue;

import baml_sdk.literals.Fns;
import baml_sdk.literals.Literals;
import org.junit.jupiter.api.Test;

class TestLiterals {

    @Test
    void test_literals_return_literals() {
        assertEquals(42L, Fns.return_literal42());
        assertEquals(-1L, Fns.return_literal_neg_one());
        assertEquals("draft", Fns.return_literal_draft());
        assertEquals("has \"quotes\"", Fns.return_literal_escaped());
        assertTrue(Fns.return_literal_true());
        assertFalse(Fns.return_literal_false());
    }

    @Test
    void test_literals_round_trip_literal42() {
        assertEquals(42L, Fns.round_trip_literal42(42L));
    }

    @Test
    void test_literals_round_trip_literal_draft() {
        assertEquals("draft", Fns.round_trip_literal_draft("draft"));
    }

    @Test
    void test_literals_round_trip_literal_escaped() {
        assertEquals("has \"quotes\"", Fns.round_trip_literal_escaped("has \"quotes\""));
    }

    @Test
    void test_literals_round_trip_literal_true() {
        assertTrue(Fns.round_trip_literal_true(true));
    }

    @Test
    void test_literals_round_trip_literal_false() {
        assertFalse(Fns.round_trip_literal_false(false));
    }

    @Test
    void test_literals_round_trip_literals() {
        Literals lit = new Literals(42L, "draft", "has \"quotes\"", true, false);
        assertEquals(lit, Fns.round_trip_literals(lit));
    }
}
