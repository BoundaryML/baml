//! Unified tests for shell operations.

use baml_tests::baml_test;
use bex_engine::BexExternalValue;

#[tokio::test]
async fn shell_echo() {
    let output = baml_test!(
        r#"
            function main() -> string {
                baml.sys.shell("echo 'Hello From Shell!'")
            }
        "#
    );

    insta::assert_snapshot!(output.bytecode, @r#"
    function main() -> string {
        load_const "echo 'Hello From Shell!'"
        dispatch_future baml.sys.shell
        await
        return
    }
    "#);
    assert_eq!(
        output.result,
        Ok(BexExternalValue::String("Hello From Shell!\n".to_string()))
    );
}

#[tokio::test]
async fn shell_with_pipe() {
    let output = baml_test!(
        r#"
            function main() -> string {
                baml.sys.shell("echo 'hello world' | tr 'a-z' 'A-Z'")
            }
        "#
    );

    insta::assert_snapshot!(output.bytecode, @r#"
    function main() -> string {
        load_const "echo 'hello world' | tr 'a-z' 'A-Z'"
        dispatch_future baml.sys.shell
        await
        return
    }
    "#);
    assert_eq!(
        output.result,
        Ok(BexExternalValue::String("HELLO WORLD\n".to_string()))
    );
}

#[tokio::test]
async fn shell_failing_command() {
    let output = baml_test!(
        r#"
            function main() -> string {
                baml.sys.shell("exit 1")
            }
        "#
    );

    insta::assert_snapshot!(output.bytecode, @r#"
    function main() -> string {
        load_const "exit 1"
        dispatch_future baml.sys.shell
        await
        return
    }
    "#);
    insta::assert_snapshot!(output.result.unwrap_err().to_string(), @"failed to call baml.sys.shell: Command 'exit 1' failed with exit code 1: ");
}

#[tokio::test]
async fn shell_nonexistent_command() {
    let output = baml_test!(
        r#"
            function main() -> string {
                baml.sys.shell("nonexistent_command_12345")
            }
        "#
    );

    insta::assert_snapshot!(output.bytecode, @r#"
    function main() -> string {
        load_const "nonexistent_command_12345"
        dispatch_future baml.sys.shell
        await
        return
    }
    "#);
    // Error message includes shell output which differs across platforms.
    assert!(output.result.is_err());
}

#[tokio::test]
async fn shell_with_variable() {
    let output = baml_test!(
        r#"
            function main() -> string {
                let cmd = "echo 'dynamic'";
                baml.sys.shell(cmd)
            }
        "#
    );

    insta::assert_snapshot!(output.bytecode, @r#"
    function main() -> string {
        load_const "echo 'dynamic'"
        dispatch_future baml.sys.shell
        await
        return
    }
    "#);
    assert_eq!(
        output.result,
        Ok(BexExternalValue::String("dynamic\n".to_string()))
    );
}
