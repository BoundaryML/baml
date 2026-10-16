//! The runtime shuts down when the process exits, without an explicit
//! `runtime::shutdown`. Only process exit runs the hook, so each scenario
//! re-executes this test binary as a child process.

use std::{
    collections::HashMap,
    convert::Infallible,
    path::Path,
    process::Command,
    time::{Duration, Instant},
};

mod common;

use baml_bridge::{encode, runtime};

const TEST_NAME: &str = "the_runtime_shuts_down_at_process_exit";
const SCENARIO: &str = "BAML_BRIDGE_TEST_EXIT_SCENARIO";
/// File in the child's `BAML_HOME` holding the completed-file count it saw
/// just before ending.
const BEFORE_EXIT: &str = "completed-files-before-exit";
const BAML_SRC: &str = "
function answer() -> int { 42 }

function stuck() -> int {
  baml.sys.sleep(baml.time.Duration.from_seconds(600));
  1
}
";

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

/// The child: make a call, then end the process without `shutdown`.
fn exit_without_shutdown(scenario: &str) {
    common::locate_dev_engine();
    let files = HashMap::from([("main.baml".to_string(), BAML_SRC.to_string())]);
    // The vfs rejects trailing slashes (macOS's temp_dir ends in one).
    let root = std::env::temp_dir();
    let root = root.to_string_lossy();
    runtime::initialize_from_files(root.trim_end_matches('/'), &files)
        .expect("runtime initializes");
    let answer: i64 =
        runtime::invoke_sync::<i64, Infallible>("user.answer", encode::kwargs(vec![]), vec![])
            .expect("call");
    assert_eq!(answer, 42);
    if scenario == "stuck" {
        // Still in flight when the process exits.
        std::thread::spawn(|| {
            let _ = runtime::invoke_sync::<i64, Infallible>(
                "user.stuck",
                encode::kwargs(vec![]),
                vec![],
            );
        });
        std::thread::sleep(Duration::from_millis(500));
    }
    let home = std::env::var_os("BAML_HOME").expect("BAML_HOME");
    let home = Path::new(&home);
    std::fs::write(
        home.join(BEFORE_EXIT),
        completed_telemetry_files(home).to_string(),
    )
    .expect("record the count");
    if scenario == "exit" {
        #[expect(clippy::exit, reason = "the exit path under test")]
        std::process::exit(0);
    }
}

#[test]
fn the_runtime_shuts_down_at_process_exit() {
    if let Ok(scenario) = std::env::var(SCENARIO) {
        exit_without_shutdown(&scenario);
        return;
    }
    let scenarios: &[&str] = if cfg!(unix) {
        // On Windows, `std::process::exit` skips C exit handlers.
        &["return", "exit", "stuck"]
    } else {
        &["return", "stuck"]
    };
    for &scenario in scenarios {
        let home = std::env::temp_dir().join(format!(
            "baml-bridge-exit-{scenario}-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&home);
        std::fs::create_dir_all(&home).expect("create BAML_HOME");
        let started = Instant::now();
        // Record telemetry to local files under a private BAML_HOME only.
        let output = Command::new(std::env::current_exe().expect("test binary"))
            .args(["--exact", TEST_NAME, "--nocapture"])
            .env(SCENARIO, scenario)
            .env("BAML_HOME", &home)
            .env("BAML_TELEMETRY", "medium")
            .env("BOUNDARY_API_KEY", "local")
            .env_remove("BOUNDARY_API_URL")
            .env_remove("BOUNDARY_PROJECT")
            .output()
            .expect("run child");
        let elapsed = started.elapsed();
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            output.status.success(),
            "{scenario}: {}\n{stdout}\n{stderr}",
            output.status
        );
        let before: usize = std::fs::read_to_string(home.join(BEFORE_EXIT))
            .ok()
            .and_then(|count| count.trim().parse().ok())
            .unwrap_or_else(|| panic!("{scenario}: the child recorded no count:\n{stdout}"));
        let after = completed_telemetry_files(&home);
        if scenario == "stuck" {
            // The engine would wait 600 s for the call; the hook gives up.
            assert!(elapsed < Duration::from_secs(60), "exit took {elapsed:?}");
            assert!(
                stderr.contains("did not finish within"),
                "{scenario}: {stderr}"
            );
        } else {
            assert!(
                after > before,
                "{scenario}: exit must seal the recording ({before} completed files \
                 before, {after} after)\n{stderr}"
            );
        }
        std::fs::remove_dir_all(&home).expect("clean up BAML_HOME");
    }
}
