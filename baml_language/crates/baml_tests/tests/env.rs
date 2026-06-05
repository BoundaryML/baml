//! Unified tests for environment variable operations.

#![allow(unsafe_code)]

use baml_tests::baml_test;
use bex_engine::BexExternalValue;

#[tokio::test]
async fn env_get_or_panic_existing_var() {
    unsafe { std::env::set_var("BAML_TEST_ENV_PANIC", "panic_value") };
    let output = baml_test!(
        r#"
            function main() -> string {
                baml.env.get_or_panic("BAML_TEST_ENV_PANIC")
            }
        "#
    );

    insta::assert_snapshot!(output.bytecode, @r#"
    function main() -> string {
        load_const "BAML_TEST_ENV_PANIC"
        call baml.env.get_or_panic
        return
    }
    "#);
    assert_eq!(
        output.result,
        Ok(BexExternalValue::String("panic_value".to_string().into()))
    );
}

#[tokio::test]
async fn env_get_existing_var() {
    unsafe { std::env::set_var("BAML_TEST_ENV_GET", "hello_env") };
    let output = baml_test!(
        r#"
            function main() -> string? {
                baml.env.get("BAML_TEST_ENV_GET")
            }
        "#
    );

    insta::assert_snapshot!(output.bytecode, @r#"
    function main() -> string | null {
        load_const "BAML_TEST_ENV_GET"
        sys_op baml.env.get
        return
    }
    "#);
    assert_eq!(
        output.result,
        Ok(BexExternalValue::String("hello_env".to_string().into()))
    );
}

#[tokio::test]
async fn env_sugar_existing_var() {
    unsafe { std::env::set_var("BAML_TEST_SUGAR_VAR", "sugar_value") };
    let output = baml_test!(
        r#"
            function main() -> string {
                env.BAML_TEST_SUGAR_VAR
            }
        "#
    );

    insta::assert_snapshot!(output.bytecode, @r#"
    function main() -> string {
        load_const "BAML_TEST_SUGAR_VAR"
        call baml.env.get_or_panic
        return
    }
    "#);
    assert_eq!(
        output.result,
        Ok(BexExternalValue::String("sugar_value".to_string().into()))
    );
}
