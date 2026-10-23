//! `runtime::shutdown` against a live engine. A test binary of its own:
//! shutdown ends the process-global runtime that every test in a binary
//! shares.

use std::{collections::HashMap, convert::Infallible, path::Path};

mod common;

use baml_bridge::{encode, runtime};

const BAML_SRC: &str = "function answer() -> int { 42 }";

/// Completed telemetry files across every recording under `home`. Files
/// are immutable once complete, so sealing the recording at shutdown adds
/// one; periodic flushes add files only while events are buffered.
fn completed_telemetry_files(home: &Path) -> usize {
    let Ok(recordings) = std::fs::read_dir(home.join("btel").join("recordings")) else {
        return 0;
    };
    recordings
        .map(|recording| recording.expect("recording entry").path())
        .filter(|recording| recording.is_dir())
        .flat_map(|recording| std::fs::read_dir(recording).expect("read recording"))
        .map(|file| file.expect("recording file").path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "btel"))
        .count()
}

#[test]
fn shutdown_seals_the_recording_is_idempotent_and_ends_the_runtime() {
    let home = std::env::temp_dir().join(format!("baml-bridge-shutdown-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&home);
    std::fs::create_dir_all(&home).expect("create BAML_HOME");
    // SAFETY: the only test in this binary, before the runtime starts and
    // before any other thread reads the environment.
    #[expect(unsafe_code)]
    unsafe {
        // Record telemetry to local files under a private BAML_HOME only.
        std::env::set_var("BAML_HOME", &home);
        std::env::set_var("BAML_TELEMETRY", "medium");
        std::env::set_var("BOUNDARY_API_KEY", "local");
        std::env::remove_var("BOUNDARY_API_URL");
        std::env::remove_var("BOUNDARY_PROJECT");
    }
    common::locate_dev_engine();
    let files = HashMap::from([("main.baml".to_string(), BAML_SRC.to_string())]);
    // The vfs rejects trailing slashes (macOS's temp_dir ends in one).
    let root = std::env::temp_dir();
    let root = root.to_string_lossy();
    runtime::initialize_from_files(root.trim_end_matches('/'), &files)
        .expect("runtime initializes");

    let answer: i64 =
        runtime::invoke_sync::<i64, Infallible>("user.answer", encode::kwargs(vec![]), vec![])
            .expect("call before shutdown");
    assert_eq!(answer, 42);

    let before = completed_telemetry_files(&home);
    runtime::shutdown().expect("shutdown");
    let sealed = completed_telemetry_files(&home);
    assert!(
        sealed > before,
        "shutdown must seal the recording ({before} completed files before, {sealed} after)"
    );

    runtime::shutdown().expect("a second shutdown is harmless");
    assert_eq!(
        completed_telemetry_files(&home),
        sealed,
        "a second shutdown has nothing left to record"
    );
    let after =
        runtime::invoke_sync::<i64, Infallible>("user.answer", encode::kwargs(vec![]), vec![]);
    assert!(
        after.is_err(),
        "a call after shutdown must fail, got {after:?}"
    );

    std::fs::remove_dir_all(&home).expect("clean up BAML_HOME");
}
