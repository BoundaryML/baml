//! Unified tests for shell operations.

use baml_tests::baml_test;
use bex_engine::BexExternalValue;

// === shell() tests ===

#[tokio::test]
async fn shell_echo() {
    let output = baml_test!(
        r#"
            function main() -> string {
                baml.sys.shell("echo hello_from_shell", null).stdout.to_string()
            }
        "#
    );
    assert!(output.result.is_ok());
    if let Ok(BexExternalValue::String(stdout)) = &output.result {
        assert!(stdout.contains("hello_from_shell"));
    }
}

#[tokio::test]
#[cfg(not(target_os = "windows"))]
async fn shell_with_pipe() {
    let output = baml_test!(
        r#"
            function main() -> string {
                baml.sys.shell("echo 'hello world' | tr 'a-z' 'A-Z'", null).stdout.to_string()
            }
        "#
    );

    insta::assert_snapshot!(output.bytecode, @r#"
    function main() -> string {
        load_const "echo 'hello world' | tr 'a-z' 'A-Z'"
        load_const null
        sys_op baml.sys.shell
        load_field .stdout
        call baml.Uint8Array.to_string
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
    assert_eq!(output.result, Ok(BexExternalValue::Int(1)));
}

#[tokio::test]
async fn shell_nonexistent_command() {
    let output = baml_test!(
        r#"
            function main() -> int {
                baml.sys.shell("nonexistent_command_12345", null).exit_code
            }
        "#
    );
    // Should succeed (shell itself spawns fine) with a non-zero exit code.
    // Unix shells return 127, cmd returns 9009, PowerShell returns 1.
    assert!(output.result.is_ok());
    if let Ok(BexExternalValue::Int(code)) = &output.result {
        assert_ne!(
            *code, 0,
            "nonexistent command should have non-zero exit code"
        );
    }
}

#[tokio::test]
async fn shell_with_variable() {
    let output = baml_test!(
        r#"
            function main() -> string {
                let cmd = "echo dynamic_value";
                baml.sys.shell(cmd, null).stdout.to_string()
            }
        "#
    );
    assert!(output.result.is_ok());
    if let Ok(BexExternalValue::String(stdout)) = &output.result {
        assert!(stdout.contains("dynamic_value"));
    }
}

#[tokio::test]
#[cfg(not(target_os = "windows"))]
async fn shell_stderr() {
    let output = baml_test!(
        r#"
            function main() -> string {
                baml.sys.shell("echo 'error output' >&2", null).stderr.to_string()
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
                baml.sys.exec("echo", ["Hello From Exec!"], null).stdout.to_string()
            }
        "#
    );
    assert!(output.result.is_ok());
    if let Ok(BexExternalValue::String(stdout)) = &output.result {
        assert!(stdout.contains("Hello From Exec!"));
    }
}

#[tokio::test]
#[cfg(not(target_os = "windows"))]
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
#[cfg(target_os = "windows")]
async fn exec_failing() {
    // cmd /c "exit 1" exits with code 1 — should NOT throw
    let output = baml_test!(
        r#"
            function main() -> int {
                baml.sys.exec("cmd", ["/c", "exit 1"], null).exit_code
            }
        "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(1)));
}

#[tokio::test]
#[cfg(not(target_os = "windows"))]
async fn exec_with_args() {
    let output = baml_test!(
        r#"
            function main() -> string {
                baml.sys.exec("printf", ["%s %s", "hello", "world"], null).stdout.to_string()
            }
        "#
    );
    assert_eq!(
        output.result,
        Ok(BexExternalValue::String("hello world".to_string()))
    );
}

#[tokio::test]
#[cfg(target_os = "windows")]
async fn exec_with_args() {
    let output = baml_test!(
        r#"
            function main() -> string {
                baml.sys.exec("cmd", ["/c", "echo hello world"], null).stdout.to_string()
            }
        "#
    );
    assert!(output.result.is_ok());
    if let Ok(BexExternalValue::String(stdout)) = &output.result {
        assert!(stdout.contains("hello world"));
    }
}

#[tokio::test]
#[cfg(not(target_os = "windows"))]
async fn exec_stderr() {
    let output = baml_test!(
        r#"
            function main() -> string {
                baml.sys.exec("sh", ["-c", "echo err >&2"], null).stderr.to_string()
            }
        "#
    );
    assert!(output.result.is_ok());
    if let Ok(BexExternalValue::String(stderr)) = &output.result {
        assert!(stderr.contains("err"));
    }
}

#[tokio::test]
#[cfg(target_os = "windows")]
async fn exec_stderr() {
    let output = baml_test!(
        r#"
            function main() -> string {
                baml.sys.exec("cmd", ["/c", "echo err 1>&2"], null).stderr.to_string()
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
#[cfg(not(target_os = "windows"))]
async fn exec_with_cwd() {
    let output = baml_test!(
        r#"
            function main() -> string {
                baml.sys.exec("pwd", null, baml.sys.ProcessOptions { cwd: "/tmp" }).stdout.to_string()
            }
        "#
    );
    assert!(output.result.is_ok());
    if let Ok(BexExternalValue::String(stdout)) = &output.result {
        assert!(stdout.trim().contains("tmp"));
    }
}

#[tokio::test]
#[cfg(target_os = "windows")]
async fn exec_with_cwd() {
    let output = baml_test!(
        r#"
            function main() -> string {
                baml.sys.exec("cmd", ["/c", "cd"], baml.sys.ProcessOptions { cwd: "C:\\Windows\\Temp" }).stdout.to_string()
            }
        "#
    );
    assert!(output.result.is_ok());
    if let Ok(BexExternalValue::String(stdout)) = &output.result {
        assert!(stdout.trim().contains("Temp"));
    }
}

#[tokio::test]
#[cfg(not(target_os = "windows"))]
async fn exec_with_stdin() {
    let output = baml_test!(
        r#"
            function main() -> string {
                baml.sys.exec("cat", null, baml.sys.ProcessOptions { stdin: "hello from stdin" }).stdout.to_string()
            }
        "#
    );
    assert_eq!(
        output.result,
        Ok(BexExternalValue::String("hello from stdin".to_string()))
    );
}

#[tokio::test]
#[cfg(target_os = "windows")]
async fn exec_with_stdin() {
    let output = baml_test!(
        r#"
            function main() -> string {
                baml.sys.exec("findstr", [".*"], baml.sys.ProcessOptions { stdin: "hello from stdin" }).stdout.to_string()
            }
        "#
    );
    assert!(output.result.is_ok());
    if let Ok(BexExternalValue::String(stdout)) = &output.result {
        assert!(stdout.contains("hello from stdin"));
    }
}

#[tokio::test]
#[cfg(not(target_os = "windows"))]
async fn exec_with_timeout() {
    let output = baml_test!(
        r#"
            function main() -> string {
                baml.sys.exec("sleep", ["10"], baml.sys.ProcessOptions { timeout_ms: 100 }).stdout.to_string()
            }
        "#
    );
    // Should timeout and throw (not return ShellOutput)
    assert!(output.result.is_err());
}

#[tokio::test]
#[cfg(target_os = "windows")]
async fn exec_with_timeout() {
    let output = baml_test!(
        r#"
            function main() -> string {
                baml.sys.exec("ping", ["-n", "11", "127.0.0.1"], baml.sys.ProcessOptions { timeout_ms: 100 }).stdout.to_string()
            }
        "#
    );
    // Should timeout and throw (not return ShellOutput)
    assert!(output.result.is_err());
}

#[tokio::test]
#[cfg(not(target_os = "windows"))]
async fn shell_with_options() {
    let output = baml_test!(
        r#"
            function main() -> string {
                baml.sys.shell("pwd", baml.sys.ProcessOptions { cwd: "/tmp" }).stdout.to_string()
            }
        "#
    );
    assert!(output.result.is_ok());
    if let Ok(BexExternalValue::String(stdout)) = &output.result {
        assert!(stdout.trim().contains("tmp"));
    }
}

#[tokio::test]
#[cfg(target_os = "windows")]
async fn shell_with_options() {
    // Use `cmd /c cd` so the inner cmd.exe prints the inherited cwd. This works
    // regardless of whether the outer shell resolves to PowerShell or cmd.exe:
    // PowerShell's bare `cd` is `Set-Location` and prints nothing, so we
    // delegate the "print cwd" job to a cmd.exe subprocess in both cases.
    let output = baml_test!(
        r#"
            function main() -> string {
                baml.sys.shell("cmd /c cd", baml.sys.ProcessOptions { cwd: "C:\\Windows\\Temp" }).stdout.to_string()
            }
        "#
    );
    assert!(output.result.is_ok());
    if let Ok(BexExternalValue::String(stdout)) = &output.result {
        assert!(stdout.trim().to_lowercase().contains("temp"));
    }
}

// === stdout / stderr as uint8array field tests ===

#[tokio::test]
async fn shell_stdout_bytes() {
    let output = baml_test!(
        r#"
            function main() -> uint8array {
                baml.sys.shell("echo hello", null).stdout
            }
        "#
    );
    assert!(output.result.is_ok());
    if let Ok(BexExternalValue::Uint8Array(bytes)) = &output.result {
        // "hello" should appear in the output bytes
        let stdout = String::from_utf8_lossy(bytes);
        assert!(stdout.contains("hello"));
    } else {
        panic!("expected Uint8Array, got {:?}", output.result);
    }
}

#[tokio::test]
#[cfg(not(target_os = "windows"))]
async fn shell_stderr_bytes() {
    let output = baml_test!(
        r#"
            function main() -> uint8array {
                baml.sys.shell("echo err >&2", null).stderr
            }
        "#
    );
    assert!(output.result.is_ok());
    if let Ok(BexExternalValue::Uint8Array(bytes)) = &output.result {
        assert!(!bytes.is_empty());
        // "err" prefix: [101, 114, 114]
        assert!(bytes.starts_with(&[101, 114, 114]));
    } else {
        panic!("expected Uint8Array, got {:?}", output.result);
    }
}
