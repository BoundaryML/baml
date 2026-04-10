//! Tests for byte string literals (`b"..."`) and uint8array behavior.

use baml_tests::baml_test;
use bex_engine::BexExternalValue;

// ============================================================================
// B1. Byte string literal construction
// ============================================================================

#[tokio::test]
async fn literal_basic() {
    let output = baml_test!(
        r#"
        function main() -> uint8array {
            let x = b"hello";
            x
        }
    "#
    );
    assert_eq!(
        output.result,
        Ok(BexExternalValue::Uint8Array(b"hello".to_vec()))
    );
}

#[tokio::test]
async fn literal_empty() {
    let output = baml_test!(
        r#"
        function main() -> uint8array {
            let x = b"";
            x
        }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Uint8Array(vec![])));
}

#[tokio::test]
async fn literal_hex_escapes() {
    let output = baml_test!(
        r#"
        function main() -> uint8array {
            let x = b"\x00\x7f\xff";
            x
        }
    "#
    );
    assert_eq!(
        output.result,
        Ok(BexExternalValue::Uint8Array(vec![0x00, 0x7f, 0xff]))
    );
}

#[tokio::test]
async fn literal_standard_escapes() {
    let output = baml_test!(
        r#"
        function main() -> uint8array {
            let x = b"\n\t\r\0\\\"";
            x
        }
    "#
    );
    assert_eq!(
        output.result,
        Ok(BexExternalValue::Uint8Array(vec![
            b'\n', b'\t', b'\r', 0, b'\\', b'"'
        ]))
    );
}

#[tokio::test]
async fn literal_mixed_ascii_and_hex() {
    let output = baml_test!(
        r#"
        function main() -> uint8array {
            let x = b"A\x42C";
            x
        }
    "#
    );
    assert_eq!(
        output.result,
        Ok(BexExternalValue::Uint8Array(vec![b'A', 0x42, b'C']))
    );
}

// ============================================================================
// B2. Uint8Array methods
// ============================================================================

#[tokio::test]
async fn length_basic() {
    let output = baml_test!(
        r#"
        function main() -> int {
            b"hello".length()
        }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(5)));
}

#[tokio::test]
async fn length_empty() {
    let output = baml_test!(
        r#"
        function main() -> int {
            b"".length()
        }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(0)));
}

#[tokio::test]
async fn at_valid_index() {
    let output = baml_test!(
        r#"
        function main() -> int? {
            b"\x41\x42\x43".at(1)
        }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(0x42)));
}

#[tokio::test]
async fn at_out_of_bounds() {
    let output = baml_test!(
        r#"
        function main() -> int? {
            b"abc".at(10)
        }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Null));
}

#[tokio::test]
async fn at_negative_index() {
    let output = baml_test!(
        r#"
        function main() -> int? {
            b"abc".at(-1)
        }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Null));
}

// ============================================================================
// B3. Uint8Array.zeroes()
// ============================================================================

#[tokio::test]
async fn zeroes_basic() {
    let output = baml_test!(
        r#"
        function main() -> uint8array {
            let x = b"\x00\x00\x00";
            x
        }
    "#
    );
    assert_eq!(
        output.result,
        Ok(BexExternalValue::Uint8Array(vec![0, 0, 0]))
    );
}

// ============================================================================
// B4. Index read/write via []
// ============================================================================

#[tokio::test]
async fn index_read() {
    let output = baml_test!(
        r#"
        function main() -> int {
            let data = b"\x41\x42";
            data[0]
        }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(0x41)));
}

#[tokio::test]
async fn index_write() {
    let output = baml_test!(
        r#"
        function main() -> int? {
            let data = b"\x00\x00";
            data[0] = 255;
            data.at(0)
        }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(255)));
}

#[tokio::test]
async fn index_write_out_of_range() {
    let output = baml_test!(
        r#"
        function main() -> int {
            let data = b"\x00";
            data[0] = 256;
            data[0]
        }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(0)));
}

#[tokio::test]
async fn index_out_of_bounds() {
    let output = baml_test!(
        r#"
        function main() -> int {
            b"a"[5]
        }
    "#
    );
    assert!(output.result.is_err());
}

#[tokio::test]
async fn index_negative() {
    let output = baml_test!(
        r#"
        function main() -> int {
            b"a"[-1]
        }
    "#
    );
    assert!(output.result.is_err());
}

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

#[tokio::test]
async fn return_uint8array() {
    let output = baml_test!(
        r#"
        function main() -> uint8array {
            let x = b"abc";
            x
        }
    "#
    );
    assert_eq!(
        output.result,
        Ok(BexExternalValue::Uint8Array(b"abc".to_vec()))
    );
}

// ============================================================================
// B6. Deep equality
// ============================================================================

#[tokio::test]
async fn equality_same_content() {
    let output = baml_test!(
        r#"
        function main() -> bool {
            b"abc" == b"abc"
        }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Bool(true)));
}

#[tokio::test]
async fn equality_different_content() {
    let output = baml_test!(
        r#"
        function main() -> bool {
            b"abc" == b"xyz"
        }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Bool(false)));
}

#[tokio::test]
async fn equality_empty() {
    let output = baml_test!(
        r#"
        function main() -> bool {
            b"" == b""
        }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Bool(true)));
}

#[tokio::test]
async fn equality_different_lengths() {
    let output = baml_test!(
        r#"
        function main() -> bool {
            b"ab" == b"abc"
        }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Bool(false)));
}

#[tokio::test]
async fn equality_different_lengths_reversed() {
    let output = baml_test!(
        r#"
        function main() -> bool {
            b"abc" == b"ab"
        }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Bool(false)));
}

#[tokio::test]
async fn inequality_same() {
    let output = baml_test!(
        r#"
        function main() -> bool {
            b"abc" != b"abc"
        }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Bool(false)));
}

#[tokio::test]
async fn inequality_different() {
    let output = baml_test!(
        r#"
        function main() -> bool {
            b"abc" != b"xyz"
        }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Bool(true)));
}

// ============================================================================
// B7. Disambiguation: `b` as identifier vs byte string prefix
// ============================================================================

#[tokio::test]
async fn b_as_identifier() {
    let output = baml_test!(
        r#"
        function main() -> int {
            let b = 42;
            b
        }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(42)));
}

// ============================================================================
// B8. Byte string as call argument
// ============================================================================

#[tokio::test]
async fn literal_as_call_argument() {
    let output = baml_test!(
        r#"
        function get_len(data: uint8array) -> int {
            data.length()
        }

        function main() -> int {
            get_len(b"hello")
        }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(5)));
}

// ============================================================================
// B9. Direct index read (data[0] not .at(0))
// ============================================================================

#[tokio::test]
async fn index_read_direct() {
    let output = baml_test!(
        r#"
        function main() -> int {
            let data = b"\x41\x42\x43";
            data[0]
        }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(0x41)));
}
