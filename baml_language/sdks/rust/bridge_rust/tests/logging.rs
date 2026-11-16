//! The bridge prints nothing on its own: every message goes through the
//! `log` facade, so an app without a logger sees nothing and an app with one
//! gets the records.

use std::{collections::HashMap, convert::Infallible, process::Command, sync::Mutex};

mod common;

use baml_bridge::{loader, runtime};

const TEST_NAME: &str = "the_bridge_logs_only_through_the_log_facade";
const CHILD: &str = "BAML_BRIDGE_TEST_LOGGING_CHILD";

/// Every record logged in this process, as `(level, target, message)`.
struct Capture(Mutex<Vec<(log::Level, String, String)>>);

impl log::Log for Capture {
    fn enabled(&self, _: &log::Metadata<'_>) -> bool {
        true
    }

    fn log(&self, record: &log::Record<'_>) {
        self.0.lock().expect("capture lock").push((
            record.level(),
            record.target().to_string(),
            record.args().to_string(),
        ));
    }

    fn flush(&self) {}
}

static CAPTURE: Capture = Capture(Mutex::new(Vec::new()));

/// The child: load the engine and make a call with no logger installed.
fn load_and_call() {
    common::locate_dev_engine();
    let files = HashMap::from([(
        "main.baml".to_string(),
        "function answer() -> int { 42 }".to_string(),
    )]);
    // The vfs rejects trailing slashes (macOS's temp_dir ends in one).
    let root = std::env::temp_dir();
    let root = root.to_string_lossy();
    runtime::initialize_from_files(root.trim_end_matches('/'), &files)
        .expect("runtime initializes");
    let answer: i64 =
        runtime::invoke_sync::<i64, Infallible>("user.answer", |_| Ok(vec![])).expect("call");
    assert_eq!(answer, 42);
}

#[test]
fn the_bridge_logs_only_through_the_log_facade() {
    if std::env::var_os(CHILD).is_some() {
        load_and_call();
        return;
    }

    let home = std::env::temp_dir().join(format!("baml-bridge-logging-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&home);
    std::fs::create_dir_all(&home).expect("create BAML_HOME");
    // `BAML_LOG` is set to show that it no longer turns on any output.
    let output = Command::new(std::env::current_exe().expect("test binary"))
        .args(["--exact", TEST_NAME, "--nocapture"])
        .env(CHILD, "1")
        .env("BAML_LOG", "trace")
        .env("BAML_HOME", &home)
        .env("BAML_TELEMETRY", "off")
        .env("BOUNDARY_API_KEY", "local")
        .env_remove("BOUNDARY_API_URL")
        .env_remove("BOUNDARY_PROJECT")
        .output()
        .expect("run child");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "{}\n{stdout}\n{stderr}",
        output.status
    );
    assert!(
        stderr.is_empty(),
        "with no logger installed the bridge must print nothing, got:\n{stderr}"
    );
    std::fs::remove_dir_all(&home).expect("clean up BAML_HOME");

    log::set_logger(&CAPTURE).expect("no other logger in this test binary");
    log::set_max_level(log::LevelFilter::Trace);
    common::locate_dev_engine();
    loader::preload().expect("engine loads");
    let records = CAPTURE.0.lock().expect("capture lock");
    assert!(
        records.iter().any(|(level, target, message)| {
            *level == log::Level::Debug
                && target.starts_with("baml_bridge")
                && message.starts_with("Loaded BAML v")
        }),
        "the installed logger must receive the load record, got: {records:?}"
    );
}
