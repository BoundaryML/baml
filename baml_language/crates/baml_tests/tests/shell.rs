//! Unified tests for shell operations.
//!
//! Platform-specific and timing-sensitive tests:
//!   shell_with_pipe        — Unix-only (`tr`), has insta bytecode snapshot.
//!   shell_stderr           — Unix-only `>&2` redirect.
//!   capture_failing           — platform-split binary (`false` vs `cmd`).
//!   capture_with_args         — platform-split (`printf` vs `cmd`).
//!   capture_stderr            — platform-split redirection syntax.
//!   capture_with_cwd          — platform-split (`pwd` vs `cmd /c cd`).
//!   capture_with_stdin        — platform-split (`cat` vs `findstr`).
//!   capture_with_timeout      — timing-dependent; assert is_err().
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
                baml.sys.shell("echo 'hello world' | tr 'a-z' 'A-Z'", options = null).stdout.to_string()
            }
        "#
    );

    insta::assert_snapshot!(output.bytecode, @r#"
    function main() -> string {
        load_const "echo 'hello world' | tr 'a-z' 'A-Z'"
        load_const null
        call baml.sys.shell
        load_field .stdout
        load_type baml.ToString
        load_const "to_string"
        virtual_call nargs=1 ntypeargs=0 self_arg=0
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
                baml.sys.shell("echo 'error output' >&2", options = null).stderr.to_string()
            }
        "#
    );

    assert!(output.result.is_ok());
    if let Ok(BexExternalValue::String(stderr)) = &output.result {
        assert!(stderr.contains("error output"));
    }
}

// === capture() tests ===

#[tokio::test]
#[cfg(not(target_os = "windows"))]
async fn capture_failing() {
    // `false` exits with code 1 — should NOT throw
    let output = baml_test!(
        r#"
            function main() -> int {
                baml.sys.capture("false", args = [], options = null).exit_code
            }
        "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(1)));
}

#[tokio::test]
#[cfg(target_os = "windows")]
async fn capture_failing() {
    // cmd /c "exit 1" exits with code 1 — should NOT throw
    let output = baml_test!(
        r#"
            function main() -> int {
                baml.sys.capture("cmd", args = ["/c", "exit 1"], options = null).exit_code
            }
        "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(1)));
}

#[tokio::test]
#[cfg(not(target_os = "windows"))]
async fn capture_with_args() {
    let output = baml_test!(
        r#"
            function main() -> string {
                baml.sys.capture("printf", args = ["%s %s", "hello", "world"], options = null).stdout.to_string()
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
async fn capture_with_args() {
    let output = baml_test!(
        r#"
            function main() -> string {
                baml.sys.capture("cmd", args = ["/c", "echo hello world"], options = null).stdout.to_string()
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
async fn capture_stderr() {
    let output = baml_test!(
        r#"
            function main() -> string {
                baml.sys.capture("sh", args = ["-c", "echo err >&2"], options = null).stderr.to_string()
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
async fn capture_stderr() {
    let output = baml_test!(
        r#"
            function main() -> string {
                baml.sys.capture("cmd", args = ["/c", "echo err 1>&2"], options = null).stderr.to_string()
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
async fn capture_with_cwd() {
    let output = baml_test!(
        r#"
            function main() -> string {
                baml.sys.capture("pwd", args = [], options = baml.sys.ProcessOptions { cwd: "/tmp" }).stdout.to_string()
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
async fn capture_with_cwd() {
    let output = baml_test!(
        r#"
            function main() -> string {
                baml.sys.capture("cmd", args = ["/c", "cd"], options = baml.sys.ProcessOptions { cwd: "C:\\Windows\\Temp" }).stdout.to_string()
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
async fn capture_with_stdin() {
    let output = baml_test!(
        r#"
            function main() -> string {
                baml.sys.capture("cat", args = [], options = baml.sys.ProcessOptions { stdin: "hello from stdin" }).stdout.to_string()
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
async fn capture_with_stdin() {
    let output = baml_test!(
        r#"
            function main() -> string {
                baml.sys.capture("findstr", args = [".*"], options = baml.sys.ProcessOptions { stdin: "hello from stdin" }).stdout.to_string()
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
async fn capture_with_timeout() {
    let output = baml_test!(
        r#"
            function main() -> string {
                baml.sys.capture("sleep", args = ["10"], options = baml.sys.ProcessOptions { timeout: baml.time.Duration.from_milliseconds(100) }).stdout.to_string()
            }
        "#
    );
    // Should timeout and throw (not return ProcessOutput)
    assert!(output.result.is_err());
}

#[tokio::test]
#[cfg(target_os = "windows")]
async fn capture_with_timeout() {
    let output = baml_test!(
        r#"
            function main() -> string {
                baml.sys.capture("ping", args = ["-n", "11", "127.0.0.1"], options = baml.sys.ProcessOptions { timeout: baml.time.Duration.from_milliseconds(100) }).stdout.to_string()
            }
        "#
    );
    // Should timeout and throw (not return ProcessOutput)
    assert!(output.result.is_err());
}

// === start_process() streaming tests ===

#[tokio::test]
#[cfg(not(target_os = "windows"))]
async fn start_process_yields_stdout_before_exit() {
    let output = baml_test!(
        r#"
            function main() -> bool throws baml.errors.InvalidArgument | baml.errors.Io | baml.errors.Timeout {
                let process = baml.sys.start_process(
                    "sh",
                    args = ["-c", "printf 'first\n'; while :; do :; done"],
                    options = baml.sys.ProcessOptions { timeout: baml.time.Duration.from_milliseconds(2000) },
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
            function main() -> string throws baml.errors.InvalidArgument | baml.errors.Io | baml.errors.Timeout {
                let process = baml.sys.start_process(
                    "sh",
                    args = ["-c", "printf 'one\ntwo'"],
                    options = null,
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
async fn start_process_reads_complete_stdout_as_text() {
    let output = baml_test!(
        r#"
            function main() -> string throws baml.errors.InvalidArgument | baml.errors.Io | baml.errors.ParseError | baml.errors.Timeout {
                let process = baml.sys.start_process(
                    "sh",
                    args = ["-c", "printf 'hello'"],
                    options = null,
                );
                defer { process.close() }

                let stdout = process.stdout.text();
                let exit = process.wait();
                if (!exit.ok()) {
                    return "bad exit";
                }
                stdout
            }
        "#
    );

    assert_eq!(
        output.result,
        Ok(BexExternalValue::String("hello".to_string().into()))
    );
}

#[tokio::test]
#[cfg(not(target_os = "windows"))]
async fn start_process_supports_incremental_stdin() {
    let output = baml_test!(
        r#"
            function main() -> string throws baml.errors.InvalidArgument | baml.errors.Io | baml.errors.Timeout {
                let process = baml.sys.start_process("cat", args = [], options = null);
                defer { process.close() }

                let out = process.stdout.lines();
                process.stdin.write("one\n");
                let one = match (out.next()) {
                    let line: string => line,
                    baml.iter.Done => "",
                };
                process.stdin.write("two\n");
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

#[tokio::test]
#[cfg(not(target_os = "windows"))]
async fn start_process_stdout_read_is_cancellable() {
    let output = baml_test!(
        r#"
            function main() -> string {
                let process = baml.sys.start_process(
                    "sh",
                    args = ["-c", "while :; do :; done"],
                    options = null,
                );
                defer { process.close() }

                let tok = baml.spawn.CancelToken.new();
                let read = spawn with tok {
                    process.stdout.lines().next()
                };
                let deadline = spawn {
                    baml.sys.sleep(baml.time.Duration.from_milliseconds(25n));
                    tok.cancel()
                };
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
        Ok(BexExternalValue::String("cancelled".into()))
    );
}

#[tokio::test]
#[cfg(not(target_os = "windows"))]
async fn start_process_stdout_close_cancels_pending_read() {
    let output = baml_test!(
        r#"
            function main() -> string {
                let process = baml.sys.start_process(
                    "sh",
                    args = ["-c", "while :; do :; done"],
                    options = null,
                );
                defer { process.close() }

                let read = spawn { process.stdout.read(1024) };
                baml.sys.sleep(baml.time.Duration.from_milliseconds(25n));
                process.stdout.close();
                (await read) catch (e) {
                    baml.errors.Io => { return "closed"; }
                };
                "completed"
            }
        "#
    );

    assert_eq!(output.result, Ok(BexExternalValue::String("closed".into())));
}

#[tokio::test]
#[cfg(unix)]
async fn claude_code_client_preserves_process_wait_timeout() {
    use std::os::unix::fs::PermissionsExt as _;

    let temp = tempfile::tempdir().expect("tempdir for Claude Code timeout probe");
    let script = temp.path().join("claude-code-timeout-probe.sh");
    std::fs::write(
        &script,
        "#!/bin/sh\nprintf '%s\\n' '{\"type\":\"result\"}'\nexec 1>&-\nwhile :; do :; done\n",
    )
    .expect("write Claude Code timeout probe");
    let mut permissions = std::fs::metadata(&script)
        .expect("stat Claude Code timeout probe")
        .permissions();
    permissions.set_mode(0o700);
    std::fs::set_permissions(&script, permissions)
        .expect("make Claude Code timeout probe executable");

    let executable = script.to_string_lossy().into_owned();
    let output = baml_test! {
        baml: r#"
            function TimeoutProviderSpec() -> string {
                client: "openai/gpt-4o-mini"
                prompt: `Return one string. ${ctx.output_format()}`
            }

            function timeout_provider_input() -> ai.ModelTurnInput {
                let spec = TimeoutProviderSpec@spec();
                ai.ModelTurnInput {
                    prompt: spec.prompt_template,
                    journal: ai.Journal.new(spec),
                    toolbox: spec.tools(),
                    output_type: spec.output_type(),
                }
            }

            function main(executable: string) -> string {
                let cl = claude_code.ClaudeCodeClient.new(
                    model = "offline-timeout-probe",
                    executable = executable,
                    // Leave room for process startup and result parsing under test load.
                    // This probe checks the subsequent process-wait timeout.
                    timeout_ms = 1000,
                );
                let _ = cl.invoke(timeout_provider_input()) catch_all (e) {
                    let timeout: baml.errors.Timeout => {
                        return `Timeout:${timeout.message}:${timeout.duration?.to_milliseconds() ?? -1n}`;
                    },
                    _ => { return `unexpected:${e.to_string()}`; },
                };
                "accepted"
            }
        "#,
        args: {
            "executable" => BexExternalValue::String(executable.into()),
        },
    };

    let Ok(BexExternalValue::String(result)) = output.result else {
        panic!("expected a string timeout result, got {:?}", output.result);
    };
    assert!(result.starts_with("Timeout:"), "{result}");
    assert!(result.contains("timed out after 1000ms"), "{result}");
    assert!(result.ends_with(":1000"), "{result}");
}

#[tokio::test]
#[cfg(not(target_os = "windows"))]
async fn shell_with_options() {
    let output = baml_test!(
        r#"
            function main() -> string {
                baml.sys.shell("pwd", options = baml.sys.ProcessOptions { cwd: "/tmp" }).stdout.to_string()
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
                baml.sys.shell("cmd /c cd", options = baml.sys.ProcessOptions { cwd: "C:\\Windows\\Temp" }).stdout.to_string()
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
                baml.sys.shell("echo err >&2", options = null).stderr
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

#[tokio::test]
#[cfg(not(target_os = "windows"))]
async fn start_process_stderr_pipe_is_readable() {
    let output = baml_test!(
        r#"
            function main() -> string {
                let process = baml.sys.start_process(
                    "sh",
                    args = ["-c", "printf 'boom\n' >&2"],
                    options = baml.sys.ProcessOptions { stderr: baml.sys.StderrMode.Pipe },
                );
                defer { process.close() }

                match (process.stderr) {
                    null => "no pipe",
                    let err: baml.sys.ReadPipe => match (err.lines().next()) {
                        let line: string => line,
                        baml.iter.Done => "eof",
                    },
                }
            }
        "#
    );

    assert_eq!(output.result, Ok(BexExternalValue::String("boom".into())));
}

#[tokio::test]
#[cfg(not(target_os = "windows"))]
async fn start_process_stderr_modes_without_pipe() {
    let output = baml_test!(
        r#"
            function main() -> bool {
                let inherited = baml.sys.start_process("sh", args = ["-c", "printf 'x' >&2"], options = null);
                defer { inherited.close() }
                let discarded = baml.sys.start_process(
                    "sh",
                    args = ["-c", "printf 'x' >&2"],
                    options = baml.sys.ProcessOptions { stderr: baml.sys.StderrMode.Discard },
                );
                defer { discarded.close() }

                inherited.stderr == null &&
                    discarded.stderr == null &&
                    inherited.wait().ok() &&
                    discarded.wait().ok()
            }
        "#
    );

    assert_eq!(output.result, Ok(BexExternalValue::Bool(true)));
}

#[tokio::test]
#[cfg(unix)]
async fn process_environment_overlay_and_clear() {
    let output = baml_test!(
        r#"
        function main() -> bool {
            let inherited = baml.sys.capture("env", args = [], options = baml.sys.ProcessOptions { env: map { "BAML_PROCESS_TEST": "1" } }).stdout.to_string();
            let cleared = baml.sys.capture("env", args = [], options = baml.sys.ProcessOptions { clear_env: true, env: map { "BAML_PROCESS_TEST": "1" } }).stdout.to_string();
            inherited.includes("PATH=") && cleared == "BAML_PROCESS_TEST=1\n"
        }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Bool(true)));
}

#[tokio::test]
async fn inherited_process_status_and_launch_failure() {
    #[cfg(unix)]
    let program = r#"
        function main() -> bool {
            let status = baml.sys.run("sh", args = ["-c", "exit 17"]);
            let failed = false;
            { baml.sys.exec("/definitely-missing-baml-executable"); }
            catch (e) { baml.errors.Io => { failed = true; } }
            status.exit_code == 17 && failed
        }
    "#;
    #[cfg(windows)]
    let program = r#"
        function main() -> bool {
            let status = baml.sys.run("cmd", args = ["/c", "exit 17"]);
            let failed = false;
            { baml.sys.exec("Z:/definitely-missing-baml-executable.exe"); }
            catch (e) { baml.errors.Io => { failed = true; } }
            status.exit_code == 17 && failed
        }
    "#;
    let output = baml_test!(program);
    assert_eq!(output.result, Ok(BexExternalValue::Bool(true)));
}

#[tokio::test]
#[cfg(unix)]
async fn capture_drains_output_while_sending_input() {
    let output = baml_test!(
        r#"
        function main() -> bool {
            let output = baml.sys.capture("sh", args = ["-c", "head -c 200000 /dev/zero; wc -c"],
                options = baml.sys.ProcessOptions { stdin: "x".repeat(200000), timeout: baml.time.Duration.from_seconds(2) });
            output.ok() && output.stdout.slice(200000, output.stdout.length()).to_string().trim() == "200000"
        }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Bool(true)));
}

#[tokio::test]
#[cfg(unix)]
async fn capture_deadline_includes_blocked_stdin() {
    let started = std::time::Instant::now();
    let output = baml_test!(
        r#"
        function main() -> bool {
            { baml.sys.capture("sleep", args = ["5"], options = baml.sys.ProcessOptions {
                stdin: "x".repeat(200000), timeout: baml.time.Duration.from_milliseconds(50)
            }); false } catch (e) { baml.errors.Timeout => true }
        }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Bool(true)));
    assert!(started.elapsed() < std::time::Duration::from_secs(4));
}

#[tokio::test]
#[cfg(unix)]
async fn binary_input_and_streaming_backpressure() {
    let output = baml_test!(
        r#"
        function main() -> bool {
            let bytes = b"\x00\xff\x80";
            let captured = baml.sys.capture("cat", options = baml.sys.ProcessOptions { stdin: bytes });
            let process = baml.sys.start_process("sh", args = ["-c", "head -c 200000 /dev/zero; wc -c"],
                options = baml.sys.ProcessOptions { stdin: "x".repeat(200000), timeout: baml.time.Duration.from_seconds(2) });
            defer { process.close() }
            let stdout = process.stdout.bytes();
            captured.stdout == bytes && process.wait().ok() && stdout.slice(200000, stdout.length()).to_string().trim() == "200000"
        }
        "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Bool(true)));
}
