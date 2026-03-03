//! Unified tests for environment variable operations.

#![allow(unsafe_code)]

use baml_tests::baml_test;
use bex_engine::{BexExternalValue, Ty};

#[tokio::test]
async fn env_get_or_panic_existing_var() {
    unsafe { std::env::set_var("BAML_TEST_ENV_PANIC", "panic_value") };
    let output = baml_test!(
        r#"
            function main() -> string {
                env.get_or_panic("BAML_TEST_ENV_PANIC")
            }
        "#
    );

    insta::assert_snapshot!(output.bytecode, @r#"
    function main() -> string {
        load_const "BAML_TEST_ENV_PANIC"
        dispatch_future env.get_or_panic
        await
        return
    }
    "#);
    assert_eq!(
        output.result,
        Ok(BexExternalValue::String("panic_value".to_string()))
    );
}

#[tokio::test]
async fn env_get_or_panic_missing_var() {
    unsafe { std::env::remove_var("BAML_TEST_MISSING_PANIC") };
    let output = baml_test!(
        r#"
            function main() -> string {
                env.get_or_panic("BAML_TEST_MISSING_PANIC")
            }
        "#
    );

    insta::assert_snapshot!(output.bytecode, @r#"
    function main() -> string {
        load_const "BAML_TEST_MISSING_PANIC"
        dispatch_future env.get_or_panic
        await
        return
    }
    "#);
    insta::assert_snapshot!(output.result.unwrap_err().to_string(), @"failed to call env.get_or_panic: Environment variable 'BAML_TEST_MISSING_PANIC' not found");
}

#[tokio::test]
async fn env_get_existing_var() {
    unsafe { std::env::set_var("BAML_TEST_ENV_GET", "hello_env") };
    let output = baml_test!(
        r#"
            function main() -> string? {
                env.get("BAML_TEST_ENV_GET")
            }
        "#
    );

    insta::assert_snapshot!(output.bytecode, @r#"
    function main() -> string? {
        load_const "BAML_TEST_ENV_GET"
        dispatch_future env.get
        await
        return
    }
    "#);
    assert_eq!(
        output.result,
        Ok(BexExternalValue::optional(
            BexExternalValue::String("hello_env".to_string()),
            Ty::string()
        ))
    );
}

#[tokio::test]
async fn env_get_missing_var_returns_null() {
    unsafe { std::env::remove_var("BAML_TEST_NONEXISTENT_VAR") };
    let output = baml_test!(
        r#"
            function main() -> string? {
                env.get("BAML_TEST_NONEXISTENT_VAR")
            }
        "#
    );

    insta::assert_snapshot!(output.bytecode, @r#"
    function main() -> string? {
        load_const "BAML_TEST_NONEXISTENT_VAR"
        dispatch_future env.get
        await
        return
    }
    "#);
    assert_eq!(
        output.result,
        Ok(BexExternalValue::optional(
            BexExternalValue::Null,
            Ty::string()
        ))
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
        dispatch_future env.get_or_panic
        await
        return
    }
    "#);
    assert_eq!(
        output.result,
        Ok(BexExternalValue::String("sugar_value".to_string()))
    );
}

#[tokio::test]
async fn env_sugar_missing_var() {
    unsafe { std::env::remove_var("BAML_TEST_SUGAR_MISSING") };
    let output = baml_test!(
        r#"
            function main() -> string {
                env.BAML_TEST_SUGAR_MISSING
            }
        "#
    );

    insta::assert_snapshot!(output.bytecode, @r#"
    function main() -> string {
        load_const "BAML_TEST_SUGAR_MISSING"
        dispatch_future env.get_or_panic
        await
        return
    }
    "#);
    insta::assert_snapshot!(output.result.unwrap_err().to_string(), @"failed to call env.get_or_panic: Environment variable 'BAML_TEST_SUGAR_MISSING' not found");
}
