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
            s.toLowerCase()
        }
    "#
    );

    insta::assert_snapshot!(output.bytecode, @r#"
    function main() -> string {
        load_const "HELLO World"
        call baml.String.toLowerCase
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
            s.toUpperCase()
        }
    "#
    );

    insta::assert_snapshot!(output.bytecode, @r#"
    function main() -> string {
        load_const "hello WORLD"
        call baml.String.toUpperCase
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
            s.startsWith("hello")
        }
    "#
    );

    insta::assert_snapshot!(output.bytecode, @r#"
    function main() -> bool {
        load_const "hello world"
        load_const "hello"
        call baml.String.startsWith
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
            s.endsWith("world")
        }
    "#
    );

    insta::assert_snapshot!(output.bytecode, @r#"
    function main() -> bool {
        load_const "hello world"
        load_const "world"
        call baml.String.endsWith
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
            s.substring(2, 10)  // Should clamp to string length
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
