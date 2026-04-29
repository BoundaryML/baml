//! Unified tests for shell operations.

use baml_tests::baml_test;
use bex_engine::BexExternalValue;

#[tokio::test]
async fn shell_echo() {
    let output = baml_test!(
        r#"
            function main() -> string {
                baml.sys.shell("echo 'Hello From Shell!'").stdout
            }
        "#
    );

    insta::assert_snapshot!(output.bytecode, @r#"
    function main() -> string {
        load_const "echo 'Hello From Shell!'"
        dispatch_future baml.sys.shell
        await
        load_field .stdout
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
                baml.sys.shell("echo 'hello world' | tr 'a-z' 'A-Z'").stdout
            }
        "#
    );

    insta::assert_snapshot!(output.bytecode, @r#"
    function main() -> string {
        load_const "echo 'hello world' | tr 'a-z' 'A-Z'"
        dispatch_future baml.sys.shell
        await
        load_field .stdout
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
            function main() -> int {
                baml.sys.shell("exit 1").exit_code
            }
        "#
    );

    insta::assert_snapshot!(output.bytecode, @r#"
    function main() -> int {
        load_const "exit 1"
        dispatch_future baml.sys.shell
        await
        load_field .exit_code
        return
    }
    "#);
    assert_eq!(output.result, Ok(BexExternalValue::Int(1)));
}

#[tokio::test]
async fn shell_nonexistent_command() {
    // A nonexistent command run through `sh -c` produces a shell error
    // (exit code 127) rather than a spawn failure, because `sh` itself
    // spawns successfully. This should return a ShellOutput, not an error.
    let output = baml_test!(
        r#"
            function main() -> int {
                baml.sys.shell("nonexistent_command_12345").exit_code
            }
        "#
    );

    insta::assert_snapshot!(output.bytecode, @r#"
    function main() -> int {
        load_const "nonexistent_command_12345"
        dispatch_future baml.sys.shell
        await
        load_field .exit_code
        return
    }
    "#);
    // Shell returns 127 for command not found
    assert_eq!(output.result, Ok(BexExternalValue::Int(127)));
}

#[tokio::test]
async fn shell_with_variable() {
    let output = baml_test!(
        r#"
            function main() -> string {
                let cmd = "echo 'dynamic'";
                baml.sys.shell(cmd).stdout
            }
        "#
    );

    insta::assert_snapshot!(output.bytecode, @r#"
    function main() -> string {
        load_const "echo 'dynamic'"
        dispatch_future baml.sys.shell
        await
        load_field .stdout
        return
    }
    "#);
    assert_eq!(
        output.result,
        Ok(BexExternalValue::String("dynamic\n".to_string()))
    );
}

#[tokio::test]
async fn shell_stderr() {
    let output = baml_test!(
        r#"
            function main() -> string {
                baml.sys.shell("echo 'error output' >&2").stderr
            }
        "#
    );

    assert!(output.result.is_ok());
    if let Ok(BexExternalValue::String(stderr)) = &output.result {
        assert!(stderr.contains("error output"));
    }
}

#[tokio::test]
async fn shell_ok_method() {
    let output = baml_test!(
        r#"
            function main() -> bool {
                baml.sys.shell("echo hi").ok()
            }
        "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Bool(true)));

    let output2 = baml_test!(
        r#"
            function main() -> bool {
                baml.sys.shell("exit 1").ok()
            }
        "#
    );
    assert_eq!(output2.result, Ok(BexExternalValue::Bool(false)));
}
