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
        Ok(BexExternalValue::String("panic_value".to_string()))
    );
}

#[tokio::test]
async fn env_get_or_panic_missing_var() {
    unsafe { std::env::remove_var("BAML_TEST_MISSING_PANIC") };
    let output = baml_test!(
        r#"
            function main() -> string {
                baml.env.get_or_panic("BAML_TEST_MISSING_PANIC")
            }
        "#
    );

    insta::assert_snapshot!(output.bytecode, @r#"
    function main() -> string {
        load_const "BAML_TEST_MISSING_PANIC"
        call baml.env.get_or_panic
        return
    }
    "#);
    insta::assert_snapshot!(output.result.unwrap_err().to_string(), @r#"
    Traceback (most recent call last):
      File "test.baml", line 3, in user.main
      File "<builtin>/baml/ns_env/env.baml", line 8, in baml.env.get_or_panic
    uncaught throw: Instance { class_name: "baml.panics.UserPanic", fields: {"message": String("env var not found: BAML_TEST_MISSING_PANIC")} }
    "#);
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
    function main() -> string? {
        load_const "BAML_TEST_ENV_GET"
        sys_op baml.env.get
        return
    }
    "#);
    assert_eq!(
        output.result,
        Ok(BexExternalValue::String("hello_env".to_string()))
    );
}

#[tokio::test]
async fn env_get_missing_var_returns_null() {
    unsafe { std::env::remove_var("BAML_TEST_NONEXISTENT_VAR") };
    let output = baml_test!(
        r#"
            function main() -> string? {
                baml.env.get("BAML_TEST_NONEXISTENT_VAR")
            }
        "#
    );

    insta::assert_snapshot!(output.bytecode, @r#"
    function main() -> string? {
        load_const "BAML_TEST_NONEXISTENT_VAR"
        sys_op baml.env.get
        return
    }
    "#);
    assert_eq!(output.result, Ok(BexExternalValue::Null));
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
        call baml.env.get_or_panic
        return
    }
    "#);
    insta::assert_snapshot!(output.result.unwrap_err().to_string(), @r#"
    Traceback (most recent call last):
      File "test.baml", line 3, in user.main
      File "<builtin>/baml/ns_env/env.baml", line 8, in baml.env.get_or_panic
    uncaught throw: Instance { class_name: "baml.panics.UserPanic", fields: {"message": String("env var not found: BAML_TEST_SUGAR_MISSING")} }
    "#);
}
