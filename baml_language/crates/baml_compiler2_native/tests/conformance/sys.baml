// `baml.sys`: what the process knows of itself, and running a program to
// its end. The programs run are `/bin/echo`, `/bin/cat`, `/usr/bin/true`
// and `/bin/sleep`, and paths are relative to the directory the tests run
// in, which is this crate's.

function sys_argv() -> int {
  baml.sys.argv().length()
}
// Each process has its own id and its own executable.
function sys_pid_and_exe() -> bool {
  baml.sys.pid() > 0 && baml.sys.current_exe().length() > 0
}
function sys_where_and_what() -> string {
  let info = baml.sys.host_info();
  `${baml.sys.current_dir()} ${baml.sys.home_dir() ?? "none"} ${baml.sys.platform()} ${baml.sys.host_target()} ${info.platform} ${info.architecture} ${info.cpu_count > 0}`
}
// A collection asked for moves every object, and what the program holds
// is where it goes on.
function sys_collect_garbage() -> string {
  let kept = task_ints(40);
  let text = rep("kept", 3);
  baml.sys.collect_garbage();
  let more = task_ints(10);
  baml.sys.collect_garbage();
  `${kept[39]} ${text} ${more[9]}`
}
function sys_run_captures() -> string {
  let ran = baml.sys.run("/bin/echo", args = [rep("one", 1), "two words"], capture_output = true);
  `${ran.stdout.to_hex()} [${ran.stderr.to_hex()}] ${ran.exit_code} ${ran.signal == null} ${ran.ok()}`
}
function sys_run_writes_input() -> string {
  let text = baml.sys.run("/bin/cat", input = rep("fed as text ", 2), stdout = "pipe");
  let bytes = baml.sys.run("/bin/cat", input = b"\x00\xff", stdout = "pipe", stderr = "ignore");
  `${text.stdout.to_hex()} ${bytes.stdout.to_hex()}`
}
function sys_run_in_a_directory() -> string {
  let ran = baml.sys.run("/bin/cat", args = ["support.baml"], cwd = rep("tests/", 1) + "conformance", capture_output = true, timeout = baml.time.Duration.from_seconds(30));
  `${ran.stdout.length() > 0} ${ran.exit_code}`
}
function sys_run_fails_without_throwing() -> string {
  let ran = baml.sys.run("/bin/cat", args = ["/no/such/file"], stdout = "ignore", stderr = "pipe", inherit_env = false, env = { "LC_ALL": "C", "UNSET": null });
  `${ran.exit_code} ${ran.ok()} ${ran.stderr.length() > 0} ${ran.stdout.length()}`
}
function sys_run_nothing_to_capture() -> string {
  let ran = baml.sys.run("/usr/bin/true", stdin = "ignore", stdout = "ignore", stderr = "ignore");
  `${ran.exit_code} ${ran.stdout.length()} ${ran.stderr.length()}`
}
function sys_run_no_such_program() -> string {
  let ran = baml.sys.run(rep("/no/such/", 1) + "program", capture_output = true) catch (e) {
    baml.errors.Io => { return `io: ${e.message}`; },
    baml.errors.Timeout => { return "timeout"; },
    baml.errors.InvalidArgument => { return "invalid"; },
  };
  `${ran.exit_code}`
}
function sys_run_no_such_program_uncaught() -> int {
  baml.sys.run(rep("/no/such/", 1) + "program").exit_code
}
// Two programs run at once while a third task allocates.
function sys_run_by_tasks() -> string {
  let a = spawn { baml.sys.run("/bin/echo", args = ["a"], capture_output = true).stdout.to_hex() };
  let b = spawn { let kept = task_ints(30); let ran = baml.sys.run("/bin/cat", input = "b", capture_output = true); `${ran.stdout.to_hex()}${kept[29]}` };
  let busy = spawn { task_churn(80) };
  let n = await busy;
  `${await a} ${await b} ${n}`
}
// A program that outlasts its limit is killed, and the error says which
// limit passed.
function sys_run_times_out() -> string {
  let ran = baml.sys.run("/bin/sleep", args = [rep("2", 2)], timeout = baml.time.Duration.from_milliseconds(150)) catch (e) {
    baml.errors.Io => { return `io: ${e.message}`; },
    baml.errors.Timeout => { return `timeout: ${e.message} ${e.duration?.to_milliseconds() ?? 0n} ${e.timeout_type}`; },
    baml.errors.InvalidArgument => { return "invalid"; },
  };
  `${ran.exit_code}`
}
function sys_run_times_out_uncaught() -> int {
  baml.sys.run("/bin/sleep", args = [rep("2", 2)], capture_output = true, timeout = baml.time.Duration.from_milliseconds(150)).exit_code
}
// A limit the program ends within is no limit on what it leaves.
function sys_run_within_its_limit() -> string {
  let ran = baml.sys.run("/bin/sleep", args = ["0.05"], timeout = baml.time.Duration.from_seconds(20));
  `${ran.exit_code} ${ran.signal == null}`
}
// `exit` ends the program with its code as the status and nothing said,
// after the `defer`s it is thrown through.
function sys_exit_code(code: int) -> int {
  defer { baml.io.println("inner deferred"); }
  baml.sys.exit(code)
}
function sys_exit() -> int {
  defer { baml.io.println("outer deferred"); }
  baml.io.println("before");
  sys_exit_code(3)
}
function sys_exit_zero() -> int {
  baml.sys.exit(0)
}
// A status holds less than an `int`.
function sys_exit_past_a_status() -> int {
  baml.sys.exit(5000000000)
}
function sys_exit_negative() -> int {
  baml.sys.exit(0 - 1)
}
function sys_exit_caught() -> int {
  sys_exit_code(8) catch (e) {
    baml.panics.Exit => { return e.code + 100; },
  }
}
// From a task, it ends the program once the task's `defer`s have run.
function sys_exit_in_a_task() -> int {
  let leaving = spawn { sys_exit_code(4) };
  await leaving
}
// Whether or not anything awaits the task.
function sys_exit_in_a_task_nothing_awaits() -> int {
  let leaving = spawn { sys_exit_code(5) };
  baml.sys.sleep(baml.time.Duration.from_milliseconds(20));
  baml.sys.exit(6)
}
// A task can catch it as any error.
function sys_exit_caught_in_a_task() -> int {
  let caught = spawn { sys_exit_caught() };
  await caught
}
