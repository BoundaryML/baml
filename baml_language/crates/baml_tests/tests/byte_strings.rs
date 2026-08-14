//! FFI-boundary tests for byte string literals (`b"..."`) and uint8array behavior.
//!
//! Exercises the host↔VM call boundary by passing a host
//! `BexExternalValue::Uint8Array` argument directly and asserting the conversion
//! that occurs only at that boundary. A BAML-side call with a `b"..."` literal
//! would not cover the host→VM path.

use baml_tests::baml_test;
use bex_engine::BexExternalValue;

// ============================================================================
// B5. Type identity & passing as arguments
// ============================================================================

#[tokio::test]
async fn pass_as_argument() {
    let output = baml_test! {
        baml: r#"
            function get_len(data: uint8array) -> int {
                data.length()
            }
        "#,
        entry: "get_len",
        args: { "data" => BexExternalValue::Uint8Array(b"hello".to_vec()) },
    };
    assert_eq!(output.result, Ok(BexExternalValue::Int(5)));
}
