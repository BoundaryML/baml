//! Tests for byte string literals (`b"..."`) and uint8array behavior.

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
