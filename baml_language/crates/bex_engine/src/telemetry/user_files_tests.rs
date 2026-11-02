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

fn project(root: &Path) -> (PathBuf, PathBuf) {
    (
        root.join(".baml/btel/recordings"),
        root.join(".baml/btel/cas"),
    )
}

#[test]
fn project_enclosing_cwd_precedes_home() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("app");
    let nested = root.join("src/deep");
    std::fs::create_dir_all(&nested).unwrap();
    std::fs::write(root.join("baml.toml"), "").unwrap();
    let home = dir.path().join("home");

    assert_eq!(
        user_file_roots(None, Some(&nested), Some(home.clone())),
        vec![project(&root), btel(&home.join(".baml"))]
    );
}

#[test]
fn baml_home_overrides_project() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("baml.toml"), "").unwrap();
    let baml_home = dir.path().join("state");

    assert_eq!(
        user_file_roots(Some(baml_home.clone().into()), Some(dir.path()), None),
        vec![btel(&baml_home)]
    );
}

#[test]
fn empty_baml_home_is_unset() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("baml.toml"), "").unwrap();
    let home = dir.path().join("home");

    assert_eq!(
        user_file_roots(Some("".into()), Some(dir.path()), Some(home.clone())),
        vec![project(dir.path()), btel(&home.join(".baml"))]
    );
}

#[test]
fn no_project_uses_home() {
    let dir = tempfile::tempdir().unwrap();
    let home = dir.path().join("home");

    assert_eq!(
        user_file_roots(None, Some(dir.path()), Some(home.clone())),
        vec![btel(&home.join(".baml"))]
    );
    assert_eq!(
        user_file_roots(None, None, Some(home.clone())),
        vec![btel(&home.join(".baml"))]
    );
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

    let recordings = root.join(".baml/btel/recordings");
    let fallback = home.join(".baml/btel/recordings");
    std::fs::create_dir_all(&recordings).unwrap();
    std::fs::create_dir_all(&fallback).unwrap();
    run(&recordings, false);
    // Packed programs keep recording to the user store.
    run(&fallback, true);

    // An unwritable project store falls back to the user store.
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        let readonly = std::fs::Permissions::from_mode(0o555);
        std::fs::set_permissions(&recordings, readonly).unwrap();
        let writable = std::fs::File::create(recordings.join("probe")).is_ok();
        if !writable {
            run(&fallback, false);
        }
        std::fs::set_permissions(&recordings, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
}
