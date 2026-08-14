//! Roundtrip coverage for `baml_sdk::void` — `void` return lowers to `()`.

use baml_sdk::void::no_op;

#[test]
fn test_void_no_op() {
    // `-> void` lowers to `()`; the successful unwrap is the `is None`
    // assertion.
    no_op().unwrap();
}
