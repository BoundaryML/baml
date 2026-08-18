//! Unified tests for shell operations.
//!
//! Platform-specific and timing-sensitive tests:
//!   shell_with_pipe        — Unix-only (`tr`), has insta bytecode snapshot.
//!   shell_stderr           — Unix-only `>&2` redirect.
//!   exec_failing           — platform-split binary (`false` vs `cmd`).
//!   exec_with_args         — platform-split (`printf` vs `cmd`).
//!   exec_stderr            — platform-split redirection syntax.
//!   exec_with_cwd          — platform-split (`pwd` vs `cmd /c cd`).
//!   exec_with_stdin        — platform-split (`cat` vs `findstr`).
//!   exec_with_timeout      — timing-dependent; assert is_err().
//!   shell_with_options     — platform-split (`pwd` vs `cmd /c cd`).
//!   shell_stderr_bytes     — Unix-only `>&2` redirect, byte-prefix assertion.

use baml_tests::baml_test;
use bex_engine::BexExternalValue;

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
        call baml.Uint8Array.baml.ToString.to_string
        return
    }
    "#);
    assert_eq!(
        output.result,
        Ok(BexExternalValue::String("HELLO WORLD\n".to_string().into()))
    );
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

// === exec() tests ===

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
        Ok(BexExternalValue::String("hello world".to_string().into()))
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
        Ok(BexExternalValue::String(
            "hello from stdin".to_string().into()
        ))
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

// === start_process() streaming tests ===

#[tokio::test]
#[cfg(not(target_os = "windows"))]
async fn start_process_yields_stdout_before_exit() {
    let output = baml_test!(
        r#"
            function main() -> bool throws baml.errors.Io | baml.errors.Timeout {
                let process = baml.sys.start_process(
                    "sh",
                    ["-c", "printf 'first\n'; while :; do :; done"],
                    baml.sys.ProcessOptions { timeout_ms: 2000 },
                );
                defer { process.close() }

                let first = match (process.stdout.lines().next()) {
                    let line: string => line,
                    baml.iter.Done => "",
                };
                process.kill();
                let exit = process.wait();
                first == "first" && !exit.ok()
            }
        "#
    );

    assert_eq!(output.result, Ok(BexExternalValue::Bool(true)));
}

#[tokio::test]
#[cfg(not(target_os = "windows"))]
async fn start_process_iterates_lines_and_final_unterminated_line() {
    let output = baml_test!(
        r#"
            function main() -> string throws baml.errors.Io | baml.errors.Timeout {
                let process = baml.sys.start_process(
                    "sh",
                    ["-c", "printf 'one\ntwo'"],
                    null,
                );
                defer { process.close() }

                let lines = process.stdout.lines().collect();
                let exit = process.wait();
                if (!exit.ok()) {
                    return "bad exit";
                }
                lines.join("|")
            }
        "#
    );

    assert_eq!(
        output.result,
        Ok(BexExternalValue::String("one|two".to_string().into()))
    );
}

#[tokio::test]
#[cfg(not(target_os = "windows"))]
async fn start_process_supports_incremental_stdin() {
    let output = baml_test!(
        r#"
            function main() -> string throws baml.errors.Io | baml.errors.Timeout {
                let process = baml.sys.start_process("cat", [], null);
                defer { process.close() }

                let out = process.stdout.lines();
                process.stdin.write_all("one\n".to_utf8());
                let one = match (out.next()) {
                    let line: string => line,
                    baml.iter.Done => "",
                };
                process.stdin.write_all("two\n".to_utf8());
                let two = match (out.next()) {
                    let line: string => line,
                    baml.iter.Done => "",
                };
                process.stdin.close();
                let exit = process.wait();
                if (!exit.ok()) {
                    return "bad exit";
                }
                one + "|" + two
            }
        "#
    );

    assert_eq!(
        output.result,
        Ok(BexExternalValue::String("one|two".to_string().into()))
    );
}

// `timeout_ms` bounds `wait()`, not the pipes: reading stdout goes through
// `baml.io.Read`, whose only error class is `Io`, so a blocked read is bounded
// by the cancellation system rather than a deadline baked into the read.
#[tokio::test]
#[cfg(not(target_os = "windows"))]
async fn start_process_stdout_read_is_cancellable() {
    let output = baml_test!(
        r#"
            function main() -> string {
                let process = baml.sys.start_process(
                    "sh",
                    ["-c", "while :; do :; done"],
                    null,
                );
                defer { process.close() }

                let tok = baml.spawn.CancelToken.new();
                let read = spawn with baml.spawn.options(cancel = tok) {
                    process.stdout.lines().next()
                };
                let deadline = spawn {
                    baml.sys.sleep(baml.time.Duration.from_milliseconds(25n));
                    tok.cancel()
                };
                // The cancel arm and a real line are both strings, so the
                // assertion below only holds if the read was actually
                // cancelled: a line would return itself, and a completed
                // stream returns "eof".
                let outcome = (await read) catch (e) {
                    baml.panics.Cancelled => "cancelled"
                };
                match (outcome) {
                    let line: string => line,
                    baml.iter.Done => "eof",
                }
            }
        "#
    );

    assert_eq!(
        output.result,
        Ok(BexExternalValue::String("cancelled".into())),
        "a line read from a silent child should be cancelled by the deadline task"
    );
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

// === pid() tests ===

/// `baml.sys.pid` reports the ID of the process running the VM, not of any
/// child it spawns. The test harness runs the engine in-process, so the only
/// correct answer is this test binary's own PID.
#[tokio::test]
async fn pid_is_the_host_process() {
    let output = baml_test!(
        r#"
            function main() -> int {
                baml.sys.pid()
            }
        "#
    );

    assert_eq!(
        output.result,
        Ok(BexExternalValue::Int(i64::from(std::process::id())))
    );
}

// === stdout / stderr as uint8array field tests ===

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

// === StderrMode ===

#[tokio::test]
#[cfg(not(target_os = "windows"))]
async fn start_process_stderr_pipe_is_readable() {
    let output = baml_test!(
        r#"
            function main() -> string throws baml.errors.Io | baml.errors.Timeout {
                let process = baml.sys.start_process(
                    "sh",
                    ["-c", "printf 'boom\n' >&2"],
                    baml.sys.ProcessOptions { stderr: baml.sys.StderrMode.Pipe },
                );
                defer { process.close() }

                match (process.stderr) {
                    null => "no pipe",
                    let err: baml.sys.ReadPipe => {
                        match (err.lines().next()) {
                            let line: string => line,
                            baml.iter.Done => "eof",
                        }
                    },
                }
            }
        "#
    );

    assert_eq!(
        output.result,
        Ok(BexExternalValue::String("boom".to_string().into()))
    );
}

// Inherit is the default, so a caller that never asks for the pipe cannot
// wedge a child on an unread stderr — `Process.stderr` is simply absent.
#[tokio::test]
#[cfg(not(target_os = "windows"))]
async fn start_process_stderr_defaults_to_inherit() {
    let output = baml_test!(
        r#"
            function main() -> bool throws baml.errors.Io | baml.errors.Timeout {
                let process = baml.sys.start_process("sh", ["-c", "printf 'x' >&2"], null);
                defer { process.close() }

                let exit = process.wait();
                process.stderr == null && exit.ok()
            }
        "#
    );

    assert_eq!(output.result, Ok(BexExternalValue::Bool(true)));
}

#[tokio::test]
#[cfg(not(target_os = "windows"))]
async fn start_process_stderr_discard_leaves_no_pipe() {
    let output = baml_test!(
        r#"
            function main() -> bool throws baml.errors.Io | baml.errors.Timeout {
                let process = baml.sys.start_process(
                    "sh",
                    ["-c", "printf 'x' >&2"],
                    baml.sys.ProcessOptions { stderr: baml.sys.StderrMode.Discard },
                );
                defer { process.close() }

                let exit = process.wait();
                process.stderr == null && exit.ok()
            }
        "#
    );

    assert_eq!(output.result, Ok(BexExternalValue::Bool(true)));
}

// A child that floods stderr past the OS pipe buffer must still finish when
// the caller drains the pipe concurrently with stdout — the deadlock this
// mode is designed to make possible.
#[tokio::test]
#[cfg(not(target_os = "windows"))]
async fn start_process_stderr_pipe_survives_a_flood_when_drained() {
    let output = baml_test!(
        r#"
            function main() -> bool throws baml.errors.Io | baml.errors.Timeout {
                let process = baml.sys.start_process(
                    "sh",
                    ["-c", "i=0; while [ $i -lt 4000 ]; do printf 'noise-noise-noise-noise\n' >&2; i=$((i+1)); done; printf 'done\n'"],
                    baml.sys.ProcessOptions { stderr: baml.sys.StderrMode.Pipe },
                );
                defer { process.close() }

                let drain = spawn {
                    match (process.stderr) {
                        null => 0,
                        let err: baml.sys.ReadPipe => err.lines().collect().length(),
                    }
                };
                let out = match (process.stdout.lines().next()) {
                    let line: string => line,
                    baml.iter.Done => "eof",
                };
                let noise = await drain;
                let exit = process.wait();
                out == "done" && noise == 4000 && exit.ok()
            }
        "#
    );

    assert_eq!(output.result, Ok(BexExternalValue::Bool(true)));
}
