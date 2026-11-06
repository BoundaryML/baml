use std::{
    path::{Path, PathBuf},
    process::Command,
    sync::Arc,
};

use btel_recorder::RecordingConfig;
use btel_types::context::Context;

use super::{TelemetryRecording, user_file_roots};

fn btel(root: &Path) -> (PathBuf, PathBuf) {
    (root.join("btel/recordings"), root.join("btel/cas"))
}

fn project(root: &Path, baml_home: Option<PathBuf>, home: Option<PathBuf>) -> (PathBuf, PathBuf) {
    btel(&btel_settings::local_store::resolve(Some(root), baml_home.map(Into::into), home).unwrap())
}

#[test]
fn project_enclosing_cwd_is_namespaced_under_home() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("app");
    let nested = root.join("src/deep");
    std::fs::create_dir_all(&nested).unwrap();
    std::fs::write(root.join("baml.toml"), "").unwrap();
    let home = dir.path().join("home");

    assert_eq!(
        user_file_roots(None, Some(&nested), Some(home.clone())).unwrap(),
        project(&root, None, Some(home))
    );
}

#[test]
fn baml_home_contains_the_project_store() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("baml.toml"), "").unwrap();
    let baml_home = dir.path().join("state");

    assert_eq!(
        user_file_roots(Some(baml_home.clone().into()), Some(dir.path()), None).unwrap(),
        project(dir.path(), Some(baml_home), None)
    );
}

#[test]
fn empty_baml_home_is_unset() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("baml.toml"), "").unwrap();
    let home = dir.path().join("home");

    assert_eq!(
        user_file_roots(Some("".into()), Some(dir.path()), Some(home.clone())).unwrap(),
        project(dir.path(), None, Some(home))
    );
}

#[test]
fn no_project_uses_home() {
    let dir = tempfile::tempdir().unwrap();
    let home = dir.path().join("home");

    assert_eq!(
        user_file_roots(None, Some(dir.path()), Some(home.clone())).unwrap(),
        btel(&home.join(".baml"))
    );
    assert_eq!(
        user_file_roots(None, None, Some(home.clone())).unwrap(),
        btel(&home.join(".baml"))
    );
}

/// A blocked destination disables delivery instead of selecting an unrelated home.
#[test]
fn an_unwritable_selected_store_does_not_redirect_recordings() {
    const CHILD: &str = "BAML_TEST_BLOCKED_STORE";
    if let Some(root) = baml_env::os_var(CHILD) {
        let root = PathBuf::from(root);
        let (runtime, delivery) =
            TelemetryRecording::project_files(&root, RecordingConfig::default())
                .start(None, Arc::default(), &Context::default())
                .unwrap();
        assert!(delivery.is_none(), "must not open a different store");
        assert!(runtime.finish().is_err());
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    // A regular file blocks directory creation even for privileged test runners.
    let blocked = dir.path().join("blocked");
    std::fs::write(&blocked, "not a directory").unwrap();
    let output = Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "telemetry::user_files_tests::an_unwritable_selected_store_does_not_redirect_recordings", "--nocapture"])
        .env(CHILD, dir.path())
        .env("BAML_HOME", blocked)
        .env("HOME", dir.path().join("user"))
        .env("USERPROFILE", dir.path().join("user"))
        .output().unwrap();
    assert!(output.status.success(), "{output:?}");
    assert!(!dir.path().join(".baml").exists());
    assert!(!dir.path().join("user").exists());
}

/// Starts real recordings in child processes whose working directory is a
/// project, since the destination depends on process-wide state.
#[test]
fn recording_starts_in_cwd_project() {
    const EXPECTED: &str = "BAML_USER_FILES_TEST_EXPECTED";
    const PACKED: &str = "BAML_USER_FILES_TEST_PACKED";
    if let Some(expected) = baml_env::os_var(EXPECTED) {
        let mut recording = TelemetryRecording::user_files(RecordingConfig::default());
        if !baml_env::has_var(PACKED) {
            recording = recording.preferring_cwd_project();
        }
        let (runtime, delivery) = recording
            .start(None, Arc::default(), &Context::default())
            .unwrap();
        let directory = delivery.unwrap().directory().unwrap().to_path_buf();
        runtime.finish().unwrap();
        let expected = std::fs::canonicalize(expected).unwrap();
        assert_eq!(
            std::fs::canonicalize(directory.parent().unwrap()).unwrap(),
            expected
        );
        return;
    }

    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("app");
    std::fs::create_dir_all(root.join("baml_src")).unwrap();
    std::fs::write(root.join("baml.toml"), "").unwrap();
    let home = dir.path().join("home");
    std::fs::create_dir_all(&home).unwrap();

    let run = |expected: &Path, packed: bool| {
        let mut command = Command::new(std::env::current_exe().unwrap());
        if packed {
            command.env(PACKED, "1");
        }
        let output = command
            .args([
                "--exact",
                "telemetry::user_files_tests::recording_starts_in_cwd_project",
                "--nocapture",
            ])
            .current_dir(root.join("baml_src"))
            .env_remove("BAML_HOME")
            .env("HOME", &home)
            .env("USERPROFILE", &home)
            .env(EXPECTED, expected)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr),
        );
    };

    let recordings = project(&root, None, Some(home.clone())).0;
    let fallback = home.join(".baml/btel/recordings");
    std::fs::create_dir_all(&recordings).unwrap();
    std::fs::create_dir_all(&fallback).unwrap();
    run(&recordings, false);
    // Packed programs keep recording to the user store.
    run(&fallback, true);

    // Manifest settings cannot redirect recordings away from the fixed project store.
    std::fs::write(root.join("baml.toml"), "[tracing]\nstore = 'custom'\n").unwrap();
    run(&recordings, false);
    run(&fallback, true);
    assert!(!root.join("custom").exists());
    assert!(!root.join(".baml").exists());
}
