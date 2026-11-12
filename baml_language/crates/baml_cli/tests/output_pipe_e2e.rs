//! Closed consumers must not turn wrapper output or host diagnostics into Rust panics.
#![cfg(unix)]
mod common;

use std::{
    io::{BufRead as _, BufReader, Write as _, pipe},
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

fn command(binary: &Path, root: &Path) -> Command {
    let mut command = Command::new(binary);
    command
        .current_dir(root)
        .env("HOME", root)
        .env("BAML_HOME", root.join(".baml"))
        .env("BAML_TELEMETRY", "off")
        .env("DO_NOT_TRACK", "1")
        .env("BAML_CLI_ALLOW_DIRECT", "1")
        .env("DEV_BAML_CLI_DISABLE_AGENT_DETECTION", "1")
        .env_remove("BOUNDARY_API_KEY")
        .env_remove("BOUNDARY_PROJECT")
        .env_remove("BAML_LOG")
        .env_remove("BAML_CONTEXT");
    command
}

fn closed_pipe() -> Stdio {
    let (reader, writer) = pipe().unwrap();
    drop(reader);
    writer.into()
}

fn wrapper(root: &Path) -> PathBuf {
    let built = common::ensure_built();
    let binary = root.join("wrapper");
    let output = command(Path::new("python3"), root)
        .arg(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../wrapper/build"))
        .arg("--cli")
        .arg(&built.baml_cli)
        .arg("--pack-host")
        .arg(&built.baml_pack_host)
        .arg("--output")
        .arg(&binary)
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
    binary
}

#[test]
fn wrapper_closed_output_exits_141_without_traceback() {
    let temp = tempfile::tempdir().unwrap();
    let binary = wrapper(temp.path());
    for args in [vec!["--help"], vec!["--version"], vec!["toolchain", "list"]] {
        let output = command(&binary, temp.path())
            .args(&args)
            .env("BAML_TOOLCHAIN", common::baml_cli())
            .stdout(closed_pipe())
            .output()
            .unwrap();
        assert_eq!(output.status.code(), Some(141), "{args:?}: {output:?}");
        assert!(output.stderr.is_empty(), "{output:?}");
    }
    let output = command(&binary, temp.path())
        .arg("--help")
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
    assert!(String::from_utf8_lossy(&output.stdout).contains("Usage: baml"));
    assert!(output.stderr.is_empty(), "{output:?}");

    let output = command(&binary, temp.path())
        .arg("--help")
        .stdout(closed_pipe())
        .stderr(closed_pipe())
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(141), "{output:?}");

    // Linux provides a deterministic non-BrokenPipe write failure. Preserve
    // its traceback and failure status instead of treating it as a pipeline.
    if let Ok(full) = std::fs::OpenOptions::new().write(true).open("/dev/full") {
        let output = command(&binary, temp.path())
            .arg("--help")
            .stdout(full)
            .output()
            .unwrap();
        assert_eq!(output.status.code(), Some(1), "{output:?}");
        assert!(
            String::from_utf8_lossy(&output.stderr).contains("HostUnavailable"),
            "{output:?}"
        );
    }

    // The ordinary error handler also owns its stderr output.
    let output = command(&binary, temp.path())
        .arg("--project")
        .stderr(closed_pipe())
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(141), "{output:?}");

    // A FIFO blocks state loading after the first two flushed list lines, so
    // closing stdout here is deterministic rather than a race against `head`.
    let state = temp.path().join(".baml/state.toml");
    std::fs::create_dir_all(state.parent().unwrap()).unwrap();
    let status = Command::new("mkfifo").arg(&state).status().unwrap();
    assert!(status.success());
    let mut child = command(&binary, temp.path())
        .args(["toolchain", "list"])
        .env("BAML_TOOLCHAIN", common::baml_cli())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut reader = BufReader::new(child.stdout.take().unwrap());
    for expected in ["default selector:", "active selector:"] {
        let mut line = String::new();
        reader.read_line(&mut line).unwrap();
        assert!(line.starts_with(expected), "{line}");
    }
    drop(reader);
    // Empty state unblocks the next output without introducing a fixture error.
    drop(
        std::fs::OpenOptions::new()
            .write(true)
            .open(&state)
            .unwrap(),
    );
    let output = child.wait_with_output().unwrap();
    assert_eq!(output.status.code(), Some(141), "{output:?}");
    assert!(output.stderr.is_empty(), "{output:?}");
}

#[test]
fn broken_diagnostics_preserve_terminal_recording_and_shutdown() {
    let _built = common::ensure_built();
    let temp = tempfile::tempdir().unwrap();
    common::write_project(
        temp.path(),
        r#"
function main(mode: string) -> void {
    if (mode == "output") {
        baml.io.println("first");
        baml.io.input(null);
        baml.io.println("second");
    } else {
        baml.sys.panic("original failure");
    }
}
"#,
    );
    let manifest = temp.path().join("baml.toml");
    std::fs::OpenOptions::new()
        .append(true)
        .open(&manifest)
        .unwrap()
        .write_all(b"\n[pack.env_var_names]\nBAML_TELEMETRY = \"BAML_TELEMETRY\"\n")
        .unwrap();
    let binary = temp.path().join("packed");
    let output = command(&common::baml_cli(), temp.path())
        .args(["--agent-skill-check", "off", "pack", "main", "-o"])
        .arg(&binary)
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");

    let home = temp.path().join("recorded");
    std::fs::create_dir(&home).unwrap();
    let output = command(&binary, &home)
        .args(["--mode", "panic"])
        .env("BAML_TELEMETRY", "medium")
        .stderr(closed_pipe())
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1), "{output:?}");
    let directories: Vec<_> = std::fs::read_dir(home.join(".baml/btel/recordings"))
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .collect();
    assert_eq!(directories.len(), 1);
    let recording = btel_file::read_directory(&directories[0]).unwrap();
    assert!(recording.issues.is_empty(), "{:?}", recording.issues);
    assert!(
        recording.has_recording_end,
        "shutdown must seal the recording"
    );
    let end = recording
        .files
        .iter()
        .find_map(|file| file.end.as_ref())
        .unwrap();
    assert_eq!(
        end.process_end.as_ref().unwrap().status,
        btel_recorder::proto::ProcessStatus::Panicked as i32,
        "original panic outcome"
    );

    // Public println still panics when not handled, including after a previous
    // successful write; closing both streams must preserve exit 1, never 101.
    let mut child = command(&binary, temp.path())
        .args(["--mode", "output"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(closed_pipe())
        .spawn()
        .unwrap();
    let mut reader = BufReader::new(child.stdout.take().unwrap());
    let mut line = String::new();
    reader.read_line(&mut line).unwrap();
    assert_eq!(line, "first\n");
    drop(reader);
    child
        .stdin
        .take()
        .unwrap()
        .write_all(b"continue\n")
        .unwrap();
    assert_eq!(child.wait().unwrap().code(), Some(1));
}
