//! Unified tests for shell operations.

use baml_tests::baml_test;
use bex_engine::BexExternalValue;

#[tokio::test]
async fn shell_echo() {
    let output = baml_test!(
        r#"
            function main() -> string {
                baml.sys.shell("echo 'Hello From Shell!'", null).stdout
            }
        "#
    );

    insta::assert_snapshot!(output.bytecode, @r#"
    function main() -> string {
        load_const "echo 'Hello From Shell!'"
        load_const null
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
                baml.sys.shell("echo 'hello world' | tr 'a-z' 'A-Z'", null).stdout
            }
        "#
    );

    insta::assert_snapshot!(output.bytecode, @r#"
    function main() -> string {
        load_const "echo 'hello world' | tr 'a-z' 'A-Z'"
        load_const null
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
                baml.sys.shell("exit 1", null).exit_code
            }
        "#
    );

    insta::assert_snapshot!(output.bytecode, @r#"
    function main() -> int {
        load_const "exit 1"
        load_const null
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
                baml.sys.shell("nonexistent_command_12345", null).exit_code
            }
        "#
    );

    insta::assert_snapshot!(output.bytecode, @r#"
    function main() -> int {
        load_const "nonexistent_command_12345"
        load_const null
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
                baml.sys.shell(cmd, null).stdout
            }
        "#
    );

    insta::assert_snapshot!(output.bytecode, @r#"
    function main() -> string {
        load_const "echo 'dynamic'"
        load_const null
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
                baml.sys.shell("echo 'error output' >&2", null).stderr
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
                baml.sys.shell("echo hi", null).ok()
            }
        "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Bool(true)));

    let output2 = baml_test!(
        r#"
            function main() -> bool {
                baml.sys.shell("exit 1", null).ok()
            }
        "#
    );
    assert_eq!(output2.result, Ok(BexExternalValue::Bool(false)));
}

// === exec() tests ===

#[tokio::test]
async fn exec_echo() {
    let output = baml_test!(
        r#"
            function main() -> string {
                baml.sys.exec("echo", ["Hello From Exec!"], null).stdout
            }
        "#
    );
    assert_eq!(
        output.result,
        Ok(BexExternalValue::String("Hello From Exec!\n".to_string()))
    );
}

#[tokio::test]
async fn exec_failing() {
    // `false` exits with code 1 — should NOT throw
    let output = baml_test!(
        r#"
            function main() -> int {
                baml.sys.exec("false", null, null).exit_code
            }
        "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(1)));
}

#[tokio::test]
async fn exec_with_args() {
    let output = baml_test!(
        r#"
            function main() -> string {
                baml.sys.exec("printf", ["%s %s", "hello", "world"], null).stdout
            }
        "#
    );
    assert_eq!(
        output.result,
        Ok(BexExternalValue::String("hello world".to_string()))
    );
}

#[tokio::test]
async fn exec_stderr() {
    let output = baml_test!(
        r#"
            function main() -> string {
                baml.sys.exec("sh", ["-c", "echo err >&2"], null).stderr
            }
        "#
    );
    assert!(output.result.is_ok());
    if let Ok(BexExternalValue::String(stderr)) = &output.result {
        assert!(stderr.contains("err"));
    }
}

// === ProcessOptions tests ===

#[tokio::test]
async fn exec_with_cwd() {
    let output = baml_test!(
        r#"
            function main() -> string {
                baml.sys.exec("pwd", null, baml.sys.ProcessOptions { cwd: "/tmp" }).stdout
            }
        "#
    );
    assert!(output.result.is_ok());
    if let Ok(BexExternalValue::String(stdout)) = &output.result {
        assert!(stdout.trim().contains("tmp"));
    }
}

#[tokio::test]
async fn exec_with_stdin() {
    let output = baml_test!(
        r#"
            function main() -> string {
                baml.sys.exec("cat", null, baml.sys.ProcessOptions { stdin: "hello from stdin" }).stdout
            }
        "#
    );
    assert_eq!(
        output.result,
        Ok(BexExternalValue::String("hello from stdin".to_string()))
    );
}

#[tokio::test]
async fn exec_with_timeout() {
    let output = baml_test!(
        r#"
            function main() -> string {
                baml.sys.exec("sleep", ["10"], baml.sys.ProcessOptions { timeout_ms: 100 }).stdout
            }
        "#
    );
    // Should timeout and throw (not return ShellOutput)
    assert!(output.result.is_err());
}

#[tokio::test]
async fn shell_with_options() {
    let output = baml_test!(
        r#"
            function main() -> string {
                baml.sys.shell("pwd", baml.sys.ProcessOptions { cwd: "/tmp" }).stdout
            }
        "#
    );
    assert!(output.result.is_ok());
    if let Ok(BexExternalValue::String(stdout)) = &output.result {
        assert!(stdout.trim().contains("tmp"));
    }
}
