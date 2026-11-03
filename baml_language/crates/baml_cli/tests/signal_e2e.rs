// End-to-end tests for termination signals.
//
// SIGINT, SIGTERM and SIGHUP cancel the root call, so `defer` blocks run,
// and the process then dies by the same signal. Covers both hosts:
// `baml run` and a `baml pack` binary.

#![cfg(unix)]

mod common;

use std::{
    os::unix::process::ExitStatusExt as _,
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
        .args(["pack", "--from"])
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
