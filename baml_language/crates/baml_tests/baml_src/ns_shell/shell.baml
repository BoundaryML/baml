// Unified tests for shell and exec operations.
//
// Converted from crates/baml_tests/tests/shell.rs. Each Rust test that runs a
// deterministic, cross-platform command becomes a top-level `function` + a
// `test` block. Functions are used (rather than inline test-block bodies) to
// avoid the test-block local-boxing VM bug — locals compared against literals
// inside a test block may compare unequal.
//
// Tests left in Rust (not converted):
//
//   shell_with_pipe        — Unix-only command (`tr`); also has an insta
//                            bytecode snapshot that only runs in the Rust harness.
//   shell_nonexistent_command — asserts exit_code != 0, but the exact value is
//                            platform-dependent (127 on Unix, 9009 on Windows,
//                            etc.); non-deterministic enough to leave.
//   shell_stderr           — uses `>&2` stderr redirection, Unix-only.
//   exec_failing           — platform-split: Unix uses `false` binary, Windows
//                            uses `cmd /c exit 1`.
//   exec_with_args         — platform-split: Unix uses `printf`, Windows uses `cmd`.
//   exec_stderr            — platform-split redirection syntax.
//   exec_with_cwd          — platform-split: Unix uses `pwd`, Windows uses `cmd /c cd`.
//   exec_with_stdin        — platform-split: Unix uses `cat`, Windows uses `findstr`.
//   exec_with_timeout      — timing-dependent; not safe for deterministic assertion.
//   shell_with_options     — platform-split: Unix uses `pwd`, Windows uses `cmd /c cd`.
//   shell_stderr_bytes     — uses `>&2` redirection; Unix-only.

// ============================================================================
// shell() tests
// ============================================================================

// `echo hello_from_shell` works in bash, zsh, sh, PowerShell, and cmd.
function shell_echo_fn() -> bool {
    baml.sys.shell("echo hello_from_shell", null).stdout.to_string().includes("hello_from_shell")
}

test "shell_echo" {
    assert.is_true(shell_echo_fn())
}

// `exit 1` is a built-in understood by bash/zsh/sh, PowerShell, and cmd — all
// report exit code 1.
function shell_failing_command_fn() -> int {
    baml.sys.shell("exit 1", null).exit_code
}

test "shell_failing_command" {
    assert.is_true(baml.deep_equals(shell_failing_command_fn(), 1))
}

// Dynamic command string: the variable assignment happens inside BAML, then
// the shell sees the expanded string.
function shell_with_variable_fn() -> bool {
    let cmd = "echo dynamic_value";
    baml.sys.shell(cmd, null).stdout.to_string().includes("dynamic_value")
}

test "shell_with_variable" {
    assert.is_true(shell_with_variable_fn())
}

// ShellOutput.ok() returns true for exit code 0 …
function shell_ok_success_fn() -> bool {
    baml.sys.shell("echo hi", null).ok()
}

test "shell_ok_method_success" {
    assert.is_true(shell_ok_success_fn())
}

// … and false for a non-zero exit code.
function shell_ok_failure_fn() -> bool {
    baml.sys.shell("exit 1", null).ok()
}

test "shell_ok_method_failure" {
    assert.is_true(baml.deep_equals(shell_ok_failure_fn(), false))
}

// stdout field is a uint8array; to_string() decodes it.
function shell_stdout_bytes_fn() -> bool {
    baml.sys.shell("echo hello", null).stdout.to_string().includes("hello")
}

test "shell_stdout_bytes" {
    assert.is_true(shell_stdout_bytes_fn())
}

// ============================================================================
// exec() tests
// ============================================================================

// exec() with no platform guard in the original Rust test — `echo` resolves as
// a PATH binary on Unix and is available cross-platform enough for CI.
function exec_echo_fn() -> bool {
    baml.sys.exec("echo", ["Hello From Exec!"], null).stdout.to_string().includes("Hello From Exec!")
}

test "exec_echo" {
    assert.is_true(exec_echo_fn())
}
