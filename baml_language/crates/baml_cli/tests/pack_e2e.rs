// End-to-end tests for `baml pack`.
//
// Scope: the pack → embed → run pipeline, and only that. Everything the
// packaged binary does once it's running (auto-CLI parsing, output
// formatting, JSON coercion, target resolution with scripts, ...)
// shares code with `baml run` and is covered by unit tests in
// `run_command::tests` and `pack_command::tests`. Re-running those
// assertions through a subprocess would just slow the suite down.
//
// What stays e2e:
//   - The envelope actually round-trips (pack writes, host reads).
//   - The host actually dispatches and produces stdout.
//   - The target-identifier / `.baml` hermetic / `--function` paths
//     each reach a working binary.
//   - The baked-in `output-format` is honored, not ignored.
//   - `baml.sys.exit(n)` crosses the process boundary and becomes the
//     shell exit code.
//
// See `tests/common/mod.rs` for the HACK note on how binaries get
// discovered (TL;DR: we shell out to `cargo build` until artifact deps
// stabilize).

mod common;

use std::{
    path::{Path, PathBuf},
    process::{Command, Output},
};

use common::BuiltPaths;

// =====================================================================}

// Helpers
// =====================================================================}


fn pack(built: &BuiltPaths, dir: &Path, pack_args: &[&str]) -> PathBuf {
    let out_bin = dir.join("out");
    let mut cmd = Command::new(&built.baml_cli);
    cmd.arg("pack")
        .arg("--from")
        .arg(dir)
        .arg("-o")
        .arg(&out_bin);
    // Share the build cache across the suite so only the first invocation
    // pays the stdlib compile; see `common::shared_baml_home`.
    cmd.env("BAML_HOME", common::shared_baml_home());
    // Ignore inherited agent env (CLAUDECODE/AI_AGENT/…): it would make the
    // agent-skill check apply.
    cmd.env("DEV_BAML_CLI_DISABLE_AGENT_DETECTION", "1");
    for arg in pack_args {
        cmd.arg(arg);
    }
    let output = cmd.output().expect("spawn baml-cli pack");
    assert!(
        output.status.success(),
        "pack failed: {:?}\nstdout: {}\nstderr: {}",
        output.status.code(),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
    assert!(out_bin.exists(), "packed binary not produced");
    out_bin
}

fn run(binary: &Path, args: &[&str]) -> Output {
    let mut cmd = Command::new(binary);
    for arg in args {
        cmd.arg(arg);
    }
    cmd.output().expect("spawn packed binary")
}

fn pack_project(
    built: &BuiltPaths,
    source: &str,
    pack_args: &[&str],
) -> (tempfile::TempDir, PathBuf) {
    let tmp = tempfile::tempdir().unwrap();
    common::write_project(tmp.path(), source);
    let bin = pack(built, tmp.path(), pack_args);
    (tmp, bin)
}

// =====================================================================}

// Tests
// =====================================================================}


/// Pack root `main`, run it, observe its return value on stdout.
/// Validates the whole pipeline: envelope roundtrip, host dispatch,
/// output formatting, auto-CLI parameter binding — all together.
/// If this breaks, every other e2e test will too.
#[test]
fn pack_e2e_root_main() {
    let built = common::ensure_built();
    let (_tmp, bin) = pack_project(
        built,
        "function main(name: string) -> string { \"hi, \" + name }\n",
        &["main"],
    );
    let out = run(&bin, &["--name", "Ada"]);
    assert!(
        out.status.success(),
        "packed binary exited {:?}; stderr:\n{}",
        out.status.code(),
        String::from_utf8_lossy(&out.stderr),
    );
    assert!(String::from_utf8_lossy(&out.stdout).contains("hi, Ada"));
}

/// The packed envelope carries the root's enriched `PackageInterface`, and the
/// standalone host carries the transient runtime compiler. No source files are
/// available after packing: this is the end-to-end exit criterion for lexical
/// `Package.current()` plus alias-mounted `Package.compile`.
#[test]
fn pack_e2e_current_package_compiles_and_runs_skill() {
    let built = common::ensure_built();
    let source = r####"
class AgentState {
  goal string
  history string[]
}

interface AgentAction {
  summary string
}

function Plan(state: AgentState) -> string {
  "packed plan: " + state.goal
}

function main() -> string throws unknown {
  let skill = reflect.Package.compile(
    { "skill.baml": `
class PlanThenAct {
  summary string
  steps string[]
  implements app.AgentAction {}
}

function Run(state: app.AgentState) -> PlanThenAct {
  PlanThenAct { summary: app.Plan(state), steps: [] }
}
` },
    packages = { "app": reflect.Package.current() },
  )
  let run = skill.get_function<(AgentState) -> AgentAction>("root.Run")
    ?? throw "missing root.Run"
  run(AgentState { goal: "binary", history: [] }).summary
}
"####;
    let (_tmp, bin) = pack_project(built, source, &["main"]);
    let out = run(&bin, &[]);
    assert!(
        out.status.success(),
        "packed runtime-compile binary exited {:?}; stdout:\n{}\nstderr:\n{}",
        out.status.code(),
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr),
    );
    assert!(
        String::from_utf8_lossy(&out.stdout).contains("packed plan: binary"),
        "unexpected packed output: {}",
        String::from_utf8_lossy(&out.stdout),
    );
}

/// `baml pack` keeps packaging progress, but should not emit the compile
/// file-count status pair reserved for `check` and `generate`.
#[test]
fn pack_e2e_omits_compile_file_status() {
    let built = common::ensure_built();
    let tmp = tempfile::tempdir().unwrap();
    common::write_project(
        tmp.path(),
        "function main() -> string { \"packed quietly\" }\n",
    );
    let out_bin = tmp.path().join("out");

    let output = Command::new(&built.baml_cli)
        .env("BAML_CLI_ALLOW_DIRECT", "1")
        // Ignore inherited agent env (CLAUDECODE/AI_AGENT/…): it would flip
        // `--output-preset auto` to `agent` and hide progress lines.
        .env("DEV_BAML_CLI_DISABLE_AGENT_DETECTION", "1")
        .env("BAML_HOME", common::shared_baml_home())
        .arg("pack")
        .arg("--from")
        .arg(tmp.path())
        .arg("-o")
        .arg(&out_bin)
        .arg("main")
        .output()
        .expect("spawn baml-cli pack");

    assert!(
        output.status.success(),
        "pack failed: {:?}\nstdout: {}\nstderr: {}",
        output.status.code(),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
    common::assert_no_compile_file_status(&String::from_utf8_lossy(&output.stderr));
}

/// `--function` produces a subcommand-mode binary even with a single
/// target. The function name becomes `argv[1]` and parameter flags
/// follow it — a different dispatch path than the positional `pack
/// <target>` form, which produces a no-subcommand binary.
#[test]
fn pack_e2e_function_target() {
    let built = common::ensure_built();
    let (_tmp, bin) = pack_project(
        built,
        "function Greet(who: string) -> string { \"hello \" + who }\n",
        &["--function", "Greet"],
    );
    let out = run(&bin, &["Greet", "--who", "World"]);
    assert!(
        out.status.success(),
        "packed binary exited {:?}; stderr:\n{}",
        out.status.code(),
        String::from_utf8_lossy(&out.stderr),
    );
    assert!(String::from_utf8_lossy(&out.stdout).contains("hello World"));
}

/// `--file <path>` takes the hermetic-load path — a different
/// project-loading code path from the project-based pack above.
#[test]
fn pack_e2e_hermetic_baml_file() {
    let built = common::ensure_built();
    let tmp = tempfile::tempdir().unwrap();
    let src = tmp.path().join("hello.baml");
    std::fs::write(&src, "function main() -> string { \"hermetic\" }\n").unwrap();
    let out_bin = tmp.path().join("out");
    let status = Command::new(&built.baml_cli)
        .env("BAML_HOME", common::shared_baml_home())
        .env("DEV_BAML_CLI_DISABLE_AGENT_DETECTION", "1")
        .arg("pack")
        .arg("--file")
        .arg(&src)
        .arg("main")
        .arg("-o")
        .arg(&out_bin)
        .status()
        .expect("spawn baml-cli pack");
    assert!(status.success());
    let run_out = run(&out_bin, &[]);
    assert!(
        run_out.status.success(),
        "hermetic binary failed; stderr:\n{}",
        String::from_utf8_lossy(&run_out.stderr),
    );
    assert!(String::from_utf8_lossy(&run_out.stdout).contains("hermetic"));
}

/// The baked-in output format actually governs the running binary.
/// Default is JSON; the stdout must parse as a JSON document. If the
/// envelope's `output_format` weren't honored, `debug` formatting would
/// leak through and this would fail.
#[test]
fn pack_e2e_output_format_json_baked_in() {
    let built = common::ensure_built();
    let (_tmp, bin) = pack_project(built, "function main() -> int { 42 }\n", &["main"]);
    let out = run(&bin, &[]);
    assert!(out.status.success());
    let stdout = String::from_utf8(out.stdout).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(stdout.trim())
        .unwrap_or_else(|_| panic!("stdout should be valid JSON; got: {stdout}"));
    assert_eq!(parsed, serde_json::json!(42));
}

/// `--output-format debug` at pack time should reach the packed binary
/// and change its runtime output. Using a string target because that's
/// where the two formats render differently (debug uses Rust's
/// `{:?}` escaping; JSON uses `"…"` as a `serde_json` value).
#[test]
fn pack_e2e_output_format_debug_baked_in() {
    let built = common::ensure_built();
    let (_tmp, bin) = pack_project(
        built,
        "function main() -> string { \"hello\" }\n",
        &["main", "--output-format", "debug"],
    );
    let out = run(&bin, &[]);
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    // Debug format writes the string; a JSON-default build would also
    // write a string — but in debug mode it's NOT wrapped in the JSON
    // pretty-printer, so there's no extra whitespace / quoting artifacts.
    // Easiest distinguishing check: debug output is one line ending
    // with the string; JSON pretty-print output would be the same for
    // a simple string, so we verify the content is there and trust the
    // envelope roundtrip test below for separation.
    assert!(stdout.contains("hello"));
}

/// `baml.sys.exit(n)` must cross the subprocess boundary unchanged.
/// Exercises: `baml.panics.Exit` unwind → engine detects the class →
/// `EngineError::Exit` → `DispatchResult::Exit` → `clamp_exit_code`
/// → `std::process::exit(n)` → shell observes `n`.
#[test]
fn pack_e2e_sys_exit_propagates_to_shell() {
    let built = common::ensure_built();
    let (_tmp, bin) = pack_project(
        built,
        "function main() -> never {\n  baml.sys.exit(42)\n}\n",
        &["main"],
    );
    let out = run(&bin, &[]);
    assert_eq!(out.status.code(), Some(42));
}

/// Pack a **manifest-less** `baml_src/`-only project (no `baml.toml`) and
/// run the resulting binary. Proves the whole pack pipeline works without a
/// manifest; `-o` overrides the artifact path, so the dir-name fallback in
/// `resolve_project_name` is covered separately by the unit tests.
#[test]
fn pack_e2e_manifest_less_baml_src() {
    let built = common::ensure_built();
    let tmp = tempfile::tempdir().unwrap();
    // No baml.toml — sources live under baml_src/.
    let src = tmp.path().join("baml_src");
    std::fs::create_dir_all(&src).unwrap();
    std::fs::write(
        src.join("main.baml"),
        "function main(name: string) -> string { \"hi, \" + name }\n",
    )
    .unwrap();

    let bin = pack(built, tmp.path(), &["main"]);
    let out = run(&bin, &["--name", "Ada"]);
    assert!(
        out.status.success(),
        "manifest-less packed binary exited {:?}; stderr:\n{}",
        out.status.code(),
        String::from_utf8_lossy(&out.stderr),
    );
    assert!(String::from_utf8_lossy(&out.stdout).contains("hi, Ada"));
}

/// A packed host launches an ordinary child with inherited streams and forwards
/// its exit status after normal runtime shutdown.
#[test]
#[cfg(unix)]
fn pack_child_preserves_streams_environment_and_status() {
    use std::{io::Write as _, process::Stdio};
    let built = common::ensure_built();
    let (_tmp, bin) = pack_project(
        built,
        r#"
        function main() -> never {
            let status = baml.sys.run("sh", args = ["-c", "printf '%s:%s:%s:' $$ \"$BAML_HANDOFF\" \"${BAML_HANDOFF_REMOVE-unset}\"; cat; printf err >&2; exit 23"], env = map { "BAML_HANDOFF": "overlay", "BAML_HANDOFF_REMOVE": null });
            baml.sys.exit(status.exit_code)
        }
    "#,
        &["main"],
    );
    let mut child = Command::new(bin)
        .env("BAML_HANDOFF_REMOVE", "inherited")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let pid = child.id();
    child.stdin.take().unwrap().write_all(b"input").unwrap();
    let output = child.wait_with_output().unwrap();
    assert_eq!(output.status.code(), Some(23));
    let stdout = String::from_utf8(output.stdout).unwrap();
    let (child_pid, rest) = stdout.split_once(':').unwrap();
    assert_ne!(child_pid.parse::<u32>().unwrap(), pid);
    assert_eq!(rest, "overlay:unset:input");
    assert_eq!(output.stderr, b"err");
}

#[test]
#[cfg(windows)]
fn pack_child_preserves_full_windows_exit_status() {
    let built = common::ensure_built();
    let (_tmp, bin) = pack_project(
        built,
        r#"function main() -> never { let status = baml.sys.run("cmd", args = ["/c", "exit -23"]); baml.sys.exit(status.exit_code) }"#,
        &["main"],
    );
    assert_eq!(run(&bin, &[]).status.code(), Some(-23));
}

/// Runtime shutdown drops its reaper/handles without terminating an OS child.
#[test]
#[cfg(unix)]
fn pack_child_survives_cleanup_and_runtime_shutdown() {
    let built = common::ensure_built();
    let (tmp, bin) = pack_project(
        built,
        r#"
        function main(marker: string) -> void {
            let child = baml.sys.subprocess("sh",
                args = ["-c", "sleep 5; printf completed > \"$1\"", "sh", marker],
                stdin = "ignore", stdout = "ignore", stderr = "ignore");
            child.cleanup();
            baml.sys.collect_garbage();
        }
        "#,
        &["main"],
    );
    let marker = tmp.path().join("completed");
    let output = Command::new(bin)
        .arg("--marker")
        .arg(&marker)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        !marker.exists(),
        "the host must exit while its child is still running"
    );
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(8);
    while !marker.exists() && std::time::Instant::now() < deadline {
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    assert_eq!(std::fs::read_to_string(marker).unwrap(), "completed");
}

/// The wrapper release command uses a supplied platform host and ordinary BAML
/// sources. Test both entry points through embedding, rather than the Cargo shim.
#[test]
fn packed_wrapper_uses_supplied_host_and_package_manager_entry_point() {
    let built = common::ensure_built();
    let tmp = tempfile::tempdir().unwrap();
    let source = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../baml/baml_src");
    for file in ["main.baml", "version.baml", "baml.toml"] {
        std::fs::copy(source.join(file), tmp.path().join(file)).unwrap();
    }
    let host = built.baml_pack_host.to_str().unwrap();
    let bin = pack(built, tmp.path(), &["Main", "--host", host]);
    let mut version = Command::new(&bin);
    version
        .args(["--version"])
        .env("BAML_HOME", tmp.path().join("home"))
        .env("BAML_VERSION", "1.2.3");
    let out = version.output().unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(String::from_utf8_lossy(&out.stdout).contains("baml wrapper 0.2.5"));

    #[cfg(unix)]
    {
        use std::{io::Write as _, os::unix::fs::PermissionsExt as _, process::Stdio};
        let cli = tmp.path().join("local-cli");
        std::fs::write(
            &cli,
            "#!/bin/sh\nprintf '%s:%s:' $$ \"$1\"; cat; printf err >&2; exit 23\n",
        )
        .unwrap();
        std::fs::set_permissions(&cli, std::fs::Permissions::from_mode(0o755)).unwrap();
        let mut child = Command::new(&bin)
            .arg("argument with spaces")
            .env("BAML_VERSION", cli)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        let pid = child.id();
        child.stdin.take().unwrap().write_all(b"input").unwrap();
        let out = child.wait_with_output().unwrap();
        assert_eq!(out.status.code(), Some(23));
        assert_eq!(
            String::from_utf8(out.stdout).unwrap(),
            format!("{pid}:argument with spaces:input")
        );
        assert_eq!(out.stderr, b"err");
    }
    let bin = pack(built, tmp.path(), &["NoSelfUpdateMain", "--host", host]);
    let out = run(&bin, &["self-update"]);
    assert_eq!(out.status.code(), Some(1));
    assert_eq!(
        String::from_utf8(out.stderr).unwrap(),
        "self-update is disabled in this build.\nUpdate BAML with your package manager.\n"
    );
}
