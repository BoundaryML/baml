#![cfg(unix)]

use std::{fs, os::unix::fs::PermissionsExt, path::Path, process::Command};

fn fixture(root: &Path, local_cli: bool) -> std::path::PathBuf {
    let bin = root.join("bin");
    fs::create_dir_all(&bin).unwrap();
    let wrapper = bin.join("baml");
    fs::copy(env!("CARGO_BIN_EXE_baml"), &wrapper).unwrap();
    if local_cli {
        let cli = bin.join("baml-cli");
        fs::write(&cli, "#!/bin/sh\nprintf 'CLI:%s\\n' \"$*\"\n").unwrap();
        fs::set_permissions(cli, fs::Permissions::from_mode(0o755)).unwrap();
    }
    wrapper
}

fn command(wrapper: &Path, root: &Path) -> Command {
    let mut command = Command::new(wrapper);
    command
        .current_dir(root)
        .env("BAML_HOME", root.join("home"))
        .env("HOME", root)
        .env("PATH", root.join("bin"))
        .env("BAML_VERSION", "0.15.0")
        .env("BAML_MANIFEST_BASE_URL", "http://127.0.0.1:1/manifest");
    command
}

#[test]
fn root_help_and_version_include_wrapper_and_local_cli() {
    let root = tempfile::tempdir().unwrap();
    let wrapper = fixture(root.path(), true);

    let help = command(&wrapper, root.path())
        .arg("--help")
        .output()
        .unwrap();
    assert!(help.status.success());
    let stdout = String::from_utf8_lossy(&help.stdout);
    assert!(
        stdout.contains("Manage installed BAML toolchains"),
        "{stdout}"
    );
    assert!(stdout.contains("CLI:--help"), "{stdout}");
    assert!(!String::from_utf8_lossy(&help.stderr).contains("127.0.0.1"));

    let version = command(&wrapper, root.path())
        .arg("--version")
        .output()
        .unwrap();
    assert!(version.status.success());
    let stdout = String::from_utf8_lossy(&version.stdout);
    assert!(stdout.contains("baml wrapper"), "{stdout}");
    assert!(stdout.contains("CLI:--version"), "{stdout}");
}

#[test]
fn ordinary_commands_attempt_install_then_fail_open_to_local_cli() {
    let root = tempfile::tempdir().unwrap();
    let wrapper = fixture(root.path(), true);
    let output = command(&wrapper, root.path())
        .args(["generate", "--help", "--unknown-flag"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stdout.contains("CLI:generate --help --unknown-flag"),
        "{stdout}"
    );
    assert!(stderr.contains("/version/0.15.0.json"), "{stderr}");
    assert!(stderr.contains("using local"), "{stderr}");
}

#[test]
fn missing_cli_without_fallback_reports_install_error() {
    let root = tempfile::tempdir().unwrap();
    let wrapper = fixture(root.path(), false);
    let output = command(&wrapper, root.path())
        .arg("generate")
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("/version/0.15.0.json"));
}

#[test]
fn root_help_attempts_install_without_local_cli_but_still_shows_wrapper_help() {
    let root = tempfile::tempdir().unwrap();
    let wrapper = fixture(root.path(), false);
    let output = command(&wrapper, root.path())
        .arg("--help")
        .output()
        .unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stdout.contains("Manage installed BAML toolchains"),
        "{stdout}"
    );
    assert!(stderr.contains("/version/0.15.0.json"), "{stderr}");
    assert!(stderr.contains("help/version unavailable"), "{stderr}");
}

#[test]
fn unpinned_first_use_attempts_canary_install() {
    let root = tempfile::tempdir().unwrap();
    let wrapper = fixture(root.path(), false);
    let output = command(&wrapper, root.path())
        .env_remove("BAML_VERSION")
        .arg("generate")
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("/canary.json"));
}
