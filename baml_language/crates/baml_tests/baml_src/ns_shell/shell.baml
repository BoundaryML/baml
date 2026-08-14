// Unified tests for shell and exec operations.
//
// Functions are used (rather than inline test-block bodies) to avoid the
// test-block local-boxing VM bug — locals compared against literals inside a
// test block may compare unequal.

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
    assert.equal(shell_failing_command_fn(), 1)
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
    assert.is_true(!shell_ok_failure_fn())
}

// stdout field is a uint8array; to_string() decodes it.
function shell_stdout_bytes_fn() -> bool {
    baml.sys.shell("echo hello", null).stdout.to_string().includes("hello")
}

test "shell_stdout_bytes" {
    assert.is_true(shell_stdout_bytes_fn())
}

// A nonexistent command still spawns the shell fine (no throw); only the exit
// code is non-zero. The exact code is platform-dependent (127 on Unix shells,
// 9009 on cmd, 1 on PowerShell), so — exactly like the original Rust test —
// only `!= 0` is asserted.
function shell_nonexistent_command_fn() -> bool {
    baml.sys.shell("nonexistent_command_12345", null).exit_code != 0
}

test "shell_nonexistent_command" {
    assert.is_true(shell_nonexistent_command_fn())
}

// ============================================================================
// B-251 regression: concat string -> baml.io.println -> baml.sys.shell
// ============================================================================
//
// A string built via concatenation, consumed by `baml.io.println` first and
// then passed to `baml.sys.shell`, was silently emptied — the shell saw an
// empty command and produced no output (exit 0, no error). The failure mode is
// completely silent, so it is nearly impossible to spot without a targeted test.
//
// Root cause (fixed by #3776, "fix(bex_str): stop flatten() from emptying shared
// Concat nodes"): a `BexStr` concatenation whose result exceeds the 54-byte
// inline cap is stored as a shared `Arc<ConcatNode>` rather than copied inline.
// `flatten()` — triggered the first time a sysop materializes the string — used
// to destructively empty each *shared inner* Concat node while folding its bytes
// into the parent. So `println("-> " + cmd)` flattened the enclosing concat and
// emptied the shared `cmd`; the later `shell(cmd, null)` then read "".
//
// Two conditions are REQUIRED for the bug to bite, so the tests below are
// deliberately built to satisfy both:
//   1. `cmd` must be longer than 54 bytes, otherwise it is eagerly inlined and
//      is never a shared Concat node (the literal "echo " + "hello_world" from
//      the ticket is only 16 bytes and could NOT reproduce the bug).
//   2. `cmd` must be embedded in an enclosing concat that is flattened first
//      (here, `"-> " + cmd` handed to `println`), making it a shared inner node.
//
// Each function asserts stdout is BOTH non-empty (the precise encoding of the
// "silently emptied" failure) and contains the echoed marker; `includes(...)`
// keeps the content check CRLF-agnostic, matching the shell tests above.

// A >54-byte payload so `"echo " + PAYLOAD` becomes a shared heap Concat node.
// (Defined once; reused so every variant clears the inline cap by construction.)
function b251_payload() -> string {
    "hello_world_b251_then_padding_past_the_54_byte_inline_cap"
}

// Variant 1: a concat-built command, printed before it is run.
function b251_concat_println_shell_fn() -> bool {
    let cmd = "echo " + b251_payload();
    baml.io.println("-> " + cmd);
    let out = baml.sys.shell(cmd, null).stdout.to_string();
    out.length() > 0 && out.includes("hello_world")
}

test "b251_concat_println_then_shell" {
    assert.is_true(b251_concat_println_shell_fn())
}

// Variant 2: `println` called twice before the shell runs.
function b251_println_twice_fn() -> bool {
    let cmd = "echo " + b251_payload();
    baml.io.println("-> " + cmd);
    baml.io.println("-> still " + cmd);
    let out = baml.sys.shell(cmd, null).stdout.to_string();
    out.length() > 0 && out.includes("hello_world")
}

test "b251_println_twice_then_shell" {
    assert.is_true(b251_println_twice_fn())
}

// Variant 3: multi-step concat built from two separate `let` bindings.
function b251_multi_let_fn() -> bool {
    let prefix = "echo ";
    let payload = b251_payload();
    let cmd = prefix + payload;
    baml.io.println("-> " + cmd);
    let out = baml.sys.shell(cmd, null).stdout.to_string();
    out.length() > 0 && out.includes("hello_world")
}

test "b251_multi_let_then_shell" {
    assert.is_true(b251_multi_let_fn())
}

// Variant 4: the `println` result is bound to `_` before the shell runs.
function b251_println_discarded_fn() -> bool {
    let cmd = "echo " + b251_payload();
    let _ = baml.io.println("-> " + cmd);
    let out = baml.sys.shell(cmd, null).stdout.to_string();
    out.length() > 0 && out.includes("hello_world")
}

test "b251_println_discarded_then_shell" {
    assert.is_true(b251_println_discarded_fn())
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
