// Roundtrip coverage for `baml_sdk.void` — `void` return lowers to `null`.
//
// Port of python_pydantic2/type_shapes/customizable/roundtrip_tests/
// test_void.py — same test names, cases, inputs, assertions.
package roundtrip_tests;

import static org.junit.jupiter.api.Assertions.assertNull;

import org.junit.jupiter.api.Test;

class TestVoid {

    @Test
    void test_no_op() {
        // java-port note: the BAML namespace is literally named `void`,
        // which is a reserved Java keyword and cannot appear as a package
        // segment verbatim (unlike Python, where `void` is a legal module
        // name). Following the same `$`-escape precedent the conventions
        // doc uses for a user symbol colliding with the `Fns` holder name
        // (`Fns` -> `Fns$`), this port assumes codegen escapes the
        // namespace package itself to `void$`. This is an extrapolation
        // beyond the doc and needs confirmation.
        assertNull(baml_sdk.void$.Fns.no_op());
    }
}
