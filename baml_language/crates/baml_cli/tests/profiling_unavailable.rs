mod common;

use std::process::Command;

#[test]
fn retired_profile_store_commands_report_unavailable() {
    let project = tempfile::tempdir().unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_baml-cli"))
        .arg("clean")
        .current_dir(project.path())
        .env("DEV_BAML_CLI_DISABLE_AGENT_DETECTION", "1")
        .output()
        .expect("run CLI");
    assert!(!output.status.success(), "clean must not report success");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("unavailable"), "clean: {stderr}");
    assert!(!project.path().join(".baml/profiles-v1").exists());
}
