// End-to-end tests for termination signals.
//
// SIGINT, SIGTERM and SIGHUP cancel the root call, so `defer` blocks run,
// and the process then dies by the same signal. Covers both hosts:
// `baml run` and a `baml pack` binary.

#![cfg(unix)]

mod common;

use std::{
    os::unix::process::{CommandExt as _, ExitStatusExt as _},
    path::Path,
    process::{Child, Command, ExitStatus, Stdio},
    time::{Duration, Instant},
};

const SERVE_WITH_DEFER: &str = r#"
function main() -> never {
    defer { baml.fs.write("cleanup", "ran") catch_all (_) { _ => null }; }
    let server = baml.http.Server.bind("127.0.0.1:0");
    baml.fs.write("ready", "") catch_all (_) { _ => null };
    server.serve((req) -> { baml.http.Response.new(200, { }, "hi".to_utf8()) })
}
"#;

const SIGNALS: [i32; 3] = [libc::SIGINT, libc::SIGTERM, libc::SIGHUP];

fn wait_for_file(path: &Path, child: &mut Child) {
    let deadline = Instant::now() + Duration::from_secs(60);
    while !path.exists() {
        if let Some(status) = child.try_wait().unwrap() {
            panic!(
                "process exited before {} appeared: {status}",
                path.display()
            );
        }
        assert!(
            Instant::now() < deadline,
            "timed out waiting for {}",
            path.display()
        );
        std::thread::sleep(Duration::from_millis(50));
    }
}

fn wait_with_timeout(child: &mut Child) -> ExitStatus {
    let deadline = Instant::now() + Duration::from_secs(60);
    loop {
        if let Some(status) = child.try_wait().unwrap() {
            return status;
        }
        if Instant::now() >= deadline {
            child.kill().ok();
            panic!("process did not exit after the signal");
        }
        std::thread::sleep(Duration::from_millis(50));
    }
}

fn send(child: &Child, signal: i32) {
    let pid = libc::pid_t::try_from(child.id()).unwrap();
    // SAFETY: `kill` with a live child pid and a valid signal number.
    #[allow(unsafe_code)]
    let rc = unsafe { libc::kill(pid, signal) };
    assert_eq!(rc, 0, "kill({pid}, {signal}) failed");
}

/// Start `cmd` in `dir`, signal it once it is serving, and check that the
/// `defer` ran and the process died by that signal.
fn assert_signal_runs_defer(mut cmd: Command, dir: &Path, signal: i32) {
    let ready = dir.join("ready");
    let cleanup = dir.join("cleanup");
    std::fs::remove_file(&ready).ok();
    std::fs::remove_file(&cleanup).ok();
    let mut child = cmd
        .current_dir(dir)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn");
    wait_for_file(&ready, &mut child);
    send(&child, signal);
    let status = wait_with_timeout(&mut child);
    assert_eq!(
        status.signal(),
        Some(signal),
        "expected death by signal {signal}, got {status}"
    );
    assert_eq!(
        std::fs::read_to_string(&cleanup).ok().as_deref(),
        Some("ran"),
        "defer did not run for signal {signal}"
    );
}

#[test]
fn run_signal_cancels_main_and_runs_defer() {
    let tmp = tempfile::tempdir().unwrap();
    common::write_project(tmp.path(), SERVE_WITH_DEFER);
    for signal in SIGNALS {
        let mut cmd = Command::new(common::baml_cli());
        cmd.args(["run", "main"])
            .env("BAML_HOME", common::shared_baml_home())
            .env("BAML_CLI_ALLOW_DIRECT", "1")
            .env("DEV_BAML_CLI_DISABLE_AGENT_DETECTION", "1");
        assert_signal_runs_defer(cmd, tmp.path(), signal);
    }
}

#[test]
fn pack_signal_cancels_main_and_runs_defer() {
    let built = common::ensure_built();
    let tmp = tempfile::tempdir().unwrap();
    common::write_project(tmp.path(), SERVE_WITH_DEFER);
    let bin = tmp.path().join("out");
    let output = Command::new(&built.baml_cli)
        .args(["pack", "--project-dir"])
        .arg(tmp.path())
        .arg("-o")
        .arg(&bin)
        .arg("main")
        .env("BAML_HOME", common::shared_baml_home())
        .env("DEV_BAML_CLI_DISABLE_AGENT_DETECTION", "1")
        .output()
        .expect("spawn baml-cli pack");
    assert!(
        output.status.success(),
        "pack failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    for signal in SIGNALS {
        assert_signal_runs_defer(Command::new(&bin), tmp.path(), signal);
    }
}

/// Two tasks that each hold the key the other wants. Each takes its first
/// key and waits until the other has its own before asking for the next, so
/// the deadlock forms every time; `ready` appears once it has.
const DEADLOCK: &str = r#"
class Log {
    entries: string[],
}

function pause() -> void {
    baml.sys.sleep(baml.time.Duration.from_milliseconds(5))
}

function cross(first: Log, second: Log, started: Log) -> void {
    baml.sync.atomic(first, () -> void {
        started.entries.push("in");
        while (started.entries.length() < 2) {
            pause();
        }
        baml.sync.atomic(second, () -> void { });
    })
}

function main() -> void {
    let a = Log { entries: [] };
    let b = Log { entries: [] };
    let started = Log { entries: [] };
    let x = spawn { cross(a, b, started) };
    let y = spawn { cross(b, a, started) };
    while (started.entries.length() < 2) {
        pause();
    }
    baml.sys.sleep(baml.time.Duration.from_milliseconds(100));
    baml.fs.write("ready", "") catch_all (_) { _ => null };
    await x;
    await y;
}
"#;

/// The first signal prints where every task is and names the deadlock before
/// it cancels the root call; the process then dies by the signal.
#[test]
fn first_signal_reports_where_each_task_is() {
    let tmp = tempfile::tempdir().unwrap();
    common::write_project(tmp.path(), DEADLOCK);
    let mut child = Command::new(common::baml_cli())
        .args(["run", "main"])
        .current_dir(tmp.path())
        .env("BAML_HOME", common::shared_baml_home())
        .env("BAML_CLI_ALLOW_DIRECT", "1")
        .env("DEV_BAML_CLI_DISABLE_AGENT_DETECTION", "1")
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn baml-cli");
    wait_for_file(&tmp.path().join("ready"), &mut child);
    send(&child, libc::SIGINT);
    let status = wait_with_timeout(&mut child);
    let mut stderr = String::new();
    std::io::Read::read_to_string(&mut child.stderr.take().unwrap(), &mut stderr).unwrap();
    assert_eq!(
        status.signal(),
        Some(libc::SIGINT),
        "got {status}; stderr:\n{stderr}"
    );
    for expected in [
        "interrupted: 3 tasks still running:",
        "(called from the host), awaiting task ",
        ", waiting for a key held by task ",
        "in user.cross",
        "deadlock: task ",
    ] {
        assert!(
            stderr.contains(expected),
            "missing {expected:?} in:\n{stderr}"
        );
    }
}

/// A second signal while the root call is still unwinding ends the process
/// at once instead of waiting for a slow `defer`.
#[test]
fn second_signal_terminates_during_slow_defer() {
    let tmp = tempfile::tempdir().unwrap();
    common::write_project(
        tmp.path(),
        r#"
function main() -> never {
    defer { baml.sys.sleep(baml.time.Duration.from_seconds(600)); }
    let server = baml.http.Server.bind("127.0.0.1:0");
    baml.fs.write("ready", "") catch_all (_) { _ => null };
    server.serve((req) -> { baml.http.Response.new(200, { }, "hi".to_utf8()) })
}
"#,
    );
    let mut child = Command::new(common::baml_cli())
        .args(["run", "main"])
        .current_dir(tmp.path())
        .env("BAML_HOME", common::shared_baml_home())
        .env("BAML_CLI_ALLOW_DIRECT", "1")
        .env("DEV_BAML_CLI_DISABLE_AGENT_DETECTION", "1")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn baml-cli");
    wait_for_file(&tmp.path().join("ready"), &mut child);
    send(&child, libc::SIGINT);
    std::thread::sleep(Duration::from_millis(500));
    assert!(
        child.try_wait().unwrap().is_none(),
        "first signal should wait for the defer"
    );
    send(&child, libc::SIGINT);
    let status = wait_with_timeout(&mut child);
    assert_eq!(status.signal(), Some(libc::SIGINT), "got {status}");
}

/// A signal the process started with ignored (as under `nohup`) stays
/// ignored; other signals still cancel the root call.
#[test]
fn inherited_ignored_signal_stays_ignored() {
    let tmp = tempfile::tempdir().unwrap();
    common::write_project(tmp.path(), SERVE_WITH_DEFER);
    let mut cmd = Command::new(common::baml_cli());
    cmd.args(["run", "main"])
        .current_dir(tmp.path())
        .env("BAML_HOME", common::shared_baml_home())
        .env("BAML_CLI_ALLOW_DIRECT", "1")
        .env("DEV_BAML_CLI_DISABLE_AGENT_DETECTION", "1")
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    // SAFETY: `signal` is async-signal-safe, as `pre_exec` requires.
    #[allow(unsafe_code)]
    unsafe {
        cmd.pre_exec(|| {
            libc::signal(libc::SIGHUP, libc::SIG_IGN);
            Ok(())
        });
    }
    let mut child = cmd.spawn().expect("spawn baml-cli");
    wait_for_file(&tmp.path().join("ready"), &mut child);
    send(&child, libc::SIGHUP);
    std::thread::sleep(Duration::from_millis(500));
    assert!(
        child.try_wait().unwrap().is_none(),
        "an inherited-ignored SIGHUP must not stop the process"
    );
    assert!(!tmp.path().join("cleanup").exists());
    send(&child, libc::SIGTERM);
    let status = wait_with_timeout(&mut child);
    assert_eq!(status.signal(), Some(libc::SIGTERM), "got {status}");
    assert_eq!(
        std::fs::read_to_string(tmp.path().join("cleanup"))
            .ok()
            .as_deref(),
        Some("ran")
    );
}
