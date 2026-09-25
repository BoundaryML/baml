mod common;

use std::process::Command;

#[test]
fn retired_profiling_commands_report_unavailable() {
    let project = tempfile::tempdir().unwrap();
    for args in [
        vec!["query", "SELECT 1"],
        vec!["query", "--schema"],
        vec!["clean"],
    ] {
        let output = Command::new(env!("CARGO_BIN_EXE_baml-cli"))
            .args(&args)
            .current_dir(project.path())
            .output()
            .expect("run CLI");
        assert!(!output.status.success(), "{args:?} must not report success");
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(stderr.contains("unavailable"), "{args:?}: {stderr}");
        assert!(!project.path().join(".baml/profiles-v1").exists());
    }
}
