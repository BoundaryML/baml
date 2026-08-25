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
async fn start_process_reads_complete_stdout_as_text() {
    let output = baml_test!(
        r#"
            function main() -> string throws baml.errors.Io | baml.errors.ParseError | baml.errors.Timeout {
                let process = baml.sys.start_process(
                    "sh",
                    ["-c", "printf 'hello'"],
                    null,
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
            function main() -> string throws baml.errors.Io | baml.errors.Timeout {
                let process = baml.sys.start_process("cat", [], null);
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
                    ["-c", "while :; do :; done"],
                    null,
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
                    timeout_ms = 25,
                );
                let _ = cl.invoke(timeout_provider_input()) catch_all (e) {
                    let timeout: baml.errors.Timeout => {
                        return `Timeout:${timeout.message}:${timeout.duration_ms ?? -1}`;
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
    assert!(result.contains("timed out after 25ms"), "{result}");
    assert!(result.ends_with(":25"), "{result}");
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

#[tokio::test]
#[cfg(not(target_os = "windows"))]
async fn start_process_stderr_pipe_is_readable() {
    let output = baml_test!(
        r#"
            function main() -> string {
                let process = baml.sys.start_process(
                    "sh",
                    ["-c", "printf 'boom\n' >&2"],
                    baml.sys.ProcessOptions { stderr: baml.sys.StderrMode.Pipe },
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
                let inherited = baml.sys.start_process("sh", ["-c", "printf 'x' >&2"], null);
                defer { inherited.close() }
                let discarded = baml.sys.start_process(
                    "sh",
                    ["-c", "printf 'x' >&2"],
                    baml.sys.ProcessOptions { stderr: baml.sys.StderrMode.Discard },
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

// === platform() capability token / is_alive / Unix signalling tests ===

/// `baml.sys.platform()` hands out exactly one platform capability: a native
/// host is either Unix (`unix()` non-null, `windows()` null) or Windows (the
/// reverse).
#[tokio::test]
async fn platform_capability_shape() {
    let output = baml_test!(
        r#"
            function main() -> bool {
                let p = baml.sys.platform();
                let has_unix = match (p.unix()) {
                    let u: baml.sys.Unix => true,
                    null => false,
                };
                let has_windows = match (p.windows()) {
                    let w: baml.sys.Windows => true,
                    null => false,
                };
                has_unix != has_windows
            }
        "#
    );

    assert_eq!(output.result, Ok(BexExternalValue::Bool(true)));
}

/// Matching `baml.sys.platform()` against `baml.sys.Unix` yields the
/// capability whose `terminate` (SIGTERM) lets `wait()` observe signal 15,
/// and `baml.sys.is_alive` tracks the child across the call: alive before,
/// dead once `wait()` has reaped it. The child prints its own pid (`$$`,
/// kept across `exec`) so the test can signal it by pid without any
/// handle-to-pid API.
#[tokio::test]
#[cfg(not(target_os = "windows"))]
async fn is_alive_and_terminate_by_pid() {
    let output = baml_test!(
        r#"
            function main() -> bool throws baml.errors.Io | baml.errors.Timeout {
                let process = baml.sys.start_process("sh", ["-c", "echo $$; exec sleep 30"], null);
                defer { process.close() }

                let pid = baml.json.from_string<int>(process.stdout.lines().next() ?? "0") catch_all (e) {
                    _ => 0,
                };
                if (pid <= 0 || !baml.sys.is_alive(pid)) {
                    return false;
                }
                match (baml.sys.platform()) {
                    let u: baml.sys.Unix => u.terminate(pid),
                    _ => { return false; },
                }
                let exit = process.wait();
                // After wait() reaps the child, the pid reads as dead.
                if (baml.sys.is_alive(pid)) {
                    return false;
                }
                match (exit.signal) {
                    let signal: string => signal == "15",
                    null => false,
                }
            }
        "#
    );

    assert_eq!(output.result, Ok(BexExternalValue::Bool(true)));
}

/// `baml.sys.is_alive` always reports true for the current process, on every
/// platform.
#[tokio::test]
async fn is_alive_reports_self() {
    let output = baml_test!(
        r#"
            function main() -> bool throws baml.errors.Io {
                baml.sys.is_alive(baml.sys.pid())
            }
        "#
    );

    assert_eq!(output.result, Ok(BexExternalValue::Bool(true)));
}

/// Windows variant of the liveness probe: a live child reads as alive, and
/// once killed its exit code is no longer `STILL_ACTIVE`, so it reads as
/// dead even if the process object has not been fully torn down yet.
#[tokio::test]
#[cfg(target_os = "windows")]
async fn is_alive_tracks_child_lifetime() {
    let output = baml_test!(
        r#"
            function main() -> bool throws baml.errors.Io | baml.errors.Timeout {
                let process = baml.sys.start_process(
                    "powershell",
                    ["-NoProfile", "-Command", "$PID; Start-Sleep -Seconds 30"],
                    null,
                );
                defer { process.close() }

                let pid = baml.json.from_string<int>(process.stdout.lines().next() ?? "0") catch_all (e) {
                    _ => 0,
                };
                if (pid <= 0 || !baml.sys.is_alive(pid)) {
                    return false;
                }
                process.kill();
                process.wait();
                !baml.sys.is_alive(pid)
            }
        "#
    );

    assert_eq!(output.result, Ok(BexExternalValue::Bool(true)));
}

/// `Unix.signal_group` signals every process in a process group at once.
/// `setsid` runs the child as leader of a brand-new session (and process
/// group), so the pid it prints is also its pgid; a freshly spawned child is
/// never a process-group leader, so `setsid` execs directly without forking
/// and `wait()` observes the SIGKILL'd child itself.
///
/// Linux-only: `setsid(1)` ships with util-linux; macOS has no equivalent
/// command.
#[tokio::test]
#[cfg(target_os = "linux")]
async fn signal_group_takes_down_process_group() {
    let output = baml_test!(
        r#"
            function main() -> bool throws baml.errors.Io | baml.errors.Timeout {
                let process = baml.sys.start_process(
                    "setsid",
                    ["sh", "-c", "echo $$; exec sleep 30"],
                    null,
                );
                defer { process.close() }

                let pgid = baml.json.from_string<int>(process.stdout.lines().next() ?? "0") catch_all (e) {
                    _ => 0,
                };
                if (pgid <= 0 || !baml.sys.is_alive(pgid)) {
                    return false;
                }
                match (baml.sys.platform()) {
                    let u: baml.sys.Unix => u.signal_group(pgid, baml.sys.Signal.Kill),
                    _ => { return false; },
                }
                let exit = process.wait();
                match (exit.signal) {
                    let signal: string => signal == "9",
                    null => false,
                }
            }
        "#
    );

    assert_eq!(output.result, Ok(BexExternalValue::Bool(true)));
}

/// Non-positive pids/pgids are rejected before any syscall: `0` and negative
/// values have process-group semantics in `kill(2)` and must never reach the
/// kernel.
#[tokio::test]
#[cfg(not(target_os = "windows"))]
async fn signal_functions_reject_non_positive_ids() {
    let output = baml_test!(
        r#"
            function main() -> bool throws baml.errors.Io {
                match (baml.sys.platform()) {
                    let u: baml.sys.Unix => {
                        u.terminate(0) catch (e) {
                            let io: baml.errors.Io => { },
                            _ => { return false; },
                        };
                        u.signal_group(-1, baml.sys.Signal.Terminate) catch (e) {
                            let io: baml.errors.Io => { },
                            _ => { return false; },
                        };
                        baml.sys.is_alive(0) catch (e) {
                            let io: baml.errors.Io => { return true; },
                            _ => { return false; },
                        };
                        false
                    },
                    _ => false,
                }
            }
        "#
    );

    assert_eq!(output.result, Ok(BexExternalValue::Bool(true)));
}
