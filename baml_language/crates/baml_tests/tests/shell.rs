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
                baml.sys.shell("echo 'hello world' | tr 'a-z' 'A-Z'", capture_output = true).stdout.to_string()
            }
        "#
    );

    insta::assert_snapshot!(output.bytecode, @r#"
    function main() -> string {
        load_const "echo 'hello world' | tr 'a-z' 'A-Z'"
        load_const <omitted>
        load_const <omitted>
        load_const <omitted>
        load_const <omitted>
        load_const <omitted>
        load_const <omitted>
        load_const <omitted>
        load_const <omitted>
        load_const true
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
                baml.sys.shell("echo 'error output' >&2", capture_output = true).stderr.to_string()
            }
        "#
    );

    assert!(output.result.is_ok());
    if let Ok(BexExternalValue::String(stderr)) = &output.result {
        assert!(stderr.contains("error output"));
    }
}

// === run(capture_output = true) tests ===

#[tokio::test]
#[cfg(not(target_os = "windows"))]
async fn run_capture_failing() {
    // `false` exits with code 1 — should NOT throw
    let output = baml_test!(
        r#"
            function main() -> int {
                baml.sys.run("false", args = [], capture_output = true).exit_code
            }
        "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(1)));
}

#[tokio::test]
#[cfg(target_os = "windows")]
async fn run_capture_failing() {
    // cmd /c "exit 1" exits with code 1 — should NOT throw
    let output = baml_test!(
        r#"
            function main() -> int {
                baml.sys.run("cmd", args = ["/c", "exit 1"], capture_output = true).exit_code
            }
        "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(1)));
}

#[tokio::test]
#[cfg(not(target_os = "windows"))]
async fn run_capture_with_args() {
    let output = baml_test!(
        r#"
            function main() -> string {
                baml.sys.run("printf", args = ["%s %s", "hello", "world"], capture_output = true).stdout.to_string()
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
async fn run_capture_with_args() {
    let output = baml_test!(
        r#"
            function main() -> string {
                baml.sys.run("cmd", args = ["/c", "echo hello world"], capture_output = true).stdout.to_string()
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
async fn run_capture_stderr() {
    let output = baml_test!(
        r#"
            function main() -> string {
                baml.sys.run("sh", args = ["-c", "echo err >&2"], capture_output = true).stderr.to_string()
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
async fn run_capture_stderr() {
    let output = baml_test!(
        r#"
            function main() -> string {
                baml.sys.run("cmd", args = ["/c", "echo err 1>&2"], capture_output = true).stderr.to_string()
            }
        "#
    );
    assert!(output.result.is_ok());
    if let Ok(BexExternalValue::String(stderr)) = &output.result {
        assert!(stderr.contains("err"));
    }
}

// === named process options tests ===

#[tokio::test]
#[cfg(not(target_os = "windows"))]
async fn run_capture_with_cwd() {
    let output = baml_test!(
        r#"
            function main() -> string {
                baml.sys.run("pwd", args = [], cwd = "/tmp", capture_output = true).stdout.to_string()
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
async fn run_capture_with_cwd() {
    let output = baml_test!(
        r#"
            function main() -> string {
                baml.sys.run("cmd", args = ["/c", "cd"], cwd = "C:\\Windows\\Temp", capture_output = true).stdout.to_string()
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
async fn run_capture_with_stdin() {
    let output = baml_test!(
        r#"
            function main() -> string {
                baml.sys.run("cat", args = [], input = "hello from stdin", capture_output = true).stdout.to_string()
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
async fn run_capture_with_stdin() {
    let output = baml_test!(
        r#"
            function main() -> string {
                baml.sys.run("findstr", args = [".*"], input = "hello from stdin", capture_output = true).stdout.to_string()
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
async fn run_capture_with_timeout() {
    let output = baml_test!(
        r#"
            function main() -> string {
                baml.sys.run("sleep", args = ["10"], timeout = baml.time.Duration.from_milliseconds(100), capture_output = true).stdout.to_string()
            }
        "#
    );
    // Should timeout and throw (not return ProcessOutput)
    assert!(output.result.is_err());
}

#[tokio::test]
#[cfg(target_os = "windows")]
async fn run_capture_with_timeout() {
    let output = baml_test!(
        r#"
            function main() -> string {
                baml.sys.run("ping", args = ["-n", "11", "127.0.0.1"], timeout = baml.time.Duration.from_milliseconds(100), capture_output = true).stdout.to_string()
            }
        "#
    );
    // Should timeout and throw (not return ProcessOutput)
    assert!(output.result.is_err());
}

// === subprocess() streaming tests ===

#[tokio::test]
#[cfg(not(target_os = "windows"))]
async fn subprocess_yields_stdout_before_exit() {
    let output = baml_test!(
        r#"
            function main() -> bool throws baml.errors.InvalidArgument | baml.errors.Io | baml.errors.Timeout {
                let process = baml.sys.subprocess("sh", args = ["-c", "printf 'first\n'; while :; do :; done"], stdin = "pipe", stdout = "pipe");
                defer { process.kill(); process.close(); }

                let first = match ((process.stdout ?? baml.sys.panic("stdout is not piped")).lines().next()) {
                    let line: string => line,
                    baml.iter.Done => "",
                };
                process.kill();
                let exit = process.wait(timeout = baml.time.Duration.from_seconds(2));
                first == "first" && !exit.ok()
            }
        "#
    );

    assert_eq!(output.result, Ok(BexExternalValue::Bool(true)));
}

#[tokio::test]
#[cfg(not(target_os = "windows"))]
async fn subprocess_iterates_lines_and_final_unterminated_line() {
    let output = baml_test!(
        r#"
            function main() -> string throws baml.errors.InvalidArgument | baml.errors.Io | baml.errors.Timeout {
                let process = baml.sys.subprocess("sh", args = ["-c", "printf 'one\ntwo'"], stdin = "pipe", stdout = "pipe");
                defer { process.kill(); process.close(); }

                let lines = (process.stdout ?? baml.sys.panic("stdout is not piped")).lines().collect();
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
async fn subprocess_reads_complete_stdout_as_text() {
    let output = baml_test!(
        r#"
            function main() -> string throws baml.errors.InvalidArgument | baml.errors.Io | baml.errors.ParseError | baml.errors.Timeout {
                let process = baml.sys.subprocess("sh", args = ["-c", "printf 'hello'"], stdin = "pipe", stdout = "pipe");
                defer { process.kill(); process.close(); }

                let stdout = (process.stdout ?? baml.sys.panic("stdout is not piped")).text();
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
async fn subprocess_supports_incremental_stdin() {
    let output = baml_test!(
        r#"
            function main() -> string throws baml.errors.InvalidArgument | baml.errors.Io | baml.errors.Timeout {
                let process = baml.sys.subprocess("cat", args = [], stdin = "pipe", stdout = "pipe");
                defer { process.kill(); process.close(); }

                let out = (process.stdout ?? baml.sys.panic("stdout is not piped")).lines();
                (process.stdin ?? baml.sys.panic("stdin is not piped")).write("one\n");
                let one = match (out.next()) {
                    let line: string => line,
                    baml.iter.Done => "",
                };
                (process.stdin ?? baml.sys.panic("stdin is not piped")).write("two\n");
                let two = match (out.next()) {
                    let line: string => line,
                    baml.iter.Done => "",
                };
                (process.stdin ?? baml.sys.panic("stdin is not piped")).close();
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
async fn subprocess_stdout_read_is_cancellable() {
    let output = baml_test!(
        r#"
            function main() -> string {
                let process = baml.sys.subprocess("sh", args = ["-c", "while :; do :; done"], stdin = "pipe", stdout = "pipe");
                defer { process.kill(); process.close(); }

                let tok = baml.spawn.CancelToken.new();
                let read = spawn with tok {
                    (process.stdout ?? baml.sys.panic("stdout is not piped")).lines().next()
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
async fn subprocess_stdout_close_cancels_pending_read() {
    let output = baml_test!(
        r#"
            function main() -> string {
                let process = baml.sys.subprocess("sh", args = ["-c", "while :; do :; done"], stdin = "pipe", stdout = "pipe");
                defer { process.kill(); process.close(); }

                let read = spawn { (process.stdout ?? baml.sys.panic("stdout is not piped")).read(1024) };
                baml.sys.sleep(baml.time.Duration.from_milliseconds(25n));
                (process.stdout ?? baml.sys.panic("stdout is not piped")).close();
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
        "#!/bin/sh\nprintf '%s\\n' '{\"type\":\"result\"}'\nexec 1>&-\nexec sleep 60\n",
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
                    timeout_ms = 5000,
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
    assert!(result.contains("timed out after 5000ms"), "{result}");
    assert!(result.ends_with(":5000"), "{result}");
}

#[tokio::test]
#[cfg(not(target_os = "windows"))]
async fn shell_with_options() {
    let output = baml_test!(
        r#"
            function main() -> string {
                baml.sys.shell("pwd", cwd = "/tmp", capture_output = true).stdout.to_string()
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
                baml.sys.shell("cmd /c cd", cwd = "C:\\Windows\\Temp", capture_output = true).stdout.to_string()
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
                baml.sys.shell("echo err >&2", capture_output = true).stderr
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
async fn subprocess_stderr_pipe_is_readable() {
    let output = baml_test!(
        r#"
            function main() -> string {
                let process = baml.sys.subprocess("sh", args = ["-c", "printf 'boom\n' >&2"], stdin = "pipe", stdout = "pipe", stderr = "pipe");
                defer { process.kill(); process.close(); }

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
async fn subprocess_stderr_modes_without_pipe() {
    let output = baml_test!(
        r#"
            function main() -> bool {
                let inherited = baml.sys.subprocess("sh", args = ["-c", "printf 'x' >&2"], stdin = "pipe", stdout = "pipe");
                defer { inherited.kill(); inherited.close(); }
                let discarded = baml.sys.subprocess("sh", args = ["-c", "printf 'x' >&2"], stdin = "pipe", stdout = "pipe", stderr = "ignore");
                defer { discarded.kill(); discarded.close(); }

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
            let inherited = baml.sys.run("env", args = [], env = map { "BAML_PROCESS_TEST": "1" }, capture_output = true).stdout.to_string();
            let cleared = baml.sys.run("env", args = [], clear_env = true, env = map { "BAML_PROCESS_TEST": "1" }, capture_output = true).stdout.to_string();
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
            { baml.sys.run("/definitely-missing-baml-executable"); }
            catch (e) { baml.errors.Io => { failed = true; } }
            status.exit_code == 17 && failed
        }
    "#;
    #[cfg(windows)]
    let program = r#"
        function main() -> bool {
            let status = baml.sys.run("cmd", args = ["/c", "exit 17"]);
            let failed = false;
            { baml.sys.run("Z:/definitely-missing-baml-executable.exe"); }
            catch (e) { baml.errors.Io => { failed = true; } }
            status.exit_code == 17 && failed
        }
    "#;
    let output = baml_test!(program);
    assert_eq!(output.result, Ok(BexExternalValue::Bool(true)));
}

#[tokio::test]
#[cfg(unix)]
async fn run_capture_drains_output_while_sending_input() {
    let output = baml_test!(
        r#"
        function main() -> bool {
            let output = baml.sys.run("sh", args = ["-c", "head -c 200000 /dev/zero; wc -c"], input = "x".repeat(200000), timeout = baml.time.Duration.from_seconds(2), capture_output = true);
            output.ok() && output.stdout.slice(200000, output.stdout.length()).to_string().trim() == "200000"
        }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Bool(true)));
}

#[tokio::test]
async fn run_returns_status_when_child_closes_stdin_early() {
    let (program, args) = if cfg!(windows) {
        ("cmd.exe", vec!["/D", "/C", "echo done & exit /b 7"])
    } else {
        ("sh", vec!["-c", "printf done; exit 7"])
    };
    let args = serde_json::to_string(&args).unwrap();
    let output = baml_test!(&format!(
        r#"
        function main() -> bool {{
            let result = baml.sys.run("{program}", args = {args}, input = "x".repeat(1048576),
                timeout = baml.time.Duration.from_seconds(5), capture_output = true);
            result.exit_code == 7 && result.stdout.to_string().trim() == "done"
        }}
        "#
    ));
    assert_eq!(output.result, Ok(BexExternalValue::Bool(true)));
}

#[tokio::test]
#[cfg(unix)]
async fn run_capture_deadline_includes_blocked_stdin() {
    let started = std::time::Instant::now();
    let output = baml_test!(
        r#"
        function main() -> bool {
            { baml.sys.run("sleep", args = ["5"], input = "x".repeat(200000), timeout = baml.time.Duration.from_milliseconds(50), capture_output = true); false } catch (e) { baml.errors.Timeout => true }
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
            let captured = baml.sys.run("cat", input = bytes, capture_output = true);
            let process = baml.sys.subprocess("sh", args = ["-c", "head -c 200000 /dev/zero; wc -c"], stdin = "pipe", stdout = "pipe", input = "x".repeat(200000));
            defer { process.kill(); process.close(); }
            let stdout = (process.stdout ?? baml.sys.panic("stdout is not piped")).bytes();
            captured.stdout == bytes && process.wait(timeout = baml.time.Duration.from_seconds(2)).ok() && stdout.slice(200000, stdout.length()).to_string().trim() == "200000"
        }
        "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Bool(true)));
}

/// Both explicit cleanup and GC release the handle promptly while a finite
/// OS child continues running and performs its observable side effect.
#[tokio::test]
#[cfg(unix)]
async fn subprocess_cleanup_and_gc_preserve_running_child() {
    for explicit in [true, false] {
        let temp = tempfile::tempdir().unwrap();
        let marker = temp.path().join("completed");
        let output = baml_test! {
            baml: r#"
                function launch(marker: string, explicit: bool) -> bool {
                    let child = baml.sys.subprocess("sh",
                        args = ["-c", "sleep 2; printf completed > \"$1\"", "sh", marker],
                        stdin = "ignore", stdout = "ignore", stderr = "ignore");
                    let started = baml.time.Instant.now();
                    if (explicit) {
                        child.cleanup();
                        child.cleanup();
                    }
                    started.elapsed().to_milliseconds() < 1000n
                }
                function main(marker: string, explicit: bool) -> bool {
                    let released = launch(marker, explicit);
                    baml.sys.collect_garbage();
                    released
                }
            "#,
            args: {
                "marker" => BexExternalValue::String(marker.to_string_lossy().into_owned().into()),
                "explicit" => BexExternalValue::Bool(explicit),
            },
        };
        assert_eq!(output.result, Ok(BexExternalValue::Bool(true)));
        tokio::time::timeout(std::time::Duration::from_secs(5), async {
            while !marker.exists() {
                tokio::time::sleep(std::time::Duration::from_millis(20)).await;
            }
        })
        .await
        .expect("losing the handle must not terminate the child");
        assert_eq!(std::fs::read_to_string(marker).unwrap(), "completed");
    }
}

#[tokio::test]
#[cfg(unix)]
async fn subprocess_scoped_termination_and_repeated_kill() {
    let output = baml_test!(
        r#"
        function scoped() -> baml.sys.Subprocess {
            let child = baml.sys.subprocess("sleep", args = ["5"]);
            defer { child.kill(); }
            child
        }
        function main() -> bool {
            let child = scoped();
            let status = child.wait();
            child.kill();
            child.kill();
            status.signal == 9 && status.exit_code == 137 && child.wait().signal == 9
        }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Bool(true)));
}

#[tokio::test]
async fn subprocess_wait_timeout_does_not_terminate_child() {
    let (program, args) = if cfg!(windows) {
        ("cmd.exe", vec!["/D", "/C", "set /p ready= & exit /b 7"])
    } else {
        ("sh", vec!["-c", "read line; exit 7"])
    };
    let args = serde_json::to_string(&args).unwrap();
    let output = baml_test!(&format!(
        r#"
        function main() -> bool {{
            let child = baml.sys.subprocess("{program}", args = {args}, stdin = "pipe");
            defer {{ child.kill(); }}
            let timed_out = false;
            {{ child.wait(timeout = baml.time.Duration.from_milliseconds(50)); }} catch (e) {{ baml.errors.Timeout => {{ timed_out = true; }} }}
            let rejected = {{ child.wait(timeout = baml.time.Duration.from_milliseconds(-1)); false }} catch (e) {{ baml.errors.InvalidArgument => true }};
            let input = child.stdin ?? baml.sys.panic("stdin was not piped");
            input.write("finish\n");
            input.close();
            let status = child.wait(timeout = baml.time.Duration.from_seconds(5));
            timed_out && rejected && status.exit_code == 7 && child.wait(timeout = baml.time.Duration.from_milliseconds(0)).exit_code == 7
        }}
    "#
    ));
    assert_eq!(output.result, Ok(BexExternalValue::Bool(true)));
}

#[tokio::test]
#[cfg(unix)]
#[expect(
    unsafe_code,
    reason = "test queries the session and terminates a known child PID"
)]
async fn subprocess_detached_creates_os_session() {
    let output = baml_test!(
        r#"
        function main() -> int {
            let child = baml.sys.subprocess("sleep", args = ["2"], detached = true,
                stdin = "ignore", stdout = "ignore", stderr = "ignore");
            child.pid
        }
    "#
    );
    let Ok(BexExternalValue::Int(pid)) = output.result else {
        panic!("{:?}", output.result);
    };
    let pid = i32::try_from(pid).unwrap();
    // SAFETY: these syscalls use a known child PID and no memory pointers.
    let session = unsafe { libc::getsid(pid) };
    unsafe {
        libc::kill(pid, libc::SIGKILL);
    }
    assert_eq!(session, pid, "detached means a new OS session");
}

#[tokio::test]
#[cfg(unix)]
async fn subprocess_cleanup_preserves_owned_pipe_operations() {
    let output = baml_test!(
        r#"
        function main() -> string {
            let child = baml.sys.subprocess("sh", args = ["-c", "sleep 0.2; printf completed"], stdout = "pipe");
            let pipe = child.stdout ?? baml.sys.panic("stdout missing");
            let read = spawn { pipe.bytes().to_string() };
            child.cleanup();
            await read
        }
    "#
    );
    assert_eq!(
        output.result,
        Ok(BexExternalValue::String("completed".into()))
    );
}

#[tokio::test]
#[cfg(unix)]
async fn subprocess_gc_preserves_independently_owned_stdout() {
    let output = baml_test!(
        r#"
        function launch() -> baml.sys.ReadPipe {
            let child = baml.sys.subprocess("sh", args = ["-c", "sleep 0.2; printf completed"], stdout = "pipe");
            child.stdout ?? baml.sys.panic("stdout missing")
        }
        function main() -> string {
            let pipe = launch();
            baml.sys.collect_garbage();
            pipe.bytes().to_string()
        }
    "#
    );
    assert_eq!(
        output.result,
        Ok(BexExternalValue::String("completed".into()))
    );
}

#[tokio::test]
#[cfg(unix)]
async fn subprocess_kill_with_pending_input_reports_exit_status() {
    let output = baml_test!(
        r#"
        function main() -> bool {
            let child = baml.sys.subprocess("sleep", args = ["5"], input = "x".repeat(200000));
            child.kill();
            child.kill();
            child.wait().signal == 9
        }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Bool(true)));
}

#[tokio::test]
#[cfg(unix)]
async fn subprocess_wait_reports_child_exit_before_inherited_stdin_closes() {
    let output = baml_test!(
        r#"
        function main() -> bool {
            let child = baml.sys.subprocess("sh", args = ["-c", "sleep 2 <&0 & exit 7"],
                input = "x".repeat(200000), stdout = "ignore", stderr = "ignore");
            let started = baml.time.Instant.now();
            let status = child.wait();
            child.cleanup();
            status.exit_code == 7 && started.elapsed().to_milliseconds() < 1000n
        }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Bool(true)));
}
