// Roundtrip coverage for `baml_sdk.void` — `void` return lowers to `null`.
//
// Port of python_pydantic2/type_shapes/customizable/roundtrip_tests/
// test_void.py — same test names, cases, inputs, assertions.
package roundtrip_tests;


import org.junit.jupiter.api.Test;

class TestVoid {

    @Test
    void test_void_no_op() {
        // java-port note: the BAML namespace is literally named `void`,
        // which is a reserved Java keyword and cannot appear as a package
        // segment verbatim (unlike Python, where `void` is a legal module
        // name). Following the same `$`-escape precedent the conventions
        // doc uses for a user symbol colliding with the `Fns` holder name
        // (`Fns` -> `Fns$`), this port assumes codegen escapes the
        // namespace package itself to `void$`. This is an extrapolation
        // beyond the doc and needs confirmation.
        // java-port note: BAML `-> void` maps to Java `void` (Python gets None);
        // the strongest expressible assertion is that the call completes.
        org.junit.jupiter.api.Assertions.assertDoesNotThrow(() -> baml_sdk.void$.Fns.no_op());
    }
}
