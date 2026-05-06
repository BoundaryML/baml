//! Unified tests for string operations.

use baml_tests::baml_test;
use baml_type::Ty;
use bex_engine::BexExternalValue;

#[tokio::test]
async fn concat_strings() {
    let output = baml_test!(
        r#"
        function main() -> string {
            let a = "Hello";
            let b = " World";

            a + b
        }
    "#
    );

    insta::assert_snapshot!(output.bytecode, @r#"
    function main() -> string {
        load_const "Hello"
        load_const " World"
        bin_op +
        return
    }
    "#);

    assert_eq!(
        output.result,
        Ok(BexExternalValue::String("Hello World".to_string()))
    );
}

#[tokio::test]
async fn string_equality_true() {
    let output = baml_test!(
        r#"
        function main() -> bool {
            "Hello" == "Hello"
        }
    "#
    );

    insta::assert_snapshot!(output.bytecode, @r#"
    function main() -> bool {
        load_const "Hello"
        load_const "Hello"
        cmp_op ==
        return
    }
    "#);

    assert_eq!(output.result, Ok(BexExternalValue::Bool(true)));
}

#[tokio::test]
async fn string_equality_false() {
    let output = baml_test!(
        r#"
        function main() -> bool {
            "Hello" == "World"
        }
    "#
    );

    insta::assert_snapshot!(output.bytecode, @r#"
    function main() -> bool {
        load_const "Hello"
        load_const "World"
        cmp_op ==
        return
    }
    "#);

    assert_eq!(output.result, Ok(BexExternalValue::Bool(false)));
}

#[tokio::test]
async fn string_not_equal_true() {
    let output = baml_test!(
        r#"
        function main() -> bool {
            "Hello" != "World"
        }
    "#
    );

    insta::assert_snapshot!(output.bytecode, @r#"
    function main() -> bool {
        load_const "Hello"
        load_const "World"
        cmp_op !=
        return
    }
    "#);

    assert_eq!(output.result, Ok(BexExternalValue::Bool(true)));
}

#[tokio::test]
async fn string_less_than() {
    let output = baml_test!(
        r#"
        function main() -> bool {
            "a" < "b"
        }
    "#
    );

    insta::assert_snapshot!(output.bytecode, @r#"
    function main() -> bool {
        load_const "a"
        load_const "b"
        cmp_op <
        return
    }
    "#);

    assert_eq!(output.result, Ok(BexExternalValue::Bool(true)));
}

#[tokio::test]
async fn string_less_than_or_equal() {
    let output = baml_test!(
        r#"
        function main() -> bool {
            "a" <= "b"
        }
    "#
    );

    insta::assert_snapshot!(output.bytecode, @r#"
    function main() -> bool {
        load_const "a"
        load_const "b"
        cmp_op <=
        return
    }
    "#);

    assert_eq!(output.result, Ok(BexExternalValue::Bool(true)));
}

#[tokio::test]
async fn string_greater_than() {
    let output = baml_test!(
        r#"
        function main() -> bool {
            "b" > "a"
        }
    "#
    );

    insta::assert_snapshot!(output.bytecode, @r#"
    function main() -> bool {
        load_const "b"
        load_const "a"
        cmp_op >
        return
    }
    "#);

    assert_eq!(output.result, Ok(BexExternalValue::Bool(true)));
}

#[tokio::test]
async fn string_greater_than_or_equal() {
    let output = baml_test!(
        r#"
        function main() -> bool {
            "b" >= "a"
        }
    "#
    );

    insta::assert_snapshot!(output.bytecode, @r#"
    function main() -> bool {
        load_const "b"
        load_const "a"
        cmp_op >=
        return
    }
    "#);

    assert_eq!(output.result, Ok(BexExternalValue::Bool(true)));
}

#[tokio::test]
async fn string_length() {
    let output = baml_test!(
        r#"
        function main() -> int {
            let s = "hello";
            s.length()
        }
    "#
    );

    insta::assert_snapshot!(output.bytecode, @r#"
    function main() -> int {
        load_const "hello"
        call baml.String.length
        return
    }
    "#);

    assert_eq!(output.result, Ok(BexExternalValue::Int(5)));
}

#[tokio::test]
async fn string_to_lower_case() {
    let output = baml_test!(
        r#"
        function main() -> string {
            let s = "HELLO World";
            s.to_lower_case()
        }
    "#
    );

    insta::assert_snapshot!(output.bytecode, @r#"
    function main() -> string {
        load_const "HELLO World"
        call baml.String.to_lower_case
        return
    }
    "#);

    assert_eq!(
        output.result,
        Ok(BexExternalValue::String("hello world".to_string()))
    );
}

#[tokio::test]
async fn string_to_upper_case() {
    let output = baml_test!(
        r#"
        function main() -> string {
            let s = "hello WORLD";
            s.to_upper_case()
        }
    "#
    );

    insta::assert_snapshot!(output.bytecode, @r#"
    function main() -> string {
        load_const "hello WORLD"
        call baml.String.to_upper_case
        return
    }
    "#);

    assert_eq!(
        output.result,
        Ok(BexExternalValue::String("HELLO WORLD".to_string()))
    );
}

#[tokio::test]
async fn string_trim() {
    let output = baml_test!(
        r#"
        function main() -> string {
            let s = "  hello world  ";
            s.trim()
        }
    "#
    );

    insta::assert_snapshot!(output.bytecode, @r#"
    function main() -> string {
        load_const "  hello world  "
        call baml.String.trim
        return
    }
    "#);

    assert_eq!(
        output.result,
        Ok(BexExternalValue::String("hello world".to_string()))
    );
}

#[tokio::test]
async fn string_includes() {
    let output = baml_test!(
        r#"
        function main() -> bool {
            let s = "hello world";
            s.includes("world")
        }
    "#
    );

    insta::assert_snapshot!(output.bytecode, @r#"
    function main() -> bool {
        load_const "hello world"
        load_const "world"
        call baml.String.includes
        return
    }
    "#);

    assert_eq!(output.result, Ok(BexExternalValue::Bool(true)));
}

#[tokio::test]
async fn string_starts_with() {
    let output = baml_test!(
        r#"
        function main() -> bool {
            let s = "hello world";
            s.starts_with("hello")
        }
    "#
    );

    insta::assert_snapshot!(output.bytecode, @r#"
    function main() -> bool {
        load_const "hello world"
        load_const "hello"
        call baml.String.starts_with
        return
    }
    "#);

    assert_eq!(output.result, Ok(BexExternalValue::Bool(true)));
}

#[tokio::test]
async fn string_ends_with() {
    let output = baml_test!(
        r#"
        function main() -> bool {
            let s = "hello world";
            s.ends_with("world")
        }
    "#
    );

    insta::assert_snapshot!(output.bytecode, @r#"
    function main() -> bool {
        load_const "hello world"
        load_const "world"
        call baml.String.ends_with
        return
    }
    "#);

    assert_eq!(output.result, Ok(BexExternalValue::Bool(true)));
}

#[tokio::test]
async fn string_split() {
    let output = baml_test!(
        r#"
        function main() -> string[] {
            let s = "hello,world,test";
            s.split(",")
        }
    "#
    );

    insta::assert_snapshot!(output.bytecode, @r#"
    function main() -> string[] {
        load_const "hello,world,test"
        load_const ","
        call baml.String.split
        return
    }
    "#);

    assert_eq!(
        output.result,
        Ok(BexExternalValue::Array {
            element_type: Ty::String {
                attr: baml_base::TyAttr::default()
            },
            items: vec![
                BexExternalValue::String("hello".to_string()),
                BexExternalValue::String("world".to_string()),
                BexExternalValue::String("test".to_string()),
            ],
        })
    );
}

#[tokio::test]
async fn string_substring() {
    let output = baml_test!(
        r#"
        function main() -> string {
            let s = "hello world";
            s.substring(0, 5)
        }
    "#
    );

    insta::assert_snapshot!(output.bytecode, @r#"
    function main() -> string {
        load_const "hello world"
        load_const 0
        load_const 5
        call baml.String.substring
        return
    }
    "#);

    assert_eq!(
        output.result,
        Ok(BexExternalValue::String("hello".to_string()))
    );
}

#[tokio::test]
async fn string_substring_bounds() {
    let output = baml_test!(
        r#"
        function main() -> string {
            let s = "hello";
            s.substring(2, 10)  // end byte offset clamped to length
        }
    "#
    );

    insta::assert_snapshot!(output.bytecode, @r#"
    function main() -> string {
        load_const "hello"
        load_const 2
        load_const 10
        call baml.String.substring
        return
    }
    "#);

    assert_eq!(
        output.result,
        Ok(BexExternalValue::String("llo".to_string()))
    );
}

// ─── byte-indexed semantics (BEP-043) ─────────────────────────────────────────

#[tokio::test]
async fn string_length_is_bytes_for_non_ascii() {
    // `é` is U+00E9, encoded as two bytes in UTF-8 (0xC3 0xA9).
    let output = baml_test!(
        r#"
        function main() -> int {
            "héllo".length()
        }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(6)));
}

#[tokio::test]
async fn string_length_is_bytes_for_emoji() {
    // 🐈 (U+1F408) takes 4 bytes in UTF-8.
    let output = baml_test!(
        r#"
        function main() -> int {
            "🐈".length()
        }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(4)));
}

#[tokio::test]
async fn string_substring_byte_indexed_keeps_multibyte_char() {
    // Bytes 0..3 of "héllo" are the H plus the two bytes of `é`.
    let output = baml_test!(
        r#"
        function main() -> string {
            "héllo".substring(0, 3)
        }
    "#
    );
    assert_eq!(
        output.result,
        Ok(BexExternalValue::String("hé".to_string()))
    );
}

#[tokio::test]
async fn string_substring_mid_codepoint_throws() {
    // Byte 2 of "héllo" lands inside the `é` (which occupies bytes 1..3).
    let output = baml_test!(
        r#"
        function main() -> string {
            "héllo".substring(0, 2)
        }
    "#
    );
    let Err(bex_engine::EngineError::UnhandledThrow { .. }) = &output.result else {
        panic!("expected UnhandledThrow, got: {:?}", output.result);
    };
}

#[tokio::test]
async fn string_substring_negative_clamps_to_zero() {
    let output = baml_test!(
        r#"
        function main() -> string {
            "hello".substring(-3, 4)
        }
    "#
    );
    assert_eq!(
        output.result,
        Ok(BexExternalValue::String("hell".to_string()))
    );
}

#[tokio::test]
async fn string_substring_empty_when_start_past_end() {
    let output = baml_test!(
        r#"
        function main() -> string {
            "hello".substring(4, 2)
        }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::String("".to_string())));
}

#[tokio::test]
async fn string_char_at_byte_indexed_returns_multibyte_char() {
    // `é` starts at byte 1 of "héllo".
    let output = baml_test!(
        r#"
        function main() -> string {
            "héllo".char_at(1)
        }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::String("é".to_string())));
}

#[tokio::test]
async fn string_char_at_byte_indexed_after_multibyte() {
    // `l` after `é` lives at byte 3.
    let output = baml_test!(
        r#"
        function main() -> string {
            "héllo".char_at(3)
        }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::String("l".to_string())));
}

#[tokio::test]
async fn string_char_at_mid_codepoint_throws() {
    let output = baml_test!(
        r#"
        function main() -> string {
            "héllo".char_at(2)
        }
    "#
    );
    let Err(bex_engine::EngineError::UnhandledThrow { .. }) = &output.result else {
        panic!("expected UnhandledThrow, got: {:?}", output.result);
    };
}

#[tokio::test]
async fn string_char_at_end_returns_empty() {
    let output = baml_test!(
        r#"
        function main() -> string {
            "hi".char_at(2)
        }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::String("".to_string())));
}

#[tokio::test]
async fn string_char_at_past_end_throws() {
    let output = baml_test!(
        r#"
        function main() -> string {
            "hi".char_at(99)
        }
    "#
    );
    let Err(bex_engine::EngineError::UnhandledThrow { .. }) = &output.result else {
        panic!("expected UnhandledThrow, got: {:?}", output.result);
    };
}

#[tokio::test]
async fn string_char_at_negative_throws() {
    let output = baml_test!(
        r#"
        function main() -> string {
            "hi".char_at(-1)
        }
    "#
    );
    let Err(bex_engine::EngineError::UnhandledThrow { .. }) = &output.result else {
        panic!("expected UnhandledThrow, got: {:?}", output.result);
    };
}

#[tokio::test]
async fn string_index_of_returns_byte_offset() {
    // The first `l` in "héllo" lives at byte 3 (after the 2-byte `é`).
    let output = baml_test!(
        r#"
        function main() -> int {
            "héllo".index_of("l")
        }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(3)));
}

#[tokio::test]
async fn string_replace() {
    let output = baml_test!(
        r#"
        function main() -> string {
            let s = "hello world world";
            s.replace("world", "BAML")
        }
    "#
    );

    insta::assert_snapshot!(output.bytecode, @r#"
    function main() -> string {
        load_const "hello world world"
        load_const "world"
        load_const "BAML"
        call baml.String.replace
        return
    }
    "#);

    assert_eq!(
        output.result,
        Ok(BexExternalValue::String("hello BAML world".to_string()))
    );
}
